// crates/nc-mem/src/lib.rs
#![no_std]

pub mod heap;
pub mod paging;

pub use heap::LockedHeap;
pub use paging::{
    alloc_frame, free_frame, free_frame_count, init_pmm, run_phase1_vm_self_test, AddressSpace,
    PagePermissions, VmError, PAGE_SIZE, PTE_USER, PTE_WRITABLE, USER_SPACE_END, USER_SPACE_START,
};

pub struct Arena {
    next: usize,
    end: usize,
}

impl Arena {
    pub unsafe fn new(base: usize, size: usize) -> Arena {
        Arena { next: base, end: base + size }
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
