use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const MIN_FREE_BLOCK: usize = 32;
const ALLOC_MAGIC: u64 = 0x4E43_4845_4150_5631;
const MAX_HEAP_REGIONS: usize = 16;
static INVALID_DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct HeapRegion {
    start: usize,
    end: usize,
}

impl HeapRegion {
    const EMPTY: Self = Self { start: 0, end: 0 };
}

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

#[repr(C)]
struct AllocHeader {
    block_start: usize,
    block_size: usize,
    requested_size: usize,
    alignment: usize,
    guard: u64,
}

struct HeapState {
    head: *mut FreeBlock,
    regions: [HeapRegion; MAX_HEAP_REGIONS],
    region_count: usize,
}

pub struct LockedHeap {
    lock: AtomicBool,
    state: UnsafeCell<HeapState>,
}

unsafe impl Sync for LockedHeap {}

impl LockedHeap {
    pub const fn empty() -> Self {
        Self {
            lock: AtomicBool::new(false),
            state: UnsafeCell::new(HeapState {
                head: null_mut(),
                regions: [HeapRegion::EMPTY; MAX_HEAP_REGIONS],
                region_count: 0,
            }),
        }
    }

    /// Initialize before enabling interrupts or starting concurrent kernel tasks.
    pub unsafe fn init(&self, base: usize, size: usize) {
        let flags = self.acquire();
        (*self.state.get()).init(base, size);
        self.release(flags);
    }

    /// Add a non-overlapping usable RAM region to the free list.
    pub unsafe fn add_region(&self, base: usize, size: usize) {
        let flags = self.acquire();
        let _ = (*self.state.get()).add_region(base, size);
        self.release(flags);
    }

    pub fn invalid_dealloc_count(&self) -> usize {
        INVALID_DEALLOC_COUNT.load(Ordering::Relaxed)
    }

    unsafe fn try_deallocate(&self, ptr: *mut u8, layout: Layout) -> bool {
        let flags = self.acquire();
        let valid = (*self.state.get()).dealloc(ptr, layout);
        if !valid { INVALID_DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed); }
        self.release(flags);
        valid
    }

    // Local interrupts are disabled before acquiring the SMP lock. This prevents an IRQ
    // on the same CPU from preempting the owner and spinning forever on this allocator.
    fn acquire(&self) -> u64 {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq", "pop {}", out(reg) flags);
            #[cfg(not(test))]
            core::arch::asm!("cli", options(nomem, nostack));
        }
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        flags
    }

    fn release(&self, flags: u64) {
        self.lock.store(false, Ordering::Release);
        #[cfg(not(test))]
        unsafe { core::arch::asm!("push {}", "popfq", in(reg) flags); }
        #[cfg(test)]
        let _ = flags;
    }
}

#[inline]
fn allocation_guard(block_start: usize, block_size: usize, requested_size: usize, alignment: usize) -> u64 {
    ALLOC_MAGIC
        ^ (block_start as u64).rotate_left(11)
        ^ (block_size as u64).rotate_left(23)
        ^ (requested_size as u64).rotate_left(37)
        ^ (alignment as u64).rotate_left(49)
}

impl HeapState {
    unsafe fn init(&mut self, base: usize, size: usize) {
        self.head = null_mut();
        self.regions = [HeapRegion::EMPTY; MAX_HEAP_REGIONS];
        self.region_count = 0;
        let _ = self.add_region(base, size);
    }

    unsafe fn add_region(&mut self, base: usize, size: usize) -> bool {
        let Some(end) = base.checked_add(size) else { return false; };
        let Some(start) = base.checked_add(15).map(|v| v & !15) else { return false; };
        let aligned_end = end & !15;
        if aligned_end <= start || aligned_end - start < MIN_FREE_BLOCK
            || self.region_count >= self.regions.len()
        { return false; }
        if self.regions[..self.region_count].iter().any(|r| start < r.end && r.start < aligned_end) {
            return false;
        }
        if !self.insert_free_region(start, aligned_end - start) { return false; }
        self.regions[self.region_count] = HeapRegion { start, end: aligned_end };
        self.region_count += 1;
        true
    }

