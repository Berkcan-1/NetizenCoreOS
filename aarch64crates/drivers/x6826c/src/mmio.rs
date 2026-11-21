//! Low-level MMIO, ARMv8 barriers, generic-timer deadlines, IRQ-safe spinlocks,
//! IEEE CRC32, and fixed-capacity formatting utilities for the MT6768 / X6826C platform.

use core::fmt::{self, Write};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

/// MT6768 ARMv8 generic timer clock frequency from `stock/dtb` (`/timer`: `0xc65d40`).
pub const DEFAULT_TIMER_HZ: u64 = 13_000_000;

#[inline]
pub fn rd8(addr: usize) -> u8 {
    unsafe { read_volatile(addr as *const u8) }
}

#[inline]
pub fn wr8(addr: usize, value: u8) {
    unsafe { write_volatile(addr as *mut u8, value) }
}

#[inline]
pub fn rd16(addr: usize) -> u16 {
    unsafe { read_volatile(addr as *const u16) }
}

#[inline]
pub fn wr16(addr: usize, value: u16) {
    unsafe { write_volatile(addr as *mut u16, value) }
}

#[inline]
pub fn rd32(addr: usize) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

#[inline]
pub fn wr32(addr: usize, value: u32) {
    unsafe { write_volatile(addr as *mut u32, value) }
}

#[inline]
pub fn dsb() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("dsb sy", options(nomem, nostack));
    }
    #[cfg(not(target_arch = "aarch64"))]
    core::sync::atomic::fence(Ordering::SeqCst);
}

#[inline]
pub fn dmb() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("dmb sy", options(nomem, nostack));
    }
    #[cfg(not(target_arch = "aarch64"))]
    core::sync::atomic::fence(Ordering::SeqCst);
}

#[inline]
pub fn isb() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

#[inline]
pub fn ticks() -> u64 {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let value: u64;
        core::arch::asm!(
            "isb",
            "mrs {}, cntpct_el0",
            out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
    #[cfg(not(target_arch = "aarch64"))]
    0
}

#[inline]
pub fn freq() -> u64 {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let value: u64;
        core::arch::asm!(
            "mrs {}, cntfrq_el0",
            out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
        if value != 0 {
            value
        } else {
            DEFAULT_TIMER_HZ
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    DEFAULT_TIMER_HZ
}

/// Bounded deadline driven by `cntpct_el0` with a hard spin cap so polling loops
/// can never hang indefinitely even if the counter is stopped or unavailable.
#[derive(Clone, Copy)]
pub struct Deadline {
    #[cfg(target_arch = "aarch64")]
    start: u64,
    #[cfg(target_arch = "aarch64")]
    span_ticks: u64,
    spins: u32,
    max_spins: u32,
}

impl Deadline {
    pub fn from_us(us: u64) -> Self {
        #[cfg(target_arch = "aarch64")]
        let (start, span_ticks) = {
            let hz = freq();
            let span = (us.saturating_mul(hz) / 1_000_000).max(1);
            (ticks(), span)
        };
        let max_spins = (us.saturating_mul(600)).clamp(1_000, 50_000_000) as u32;
        Self {
            #[cfg(target_arch = "aarch64")]
            start,
            #[cfg(target_arch = "aarch64")]
            span_ticks,
            spins: 0,
            max_spins,
        }
    }

    pub fn from_ms(ms: u64) -> Self {
        Self::from_us(ms.saturating_mul(1_000))
    }

    /// Advances the spin counter and returns `true` once the deadline has elapsed.
    #[inline]
    pub fn expired(&mut self) -> bool {
        self.spins = self.spins.saturating_add(1);
        if self.spins >= self.max_spins {
            return true;
        }
        #[cfg(target_arch = "aarch64")]
        {
            let elapsed = ticks().wrapping_sub(self.start);
            if elapsed >= self.span_ticks {
                return true;
            }
        }
        core::hint::spin_loop();
        false
    }
}

pub fn delay_us(us: u32) {
    if us == 0 {
        return;
    }
    #[cfg(target_arch = "aarch64")]
    {
        let mut dl = Deadline::from_us(us as u64);
        while !dl.expired() {}
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = us;
    }
}

pub fn delay_ms(ms: u32) {
    for _ in 0..ms {
        delay_us(1000);
    }
}

#[inline]
fn irq_save_disable() -> u64 {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let daif: u64;
        core::arch::asm!(
            "mrs {daif}, daif",
            "msr daifset, #2",
            daif = out(reg) daif,
            options(nomem, nostack)
        );
        daif
    }
    #[cfg(not(target_arch = "aarch64"))]
    0
}

#[inline]
fn irq_restore(saved_daif: u64) {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "msr daif, {daif}",
            daif = in(reg) saved_daif,
            options(nomem, nostack)
        );
    }
    #[cfg(not(target_arch = "aarch64"))]
    let _ = saved_daif;
}

/// IRQ-safe spinlock that never bypasses mutual exclusion on timeout.
///
/// If the lock cannot be acquired before the deadline expires, `try_lock_timeout_us`
/// restores the caller's IRQ mask and returns `None` so two callers can never
/// concurrently access a shared hardware controller (PWRAP, MSDC0, I2C).
pub struct IrqSpinLock {
    locked: AtomicBool,
}

pub struct IrqSpinGuard<'a> {
    lock: &'a IrqSpinLock,
    saved_daif: u64,
}

impl IrqSpinLock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn try_lock_timeout_us(&self, timeout_us: u64) -> Option<IrqSpinGuard<'_>> {
        let saved_daif = irq_save_disable();
        let mut dl = Deadline::from_us(timeout_us);
        loop {
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Some(IrqSpinGuard {
                    lock: self,
                    saved_daif,
                });
            }
            if dl.expired() {
                irq_restore(saved_daif);
                return None;
            }
        }
    }
}

impl Default for IrqSpinLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IrqSpinGuard<'_> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        irq_restore(self.saved_daif);
    }
}

/// Standard IEEE 802.3 / Ethernet / GPT / Synaptics HDL CRC-32 (reflected poly `0xEDB88320`).
pub fn crc32_ieee(data: &[u8]) -> u32 {
    crc32_ieee_update(0xffff_ffff, data) ^ 0xffff_ffff
}

/// Incremental IEEE CRC-32 accumulator (start with `0xffff_ffff`, finish with `^ 0xffff_ffff`).
pub fn crc32_ieee_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

pub struct LogBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> LogBuf<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0u8; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Default for LogBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Write for LogBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let space = N.saturating_sub(self.len);
        let copy = bytes.len().min(space);
        self.buf[self.len..self.len + copy].copy_from_slice(&bytes[..copy]);
        self.len += copy;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_standard_check_vector() {
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32_ieee(b""), 0x0000_0000);
    }

    #[test]
    fn irq_spinlock_prevents_reentrant_bypass() {
        let lock = IrqSpinLock::new();
        let guard = lock.try_lock_timeout_us(100);
        assert!(guard.is_some());
        // Second acquisition must fail cleanly instead of breaking through!
        assert!(lock.try_lock_timeout_us(10).is_none());
        drop(guard);
        assert!(lock.try_lock_timeout_us(100).is_some());
    }

    #[test]
    fn deadline_expires_bounded() {
        let mut dl = Deadline::from_us(10);
        let mut steps = 0u32;
        while !dl.expired() {
            steps += 1;
        }
        assert!(steps > 0 && steps <= 50_000_000);
    }
}
