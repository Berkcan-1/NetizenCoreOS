//! ARMv8-A Virtual Memory, SMP-Safe PMM with Refcounting, Demand Paging, COW, and ASLR
//! (Features 1, 3, 4).

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};
use nc_arch::mmu;

pub const PAGE: u64 = 4096;
pub const USER_START: u64 = nc_abi::USER_SPACE_START;
pub const USER_END: u64 = USER_START + (1 << 39);

const MAX_FRAMES: usize = 1 << 21;
const MAX_REF_FRAMES: usize = 262_144; // Up to 1 GiB of refcounted frames
const WORDS: usize = MAX_FRAMES / 64;
const ADDRESS_MASK: u64 = 0x0000_FFFF_FFFF_F000;
const TABLE: u64 = 0b11;
const AP_RO: u64 = 1 << 7;
const UXN: u64 = 1 << 54;
const OWNED: u64 = 1 << 55;
const COW: u64 = 1 << 56;

static PMM_LOCK: AtomicBool = AtomicBool::new(false);
static mut BITMAP: [u64; WORDS] = [0; WORDS];
static mut REFCOUNTS: [u8; MAX_REF_FRAMES] = [0; MAX_REF_FRAMES];
static mut BASE: u64 = 0;
static mut FRAMES: usize = 0;
static mut FREE: usize = 0;
static mut HINT: usize = 0;

#[inline]
fn pmm_acquire() -> u64 {
    let flags = nc_mem::irq::save_and_disable();
    while PMM_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    flags
}

#[inline]
fn pmm_release(flags: u64) {
    PMM_LOCK.store(false, Ordering::Release);
    nc_mem::irq::restore(flags);
}

pub unsafe fn pmm_init(base: u64, size: u64) {
    let flags = pmm_acquire();
    let frames = ((size / PAGE) as usize).min(MAX_FRAMES);
    BASE = base;
    FRAMES = frames;
    FREE = frames;
    HINT = 0;
    let bitmap = &mut *core::ptr::addr_of_mut!(BITMAP);
    for word in bitmap.iter_mut() {
        *word = 0;
    }
    for frame in frames..WORDS * 64 {
        bitmap[frame / 64] |= 1 << (frame % 64);
    }
    let refs = &mut *core::ptr::addr_of_mut!(REFCOUNTS);
    for r in refs.iter_mut() {
        *r = 0;
    }
    pmm_release(flags);
}

pub fn free_frames() -> usize {
    let flags = pmm_acquire();
    let f = unsafe { FREE };
    pmm_release(flags);
    f
}

pub fn alloc_frame() -> Option<u64> {
    let flags = pmm_acquire();
    let result = unsafe {
        if FREE == 0 {
            None
        } else {
            let bitmap = &mut *core::ptr::addr_of_mut!(BITMAP);
            let refs = &mut *core::ptr::addr_of_mut!(REFCOUNTS);
            let mut allocated = None;
            for step in 0..FRAMES {
                let frame = (HINT + step) % FRAMES;
                if bitmap[frame / 64] & (1 << (frame % 64)) == 0 {
                    bitmap[frame / 64] |= 1 << (frame % 64);
                    if frame < MAX_REF_FRAMES {
                        refs[frame] = 1;
                    }
                    FREE -= 1;
                    HINT = frame + 1;
                    let address = BASE + frame as u64 * PAGE;
                    allocated = Some(address);
                    break;
                }
            }
            allocated
        }
    };
    pmm_release(flags);
    if let Some(address) = result {
        unsafe {
            core::ptr::write_bytes(address as *mut u8, 0, PAGE as usize);
        }
    }
    result
}

#[allow(dead_code)]
pub fn retain_frame(address: u64) -> bool {
    let flags = pmm_acquire();
    let ok = unsafe {
        if address < BASE || address & (PAGE - 1) != 0 {
            false
        } else {
            let frame = ((address - BASE) / PAGE) as usize;
            if frame >= FRAMES || frame >= MAX_REF_FRAMES {
                false
            } else {
                let refs = &mut *core::ptr::addr_of_mut!(REFCOUNTS);
                if refs[frame] == 0 || refs[frame] == u8::MAX {
                    false
                } else {
                    refs[frame] += 1;
                    true
                }
            }
        }
    };
    pmm_release(flags);
    ok
}

