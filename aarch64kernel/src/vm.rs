use core::arch::asm;
use nc_arch::mmu;

pub const PAGE: u64 = 4096;
pub const USER_START: u64 = nc_abi::USER_SPACE_START;
pub const USER_END: u64 = USER_START + (1 << 39);

const MAX_FRAMES: usize = 1 << 21;
const WORDS: usize = MAX_FRAMES / 64;
const ADDRESS_MASK: u64 = 0x0000_FFFF_FFFF_F000;
const TABLE: u64 = 0b11;
const OWNED: u64 = 1 << 55;

static mut BITMAP: [u64; WORDS] = [0; WORDS];
static mut BASE: u64 = 0;
static mut FRAMES: usize = 0;
static mut FREE: usize = 0;
static mut HINT: usize = 0;

pub unsafe fn pmm_init(base: u64, size: u64) {
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
}

pub fn free_frames() -> usize {
    unsafe { FREE }
}

pub fn alloc_frame() -> Option<u64> {
    unsafe {
        if FREE == 0 {
            return None;
        }
        let bitmap = &mut *core::ptr::addr_of_mut!(BITMAP);
        for step in 0..FRAMES {
            let frame = (HINT + step) % FRAMES;
            if bitmap[frame / 64] & (1 << (frame % 64)) == 0 {
                bitmap[frame / 64] |= 1 << (frame % 64);
                FREE -= 1;
                HINT = frame + 1;
                let address = BASE + frame as u64 * PAGE;
                core::ptr::write_bytes(address as *mut u8, 0, PAGE as usize);
                return Some(address);
            }
        }
        None
    }
}

pub fn free_frame(address: u64) {
    unsafe {
        if address < BASE || address & (PAGE - 1) != 0 {
            return;
        }
        let frame = ((address - BASE) / PAGE) as usize;
        if frame >= FRAMES {
            return;
        }
        let bitmap = &mut *core::ptr::addr_of_mut!(BITMAP);
        if bitmap[frame / 64] & (1 << (frame % 64)) != 0 {
            bitmap[frame / 64] &= !(1 << (frame % 64));
            FREE += 1;
        }
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
            let mut value = frame | TABLE | (1 << 10) | (1 << 2) | (3 << 8) | (1 << 11) | access | OWNED | (1 << 53);
            if !exec {
                value |= 1 << 54;
            }
            *slot = value;
            asm!("dsb ishst", options(nostack, preserves_flags));
            Some(frame)
        }
    }

    pub fn translate(&self, address: u64) -> Option<(u64, bool)> {
        let slot = self.entry(address & !(PAGE - 1), false)?;
        let value = unsafe { *slot };
        if value & 0b11 != TABLE || value & (1 << 6) == 0 {
            return None;
        }
        let writable = value & (1 << 7) == 0;
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
                core::ptr::copy_nonoverlapping(physical as *const u8, destination.as_mut_ptr().add(done), chunk);
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
                core::ptr::copy_nonoverlapping(source.as_ptr().add(done), physical as *mut u8, chunk);
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
