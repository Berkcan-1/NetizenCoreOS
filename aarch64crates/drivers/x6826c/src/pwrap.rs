//! MediaTek MT6768 PMIC Wrapper (`pwrap@1000d000`) and MT6358 PMIC bus driver.
//!
//! Verified against:
//! - X6826C `stock/dtb` (`/pwrap@1000d000`, IRQ SPI 0xc2 -> GIC INTID 226)
//! - X6826C `lk_rafw.bin.c` & `preloader_x6826c_h69c27.bin.c`
//! - MT6768 / Linux `mtk-pmic-wrap.c` WACS2 state machine (`CMD=0xc20`, `RDATA=0xc24`, `VLDCLR=0xc28`)

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use nc_device::DriverHost;

use crate::mmio::{dsb, rd32, wr32, Deadline, IrqSpinLock, LogBuf};

pub const PWRAP_BASE: usize = 0x1000_d000;
const PWRAP_WACS2_EN: usize = 0x09c;
const PWRAP_WACS2_CMD: usize = 0xc20;
const PWRAP_WACS2_RDATA: usize = 0xc24;
const PWRAP_WACS2_VLDCLR: usize = 0xc28;

const PWRAP_FSM_IDLE: u32 = 0;
const PWRAP_FSM_WFVLDCLR: u32 = 6;
const PWRAP_LOCK_TIMEOUT_US: u64 = 10_000;
const PWRAP_TX_TIMEOUT_US: u64 = 5_000;

/// Verified MT6358 register addresses (16-bit words on even byte offsets).
pub const MT6358_HWCID: u32 = 0x0008;
pub const MT6358_TOPSTATUS: u32 = 0x0028;
pub const PWRKEY_DEB: u16 = 1 << 1;
pub const HOMEKEY_DEB: u16 = 1 << 3;

/// MT6358 buck & LDO monitor registers (read-only telemetry).
pub const MT6358_VPROC11_DBG0: u32 = 0x139e;
pub const MT6358_VPROC12_DBG0: u32 = 0x141e;
pub const MT6358_VGPU_DBG0: u32 = 0x151e;
pub const MT6358_VGPU_DBG1: u32 = 0x1520;
pub const MT6358_VSRAM_PROC11_DBG0: u32 = 0x1b60;
pub const MT6358_VSRAM_PROC12_DBG0: u32 = 0x1ba2;
pub const MT6358_VSRAM_GPU_DBG0: u32 = 0x1be2;
pub const MT6358_VSRAM_GPU_DBG1: u32 = 0x1be4;
pub const MT6358_VCN18_CON0: u32 = 0x1a08;
pub const MT6358_VCN28_CON0: u32 = 0x1a26;
pub const MT6358_VCN33_BT_CON0: u32 = 0x19d0;
pub const MT6358_VCN33_WIFI_CON0: u32 = 0x19d8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PwrapError {
    InvalidAddress,
    ControllerUnavailable,
    LockTimeout,
    WaitIdleTimeout,
    WaitValidTimeout,
}

static PWRAP_LOCK: IrqSpinLock = IrqSpinLock::new();
static PWRAP_READY: AtomicBool = AtomicBool::new(false);
static PMIC_HWCID: AtomicU16 = AtomicU16::new(0);

#[inline]
pub fn is_ready() -> bool {
    PWRAP_READY.load(Ordering::Acquire)
}

#[inline]
pub fn hwcid() -> u16 {
    PMIC_HWCID.load(Ordering::Acquire)
}

#[inline]
fn fsm_state(rdata: u32) -> u32 {
    (rdata >> 16) & 7
}

fn wait_state(target: u32) -> bool {
    let mut dl = Deadline::from_us(PWRAP_TX_TIMEOUT_US);
    loop {
        if fsm_state(rd32(PWRAP_BASE + PWRAP_WACS2_RDATA)) == target {
            return true;
        }
        if dl.expired() {
            return false;
        }
    }
}

/// Ensures WACS2 is in IDLE, clearing any stale WFVLDCLR data left by an interrupted
/// prior transaction or bootloader handoff.
fn wait_idle_recover() -> Result<(), PwrapError> {
    let mut dl = Deadline::from_us(PWRAP_TX_TIMEOUT_US);
    loop {
        let rdata = rd32(PWRAP_BASE + PWRAP_WACS2_RDATA);
        if rdata == 0xffff_ffff {
            return Err(PwrapError::ControllerUnavailable);
        }
        match fsm_state(rdata) {
            PWRAP_FSM_IDLE => return Ok(()),
            PWRAP_FSM_WFVLDCLR => {
                wr32(PWRAP_BASE + PWRAP_WACS2_VLDCLR, 1);
                dsb();
            }
            _ => {}
        }
        if dl.expired() {
            return Err(PwrapError::WaitIdleTimeout);
        }
    }
}

fn read_unlocked(reg: u32) -> Result<u16, PwrapError> {
    if reg > 0xffff || (reg & 1) != 0 {
        return Err(PwrapError::InvalidAddress);
    }
    wait_idle_recover()?;
    wr32(PWRAP_BASE + PWRAP_WACS2_CMD, (reg >> 1) << 16);
    dsb();
    if !wait_state(PWRAP_FSM_WFVLDCLR) {
        return Err(PwrapError::WaitValidTimeout);
    }
    let val = (rd32(PWRAP_BASE + PWRAP_WACS2_RDATA) & 0xffff) as u16;
    wr32(PWRAP_BASE + PWRAP_WACS2_VLDCLR, 1);
    dsb();
    Ok(val)
}

