use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

static PERIOD: AtomicU64 = AtomicU64::new(0);

pub fn start(hz: u64) -> bool {
    let frequency = crate::time::frequency_hz();
    if frequency == 0 || hz == 0 {
        return false;
    }
    let period = frequency / hz;
    if period == 0 {
        return false;
    }
    PERIOD.store(period, Ordering::Relaxed);
    unsafe {
        asm!(
            "msr cntv_tval_el0, {period}",
            "msr cntv_ctl_el0, {enable}",
            "isb",
            period = in(reg) period,
            enable = in(reg) 1u64,
            options(nomem, nostack)
        );
    }
    true
}

pub fn rearm() {
    let period = PERIOD.load(Ordering::Relaxed);
    if period == 0 {
        return;
    }
    unsafe {
        asm!("msr cntv_tval_el0, {}", "isb", in(reg) period, options(nomem, nostack));
    }
}

pub fn stop() {
    unsafe {
        asm!("msr cntv_ctl_el0, xzr", "isb", options(nomem, nostack));
    }
}
