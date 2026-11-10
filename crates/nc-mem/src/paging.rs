// crates/nc-mem/src/paging.rs
use core::arch::asm;
use core::arch::x86_64::{__cpuid, __cpuid_count};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const PAGE_SIZE: usize = 4096;

pub const USER_SPACE_START: u64 = 0x0000_0080_0000_0000;
pub const USER_SPACE_END: u64   = 0x0000_7FFF_FFFF_FFFF;

pub const PTE_PRESENT: u64  = 1 << 0;
pub const PTE_WRITABLE: u64 = 1 << 1;
pub const PTE_USER: u64     = 1 << 2;
pub const PTE_PWT: u64      = 1 << 3;
pub const PTE_PCD: u64      = 1 << 4;
pub const PTE_HUGE: u64     = 1 << 7;
pub const PTE_COW: u64      = 1 << 9; // Faz 5: Copy-on-Write yazılım biti (Bit 9)
pub const PTE_NO_EXEC: u64  = 1 << 63;

pub const ADDR_MASK_4K: u64 = 0x000F_FFFF_FFFF_F000;
pub const ADDR_MASK_2M: u64 = 0x000F_FFFF_FFE0_0000;
pub const ADDR_MASK_1G: u64 = 0x000F_FFFF_C000_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PagePermissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user: bool,
}

impl PagePermissions {
    pub const KERNEL_RO: Self = Self { readable: true, writable: false, executable: false, user: false };
    pub const KERNEL_RW: Self = Self { readable: true, writable: true, executable: false, user: false };
    pub const KERNEL_RX: Self = Self { readable: true, writable: false, executable: true, user: false };

    pub const USER_RO: Self = Self { readable: true, writable: false, executable: false, user: true };
    pub const USER_RW: Self = Self { readable: true, writable: true, executable: false, user: true };
    pub const USER_RX: Self = Self { readable: true, writable: false, executable: true, user: true };

    #[inline]
    pub fn validate_wx(&self) -> Result<(), VmError> {
        if self.writable && self.executable { Err(VmError::WxViolation) } else { Ok(()) }
    }