pub fn frame_refcount(address: u64) -> u8 {
    let flags = pmm_acquire();
    let rc = unsafe {
        if address < BASE || address & (PAGE - 1) != 0 {
            0
        } else {
            let frame = ((address - BASE) / PAGE) as usize;
            if frame < FRAMES && frame < MAX_REF_FRAMES {
                (*core::ptr::addr_of!(REFCOUNTS))[frame]
            } else {
                1
            }
        }
    };
    pmm_release(flags);
    rc
}

pub fn free_frame(address: u64) {
    let flags = pmm_acquire();
    unsafe {
        if address >= BASE && address & (PAGE - 1) == 0 {
            let frame = ((address - BASE) / PAGE) as usize;
            if frame < FRAMES {
                let refs = &mut *core::ptr::addr_of_mut!(REFCOUNTS);
                if frame < MAX_REF_FRAMES && refs[frame] > 1 {
                    refs[frame] -= 1;
                    pmm_release(flags);
                    return;
                }
                if frame < MAX_REF_FRAMES {
                    refs[frame] = 0;
                }
                let bitmap = &mut *core::ptr::addr_of_mut!(BITMAP);
                if bitmap[frame / 64] & (1 << (frame % 64)) != 0 {
                    bitmap[frame / 64] &= !(1 << (frame % 64));
                    FREE += 1;
                }
            }
        }
    }
    pmm_release(flags);
}

/// Compute a hardware-CSPRNG-backed ASLR page slide in `0..max_pages`.
/// Returns `(0, false)` (degraded) when no hardware TRNG (`RNDR`) is available;
/// never uses predictable timestamps as ASLR entropy.
#[allow(dead_code)]
pub fn compute_aslr_slide_pages(max_pages: u64) -> (u64, bool) {
    if max_pages == 0 {
        return (0, false);
    }
    match nc_arch::rand::HardwareRng::try_secure_u64() {
        Some(r) => (r % max_pages, true),
        None => (0, false),
    }
}

pub struct AddressSpace {
    root: u64,
}

impl AddressSpace {
    pub fn new() -> Option<AddressSpace> {
        let root = alloc_frame()?;
        unsafe {
            *(root as *mut u64) = mmu::kernel_l1_phys() | TABLE;
        }
        Some(AddressSpace { root })
    }

    pub fn root(&self) -> u64 {
        self.root
    }

    fn entry(&self, address: u64, create: bool) -> Option<*mut u64> {
        if address < USER_START || address >= USER_END {
            return None;
        }
        unsafe {
            let mut table = self.root;
            let shifts = [39u32, 30, 21];
            for shift in shifts {
                let slot = (table as *mut u64).add(((address >> shift) & 511) as usize);
                let mut value = *slot;
                if value & 0b11 != TABLE {
                    if !create {
                        return None;
                    }
                    let frame = alloc_frame()?;
                    value = frame | TABLE;
                    *slot = value;
                }
                table = value & ADDRESS_MASK;
            }
            Some((table as *mut u64).add(((address >> 12) & 511) as usize))
        }
    }

    pub fn map_new(&mut self, address: u64, write: bool, exec: bool) -> Option<u64> {
        if write && exec {
            return None;
        }
        let slot = self.entry(address, true)?;
        unsafe {
            if *slot & 0b11 == TABLE {
                return None;
            }
            let frame = alloc_frame()?;
            let access: u64 = if write { 0b01 << 6 } else { 0b11 << 6 };
            let mut value =
                frame | TABLE | (1 << 10) | (1 << 2) | (3 << 8) | (1 << 11) | access | OWNED | (1 << 53);
            if !exec {
                value |= UXN;
            }
            *slot = value;
            asm!("dsb ishst", options(nostack, preserves_flags));
            Some(frame)
        }
    }