fn write_unlocked(reg: u32, value: u16) -> Result<(), PwrapError> {
    if reg > 0xffff || (reg & 1) != 0 {
        return Err(PwrapError::InvalidAddress);
    }
    wait_idle_recover()?;
    wr32(
        PWRAP_BASE + PWRAP_WACS2_CMD,
        (1u32 << 31) | ((reg >> 1) << 16) | (value as u32),
    );
    dsb();
    if wait_state(PWRAP_FSM_IDLE) {
        Ok(())
    } else {
        Err(PwrapError::WaitIdleTimeout)
    }
}

pub fn read_checked(reg: u32) -> Result<u16, PwrapError> {
    let _guard = PWRAP_LOCK
        .try_lock_timeout_us(PWRAP_LOCK_TIMEOUT_US)
        .ok_or(PwrapError::LockTimeout)?;
    read_unlocked(reg)
}

pub fn write_checked(reg: u32, value: u16) -> Result<(), PwrapError> {
    let _guard = PWRAP_LOCK
        .try_lock_timeout_us(PWRAP_LOCK_TIMEOUT_US)
        .ok_or(PwrapError::LockTimeout)?;
    write_unlocked(reg, value)
}

pub fn read(reg: u32) -> Option<u16> {
    read_checked(reg).ok()
}

pub fn write(reg: u32, value: u16) -> bool {
    write_checked(reg, value).is_ok()
}

/// Reads a shifted bitfield from a 16-bit MT6358 register.
pub fn read_field(reg: u32, mask: u16, shift: u8) -> Result<u16, PwrapError> {
    let raw = read_checked(reg)?;
    Ok((raw >> shift) & mask)
}

/// Performs a locked read-modify-write on a normal (non-W1C) MT6358 control register.
///
/// SAFETY NOTE: Never call this on registers containing write-1-to-clear (W1C) bits.
pub fn update_field(reg: u32, mask: u16, shift: u8, value: u16) -> Result<(), PwrapError> {
    let _guard = PWRAP_LOCK
        .try_lock_timeout_us(PWRAP_LOCK_TIMEOUT_US)
        .ok_or(PwrapError::LockTimeout)?;
    let cur = read_unlocked(reg)?;
    let shifted_mask = mask.wrapping_shl(shift as u32);
    let next = (cur & !shifted_mask) | ((value & mask).wrapping_shl(shift as u32));
    write_unlocked(reg, next)
}

/// Converts a 7-bit MT6358 buck/LDO selector (`500 mV + 6.25 mV * sel`) to millivolts.
pub const fn millivolts(selector: u16) -> u32 {
    500 + (((selector & 0x7f) as u32) * 625) / 100
}

pub fn init(host: &mut dyn DriverHost) {
    let wacs2_en = rd32(PWRAP_BASE + PWRAP_WACS2_EN);
    let mut msg = LogBuf::<192>::new();
    if wacs2_en == 0xffff_ffff {
        PWRAP_READY.store(false, Ordering::Release);
        let _ = write!(msg, "mtk-pwrap-mt6358: denetleyici erisilemez");
        host.log(msg.as_str());
        return;
    }

    match read_checked(MT6358_HWCID) {
        Ok(hwcid) if hwcid != 0 && hwcid != 0xffff => {
            PMIC_HWCID.store(hwcid, Ordering::Release);
            PWRAP_READY.store(true, Ordering::Release);
            let vproc11 = read(MT6358_VPROC11_DBG0).unwrap_or(0) & 0x7f;
            let vproc12 = read(MT6358_VPROC12_DBG0).unwrap_or(0) & 0x7f;
            let vgpu = read(MT6358_VGPU_DBG0).unwrap_or(0) & 0x7f;
            let _ = write!(
                msg,
                "mtk-pwrap-mt6358: PMIC kimlik {:#06x} (Vproc11 {} mV, Vproc12 {} mV, Vgpu {} mV)",
                hwcid,
                millivolts(vproc11),
                millivolts(vproc12),
                millivolts(vgpu)
            );
        }
        Ok(hwcid) => {
            PWRAP_READY.store(false, Ordering::Release);
            let _ = write!(
                msg,
                "mtk-pwrap-mt6358: gecersiz PMIC kimlik {:#06x} (WACS2_EN={:#x})",
                hwcid, wacs2_en
            );
        }
        Err(err) => {
            PWRAP_READY.store(false, Ordering::Release);
            let _ = write!(
                msg,
                "mtk-pwrap-mt6358: WACS2 hatasi {:?} (WACS2_EN={:#x})",
                err, wacs2_en
            );
        }
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt6358_voltage_formula() {
        assert_eq!(millivolts(0), 500);
        assert_eq!(millivolts(48), 800);
        assert_eq!(millivolts(94), 1087);
        assert_eq!(millivolts(0x7f), 1293);
    }

    #[test]
    fn fsm_state_extraction() {
        assert_eq!(fsm_state(0x0000_1234), PWRAP_FSM_IDLE);
        assert_eq!(fsm_state(0x0006_abcd), PWRAP_FSM_WFVLDCLR);
    }
}
