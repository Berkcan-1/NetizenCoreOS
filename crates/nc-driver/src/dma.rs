// crates/nc-driver/src/dma.rs
extern crate alloc;

use alloc::alloc::{alloc_zeroed, dealloc, Layout};
use core::arch::asm;
use core::ptr::NonNull;
use core::sync::atomic::{fence, Ordering};

/// Aygıtın adresleyebileceği fiziksel bellek üst sınırı (DMA Maskesi)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DmaMask {
    /// Eski PCI ve 32-bit descriptor kullanan aygıtlar (AC97, RTL8139, 32-bit E1000 yolu) -> Maks 4 GiB
    Bits32,
    /// Modern PCIe aygıtları (64-bit AHCI, NVMe, RTL8168, xHCI) -> Tam 64-bit adresleme
    Bits64,
}

impl DmaMask {
    #[inline]
    pub const fn max_phys_addr(self) -> u64 {
        match self {
            DmaMask::Bits32 => 0xFFFF_FFFF,
            DmaMask::Bits64 => u64::MAX,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DmaError {
    InvalidParameters,
    AlignmentNotPowerOfTwo,
    OutOfMemory,
    UnmappedVirtualAddress,
    AddressExceedsMask { phys_end: u64, mask_max: u64 },
}

/// Translate an x86-64 virtual address through the active four- or five-level page tables.
/// The kernel MUST map every page-table frame at its physical address (identity/direct map)
/// before calling this function. The walker dereferences those physical frame addresses;
/// it cannot independently prove that the direct map exists. Supports 4 KiB, 2 MiB and 1 GiB
/// leaves and never assumes that the target virtual address itself is identity mapped.
pub fn virt_to_phys(virt: usize) -> Option<u64> {
    const ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;
    let vaddr = virt as u64;
    let cr3: u64;
    let cr4: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
    }

    // Platform invariant: all paging-structure frames are reachable via physical identity map.
    // A missing mapping can fault here; the page-table access contract is documented above.
    let mut table_base = cr3 & ADDRESS_MASK;
    if table_base == 0 { return None; }

    // Support both four-level paging and Intel/AMD five-level paging (LA57).
    if (cr4 & (1 << 12)) != 0 {
        let pml5_idx = ((vaddr >> 48) & 0x1FF) as usize;
        let pml5e = unsafe { core::ptr::read_volatile((table_base as *const u64).add(pml5_idx)) };
        if (pml5e & 1) == 0 || (pml5e & 0x80) != 0 { return None; }
        table_base = pml5e & ADDRESS_MASK;
        if table_base == 0 { return None; }
    }

    let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4e = core::ptr::read_volatile((table_base as *const u64).add(pml4_idx));
        if (pml4e & 1) == 0 { return None; }

        let pdpt_base = pml4e & ADDRESS_MASK;
        if pdpt_base == 0 { return None; }
        let pdpte = core::ptr::read_volatile((pdpt_base as *const u64).add(pdpt_idx));
        if (pdpte & 1) == 0 { return None; }
        if (pdpte & 0x80) != 0 {
            let page_base = pdpte & 0x000F_FFFF_C000_0000;
            return Some(page_base | (vaddr & 0x3FFF_FFFF));
        }

        let pd_base = pdpte & ADDRESS_MASK;
        if pd_base == 0 { return None; }
        let pde = core::ptr::read_volatile((pd_base as *const u64).add(pd_idx));
        if (pde & 1) == 0 { return None; }
        if (pde & 0x80) != 0 {
            let page_base = pde & 0x000F_FFFF_FFE0_0000;
            return Some(page_base | (vaddr & 0x001F_FFFF));
        }

        let pt_base = pde & ADDRESS_MASK;
        if pt_base == 0 { return None; }
        let pte = core::ptr::read_volatile((pt_base as *const u64).add(pt_idx));
        if (pte & 1) == 0 { return None; }
        let page_base = pte & ADDRESS_MASK;
        Some(page_base | (vaddr & 0xFFF))
    }
}


/// Returns the physical base only when every virtual page in the range maps to the
/// immediately following physical page. DMA engines are not allowed to follow CPU
/// page tables, so a virtually contiguous allocation is insufficient by itself.
pub fn virt_range_to_phys_contiguous(virt: usize, size: usize) -> Option<u64> {
    if size == 0 {
        return None;
    }
    let last = virt.checked_add(size - 1)?;
    let first_phys = virt_to_phys(virt)?;
    let first_virtual_page_end = (virt | 0xFFF).checked_add(1)?;
    let mut cursor = first_virtual_page_end;
    while cursor <= last {
        let mapped = virt_to_phys(cursor)?;
        let expected = first_phys.checked_add((cursor - virt) as u64)?;
        if mapped != expected {
            return None;
        }
        cursor = cursor.checked_add(4096)?;
    }
    Some(first_phys)
}

/// Ömrü, hizalaması ve fiziksel DMA adresi garanti altına alınmış donanım tamponu.
/// Sürücü bu yapıyı düşürdüğünde (Drop) bellek güvenli biçimde iade edilir.
pub struct DmaBuffer {
    cpu_ptr: NonNull<u8>,
    phys_addr: u64,
    size: usize,
    layout: Layout,
    mask: DmaMask,
}

// Moving an owned DMA allocation is safe because its backing address does not move.
// Sharing it through `&DmaBuffer` is intentionally not Sync: a device may write it.
unsafe impl Send for DmaBuffer {}

impl DmaBuffer {
    /// Belirtilen boyut, hizalama ve DMA maskesine uygun sıfırlanmış DMA belleği ayırır.
    pub fn allocate(size: usize, align: usize, mask: DmaMask) -> Result<Self, DmaError> {
        if size == 0 {
            return Err(DmaError::InvalidParameters);
        }
        let safe_align = align.max(16);
        if !safe_align.is_power_of_two() {
            return Err(DmaError::AlignmentNotPowerOfTwo);
        }

        let layout = Layout::from_size_align(size, safe_align)
            .map_err(|_| DmaError::InvalidParameters)?;

        let raw_ptr = unsafe { alloc_zeroed(layout) };
        let cpu_ptr = NonNull::new(raw_ptr).ok_or(DmaError::OutOfMemory)?;

        let Some(phys_addr) = virt_range_to_phys_contiguous(cpu_ptr.as_ptr() as usize, size) else {
            unsafe { dealloc(cpu_ptr.as_ptr(), layout) };
            return Err(DmaError::UnmappedVirtualAddress);
        };

        let Some(phys_end) = phys_addr.checked_add((size - 1) as u64) else {
            unsafe { dealloc(cpu_ptr.as_ptr(), layout) };
            return Err(DmaError::InvalidParameters);
        };
        let mask_max = mask.max_phys_addr();
        if phys_end > mask_max {
            unsafe { dealloc(cpu_ptr.as_ptr(), layout) };
            return Err(DmaError::AddressExceedsMask { phys_end, mask_max });
        }

        fence(Ordering::SeqCst);

        Ok(Self {
            cpu_ptr,
            phys_addr,
            size,
            layout,
            mask,
        })
    }