    /// Handle a user-space data/translation fault at `far`.
    /// Supports:
    /// 1. Copy-on-Write (`COW` bit set, non-executable page -> W^X preserved).
    /// 2. Demand-zero lazy allocation within valid user stack or user heap bounds
    ///    (never inside stack guard page or unmapped/kernel regions).
    pub fn handle_page_fault(&mut self, far: u64, is_write: bool) -> bool {
        let va = far & !(PAGE - 1);
        if va < USER_START || va >= USER_END {
            return false;
        }

        // Case 1: Copy-on-Write write fault on an existing COW-marked non-exec page.
        if is_write {
            if let Some(slot) = self.entry(va, false) {
                let pte = unsafe { *slot };
                if (pte & 0b11) == TABLE && (pte & COW) != 0 && (pte & UXN) != 0 {
                    let old_pa = pte & ADDRESS_MASK;
                    let rc = frame_refcount(old_pa);
                    if rc <= 1 {
                        // Sole owner: upgrade in place without copying.
                        unsafe {
                            *slot = pte & !(AP_RO | COW);
                        }
                        mmu::tlb_invalidate_va_is(va);
                        return true;
                    }
                    let Some(new_pa) = alloc_frame() else {
                        return false;
                    };
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            old_pa as *const u8,
                            new_pa as *mut u8,
                            PAGE as usize,
                        );
                        let new_pte = (pte & !ADDRESS_MASK & !(AP_RO | COW)) | new_pa;
                        *slot = new_pte;
                    }
                    free_frame(old_pa);
                    mmu::tlb_invalidate_va_is(va);
                    return true;
                }
                // Existing non-COW mapping fault (e.g. write to RX code page) -> reject!
                if (pte & 0b11) == TABLE {
                    return false;
                }
            }
        }

        // Case 2: Demand-zero lazy mapping for valid user stack or heap regions.
        let in_stack = va >= nc_abi::USER_APP_STACK_BOTTOM && va < nc_abi::USER_APP_STACK_TOP;
        let in_heap = va >= nc_abi::USER_HEAP_BASE
            && va < nc_abi::USER_HEAP_BASE + nc_abi::APP_HEAP_BYTES;
        if in_stack || in_heap {
            if self.map_new(va, true, false).is_some() {
                mmu::tlb_invalidate_va_is(va);
                return true;
            }
        }
        false
    }

    pub fn translate(&self, address: u64) -> Option<(u64, bool)> {
        let slot = self.entry(address & !(PAGE - 1), false)?;
        let value = unsafe { *slot };
        if value & 0b11 != TABLE || value & (1 << 6) == 0 {
            return None;
        }
        let writable = (value & AP_RO == 0) && (value & COW == 0);
        Some(((value & ADDRESS_MASK) | (address & (PAGE - 1)), writable))
    }

    pub fn copy_from_user(&self, address: u64, destination: &mut [u8]) -> bool {
        let mut done = 0usize;
        while done < destination.len() {
            let current = match address.checked_add(done as u64) {
                Some(value) => value,
                None => return false,
            };
            let (physical, _) = match self.translate(current) {
                Some(value) => value,
                None => return false,
            };
            let chunk = (destination.len() - done).min((PAGE - (current & (PAGE - 1))) as usize);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    physical as *const u8,
                    destination.as_mut_ptr().add(done),
                    chunk,
                );
            }
            done += chunk;
        }
        true
    }

    #[allow(dead_code)]
    pub fn copy_to_user(&self, address: u64, source: &[u8]) -> bool {
        let mut done = 0usize;
        while done < source.len() {
            let current = match address.checked_add(done as u64) {
                Some(value) => value,
                None => return false,
            };
            let (physical, writable) = match self.translate(current) {
                Some(value) => value,
                None => return false,
            };
            if !writable {
                return false;
            }
            let chunk = (source.len() - done).min((PAGE - (current & (PAGE - 1))) as usize);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    source.as_ptr().add(done),
                    physical as *mut u8,
                    chunk,
                );
            }
            done += chunk;
        }
        true
    }

    pub fn destroy(self) {
        unsafe {
            let level0 = self.root as *mut u64;
            let user = *level0.add(mmu::USER_L0_INDEX);
            if user & 0b11 == TABLE {
                let level1 = (user & ADDRESS_MASK) as *mut u64;
                for i in 0..512 {
                    let entry1 = *level1.add(i);
                    if entry1 & 0b11 != TABLE {
                        continue;
                    }
                    let level2 = (entry1 & ADDRESS_MASK) as *mut u64;
                    for j in 0..512 {
                        let entry2 = *level2.add(j);
                        if entry2 & 0b11 != TABLE {
                            continue;
                        }
                        let level3 = (entry2 & ADDRESS_MASK) as *mut u64;
                        for k in 0..512 {
                            let entry3 = *level3.add(k);
                            if entry3 & 0b11 == TABLE && entry3 & OWNED != 0 {
                                free_frame(entry3 & ADDRESS_MASK);
                            }
                        }
                        free_frame(entry2 & ADDRESS_MASK);
                    }
                    free_frame(entry1 & ADDRESS_MASK);
                }
                free_frame(user & ADDRESS_MASK);
            }
            free_frame(self.root);
        }
    }
}
