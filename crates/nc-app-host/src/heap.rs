//! Ring 3 için basit, tek iş parçacıklı ilk-uyan (first-fit) boş-liste ayırıcısı.
//! (Çekirdeğin `LockedHeap`'i `cli` kullandığı için Ring 3'te çalışamaz.)
//!
//! Değişmezler: tüm adresler ve boyutlar 16'nın katıdır; bu yüzden bir deliğin öncesinde/sonrasında kalan
//! artık boşluk ya 0 ya da >= 16 bayttır (bir `Hole` başlığı sığar) ve hiçbir bayt sızmaz.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::null_mut;

const ALIGN: usize = 16;

#[repr(C)]
struct Hole {
    size: usize,
    next: *mut Hole,
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

pub struct Heap {
    head: Hole,
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap { head: Hole { size: 0, next: null_mut() } }
    }

    /// # Safety
    /// `[base, base + size)` geçerli, ayırıcıya ait ve başka kimse tarafından kullanılmayan bellek olmalı.
    pub unsafe fn init(&mut self, base: usize, size: usize) {
        let start = align_up(base, ALIGN);
        let end = (base + size) & !(ALIGN - 1);
        if end <= start + ALIGN {
            return;
        }
        let hole = start as *mut Hole;
        (*hole).size = end - start;
        (*hole).next = null_mut();
        self.head.next = hole;
    }

    fn rounded(layout: &Layout) -> usize {
        align_up(layout.size().max(1), ALIGN)
    }

    /// # Safety
    /// `init` çağrılmış olmalı.
    pub unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let size = Self::rounded(&layout);
        let align = layout.align().max(ALIGN);
        let mut prev: *mut Hole = &mut self.head;
        let mut current = (*prev).next;
        while !current.is_null() {
            let hole_start = current as usize;
            let hole_end = hole_start + (*current).size;
            let start = align_up(hole_start, align);
            if let Some(end) = start.checked_add(size) {
                if end <= hole_end {
                    let front = start - hole_start;
                    let back = hole_end - end;
                    let next = (*current).next;
                    let mut link = next;
                    if back > 0 {
                        let tail = end as *mut Hole;
                        (*tail).size = back;
                        (*tail).next = next;
                        link = tail;
                    }
                    if front > 0 {
                        (*current).size = front;
                        (*current).next = link;
                    } else {
                        (*prev).next = link;
                    }
                    return start as *mut u8;
                }
            }
            prev = current;
            current = (*current).next;
        }
        null_mut()
    }

    /// # Safety
    /// `ptr`, aynı `layout` ile bu ayırıcıdan alınmış ve henüz geri verilmemiş olmalı.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let size = Self::rounded(&layout);
        let addr = ptr as usize;
        let head: *mut Hole = &mut self.head;
        let mut prev = head;
        let mut current = (*prev).next;
        while !current.is_null() && (current as usize) < addr {
            prev = current;
            current = (*current).next;
        }
        let hole = addr as *mut Hole;
        (*hole).size = size;
        (*hole).next = current;
        (*prev).next = hole;
        if !current.is_null() && addr + size == current as usize {
            (*hole).size += (*current).size;
            (*hole).next = (*current).next;
        }
        if prev != head && (prev as usize) + (*prev).size == addr {
            (*prev).size += (*hole).size;
            (*prev).next = (*hole).next;
        }
    }

    /// Test/teşhis: (delik sayısı, toplam boş bayt).
    pub fn stats(&self) -> (usize, usize) {
        let mut count = 0;
        let mut bytes = 0;
        let mut current = self.head.next;
        while !current.is_null() {
            unsafe {
                count += 1;
                bytes += (*current).size;
                current = (*current).next;
            }
        }
        (count, bytes)
    }
}

/// `#[global_allocator]` için sarmalayıcı. Uygulama ikilisinde tanımlanır (bu crate'te DEĞİL: ana makine
/// testleri kendi ayırıcısını kullanmalı).
pub struct GlobalHeap(UnsafeCell<Heap>);

unsafe impl Sync for GlobalHeap {}

impl GlobalHeap {
    pub const fn empty() -> GlobalHeap {
        GlobalHeap(UnsafeCell::new(Heap::empty()))
    }

    /// # Safety
    /// Bkz. `Heap::init`. Yalnızca bir kez çağrılmalı.
    pub unsafe fn init(&self, base: usize, size: usize) {
        (*self.0.get()).init(base, size);
    }
}

