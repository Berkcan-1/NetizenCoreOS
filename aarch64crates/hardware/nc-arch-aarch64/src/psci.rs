//! ARMv8-A PSCI (Power State Coordination Interface) — Features 1 (SMP) & 35 (Power Management).

use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

pub const CONDUIT_NONE: u8 = 0;
pub const CONDUIT_HVC: u8 = 1;
pub const CONDUIT_SMC: u8 = 2;

pub const PSCI_SUCCESS: i64 = 0;
pub const PSCI_NOT_SUPPORTED: i64 = -1;
pub const PSCI_INVALID_PARAMETERS: i64 = -2;
pub const PSCI_DENIED: i64 = -3;
pub const PSCI_ALREADY_ON: i64 = -4;
pub const PSCI_ON_PENDING: i64 = -5;
pub const PSCI_INTERNAL_FAILURE: i64 = -6;
pub const PSCI_NOT_PRESENT: i64 = -7;

const FN_PSCI_VERSION: u64 = 0x8400_0000;
const FN_CPU_SUSPEND_64: u64 = 0xC400_0001;
const FN_CPU_OFF: u64 = 0x8400_0002;
const FN_CPU_ON_64: u64 = 0xC400_0003;
const FN_AFFINITY_INFO_64: u64 = 0xC400_0004;
const FN_SYSTEM_OFF: u64 = 0x8400_0008;
const FN_SYSTEM_RESET: u64 = 0x8400_0009;

static CONDUIT: AtomicU8 = AtomicU8::new(CONDUIT_NONE);

pub fn configure(conduit: u8) {
    CONDUIT.store(conduit, Ordering::Release);
}

pub fn conduit() -> u8 {
    CONDUIT.load(Ordering::Acquire)
}

fn call4(function: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let mut r0 = function;
    match CONDUIT.load(Ordering::Acquire) {
        CONDUIT_HVC => unsafe {
            asm!(
                "hvc #0",
                inout("x0") r0,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nomem, nostack)
            );
            r0 as i64
        },
        CONDUIT_SMC => unsafe {
            asm!(
                "smc #0",
                inout("x0") r0,
                inout("x1") arg0 => _,
                inout("x2") arg1 => _,
                inout("x3") arg2 => _,
                options(nomem, nostack)
            );
            r0 as i64
        },
        _ => PSCI_NOT_SUPPORTED,
    }
}

pub fn version() -> Option<(u16, u16)> {
    let raw = call4(FN_PSCI_VERSION, 0, 0, 0);
    if raw < 0 {
        return None;
    }
    let v = raw as u32;
    Some(((v >> 16) as u16, (v & 0xFFFF) as u16))
}

/// Power on a secondary CPU at `target_cpu` (MPIDR affinity), entering `entry_point`
/// with `x0 = context_id`.
pub fn cpu_on(target_cpu: u64, entry_point: u64, context_id: u64) -> i64 {
    call4(FN_CPU_ON_64, target_cpu, entry_point, context_id)
}

pub fn cpu_off() -> i64 {
    call4(FN_CPU_OFF, 0, 0, 0)
}

pub fn cpu_suspend(power_state: u64, entry_point: u64, context_id: u64) -> i64 {
    call4(FN_CPU_SUSPEND_64, power_state, entry_point, context_id)
}

pub fn affinity_info(target_affinity: u64, lowest_level: u64) -> i64 {
    call4(FN_AFFINITY_INFO_64, target_affinity, lowest_level, 0)
}

pub fn system_off() {
    let _ = call4(FN_SYSTEM_OFF, 0, 0, 0);
}

pub fn system_reset() {
    let _ = call4(FN_SYSTEM_RESET, 0, 0, 0);
}