    #[inline]
    pub fn virt_ptr(&self) -> *mut u8 {
        self.cpu_ptr.as_ptr()
    }

    /// 64-bit DMA destekleyen aygıtlar (AHCI, NVMe, RTL8168) için tam fiziksel adres
    #[inline]
    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    /// Yalnızca 32-bit DMA destekleyen aygıtlar için taşma kontrollü 32-bit fiziksel adres
    #[inline]
    pub fn phys_addr_u32(&self) -> Result<u32, DmaError> {
        let end = self.phys_addr.checked_add((self.size.saturating_sub(1)) as u64).ok_or(DmaError::InvalidParameters)?;
        if end > 0xFFFF_FFFF {
            Err(DmaError::AddressExceedsMask {
                phys_end: end,
                mask_max: 0xFFFF_FFFF,
            })
        } else {
            Ok(self.phys_addr as u32)
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn mask(&self) -> DmaMask {
        self.mask
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.cpu_ptr.as_ptr(), self.size) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.cpu_ptr.as_ptr(), self.size) }
    }

    /// T tipinde dizi/halka (Descriptor Ring, Command List vb.) olarak güvenli erişim
    pub unsafe fn as_typed_slice_mut<T>(&mut self, count: usize) -> Option<&mut [T]> {
        let needed = count.checked_mul(core::mem::size_of::<T>())?;
        if needed > self.size || (self.cpu_ptr.as_ptr() as usize) % core::mem::align_of::<T>() != 0 {
            return None;
        }
        Some(core::slice::from_raw_parts_mut(
            self.cpu_ptr.as_ptr() as *mut T,
            count,
        ))
    }

    /// CPU yazma işlemleri bittikten sonra donanıma (TX/Command) bildirmeden önce çağrılır
    #[inline]
    pub fn sync_for_device(&self) {
        fence(Ordering::Release);
        unsafe {
            asm!("mfence", options(nostack, preserves_flags));
        }
    }

    /// Donanım yazma işlemi (RX/Completion) bittikten sonra CPU okumadan önce çağrılır
    #[inline]
    pub fn sync_for_cpu(&self) {
        unsafe {
            asm!("lfence", options(nostack, preserves_flags));
        }
        fence(Ordering::Acquire);
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        fence(Ordering::SeqCst);
        unsafe {
            dealloc(self.cpu_ptr.as_ptr(), self.layout);
        }
    }
}
