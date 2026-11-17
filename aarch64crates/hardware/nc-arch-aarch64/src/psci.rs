use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

pub const CONDUIT_NONE: u8 = 0;
pub const CONDUIT_HVC: u8 = 1;
pub const CONDUIT_SMC: u8 = 2;

const FN_SYSTEM_OFF: u64 = 0x8400_0008;
const FN_SYSTEM_RESET: u64 = 0x8400_0009;

static CONDUIT: AtomicU8 = AtomicU8::new(CONDUIT_NONE);

pub fn configure(conduit: u8) {
    CONDUIT.store(conduit, Ordering::Relaxed);
}

fn call(function: u64) {
    match CONDUIT.load(Ordering::Relaxed) {
        CONDUIT_HVC => unsafe {
            asm!("hvc #0", inout("x0") function => _, inout("x1") 0u64 => _, inout("x2") 0u64 => _, inout("x3") 0u64 => _, options(nomem, nostack));
        },
        CONDUIT_SMC => unsafe {
            asm!("smc #0", inout("x0") function => _, inout("x1") 0u64 => _, inout("x2") 0u64 => _, inout("x3") 0u64 => _, options(nomem, nostack));
        },
        _ => {}
    }
}

pub fn system_off() {
    call(FN_SYSTEM_OFF);
}

pub fn system_reset() {
    call(FN_SYSTEM_RESET);
}