unsafe impl GlobalAlloc for GlobalHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (*self.0.get()).allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        (*self.0.get()).deallocate(ptr, layout)
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn arena(words: usize) -> Vec<u128> {
        vec![0u128; words]
    }

    #[test]
    fn single_alloc_free_restores_whole_heap() {
        let mut backing = arena(1024);
        let base = backing.as_mut_ptr() as usize;
        let total = 1024 * 16;
        let mut heap = Heap::empty();
        unsafe {
            heap.init(base, total);
            assert_eq!(heap.stats(), (1, total));
            let p = heap.allocate(Layout::from_size_align(100, 8).unwrap());
            assert!(!p.is_null() && (p as usize) % 16 == 0);
            assert_eq!(heap.stats().1, total - 112);
            heap.deallocate(p, Layout::from_size_align(100, 8).unwrap());
            assert_eq!(heap.stats(), (1, total), "tek delikte geri birlesmeli");
        }
    }

    #[test]
    fn alignment_larger_than_16_is_honored_without_leaking() {
        let mut backing = arena(4096);
        let base = backing.as_mut_ptr() as usize;
        let total = 4096 * 16;
        let mut heap = Heap::empty();
        unsafe {
            heap.init(base, total);
            let a = heap.allocate(Layout::from_size_align(24, 16).unwrap());
            let b = heap.allocate(Layout::from_size_align(4096, 4096).unwrap());
            assert_eq!((b as usize) % 4096, 0);
            heap.deallocate(a, Layout::from_size_align(24, 16).unwrap());
            heap.deallocate(b, Layout::from_size_align(4096, 4096).unwrap());
            assert_eq!(heap.stats(), (1, total));
        }
    }

    #[test]
    fn exhaustion_returns_null_not_garbage() {
        let mut backing = arena(16);
        let base = backing.as_mut_ptr() as usize;
        let mut heap = Heap::empty();
        unsafe {
            heap.init(base, 16 * 16);
            assert!(heap.allocate(Layout::from_size_align(16 * 16 + 1, 1).unwrap()).is_null());
            assert!(heap.allocate(Layout::from_size_align(isize::MAX as usize - 64, 1).unwrap()).is_null());
            let all = heap.allocate(Layout::from_size_align(16 * 16, 1).unwrap());
            assert!(!all.is_null());
            assert!(heap.allocate(Layout::from_size_align(1, 1).unwrap()).is_null());
        }
    }

    #[test]
    fn random_workload_never_overlaps_and_fully_recovers() {
        let words = 8192;
        let mut backing = arena(words);
        let base = backing.as_mut_ptr() as usize;
        let total = words * 16;
        let mut heap = Heap::empty();
        let mut live: Vec<(usize, Layout, u8)> = Vec::new();
        let mut state: u64 = 0xA5A5_1234_DEAD_BEEF;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        unsafe {
            heap.init(base, total);
            for step in 0..60_000u32 {
                if live.len() < 400 && (next() % 5 != 0 || live.is_empty()) {
                    let size = 1 + (next() as usize) % 700;
                    let align = 1usize << (next() % 8); // 1..128
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let p = heap.allocate(layout);
                    if p.is_null() {
                        continue;
                    }
                    assert_eq!((p as usize) % align.max(16), 0);
                    assert!((p as usize) >= base && (p as usize) + size <= base + total);
                    let tag = (step & 0xFF) as u8;
                    core::ptr::write_bytes(p, tag, size);
                    live.push((p as usize, layout, tag));
                } else if !live.is_empty() {
                    let index = (next() as usize) % live.len();
                    let (addr, layout, tag) = live.swap_remove(index);
                    let slice = core::slice::from_raw_parts(addr as *const u8, layout.size());
                    assert!(slice.iter().all(|&b| b == tag), "baska bir tahsis bu blogu ezdi");
                    heap.deallocate(addr as *mut u8, layout);
                }
                if step % 5000 == 0 {
                    let mut ranges: Vec<(usize, usize)> = live.iter().map(|(a, l, _)| (*a, *a + Heap::rounded(l))).collect();
                    ranges.sort();
                    for pair in ranges.windows(2) {
                        assert!(pair[0].1 <= pair[1].0, "tahsisler cakisiyor");
                    }
                }
            }
            for (addr, layout, tag) in live.drain(..) {
                let slice = core::slice::from_raw_parts(addr as *const u8, layout.size());
                assert!(slice.iter().all(|&b| b == tag));
                heap.deallocate(addr as *mut u8, layout);
            }
            assert_eq!(heap.stats(), (1, total), "her sey geri verilince tek bos delik kalmali");
        }
    }
}