    #[inline]
    pub fn to_pte_flags(&self, nx_enabled: bool) -> u64 {
        let mut flags = PTE_PRESENT;
        if self.writable { flags |= PTE_WRITABLE; }
        if self.user { flags |= PTE_USER; }
        if !self.executable && nx_enabled { flags |= PTE_NO_EXEC; }
        flags
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VmError {
    UnalignedAddress,
    NonCanonicalAddress,
    OutOfMemory,
    WxViolation,
    AlreadyMapped,
    NotMapped,
    SupervisorOverlap,
    AddressOverflow,
    InvalidFrame,
    UnmanagedFrame,
    DoubleFree,
    InvalidMemoryMap,
    TransactionCapacity,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicalRegion {
    pub start: u64,
    pub end: u64,
}

pub const MAX_PHYSICAL_ADDRESS: u64 = 4 * 1024 * 1024 * 1024;
const MAX_FRAMES: usize = (MAX_PHYSICAL_ADDRESS as usize) / PAGE_SIZE;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;
const FIRST_MANAGED_FRAME: usize = (16 * 1024 * 1024) / PAGE_SIZE;

// 1 MiB statik dizi yerine kompakt 512 elemanlık (8 KiB) paylaşımlı referans tablosu (.bss'i şişirmez)
const MAX_SHARED_SLOTS: usize = 512;

#[derive(Clone, Copy)]
struct SharedRefEntry {
    phys: u64,
    extra_refs: u32,
}

impl SharedRefEntry {
    const EMPTY: Self = Self { phys: 0, extra_refs: 0 };
}

pub struct FrameAllocator {
    free_bitmap: [u64; BITMAP_WORDS],
    managed_bitmap: [u64; BITMAP_WORDS],
    shared_table: [SharedRefEntry; MAX_SHARED_SLOTS],
    total_free: usize,
    max_frames: usize,
    nx_enabled: bool,
}

static PMM_LOCK: AtomicBool = AtomicBool::new(false);
static INVALID_FREE_COUNT: AtomicUsize = AtomicUsize::new(0);

struct PmmCell(UnsafeCell<FrameAllocator>);
unsafe impl Sync for PmmCell {}

static GLOBAL_PMM: PmmCell = PmmCell(UnsafeCell::new(FrameAllocator {
    free_bitmap: [0; BITMAP_WORDS],
    managed_bitmap: [0; BITMAP_WORDS],
    shared_table: [SharedRefEntry::EMPTY; MAX_SHARED_SLOTS],
    total_free: 0,
    max_frames: MAX_FRAMES,
    nx_enabled: false,
}));

fn lock_pmm() -> u64 {
    let flags: u64;
    unsafe {
        asm!("pushfq", "pop {}", out(reg) flags);
        #[cfg(not(test))]
        asm!("cli", options(nomem, nostack));
    }
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    flags
}

fn unlock_pmm(flags: u64) {
    PMM_LOCK.store(false, Ordering::Release);
    #[cfg(not(test))]
    unsafe { asm!("push {}", "popfq", in(reg) flags); }
    #[cfg(test)]
    let _ = flags;
}

fn detected_physical_limit() -> u64 {
    let max_ext = __cpuid(0x8000_0000).eax;
    if max_ext < 0x8000_0008 { return 1u64 << 32; }
    let physical_bits = __cpuid_count(0x8000_0008, 0).eax & 0xFF;
    if !(32..=52).contains(&physical_bits) { return 1u64 << 32; }
    (1u64 << physical_bits).min(MAX_PHYSICAL_ADDRESS)
}

#[inline]
pub fn cpu_supports_nx() -> bool {
    if __cpuid(0x8000_0000).eax < 0x8000_0001 { return false; }
    let feature_edx = __cpuid_count(0x8000_0001, 0).edx;
    if (feature_edx & (1 << 20)) == 0 { return false; }
    let efer_low: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") 0xC000_0080u32,
            out("eax") efer_low,
            out("edx") _,
            options(nomem, nostack)
        );
    }
    (efer_low & (1 << 11)) != 0
}

#[inline]
fn bit_is_set(bitmap: &[u64; BITMAP_WORDS], frame: usize) -> bool {
    (bitmap[frame / 64] & (1u64 << (frame % 64))) != 0
}

pub unsafe fn init_pmm(regions: &[PhysicalRegion]) -> Result<(), VmError> {
    if regions.is_empty() { return Err(VmError::InvalidMemoryMap); }
    let irq_flags = lock_pmm();
    let pmm = unsafe { &mut *GLOBAL_PMM.0.get() };
    pmm.free_bitmap.fill(0);
    pmm.managed_bitmap.fill(0);
    pmm.shared_table.fill(SharedRefEntry::EMPTY);
    pmm.total_free = 0;
    pmm.max_frames = (detected_physical_limit() as usize) / PAGE_SIZE;
    pmm.nx_enabled = cpu_supports_nx();

    for region in regions {
        if region.end <= region.start { continue; }
        let Some(start_aligned) = region.start.checked_add((PAGE_SIZE as u64) - 1) else { continue; };
        let start = start_aligned & !((PAGE_SIZE as u64) - 1);
        let end = region.end & !((PAGE_SIZE as u64) - 1);
        let first = ((start / PAGE_SIZE as u64) as usize).max(FIRST_MANAGED_FRAME);
        let last = ((end / PAGE_SIZE as u64) as usize).min(pmm.max_frames);
        if last <= first { continue; }
        for frame in first..last {
            let word = frame / 64;
            let mask = 1u64 << (frame % 64);
            if (pmm.managed_bitmap[word] & mask) == 0 {
                pmm.managed_bitmap[word] |= mask;
                pmm.free_bitmap[word] |= mask;
                pmm.total_free += 1;
            }
        }
    }
    let has_frames = pmm.total_free != 0;
    unlock_pmm(irq_flags);
    if has_frames { Ok(()) } else { Err(VmError::InvalidMemoryMap) }
}

#[inline]
fn frame_is_allocated(frame: usize) -> bool {
    if frame < FIRST_MANAGED_FRAME || frame >= MAX_FRAMES { return false; }
    let irq_flags = lock_pmm();
    let pmm = unsafe { &*GLOBAL_PMM.0.get() };
    let allocated = frame < pmm.max_frames
        && bit_is_set(&pmm.managed_bitmap, frame)
        && !bit_is_set(&pmm.free_bitmap, frame);
    unlock_pmm(irq_flags);
    allocated
}

pub fn alloc_frame() -> Option<u64> {
    let irq_flags = lock_pmm();
    let pmm = unsafe { &mut *GLOBAL_PMM.0.get() };
    let mut found = None;
    for word_idx in FIRST_MANAGED_FRAME / 64..(pmm.max_frames + 63) / 64 {
        let available = pmm.free_bitmap[word_idx] & pmm.managed_bitmap[word_idx];
        if available != 0 {
            let bit = available.trailing_zeros() as usize;
            let frame = word_idx * 64 + bit;
            pmm.free_bitmap[word_idx] &= !(1u64 << bit);
            pmm.total_free = pmm.total_free.saturating_sub(1);
            let phys = (frame * PAGE_SIZE) as u64;
            unsafe { core::ptr::write_bytes(phys as *mut u8, 0, PAGE_SIZE); }
            found = Some(phys);
            break;
        }
    }
    unlock_pmm(irq_flags);
    found
}

pub fn retain_frame(phys_addr: u64) -> Result<(), VmError> {
    if (phys_addr & 0xFFF) != 0
        || phys_addr < (FIRST_MANAGED_FRAME as u64 * PAGE_SIZE as u64)
        || phys_addr >= MAX_PHYSICAL_ADDRESS
    {
        return Err(VmError::InvalidFrame);
    }
    let frame = (phys_addr / PAGE_SIZE as u64) as usize;
    let irq_flags = lock_pmm();
    let pmm = unsafe { &mut *GLOBAL_PMM.0.get() };
    if frame >= pmm.max_frames
        || !bit_is_set(&pmm.managed_bitmap, frame)
        || bit_is_set(&pmm.free_bitmap, frame)
    {
        unlock_pmm(irq_flags);
        return Err(VmError::InvalidFrame);
    }

    let mut found_slot = None;
    let mut empty_slot = None;
    for i in 0..MAX_SHARED_SLOTS {
        if pmm.shared_table[i].phys == phys_addr {
            found_slot = Some(i);
            break;
        } else if empty_slot.is_none() && pmm.shared_table[i].phys == 0 {
            empty_slot = Some(i);
        }
    }

    let res = if let Some(idx) = found_slot {
        pmm.shared_table[idx].extra_refs = pmm.shared_table[idx].extra_refs.saturating_add(1);
        Ok(())
    } else if let Some(idx) = empty_slot {
        pmm.shared_table[idx] = SharedRefEntry { phys: phys_addr, extra_refs: 1 };
        Ok(())
    } else {
        Err(VmError::OutOfMemory)
    };

    unlock_pmm(irq_flags);
    res
}

pub fn frame_ref_count(phys_addr: u64) -> usize {
    if (phys_addr & 0xFFF) != 0
        || phys_addr < (FIRST_MANAGED_FRAME as u64 * PAGE_SIZE as u64)
        || phys_addr >= MAX_PHYSICAL_ADDRESS
    {
        return 0;
    }
    let frame = (phys_addr / PAGE_SIZE as u64) as usize;
    let irq_flags = lock_pmm();
    let pmm = unsafe { &*GLOBAL_PMM.0.get() };
    let count = if frame < pmm.max_frames
        && bit_is_set(&pmm.managed_bitmap, frame)
        && !bit_is_set(&pmm.free_bitmap, frame)
    {
        let extra = pmm.shared_table.iter()
            .find(|entry| entry.phys == phys_addr)
            .map(|entry| entry.extra_refs as usize)
            .unwrap_or(0);
        1 + extra
    } else {
        0
    };
    unlock_pmm(irq_flags);
    count
}

pub unsafe fn free_frame_checked(phys_addr: u64) -> Result<(), VmError> {
    if (phys_addr & 0xFFF) != 0
        || phys_addr < (FIRST_MANAGED_FRAME as u64 * PAGE_SIZE as u64)
        || phys_addr >= MAX_PHYSICAL_ADDRESS
    {
        INVALID_FREE_COUNT.fetch_add(1, Ordering::Relaxed);
        return Err(VmError::InvalidFrame);
    }
    let frame = (phys_addr / PAGE_SIZE as u64) as usize;
    let irq_flags = lock_pmm();
    let pmm = unsafe { &mut *GLOBAL_PMM.0.get() };
    if frame >= pmm.max_frames {
        unlock_pmm(irq_flags);
        INVALID_FREE_COUNT.fetch_add(1, Ordering::Relaxed);
        return Err(VmError::InvalidFrame);
    }
    let word = frame / 64;
    let bit = frame % 64;
    let mask = 1u64 << bit;

    let shared_idx = pmm.shared_table.iter().position(|entry| entry.phys == phys_addr);

    let result = if (pmm.managed_bitmap[word] & mask) == 0 {
        Err(VmError::UnmanagedFrame)
    } else if (pmm.free_bitmap[word] & mask) != 0 {
        Err(VmError::DoubleFree)
    } else if let Some(idx) = shared_idx {
        if pmm.shared_table[idx].extra_refs > 1 {
            pmm.shared_table[idx].extra_refs -= 1;
        } else {
            pmm.shared_table[idx] = SharedRefEntry::EMPTY;
        }
        Ok(())
    } else {
        pmm.free_bitmap[word] |= mask;
        pmm.total_free += 1;
        Ok(())
    };
    unlock_pmm(irq_flags);
    if result.is_err() { INVALID_FREE_COUNT.fetch_add(1, Ordering::Relaxed); }
    result
}

pub fn free_frame_count() -> usize {
    let irq_flags = lock_pmm();
    let count = unsafe { (*GLOBAL_PMM.0.get()).total_free };
    unlock_pmm(irq_flags);
    count
}

pub fn managed_frame_count() -> usize {
    let irq_flags = lock_pmm();
    let pmm = unsafe { &*GLOBAL_PMM.0.get() };
    let count = pmm.managed_bitmap.iter().map(|word| word.count_ones() as usize).sum();
    unlock_pmm(irq_flags);
    count
}

#[inline]
fn pmm_mapping_config() -> (u64, bool) {
    let irq_flags = lock_pmm();
    let pmm = unsafe { &*GLOBAL_PMM.0.get() };
    let config = ((pmm.max_frames as u64) * PAGE_SIZE as u64, pmm.nx_enabled);
    unlock_pmm(irq_flags);
    config
}

pub unsafe fn free_frame(phys_addr: u64) {
    let _ = free_frame_checked(phys_addr);
}

pub fn invalid_free_count() -> usize {
    INVALID_FREE_COUNT.load(Ordering::Relaxed)
}

#[inline]
pub fn is_canonical(vaddr: u64) -> bool {
    let sign_ext = ((vaddr as i64) << 16) >> 16;
    (sign_ext as u64) == vaddr
}

#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3 & ADDR_MASK_4K
}