    fn contains_range(&self, start: usize, end: usize) -> bool {
        if end <= start { return false; }
        // Adjacent registered extents may be coalesced into one free-list block.
        // Accept a range only when their union covers it without crossing a gap.
        let mut cursor = start;
        while cursor < end {
            let covered_end = self.regions[..self.region_count]
                .iter()
                .filter(|region| region.start <= cursor && region.end > cursor)
                .map(|region| region.end)
                .max();
            let Some(next) = covered_end else { return false; };
            cursor = next.min(end);
        }
        true
    }

    unsafe fn insert_free_region(&mut self, start: usize, size: usize) -> bool {
        if start & 15 != 0 || size < MIN_FREE_BLOCK { return false; }
        let Some(end) = start.checked_add(size) else { return false; };

        let mut prev: *mut FreeBlock = null_mut();
        let mut next = self.head;
        while !next.is_null() && (next as usize) < start {
            prev = next;
            next = (*next).next;
        }

        if !prev.is_null() {
            let prev_end = (prev as usize).saturating_add((*prev).size);
            if prev_end > start { return false; }
        }
        if !next.is_null() && end > next as usize { return false; }

        let mut inserted = start as *mut FreeBlock;
        (*inserted).size = size;
        (*inserted).next = next;
        if prev.is_null() { self.head = inserted; } else { (*prev).next = inserted; }

        // Coalesce with the previous block first, then with every adjacent next block.
        if !prev.is_null() && (prev as usize).checked_add((*prev).size) == Some(inserted as usize) {
            (*prev).size = (*prev).size.saturating_add((*inserted).size);
            (*prev).next = (*inserted).next;
            inserted = prev;
        }
        while !(*inserted).next.is_null() {
            let following = (*inserted).next;
            let inserted_end = (inserted as usize).checked_add((*inserted).size);
            if inserted_end == Some(following as usize) {
                (*inserted).size = (*inserted).size.saturating_add((*following).size);
                (*inserted).next = (*following).next;
            } else {
                break;
            }
        }
        true
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let alignment = layout.align().max(16);
        let payload_size = layout.size().max(1);
        let header_size = core::mem::size_of::<AllocHeader>();
        let free_header_size = core::mem::size_of::<FreeBlock>();

        let mut prev: *mut FreeBlock = null_mut();
        let mut current = self.head;
        while !current.is_null() {
            let block_start = current as usize;
            let Some(block_end) = block_start.checked_add((*current).size) else {
                prev = current;
                current = (*current).next;
                continue;
            };
            let payload_floor = match block_start.checked_add(free_header_size).and_then(|v| v.checked_add(header_size)) {
                Some(v) => v,
                None => { prev = current; current = (*current).next; continue; }
            };
            let Some(payload_addr) = payload_floor.checked_add(alignment - 1).map(|v| v & !(alignment - 1)) else {
                prev = current; current = (*current).next; continue;
            };
            let Some(payload_end) = payload_addr.checked_add(payload_size) else {
                prev = current; current = (*current).next; continue;
            };
            let Some(minimum_end) = payload_end.checked_add(15).map(|v| v & !15) else {
                prev = current; current = (*current).next; continue;
            };
            if minimum_end > block_end {
                prev = current;
                current = (*current).next;
                continue;
            }

            let remainder = block_end - minimum_end;
            let (allocated_size, remainder_start, next_link) = if remainder >= MIN_FREE_BLOCK {
                (minimum_end - block_start, minimum_end, (*current).next)
            } else {
                ((*current).size, block_end, (*current).next)
            };

            if remainder >= MIN_FREE_BLOCK {
                let free = remainder_start as *mut FreeBlock;
                (*free).size = block_end - remainder_start;
                (*free).next = next_link;
                if prev.is_null() { self.head = free; } else { (*prev).next = free; }
            } else if prev.is_null() {
                self.head = next_link;
            } else {
                (*prev).next = next_link;
            }

            let header = (payload_addr - header_size) as *mut AllocHeader;
            (*header).block_start = block_start;
            (*header).block_size = allocated_size;
            (*header).requested_size = payload_size;
            (*header).alignment = alignment;
            (*header).guard = allocation_guard(block_start, allocated_size, payload_size, alignment);
            return payload_addr as *mut u8;
        }
        null_mut()
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) -> bool {
        if ptr.is_null() { return false; }
        let header_size = core::mem::size_of::<AllocHeader>();
        let Some(header_addr) = (ptr as usize).checked_sub(header_size) else { return false; };
        let Some(header_end) = header_addr.checked_add(header_size) else { return false; };
        if !self.contains_range(header_addr, header_end) { return false; }

        let header = header_addr as *mut AllocHeader;
        let block_start = (*header).block_start;
        let block_size = (*header).block_size;
        let requested_size = (*header).requested_size;
        let alignment = (*header).alignment;
        let guard = (*header).guard;
        let Some(block_end) = block_start.checked_add(block_size) else { return false; };
        let expected_size = layout.size().max(1);
        let expected_alignment = layout.align().max(16);
        let Some(payload_end) = (ptr as usize).checked_add(expected_size) else { return false; };
        let Some(min_payload) = block_start
            .checked_add(core::mem::size_of::<FreeBlock>())
            .and_then(|v| v.checked_add(header_size))
        else { return false; };

        if block_start == 0 || block_start & 15 != 0 || block_size < MIN_FREE_BLOCK
            || !self.contains_range(block_start, block_end)
            || (ptr as usize) < min_payload || payload_end > block_end
            || requested_size != expected_size || alignment != expected_alignment
            || (ptr as usize) % expected_alignment != 0
            || guard != allocation_guard(block_start, block_size, requested_size, alignment)
        { return false; }

        (*header).guard = 0;
        self.insert_free_region(block_start, block_size)
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let flags = self.acquire();
        let ptr = (*self.state.get()).alloc(layout);
        self.release(flags);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = self.try_deallocate(ptr, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::LockedHeap;
    use core::alloc::{GlobalAlloc, Layout};

    #[repr(align(65536))]
    struct HeapMemory([u8; 1024 * 1024]);

    static mut MEMORY_ALIGNMENT: HeapMemory = HeapMemory([0; 1024 * 1024]);
    static mut MEMORY_DOUBLE_FREE: HeapMemory = HeapMemory([0; 1024 * 1024]);

    #[test]
    fn invalid_and_double_deallocation_are_counted_without_reinserting() {
        let heap = LockedHeap::empty();
        unsafe {
            let base = core::ptr::addr_of_mut!(MEMORY_DOUBLE_FREE.0) as *mut u8 as usize;
            heap.init(base, 1024 * 1024);

            let layout = Layout::from_size_align(96, 32).unwrap();
            let ptr = GlobalAlloc::alloc(&heap, layout);
            assert!(!ptr.is_null());
            assert!(heap.try_deallocate(ptr, layout));

            let before = heap.invalid_dealloc_count();
            assert!(!heap.try_deallocate(ptr, layout));
            assert_eq!(heap.invalid_dealloc_count(), before + 1);
        }
    }

    #[test]
    fn allocation_respects_layout_alignment_and_reuses_blocks() {
        let heap = LockedHeap::empty();
        unsafe {
            let base = core::ptr::addr_of_mut!(MEMORY_ALIGNMENT.0) as *mut u8 as usize;
            heap.init(base, 1024 * 1024);

            let alignments = [1usize, 8, 16, 32, 64, 256, 4096, 65_536];
            for alignment in alignments {
                let layout = Layout::from_size_align(173, alignment).unwrap();
                let ptr = GlobalAlloc::alloc(&heap, layout);
                assert!(!ptr.is_null());
                assert_eq!((ptr as usize) % alignment, 0, "alignment {alignment}");
                core::ptr::write_bytes(ptr, 0xA5, layout.size());
                GlobalAlloc::dealloc(&heap, ptr, layout);
            }

            // Repeated mixed-size operations exercise splitting and adjacent coalescing.
            let small = Layout::from_size_align(64, 16).unwrap();
            let large = Layout::from_size_align(4096, 4096).unwrap();
            for _ in 0..64 {
                let a = GlobalAlloc::alloc(&heap, small);
                let b = GlobalAlloc::alloc(&heap, large);
                assert!(!a.is_null() && !b.is_null());
                assert_eq!((b as usize) % 4096, 0);
                GlobalAlloc::dealloc(&heap, b, large);
                GlobalAlloc::dealloc(&heap, a, small);
            }
        }
    }
}
