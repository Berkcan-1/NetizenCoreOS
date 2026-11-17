use core::arch::asm;

#[inline]
pub unsafe fn mmio_write32(address: usize, value: u32) {
    core::ptr::write_volatile(address as *mut u32, value);
}

#[inline]
pub unsafe fn mmio_write16(address: usize, value: u16) {
    core::ptr::write_volatile(address as *mut u16, value);
}

#[inline]
pub unsafe fn mmio_read32(address: usize) -> u32 {
    core::ptr::read_volatile(address as *const u32)
}

#[inline]
pub unsafe fn mmio_write8(address: usize, value: u8) {
    core::ptr::write_volatile(address as *mut u8, value);
}

#[inline]
pub unsafe fn mmio_read8(address: usize) -> u8 {
    core::ptr::read_volatile(address as *const u8)
}

pub fn current_el() -> u8 {
    let value: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    ((value >> 2) & 3) as u8
}

#[inline]
pub fn wait_for_interrupt() {
    unsafe {
        asm!("wfi", options(nostack, preserves_flags));
    }
}

pub fn halt_forever() -> ! {
    loop {
        unsafe {
            asm!("msr daifset, #0xf", "wfe", options(nomem, nostack));
        }
    }
}

pub fn reboot() -> ! {
    crate::psci::system_reset();
    halt_forever()
}

fn cache_line_bytes() -> usize {
    let ctr: u64;
    unsafe {
        asm!("mrs {}, ctr_el0", out(reg) ctr, options(nomem, nostack, preserves_flags));
    }
    4usize << ((ctr >> 16) & 0xF)
}

pub fn cache_clean_range(start: usize, length: usize) {
    let line = cache_line_bytes();
    let mut address = start & !(line - 1);
    let end = start.saturating_add(length);
    while address < end {
        unsafe {
            asm!("dc cvac, {}", in(reg) address, options(nostack, preserves_flags));
        }
        address += line;
    }
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

pub fn cache_clean_invalidate_range(start: usize, length: usize) {
    let line = cache_line_bytes();
    let mut address = start & !(line - 1);
    let end = start.saturating_add(length);
    while address < end {
        unsafe {
            asm!("dc civac, {}", in(reg) address, options(nostack, preserves_flags));
        }
        address += line;
    }
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

pub fn enable_cpu_hardening() -> (bool, bool) {
    let isar1: u64;
    unsafe {
        asm!("mrs {}, id_aa64mmfr1_el1", out(reg) isar1, options(nomem, nostack, preserves_flags));
    }
    let pan_supported = ((isar1 >> 20) & 0xF) != 0;
    if pan_supported {
        unsafe {
            asm!(".inst 0xd500419f", options(nomem, nostack));
        }
    }
    (true, pan_supported)
}

#[inline]
pub fn irq_enable() {
    unsafe {
        asm!("msr daifclr, #2", options(nostack));
    }
}

#[inline]
pub fn irq_disable() {
    unsafe {
        asm!("msr daifset, #2", options(nostack));
    }
}

pub fn sync_icache_range(start: usize, length: usize) {
    let line = cache_line_bytes();
    let end = start.saturating_add(length);
    let mut address = start & !(line - 1);
    while address < end {
        unsafe {
            asm!("dc cvau, {}", in(reg) address, options(nostack, preserves_flags));
        }
        address += line;
    }
    unsafe {
        asm!("dsb ish", options(nostack, preserves_flags));
    }
    address = start & !(line - 1);
    while address < end {
        unsafe {
            asm!("ic ivau, {}", in(reg) address, options(nostack, preserves_flags));
        }
        address += line;
    }
    unsafe {
        asm!("dsb ish", "isb", options(nostack, preserves_flags));
    }
}