#[inline]
pub unsafe fn write_cr3(phys_pml4: u64) {
    asm!("mov cr3, {}", in(reg) (phys_pml4 & ADDR_MASK_4K), options(nostack, preserves_flags));
}

#[inline]
pub fn flush_tlb_page(vaddr: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) vaddr, options(nostack, preserves_flags));
    }
}

struct MapTransaction {
    entries: [*mut u64; 4],
    old_values: [u64; 4],
    allocated_frames: [u64; 4],
    len: usize,
}

impl MapTransaction {
    const fn new() -> Self {
        Self { entries: [core::ptr::null_mut(); 4], old_values: [0; 4], allocated_frames: [0; 4], len: 0 }
    }

    unsafe fn record(&mut self, entry: *mut u64, old: u64, allocated: u64) -> Result<(), VmError> {
        if self.len >= self.entries.len() { return Err(VmError::TransactionCapacity); }
        self.entries[self.len] = entry;
        self.old_values[self.len] = old;
        self.allocated_frames[self.len] = allocated;
        self.len += 1;
        Ok(())
    }

    unsafe fn rollback(&mut self) {
        while self.len > 0 {
            self.len -= 1;
            core::ptr::write_volatile(self.entries[self.len], self.old_values[self.len]);
            let frame = self.allocated_frames[self.len];
            if frame != 0 { free_frame(frame); }
        }
    }

