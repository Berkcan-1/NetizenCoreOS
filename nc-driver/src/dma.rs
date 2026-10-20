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

/// CPU sanal adresini aktif CR3 (PML4) sayfa tablolarını yürüyerek gerçek fiziksel adrese çevirir.
/// Stage2 (2 MiB HugePage) ve UEFI (4 KiB / 2 MiB / 1 GiB) sayfa tablolarının tamamıyla uyumludur.
pub fn virt_to_phys(virt: usize) -> Option<u64> {
    let vaddr = virt as u64;
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }

    let pml4_base = cr3 & 0x000F_FFFF_FFFF_F000;
    if pml4_base == 0 {
        return Some(vaddr);
    }

    let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;

    unsafe {
        // 1. PML4 Girişi
        let pml4e = core::ptr::read_volatile((pml4_base as *const u64).add(pml4_idx));
        if (pml4e & 1) == 0 {
            return None;
        }

        // 2. PDPT Girişi
        let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpte = core::ptr::read_volatile((pdpt_base as *const u64).add(pdpt_idx));
        if (pdpte & 1) == 0 {
            return None;
        }
        // 1 GiB Huge Page kontrolü (PS biti = bit 7)
        if (pdpte & 0x80) != 0 {
            let page_base = pdpte & 0x000F_FFFF_C000_0000;
            return Some(page_base | (vaddr & 0x3FFF_FFFF));
        }

        // 3. Page Directory (PD) Girişi
        let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
        let pde = core::ptr::read_volatile((pd_base as *const u64).add(pd_idx));
        if (pde & 1) == 0 {
            return None;
        }
        // 2 MiB Huge Page kontrolü (Stage2 bootloader varsayılanı: PS biti = bit 7)
        if (pde & 0x80) != 0 {
            let page_base = pde & 0x000F_FFFF_FFE0_0000;
            return Some(page_base | (vaddr & 0x001F_FFFF));
        }

        // 4. 4 KiB Page Table (PT) Girişi
        let pt_base = pde & 0x000F_FFFF_FFFF_F000;
        let pte = core::ptr::read_volatile((pt_base as *const u64).add(pt_idx));
        if (pte & 1) == 0 {
            return None;
        }
        let page_base = pte & 0x000F_FFFF_FFFF_F000;
        Some(page_base | (vaddr & 0xFFF))
    }
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

unsafe impl Send for DmaBuffer {}
unsafe impl Sync for DmaBuffer {}

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

        let Some(phys_addr) = virt_to_phys(cpu_ptr.as_ptr() as usize) else {
            unsafe { dealloc(cpu_ptr.as_ptr(), layout) };
            return Err(DmaError::UnmappedVirtualAddress);
        };

        let phys_end = phys_addr.saturating_add((size - 1) as u64);
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
        let end = self.phys_addr.saturating_add((self.size.saturating_sub(1)) as u64);
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
