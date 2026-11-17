#[cfg(all(target_arch = "x86_64", not(test)))]
#[inline]
pub fn save_and_disable() -> u64 {
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags);
        core::arch::asm!("cli", options(nomem, nostack));
    }
    flags
}

#[cfg(all(target_arch = "x86_64", not(test)))]
#[inline]
pub fn restore(flags: u64) {
    unsafe {
        core::arch::asm!("push {}", "popfq", in(reg) flags);
    }
}

#[cfg(all(target_arch = "aarch64", not(test)))]
#[inline]
pub fn save_and_disable() -> u64 {
    let flags: u64;
    unsafe {
        core::arch::asm!("mrs {}, daif", out(reg) flags, options(nomem, nostack, preserves_flags));
        core::arch::asm!("msr daifset, #2", options(nomem, nostack));
    }
    flags
}

#[cfg(all(target_arch = "aarch64", not(test)))]
#[inline]
pub fn restore(flags: u64) {
    unsafe {
        core::arch::asm!("msr daif, {}", in(reg) flags, options(nomem, nostack));
    }
}

#[cfg(test)]
#[inline]
pub fn save_and_disable() -> u64 {
    0
}

#[cfg(test)]
#[inline]
pub fn restore(_flags: u64) {}

#[cfg(all(not(test), not(any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[inline]
pub fn save_and_disable() -> u64 {
    0
}

#[cfg(all(not(test), not(any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[inline]
pub fn restore(_flags: u64) {}