    fn commit(&mut self) { self.len = 0; }
}

#[derive(Clone, Copy, Debug)]
pub struct AddressSpace {
    pub pml4_phys: u64,
}

impl AddressSpace {
    #[inline]
    pub fn current() -> Self {
        Self { pml4_phys: read_cr3() }
    }

    pub fn create_user_space() -> Result<Self, VmError> {
        let new_pml4 = alloc_frame().ok_or(VmError::OutOfMemory)?;
        let cur_pml4 = read_cr3();

        unsafe {
            let src = cur_pml4 as *const u64;
            let dst = new_pml4 as *mut u64;
            core::ptr::write_bytes(dst, 0, 512);
            let kernel_entry = core::ptr::read_volatile(src.add(0)) & !PTE_USER;
            core::ptr::write_volatile(dst.add(0), kernel_entry);
        }

        Ok(Self { pml4_phys: new_pml4 })
    }

    pub unsafe fn free_owned_page_tables(&self) {
        unsafe fn free_children(table_phys: u64, level: u8, first: usize, end: usize) {
            if level <= 1 { return; }
            let table = table_phys as *const u64;
            for index in first..end {
                let entry = core::ptr::read_volatile(table.add(index));
                if (entry & PTE_PRESENT) == 0 || (entry & PTE_HUGE) != 0 { continue; }
                let child = entry & ADDR_MASK_4K;
                free_children(child, level - 1, 0, 512);
                free_frame(child);
            }
        }

        free_children(self.pml4_phys, 4, 1, 256);
        free_frame(self.pml4_phys);
    }

    pub fn map_4k(
        &mut self,
        vaddr: u64,
        paddr: u64,
        perms: PagePermissions,
    ) -> Result<(), VmError> {
        self.map_4k_with_extra_flags(vaddr, paddr, perms, 0)
    }

    pub fn map_4k_with_extra_flags(
        &mut self,
        vaddr: u64,
        paddr: u64,
        perms: PagePermissions,
        extra_pte_flags: u64,
    ) -> Result<(), VmError> {
        if (vaddr & 0xFFF) != 0 || (paddr & 0xFFF) != 0 { return Err(VmError::UnalignedAddress); }
        if !is_canonical(vaddr) { return Err(VmError::NonCanonicalAddress); }
        let page_end = vaddr.checked_add(PAGE_SIZE as u64 - 1).ok_or(VmError::AddressOverflow)?;
        perms.validate_wx()?;
        if perms.user && (vaddr < USER_SPACE_START || page_end > USER_SPACE_END) {
            return Err(VmError::SupervisorOverlap);
        }
        if paddr >= MAX_PHYSICAL_ADDRESS { return Err(VmError::InvalidFrame); }
        let (physical_limit, nx) = pmm_mapping_config();
        if paddr >= physical_limit { return Err(VmError::InvalidFrame); }
        if perms.user && !frame_is_allocated((paddr / PAGE_SIZE as u64) as usize) {
            return Err(VmError::InvalidFrame);
        }

        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;

        let dir_flags = PTE_PRESENT | PTE_WRITABLE | if perms.user { PTE_USER } else { 0 };
        let mut transaction = MapTransaction::new();

        let result = (|| -> Result<(), VmError> {
            unsafe {
                let pml4 = self.pml4_phys as *mut u64;
                let pdpt_phys = Self::ensure_next_table_tx(pml4.add(pml4_idx), dir_flags, &mut transaction)?;
                let pdpt = pdpt_phys as *mut u64;
                let pd_phys = Self::ensure_next_table_tx(pdpt.add(pdpt_idx), dir_flags, &mut transaction)?;
                let pd = pd_phys as *mut u64;
                let pde_ptr = pd.add(pd_idx);
                let pde = core::ptr::read_volatile(pde_ptr);
                let pt_phys = if (pde & PTE_PRESENT) != 0 && (pde & PTE_HUGE) != 0 {
                    Self::split_2m_huge_page_tx(pde_ptr, pde, dir_flags, &mut transaction)?
                } else {
                    Self::ensure_next_table_tx(pde_ptr, dir_flags, &mut transaction)?
                };
                let pt = pt_phys as *mut u64;
                let pte_ptr = pt.add(pt_idx);
                if (core::ptr::read_volatile(pte_ptr) & PTE_PRESENT) != 0 {
                    return Err(VmError::AlreadyMapped);
                }
                let final_flags = perms.to_pte_flags(nx) | (extra_pte_flags & PTE_COW);
                core::ptr::write_volatile(pte_ptr, (paddr & ADDR_MASK_4K) | final_flags);
            }
            Ok(())
        })();

        if let Err(error) = result {
            unsafe { transaction.rollback(); }
            flush_tlb_page(vaddr);
            return Err(error);
        }

        transaction.commit();
        flush_tlb_page(vaddr);
        Ok(())
    }

