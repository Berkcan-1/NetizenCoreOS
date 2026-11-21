// crates/services/nc-mem/src/lib.rs
#![no_std]

pub mod futex;
pub mod heap;
pub mod irq;
pub mod shm;
pub mod zram;
#[cfg(target_arch = "x86_64")]
pub mod kpt;
#[cfg(target_arch = "x86_64")]
pub mod paging;

pub use heap::LockedHeap;
#[cfg(target_arch = "x86_64")]
pub use paging::{
    alloc_frame, frame_ref_count, free_frame, free_frame_checked, free_frame_count,
    init_pmm, invalid_free_count, managed_frame_count, retain_frame,
    run_phase1_vm_self_test, AddressSpace, PagePermissions, PhysicalRegion, VmError,
    MAX_PHYSICAL_ADDRESS, PAGE_SIZE, PTE_COW, PTE_USER, PTE_WRITABLE,
    USER_SPACE_END, USER_SPACE_START,
};

pub struct Arena {
    next: usize,
    end: usize,
}

impl Arena {
    pub unsafe fn new(base: usize, size: usize) -> Arena {
        Arena { next: base, end: base + size }
    }

    pub fn mark(&self) -> usize {
        self.next
    }

    /// # Safety
    /// `mark`, bu arena'dan alinmis olmali ve ondan sonra ayrilan hicbir dilim artik kullanilmamali.
    pub unsafe fn release_to(&mut self, mark: usize) {
        if mark <= self.next {
            self.next = mark;
        }
    }

    pub fn remaining(&self) -> usize {
        self.end - self.next
    }

    pub fn alloc_pixels(&mut self, count: usize) -> Option<&'static mut [u32]> {
        let bytes = count.checked_mul(4)?;
        let start = (self.next + 15) & !15;
        let end = start.checked_add(bytes)?;
        if end > self.end {
            return None;
        }
        self.next = end;
        Some(unsafe { core::slice::from_raw_parts_mut(start as *mut u32, count) })
    }

    pub fn alloc_bytes(&mut self, count: usize) -> Option<&'static mut [u8]> {
        let start = (self.next + 15) & !15;
        let end = start.checked_add(count)?;
        if end > self.end {
            return None;
        }
        self.next = end;
        Some(unsafe { core::slice::from_raw_parts_mut(start as *mut u8, count) })
    }
}
