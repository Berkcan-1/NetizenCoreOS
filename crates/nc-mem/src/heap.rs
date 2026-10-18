use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, Ordering};

struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

struct HeapState {
    head: *mut FreeBlock,
}

pub struct LockedHeap {
    lock: AtomicBool,
    state: UnsafeCell<HeapState>,
}

unsafe impl Sync for LockedHeap {}

impl LockedHeap {
    pub const fn empty() -> Self {
        LockedHeap {
            lock: AtomicBool::new(false),
            state: UnsafeCell::new(HeapState { head: null_mut() }),
        }
    }

    pub unsafe fn init(&self, base: usize, size: usize) {
        let state = &mut *self.state.get();
        state.init(base, size);
    }

    pub unsafe fn add_region(&self, base: usize, size: usize) {
        let aligned_base = (base + 15) & !15;
        let diff = aligned_base.saturating_sub(base);
        let actual_size = size.saturating_sub(diff) & !15;
        if actual_size < 64 {
            return;
        }
        self.acquire();
        let state = &mut *self.state.get();
        if state.head.is_null() {
            state.init(aligned_base, actual_size);
        } else {
            // Sahte bir tahsisat basligi olusturup dealloc ile sirali listeye birlestir (coalesce)
            *(aligned_base as *mut usize) = actual_size;
            *((aligned_base + 8) as *mut usize) = 0;
            state.dealloc((aligned_base + 16) as *mut u8);
        }
        self.release();
    }

    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

impl HeapState {
    unsafe fn init(&mut self, base: usize, size: usize) {
        let aligned_base = (base + 15) & !15;
        let diff = aligned_base.saturating_sub(base);
        let actual_size = size.saturating_sub(diff) & !15;
        if actual_size < 32 {
            self.head = null_mut();
            return;
        }
        let block = aligned_base as *mut FreeBlock;
        (*block).size = actual_size;
        (*block).next = null_mut();
        self.head = block;
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(16);
        let needed_payload = (layout.size().max(1) + (align - 1)) & !(align - 1);
        let needed_total = needed_payload + 16;

        let mut prev: *mut FreeBlock = null_mut();
        let mut curr = self.head;

        while !curr.is_null() {
            let block_size = (*curr).size;
            if block_size >= needed_total {
                let actual_size = if block_size >= needed_total + 32 {
                    let next_block = (curr as usize + needed_total) as *mut FreeBlock;
                    (*next_block).size = block_size - needed_total;
                    (*next_block).next = (*curr).next;

                    if prev.is_null() {
                        self.head = next_block;
                    } else {
                        (*prev).next = next_block;
                    }
                    needed_total
                } else {
                    if prev.is_null() {
                        self.head = (*curr).next;
                    } else {
                        (*prev).next = (*curr).next;
                    }
                    block_size
                };

                *(curr as *mut usize) = actual_size;
                *((curr as usize + 8) as *mut usize) = 0;
                return (curr as usize + 16) as *mut u8;
            }
            prev = curr;
            curr = (*curr).next;
        }
        null_mut()
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        let block = (ptr as usize - 16) as *mut FreeBlock;
        let block_size = *(block as *mut usize);
        if block_size < 32 {
            return;
        }
        (*block).size = block_size;
        (*block).next = null_mut();

        if self.head.is_null() {
            self.head = block;
            return;
        }

        if (block as usize) < (self.head as usize) {
            if (block as usize) + block_size == (self.head as usize) {
                (*block).size += (*self.head).size;
                (*block).next = (*self.head).next;
                self.head = block;
            } else {
                (*block).next = self.head;
                self.head = block;
            }
            return;
        }

        let mut curr = self.head;
        while !(*curr).next.is_null() && ((*curr).next as usize) < (block as usize) {
            curr = (*curr).next;
        }

        if curr == block || (*curr).next == block {
            return;
        }

        (*block).next = (*curr).next;
        (*curr).next = block;

        let mut scan = self.head;
        while !scan.is_null() && !(*scan).next.is_null() {
            let next = (*scan).next;
            if scan as usize + (*scan).size == next as usize {
                (*scan).size += (*next).size;
                (*scan).next = (*next).next;
            } else {
                scan = (*scan).next;
            }
        }
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.acquire();
        let ptr = (*self.state.get()).alloc(layout);
        self.release();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        self.acquire();
        (*self.state.get()).dealloc(ptr);
        self.release();
    }
}