    pub fn mark_4k_cow(&mut self, vaddr: u64) -> Result<u64, VmError> {
        if (vaddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        let pte_ptr = unsafe { self.walk_to_pte(vaddr)? };
        let phys = unsafe {
            let old = core::ptr::read_volatile(pte_ptr);
            if (old & PTE_PRESENT) == 0 || (old & PTE_USER) == 0 {
                return Err(VmError::NotMapped);
            }
            let new_pte = (old & !PTE_WRITABLE) | PTE_COW;
            core::ptr::write_volatile(pte_ptr, new_pte);
            old & ADDR_MASK_4K
        };
        flush_tlb_page(vaddr);
        Ok(phys)
    }

    pub fn protect_4k(&mut self, vaddr: u64, perms: PagePermissions) -> Result<(), VmError> {
        if (vaddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        perms.validate_wx()?;
        let page_end = vaddr.checked_add(PAGE_SIZE as u64 - 1).ok_or(VmError::AddressOverflow)?;
        if perms.user && (vaddr < USER_SPACE_START || page_end > USER_SPACE_END) {
            return Err(VmError::SupervisorOverlap);
        }
        let (_, nx) = pmm_mapping_config();
        let pte_ptr = unsafe { self.walk_to_pte(vaddr)? };
        unsafe {
            let old = core::ptr::read_volatile(pte_ptr);
            if (old & PTE_PRESENT) == 0 {
                return Err(VmError::NotMapped);
            }
            if perms.user {
                let (_, effective_flags) = self.translate_with_flags(vaddr).ok_or(VmError::NotMapped)?;
                if (effective_flags & PTE_USER) == 0 {
                    return Err(VmError::SupervisorOverlap);
                }
            }
            let phys = old & ADDR_MASK_4K;
            let new_flags = perms.to_pte_flags(nx);
            core::ptr::write_volatile(pte_ptr, phys | new_flags);
        }
        flush_tlb_page(vaddr);
        Ok(())
    }

    pub fn unmap_4k(&mut self, vaddr: u64) -> Result<u64, VmError> {
        if (vaddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        let pte_ptr = unsafe { self.walk_to_pte(vaddr)? };
        let phys = unsafe {
            let old = core::ptr::read_volatile(pte_ptr);
            if (old & PTE_PRESENT) == 0 {
                return Err(VmError::NotMapped);
            }
            core::ptr::write_volatile(pte_ptr, 0);
            old & ADDR_MASK_4K
        };
        flush_tlb_page(vaddr);
        Ok(phys)
    }

    /// Tanı amaçlı: `vaddr` için ham [PML4E, PDPTE, PDE, PTE] girdilerini döndürür (ulaşılamayanlar 0).
    /// 1 GiB / 2 MiB yaprakta yürüyüş orada durur.
    pub fn debug_walk(&self, vaddr: u64) -> [u64; 4] {
        let mut out = [0u64; 4];
        if !is_canonical(vaddr) {
            return out;
        }
        unsafe {
            let pml4e = core::ptr::read_volatile((self.pml4_phys as *const u64).add(((vaddr >> 39) & 0x1FF) as usize));
            out[0] = pml4e;
            if (pml4e & PTE_PRESENT) == 0 { return out; }
            let pdpte = core::ptr::read_volatile(((pml4e & ADDR_MASK_4K) as *const u64).add(((vaddr >> 30) & 0x1FF) as usize));
            out[1] = pdpte;
            if (pdpte & PTE_PRESENT) == 0 || (pdpte & PTE_HUGE) != 0 { return out; }
            let pde = core::ptr::read_volatile(((pdpte & ADDR_MASK_4K) as *const u64).add(((vaddr >> 21) & 0x1FF) as usize));
            out[2] = pde;
            if (pde & PTE_PRESENT) == 0 || (pde & PTE_HUGE) != 0 { return out; }
            out[3] = core::ptr::read_volatile(((pde & ADDR_MASK_4K) as *const u64).add(((vaddr >> 12) & 0x1FF) as usize));
        }
        out
    }

    pub fn translate_with_flags(&self, vaddr: u64) -> Option<(u64, u64)> {
        if !is_canonical(vaddr) {
            return None;
        }
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx   = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx   = ((vaddr >> 12) & 0x1FF) as usize;

        unsafe {
            let pml4e = core::ptr::read_volatile((self.pml4_phys as *const u64).add(pml4_idx));
            if (pml4e & PTE_PRESENT) == 0 {
                return None;
            }
            let pdpte = core::ptr::read_volatile(((pml4e & ADDR_MASK_4K) as *const u64).add(pdpt_idx));
            if (pdpte & PTE_PRESENT) == 0 {
                return None;
            }

            // 1 GiB yaprak sayfa (PDPTE.PS=1). UEFI/OVMF kimlik eşlemesini 1 GiB sayfalarla kurabilir
            // (özellikle RAM > 1 GiB iken ilk GiB bölünmeden kalır). Eskiden bu durumda PDPTE bir PD
            // işaretçisi sanılıp çöp okunuyordu ve "Kernel code mapping bulunamadi" hatası çıkıyordu.
            if (pdpte & PTE_HUGE) != 0 {
                let dir_user = (pml4e & PTE_USER) != 0 && (pdpte & PTE_USER) != 0;
                let dir_write = (pml4e & PTE_WRITABLE) != 0 && (pdpte & PTE_WRITABLE) != 0;
                let dir_nx = (pml4e & PTE_NO_EXEC) != 0 || (pdpte & PTE_NO_EXEC) != 0;
                let paddr = (pdpte & ADDR_MASK_1G) | (vaddr & 0x3FFF_FFFF);
                let mut flags = PTE_PRESENT | PTE_HUGE;
                if dir_user { flags |= PTE_USER; }
                if dir_write { flags |= PTE_WRITABLE; }
                if dir_nx { flags |= PTE_NO_EXEC; }
                return Some((paddr, flags));
            }

            let pde = core::ptr::read_volatile(((pdpte & ADDR_MASK_4K) as *const u64).add(pd_idx));
            if (pde & PTE_PRESENT) == 0 {
                return None;
            }

            let dir_user = (pml4e & PTE_USER) != 0 && (pdpte & PTE_USER) != 0 && (pde & PTE_USER) != 0;
            let dir_write = (pml4e & PTE_WRITABLE) != 0 && (pdpte & PTE_WRITABLE) != 0 && (pde & PTE_WRITABLE) != 0;
            let dir_nx = (pml4e & PTE_NO_EXEC) != 0 || (pdpte & PTE_NO_EXEC) != 0 || (pde & PTE_NO_EXEC) != 0;

            if (pde & PTE_HUGE) != 0 {
                let paddr = (pde & ADDR_MASK_2M) | (vaddr & 0x1F_FFFF);
                let mut flags = PTE_PRESENT | PTE_HUGE;
                if dir_user { flags |= PTE_USER; }
                if dir_write { flags |= PTE_WRITABLE; }
                if dir_nx { flags |= PTE_NO_EXEC; }
                return Some((paddr, flags));
            }

            let pte = core::ptr::read_volatile(((pde & ADDR_MASK_4K) as *const u64).add(pt_idx));
            if (pte & PTE_PRESENT) == 0 {
                return None;
            }
            let eff_user = dir_user && ((pte & PTE_USER) != 0);
            let eff_write = dir_write && ((pte & PTE_WRITABLE) != 0);
            let eff_nx = dir_nx || ((pte & PTE_NO_EXEC) != 0);

            let paddr = (pte & ADDR_MASK_4K) | (vaddr & 0xFFF);
            let mut flags = PTE_PRESENT | (pte & PTE_COW);
            if eff_user { flags |= PTE_USER; }
            if eff_write { flags |= PTE_WRITABLE; }
            if eff_nx { flags |= PTE_NO_EXEC; }
            Some((paddr, flags))
        }
    }

    unsafe fn ensure_next_table_tx(
        entry_ptr: *mut u64,
        req_flags: u64,
        transaction: &mut MapTransaction,
    ) -> Result<u64, VmError> {
        let current = core::ptr::read_volatile(entry_ptr);
        if (current & PTE_PRESENT) != 0 {
            let need_user = (req_flags & PTE_USER) != 0 && (current & PTE_USER) == 0;
            let need_write = (req_flags & PTE_WRITABLE) != 0 && (current & PTE_WRITABLE) == 0;
            if need_user || need_write {
                let physical = current & ADDR_MASK_4K;
                if !frame_is_allocated((physical / PAGE_SIZE as u64) as usize) {
                    return Err(VmError::SupervisorOverlap);
                }
                transaction.record(entry_ptr, current, 0)?;
                core::ptr::write_volatile(entry_ptr, current | (req_flags & (PTE_USER | PTE_WRITABLE)));
            }
            return Ok(current & ADDR_MASK_4K);
        }
        let frame = alloc_frame().ok_or(VmError::OutOfMemory)?;
        if let Err(error) = transaction.record(entry_ptr, current, frame) {
            free_frame(frame);
            return Err(error);
        }
        core::ptr::write_volatile(entry_ptr, frame | req_flags);
        Ok(frame)
    }

    unsafe fn split_2m_huge_page_tx(
        pde_ptr: *mut u64,
        old_pde: u64,
        req_flags: u64,
        transaction: &mut MapTransaction,
    ) -> Result<u64, VmError> {
        let new_pt = alloc_frame().ok_or(VmError::OutOfMemory)?;
        if let Err(error) = transaction.record(pde_ptr, old_pde, new_pt) {
            free_frame(new_pt);
            return Err(error);
        }
        let base_phys = old_pde & ADDR_MASK_2M;
        let inherited_flags = old_pde & (PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_PWT | PTE_PCD | PTE_NO_EXEC);
        let pt_ptr = new_pt as *mut u64;
        for i in 0..512usize {
            let page_phys = base_phys + (i as u64) * PAGE_SIZE as u64;
            core::ptr::write_volatile(pt_ptr.add(i), page_phys | inherited_flags);
        }
        let dir_flags = PTE_PRESENT | PTE_WRITABLE | (old_pde & PTE_USER) | (req_flags & PTE_USER);
        core::ptr::write_volatile(pde_ptr, new_pt | dir_flags);
        Ok(new_pt)
    }

    unsafe fn walk_to_pte(&self, vaddr: u64) -> Result<*mut u64, VmError> {
        if !is_canonical(vaddr) {
            return Err(VmError::NonCanonicalAddress);
        }
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx   = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx   = ((vaddr >> 12) & 0x1FF) as usize;

        let pml4e = core::ptr::read_volatile((self.pml4_phys as *const u64).add(pml4_idx));
        if (pml4e & PTE_PRESENT) == 0 {
            return Err(VmError::NotMapped);
        }
        let pdpte = core::ptr::read_volatile(((pml4e & ADDR_MASK_4K) as *const u64).add(pdpt_idx));
        if (pdpte & PTE_PRESENT) == 0 {
            return Err(VmError::NotMapped);
        }
        let pde = core::ptr::read_volatile(((pdpte & ADDR_MASK_4K) as *const u64).add(pd_idx));
        if (pde & PTE_PRESENT) == 0 || (pde & PTE_HUGE) != 0 {
            return Err(VmError::NotMapped);
        }
        Ok(((pde & ADDR_MASK_4K) as *mut u64).add(pt_idx))
    }

    pub fn validate_user_range(
        &self,
        user_addr: u64,
        len: usize,
        require_writable: bool,
    ) -> Result<(), VmError> {
        if len == 0 {
            return Ok(());
        }
        if user_addr < USER_SPACE_START || user_addr > USER_SPACE_END {
            return Err(VmError::SupervisorOverlap);
        }
        let end = user_addr
            .checked_add((len - 1) as u64)
            .ok_or(VmError::AddressOverflow)?;
        if end > USER_SPACE_END {
            return Err(VmError::SupervisorOverlap);
        }

        let start_page = user_addr & ADDR_MASK_4K;
        let end_page = end & ADDR_MASK_4K;
        let mut page = start_page;
        loop {
            let (_, flags) = self.translate_with_flags(page).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            if require_writable && (flags & PTE_WRITABLE) == 0 {
                return Err(VmError::WxViolation);
            }
            if page >= end_page {
                break;
            }
            page = match page.checked_add(PAGE_SIZE as u64) {
                Some(next) => next,
                None => break,
            };
        }
        Ok(())
    }

    pub fn copy_from_user(&self, user_src: u64, kernel_dst: &mut [u8]) -> Result<(), VmError> {
        let len = kernel_dst.len();
        if len == 0 {
            return Ok(());
        }
        if len > 64 * 1024 {
            return Err(VmError::AddressOverflow);
        }
        self.validate_user_range(user_src, len, false)?;

        let mut copied = 0usize;
        while copied < len {
            let cur_vaddr = user_src + (copied as u64);
            let page_off = (cur_vaddr & 0xFFF) as usize;
            let chunk = (PAGE_SIZE - page_off).min(len - copied);
            let (paddr, flags) = self.translate_with_flags(cur_vaddr).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    paddr as *const u8,
                    kernel_dst.as_mut_ptr().add(copied),
                    chunk,
                );
            }
            copied += chunk;
        }
        Ok(())
    }

    pub fn copy_to_user(&self, user_dst: u64, kernel_src: &[u8]) -> Result<(), VmError> {
        let len = kernel_src.len();
        if len == 0 {
            return Ok(());
        }
        if len > 64 * 1024 {
            return Err(VmError::AddressOverflow);
        }
        self.validate_user_range(user_dst, len, true)?;

        let mut copied = 0usize;
        while copied < len {
            let cur_vaddr = user_dst + (copied as u64);
            let page_off = (cur_vaddr & 0xFFF) as usize;
            let chunk = (PAGE_SIZE - page_off).min(len - copied);
            let (paddr, flags) = self.translate_with_flags(cur_vaddr).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 || (flags & PTE_WRITABLE) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    kernel_src.as_ptr().add(copied),
                    paddr as *mut u8,
                    chunk,
                );
            }
            copied += chunk;
        }
        Ok(())
    }
}

