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

/// Runtime-detected ARMv8.x hardware security features (Features 14 & 19).
#[derive(Clone, Copy, Debug)]
pub struct CpuSecurityFeatures {
    pub current_el: u8,
    pub pan: bool,
    pub pac: bool,
    pub bti: bool,
    pub mte: bool,
    pub rndr: bool,
    pub el2_present: bool,
}

pub fn probe_security_features() -> CpuSecurityFeatures {
    let mmfr1: u64;
    let isar0: u64;
    let isar1: u64;
    let pfr0: u64;
    let pfr1: u64;
    unsafe {
        asm!("mrs {}, id_aa64mmfr1_el1", out(reg) mmfr1, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, id_aa64isar0_el1", out(reg) isar0, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, id_aa64isar1_el1", out(reg) isar1, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, id_aa64pfr0_el1", out(reg) pfr0, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, id_aa64pfr1_el1", out(reg) pfr1, options(nomem, nostack, preserves_flags));
    }
    CpuSecurityFeatures {
        current_el: current_el(),
        pan: ((mmfr1 >> 20) & 0xF) != 0,
        pac: (((isar1 >> 4) & 0xF) != 0) || (((isar1 >> 8) & 0xF) != 0),
        bti: (pfr1 & 0xF) != 0,
        mte: ((pfr1 >> 8) & 0xF) >= 2,
        rndr: ((isar0 >> 60) & 0xF) != 0,
        el2_present: ((pfr0 >> 8) & 0xF) != 0,
    }
}

pub fn enable_cpu_hardening() -> (bool, bool) {
    let feats = probe_security_features();
    if feats.pan {
        unsafe {
            asm!(".inst 0xd500419f", options(nomem, nostack));
        }
    }
    (true, feats.pan)
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