pub fn run_phase1_vm_self_test() -> Result<(), &'static str> {
    let cur_cr3 = read_cr3();
    let cur = AddressSpace { pml4_phys: cur_cr3 };
    let (_, k_flags) = cur.translate_with_flags(0x0010_0000).ok_or("Kernel code mapping bulunamadi")?;
    if (k_flags & PTE_USER) != 0 {
        return Err("Guvenlik Ihlali: Cekirdek sayfasi U/S=1 (User) olarak isaretlenmis!");
    }

    let mut user_space = AddressSpace::create_user_space().map_err(|_| "User address space olusturulamadi")?;

    let test_frame = alloc_frame().ok_or("PMM test frame ayiramadi")?;
    let wx_perms = PagePermissions { readable: true, writable: true, executable: true, user: true };
    if user_space.map_4k(USER_SPACE_START, test_frame, wx_perms) != Err(VmError::WxViolation) {
        unsafe { free_frame(test_frame); }
        return Err("W^X ihlali engellenmedi!");
    }

    if user_space.map_4k(0x0020_0000, test_frame, PagePermissions::USER_RW) != Err(VmError::SupervisorOverlap) {
        unsafe { free_frame(test_frame); }
        return Err("Supervisor bolgesine User eslemesi engellenmedi!");
    }
    if user_space.map_4k(0xFFFF_8000_0000_0000, test_frame, PagePermissions::USER_RW) != Err(VmError::SupervisorOverlap) {
        unsafe { free_frame(test_frame); }
        return Err("Kernel-half User eslemesi engellenmedi!");
    }

    user_space.map_4k(USER_SPACE_START, test_frame, PagePermissions::USER_RW)
        .map_err(|_| "Gecerli 4K User eslemesi basarisiz")?;
    let (mapped_phys, u_flags) = user_space.translate_with_flags(USER_SPACE_START)
        .ok_or("Eslenen 4K User sayfasi translate edilemedi")?;
    if mapped_phys != test_frame || (u_flags & PTE_USER) == 0 {
        return Err("4K User sayfasi fiziksel adresi veya U/S bayragi hatali");
    }

    let unmapped = user_space.unmap_4k(USER_SPACE_START).map_err(|_| "4K unmap basarisiz")?;
    unsafe {
        free_frame(unmapped);
        user_space.free_owned_page_tables();
    }
    Ok(())
}

#[cfg(test)]
mod huge_page_tests {
    use super::*;

    #[repr(align(4096))]
    struct Table([u64; 512]);

    static mut PML4: Table = Table([0; 512]);
    static mut PDPT: Table = Table([0; 512]);

    /// UEFI/OVMF'nin 1 GiB yaprak sayfalarla kurduğu kimlik eşlemesi çevrilebilmeli.
    #[test]
    fn translate_handles_1gib_leaf_pages() {
        unsafe {
            let pml4 = core::ptr::addr_of_mut!(PML4.0) as *mut u64;
            let pdpt = core::ptr::addr_of_mut!(PDPT.0) as *mut u64;
            core::ptr::write(pml4, (pdpt as u64) | PTE_PRESENT | PTE_WRITABLE);
            core::ptr::write(pdpt, PTE_PRESENT | PTE_WRITABLE | PTE_HUGE); // 0..1 GiB
            core::ptr::write(pdpt.add(1), 0x4000_0000 | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE); // 1..2 GiB

            let space = AddressSpace { pml4_phys: pml4 as u64 };

            let (phys, flags) = space.translate_with_flags(0x0010_0000).expect("0x100000 çevrilmeli");
            assert_eq!(phys, 0x0010_0000);
            assert!(flags & PTE_HUGE != 0);
            assert!(flags & PTE_USER == 0, "çekirdek sayfası U/S=0 olmalı");
            assert!(flags & PTE_WRITABLE != 0);

            let (phys2, _) = space.translate_with_flags(0x4000_1234).expect("ikinci GiB çevrilmeli");
            assert_eq!(phys2, 0x4000_1234);

            // Bulunmayan GiB hâlâ None dönmeli.
            assert!(space.translate_with_flags(0x8000_0000).is_none());

            // Tanı yürüyüşü 1 GiB yaprakta durur.
            let w = space.debug_walk(0x0010_0000);
            assert_ne!(w[0], 0);
            assert_ne!(w[1], 0);
            assert_eq!(w[2], 0);
            assert_eq!(w[3], 0);
        }
    }
}
