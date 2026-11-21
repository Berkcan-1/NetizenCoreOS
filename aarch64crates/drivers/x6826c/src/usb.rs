//! USB 2.0 MUSB, connector role (IDDIG), and Richtek RT9471 primary charger monitoring
//! for the Infinix Hot 20 (X6826C / MT6768).
//!
//! Verified directly against X6826C hardware evidence:
//! - `stock/dtb` `/usb0@11200000` (`mediatek,mt6768-usb20`, IRQ SPI 0x61 -> GIC INTID 129)
//! - `stock/dtb` `/usb-phy@11210000` (`mediatek,generic-tphy-v1`, `0x11cc_0000` / `0x11cc_0800`)
//! - `stock/dtbo` `/extcon_usb` (`fake_typec_otg_detect`, `mediatek,bypss-typec-sink = 1`,
//!   `id-gpios = GPIO 41 (0x29)`, `otg_dcp` = GPIO 40 (0x28))
//! - `stock/dtbo` `/i2c5@11016000/rt-swchg@53` (`richtek,swchg`, `rt9471`, `primary_chg`,
//!   slave address `0x53`, INT GPIO 9)
//! - `lk_rafw.bin.c` L71490–71810 (RT9471 device IDs `0x09/0x0a/0x0d/0x0e` in reg `0x0b`,
//!   `ICSTAT` reg `0x0f`, `STAT0..3` regs `0x10..0x13`, `CHG_EN` reg `0x02` bit 0,
//!   `AICR` `0x03`, `MIVR` `0x04`, `VCHG/CV` `0x07`, `ICHG` `0x08`, `IEOC` `0x0a`).
//!
//! Safety policy: Strictly READ-ONLY. Never enables OTG VBUS boost or modifies charger
//! current/voltage/termination limits.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use nc_device::DriverHost;

use crate::i2c::{self, I2cBus};
use crate::mmio::{rd32, rd8, LogBuf};
use crate::pwrap;

pub const USB0_BASE: usize = 0x1120_0000;
pub const USB_PHY_BASE: usize = 0x11cc_0000;
const GPIO_BASE: usize = 0x1000_5000;
/// GPIO_DIN1 covers GPIO 32..63; GPIO 41 (IDDIG) is bit 9 (`41 - 32`).
const GPIO_DIN1: usize = 0x210;
const IDDIG_PIN_BIT: u32 = 1 << (41 - 32);

/// Mentor MUSB HDRC read-only status register offsets (`usb0@11200000`).
const MUSB_POWER: usize = 0x01;
const MUSB_DEVCTL: usize = 0x60;
const MUSB_POWER_HSMODE: u8 = 1 << 4;
const MUSB_POWER_SOFTCONN: u8 = 1 << 6;
const MUSB_DEVCTL_SESSION: u8 = 1 << 0;
const MUSB_DEVCTL_HM: u8 = 1 << 2;
const MUSB_DEVCTL_VBUS_SHIFT: u8 = 3;
const MUSB_DEVCTL_VBUS_MASK: u8 = 0x03;
const MUSB_DEVCTL_BDEVICE: u8 = 1 << 7;

/// MT6358 `CHR_CON0` (`0x0a88`) `RGS_CHRDET` at bit 5 (verified in `preloader` & `lk_rafw.bin.c`).
pub const MT6358_CHR_CON0: u32 = 0x0a88;
pub const MT6358_RGS_CHRDET_BIT: u16 = 1 << 5;

/// Richtek RT9471 primary switching charger on I2C5 (`0x53`).
pub const RT9471_ADDR: u8 = 0x53;
const RT9471_REG_VBUS: u8 = 0x02;
const RT9471_REG_AICR: u8 = 0x03;
const RT9471_REG_MIVR: u8 = 0x04;
const RT9471_REG_VCHG: u8 = 0x07;
const RT9471_REG_ICHG: u8 = 0x08;
const RT9471_REG_IEOC: u8 = 0x0a;
const RT9471_REG_INFO: u8 = 0x0b;
const RT9471_REG_ICSTAT: u8 = 0x0f;
const RT9471_REG_STAT0: u8 = 0x10;

const RT9471_STAT0_VBUS_GD: u8 = 1 << 7;
const RT9471_STAT1_MIVR: u8 = 1 << 7;
const RT9471_STAT1_AICR: u8 = 1 << 6;
const RT9471_STAT3_OTP: u8 = 1 << 7;
const RT9471_STAT3_VAC_OV: u8 = 1 << 6;

static RT9471_PRESENT: AtomicBool = AtomicBool::new(false);
static RT9471_DEVID: AtomicU8 = AtomicU8::new(0);
static USB_MMIO_OK: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbRole {
    Disconnected,
    Peripheral,
    HostOtg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargerIcState {
    Unavailable,
    Sleep,
    VbusReady,
    TrickleCharge,
    PreCharge,
    FastCharge,
    Ieoc,
    BackgroundCharge,
    ChargeDone,
    ChargeFault,
    OtgBoost,
    Unknown(u8),
}

impl ChargerIcState {
    pub const fn from_icstat(raw: u8) -> Self {
        match raw & 0x0f {
            0x0 => ChargerIcState::Sleep,
            0x1 => ChargerIcState::VbusReady,
            0x2 => ChargerIcState::TrickleCharge,
            0x3 => ChargerIcState::PreCharge,
            0x4 => ChargerIcState::FastCharge,
            0x5 => ChargerIcState::Ieoc,
            0x6 => ChargerIcState::BackgroundCharge,
            0x7 => ChargerIcState::ChargeDone,
            0x8 => ChargerIcState::ChargeFault,
            0xf => ChargerIcState::OtgBoost,
            other => ChargerIcState::Unknown(other),
        }
    }

    pub const fn is_actively_charging(self) -> bool {
        matches!(
            self,
            ChargerIcState::TrickleCharge
                | ChargerIcState::PreCharge
                | ChargerIcState::FastCharge
                | ChargerIcState::Ieoc
                | ChargerIcState::BackgroundCharge
        )
    }

    pub const fn is_full(self) -> bool {
        matches!(self, ChargerIcState::ChargeDone)
    }

    pub const fn is_fault(self) -> bool {
        matches!(self, ChargerIcState::ChargeFault)
    }

    pub const fn label(self) -> &'static str {
        match self {
            ChargerIcState::Unavailable => "unavailable",
            ChargerIcState::Sleep => "sleep",
            ChargerIcState::VbusReady => "vbus-ready",
            ChargerIcState::TrickleCharge => "trickle",
            ChargerIcState::PreCharge => "prechg",
            ChargerIcState::FastCharge => "fastchg",
            ChargerIcState::Ieoc => "ieoc",
            ChargerIcState::BackgroundCharge => "bgchg",
            ChargerIcState::ChargeDone => "done",
            ChargerIcState::ChargeFault => "fault",
            ChargerIcState::OtgBoost => "otg-boost",
            ChargerIcState::Unknown(_) => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rt9471Snapshot {
    pub present: bool,
    pub dev_id: u8,
    pub dev_rev: u8,
    pub ic_state: ChargerIcState,
    pub vbus_good: bool,
    pub chg_enabled: bool,
    pub mivr_active: bool,
    pub aicr_active: bool,
    pub jeita_stat: u8,
    pub fault_otp: bool,
    pub fault_vac_ov: bool,
    pub cv_mv: u16,
    pub ichg_ma: u16,
    pub aicr_ma: u16,
    pub mivr_mv: u16,
    pub ieoc_ma: u16,
}

impl Rt9471Snapshot {
    pub const UNAVAILABLE: Self = Self {
        present: false,
        dev_id: 0,
        dev_rev: 0,
        ic_state: ChargerIcState::Unavailable,
        vbus_good: false,
        chg_enabled: false,
        mivr_active: false,
        aicr_active: false,
        jeita_stat: 0,
        fault_otp: false,
        fault_vac_ov: false,
        cv_mv: 0,
        ichg_ma: 0,
        aicr_ma: 0,
        mivr_mv: 0,
        ieoc_ma: 0,
    };
}

/// Complete, cleanly separated USB connector, power-input, and charger telemetry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbPowerStatus {
    /// Physical cable / input presence inferred from VBUS or OTG ID pin.
    pub cable_connected: bool,
    /// Valid VBUS input detected (RT9471 `VBUS_GD`, MT6358 `RGS_CHRDET`, or MUSB VBUS valid).
    pub vbus_present: bool,
    /// PMIC `RGS_CHRDET` raw indicator (`MT6358_CHR_CON0` bit 5).
    pub pmic_chrdet: bool,
    /// Active USB data session / soft-connect observed in MUSB registers.
    pub usb_data_active: bool,
    /// High-speed USB negotiation active in MUSB.
    pub usb_high_speed: bool,
    /// Connector role derived from X6826C IDDIG GPIO 41 + VBUS + MUSB DEVCTL.
    pub role: UsbRole,
    /// Raw IDDIG GPIO 41 state (`true` = ID pin pulled low => OTG adapter/host cable).
    pub iddig_low: bool,
    /// Primary charger IC (Richtek RT9471 on I2C5 `0x53`) state.
    pub charger: Rt9471Snapshot,
}

pub const fn is_valid_rt9471_dev_id(dev_id: u8) -> bool {
    matches!(dev_id, 0x09 | 0x0a | 0x0d | 0x0e)
}

pub fn decode_rt9471_info(reg_0b: u8) -> Option<(u8, u8)> {
    let dev_id = (reg_0b & 0x7f) >> 3;
    let dev_rev = reg_0b & 0x03;
    if is_valid_rt9471_dev_id(dev_id) {
        Some((dev_id, dev_rev))
    } else {
        None
    }
}

/// Decodes RT9471 regulation registers (`lk_rafw.bin.c` L71517–71564).
pub const fn decode_rt9471_cv_mv(reg_07: u8) -> u16 {
    let mv = 3900u32 + ((reg_07 & 0x7f) as u32) * 10;
    if mv > 4700 { 4700 } else { mv as u16 }
}

pub const fn decode_rt9471_ichg_ma(reg_08: u8) -> u16 {
    ((reg_08 & 0x3f) as u16) * 50
}

pub const fn decode_rt9471_aicr_ma(reg_03: u8) -> u16 {
    let code = (reg_03 & 0x3f) as u16;
    if code == 0 { 50 } else { code * 50 + 50 }
}

pub const fn decode_rt9471_mivr_mv(reg_04: u8) -> u16 {
    3900 + ((reg_04 & 0x0f) as u16) * 100
}

pub const fn decode_rt9471_ieoc_ma(reg_0a: u8) -> u16 {
    let ma = ((reg_0a >> 4) as u16) * 50 + 50;
    if ma > 800 { 800 } else { ma }
}

pub fn rt9471_present() -> bool {
    RT9471_PRESENT.load(Ordering::Acquire)
}

pub fn read_rt9471() -> Rt9471Snapshot {
    if !i2c::is_ready(I2cBus::Bus5) {
        return Rt9471Snapshot::UNAVAILABLE;
    }
    let Ok(info_reg) = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_INFO) else {
        return Rt9471Snapshot::UNAVAILABLE;
    };
    let Some((dev_id, dev_rev)) = decode_rt9471_info(info_reg) else {
        return Rt9471Snapshot::UNAVAILABLE;
    };
    let Ok(icstat_raw) = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_ICSTAT) else {
        return Rt9471Snapshot::UNAVAILABLE;
    };
    let mut stats = [0u8; 4];
    if i2c::read_regs(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_STAT0, &mut stats).is_err() {
        return Rt9471Snapshot::UNAVAILABLE;
    }
    let reg_vbus = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_VBUS).unwrap_or(0);
    let reg_aicr = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_AICR).unwrap_or(0);
    let reg_mivr = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_MIVR).unwrap_or(0);
    let reg_vchg = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_VCHG).unwrap_or(0);
    let reg_ichg = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_ICHG).unwrap_or(0);
    let reg_ieoc = i2c::read_reg(I2cBus::Bus5, RT9471_ADDR, RT9471_REG_IEOC).unwrap_or(0);

    Rt9471Snapshot {
        present: true,
        dev_id,
        dev_rev,
        ic_state: ChargerIcState::from_icstat(icstat_raw),
        vbus_good: (stats[0] & RT9471_STAT0_VBUS_GD) != 0,
        chg_enabled: (reg_vbus & 1) != 0,
        mivr_active: (stats[1] & RT9471_STAT1_MIVR) != 0,
        aicr_active: (stats[1] & RT9471_STAT1_AICR) != 0,
        jeita_stat: stats[2],
        fault_otp: (stats[3] & RT9471_STAT3_OTP) != 0,
        fault_vac_ov: (stats[3] & RT9471_STAT3_VAC_OV) != 0,
        cv_mv: decode_rt9471_cv_mv(reg_vchg),
        ichg_ma: decode_rt9471_ichg_ma(reg_ichg),
        aicr_ma: decode_rt9471_aicr_ma(reg_aicr),
        mivr_mv: decode_rt9471_mivr_mv(reg_mivr),
        ieoc_ma: decode_rt9471_ieoc_ma(reg_ieoc),
    }
}

pub fn read_pmic_chrdet() -> Option<bool> {
    if !pwrap::is_ready() {
        return None;
    }
    let val = pwrap::read(MT6358_CHR_CON0)?;
    Some((val & MT6358_RGS_CHRDET_BIT) != 0)
}

fn read_iddig_low() -> bool {
    let din1 = rd32(GPIO_BASE + GPIO_DIN1);
    if din1 == 0xffff_ffff {
        return false;
    }
    (din1 & IDDIG_PIN_BIT) == 0
}

fn read_musb_state() -> (bool, bool, bool, bool) {
    if !USB_MMIO_OK.load(Ordering::Acquire) {
        return (false, false, false, false);
    }
    let power = rd8(USB0_BASE + MUSB_POWER);
    let devctl = rd8(USB0_BASE + MUSB_DEVCTL);
    if power == 0xff && devctl == 0xff {
        return (false, false, false, false);
    }
    let vbus_level = (devctl >> MUSB_DEVCTL_VBUS_SHIFT) & MUSB_DEVCTL_VBUS_MASK;
    let musb_vbus_valid = vbus_level >= 2;
    let host_mode = (devctl & MUSB_DEVCTL_HM) != 0 && (devctl & MUSB_DEVCTL_BDEVICE) == 0;
    let data_active = (devctl & MUSB_DEVCTL_SESSION) != 0 && (power & MUSB_POWER_SOFTCONN) != 0;
    let hs = (power & MUSB_POWER_HSMODE) != 0;
    (musb_vbus_valid, host_mode, data_active, hs)
}

/// Combines connector IDDIG, MUSB, MT6358 `RGS_CHRDET`, and RT9471 into an honest status snapshot.
pub fn resolve_usb_role(iddig_low: bool, musb_host: bool, vbus_present: bool, otg_boost: bool) -> UsbRole {
    if iddig_low || musb_host || otg_boost {
        UsbRole::HostOtg
    } else if vbus_present {
        UsbRole::Peripheral
    } else {
        UsbRole::Disconnected
    }
}

pub fn read_status() -> UsbPowerStatus {
    let charger = if rt9471_present() {
        read_rt9471()
    } else {
        Rt9471Snapshot::UNAVAILABLE
    };
    let pmic_chrdet = read_pmic_chrdet().unwrap_or(false);
    let iddig_low = read_iddig_low();
    let (musb_vbus, musb_host, usb_data_active, usb_high_speed) = read_musb_state();

    let vbus_present = if charger.present {
        charger.vbus_good || pmic_chrdet
    } else {
        pmic_chrdet || musb_vbus
    };
    let role = resolve_usb_role(
        iddig_low,
        musb_host,
        vbus_present,
        matches!(charger.ic_state, ChargerIcState::OtgBoost),
    );
    let cable_connected = vbus_present || iddig_low;

    UsbPowerStatus {
        cable_connected,
        vbus_present,
        pmic_chrdet,
        usb_data_active,
        usb_high_speed,
        role,
        iddig_low,
        charger,
    }
}

pub fn init(host: &mut dyn DriverHost) {
    let musb_probe = rd32(USB0_BASE);
    let musb_ok = musb_probe != 0xffff_ffff;
    USB_MMIO_OK.store(musb_ok, Ordering::Release);

    let rt = read_rt9471();
    RT9471_PRESENT.store(rt.present, Ordering::Release);
    if rt.present {
        RT9471_DEVID.store(rt.dev_id, Ordering::Release);
    }

    let st = read_status();
    let mut msg = LogBuf::<224>::new();
    if rt.present {
        let _ = write!(
            msg,
            "mtk-usb-chg: RT9471 id={:#x} rev={} state={} vbus_gd={} chg_en={} CV={}mV ICHG={}mA AICR={}mA | role={:?} iddig_lo={} musb={}",
            rt.dev_id,
            rt.dev_rev,
            rt.ic_state.label(),
            rt.vbus_good as u8,
            rt.chg_enabled as u8,
            rt.cv_mv,
            rt.ichg_ma,
            rt.aicr_ma,
            st.role,
            st.iddig_low as u8,
            if musb_ok { "ok" } else { "off" }
        );
    } else {
        let _ = write!(
            msg,
            "mtk-usb-chg: RT9471(i2c5@0x53) yanit vermedi; pmic_chrdet={} role={:?} iddig_lo={} musb={}",
            st.pmic_chrdet as u8,
            st.role,
            st.iddig_low as u8,
            if musb_ok { "ok" } else { "off" }
        );
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rt9471_devid_and_icstat_decoding() {
        assert_eq!(decode_rt9471_info((0x0d << 3) | 0x02), Some((0x0d, 2)));
        assert_eq!(decode_rt9471_info((0x0e << 3) | 0x01), Some((0x0e, 1)));
        assert_eq!(decode_rt9471_info(0x00), None);
        assert_eq!(decode_rt9471_info(0xff), None);

        assert_eq!(ChargerIcState::from_icstat(0x00), ChargerIcState::Sleep);
        assert_eq!(ChargerIcState::from_icstat(0x01), ChargerIcState::VbusReady);
        assert!(ChargerIcState::from_icstat(0x04).is_actively_charging());
        assert!(!ChargerIcState::from_icstat(0x01).is_actively_charging());
        assert!(ChargerIcState::from_icstat(0x07).is_full());
        assert!(ChargerIcState::from_icstat(0x08).is_fault());
        assert_eq!(ChargerIcState::from_icstat(0x0f), ChargerIcState::OtgBoost);
    }

    #[test]
    fn rt9471_electrical_parameter_decoders() {
        // X6826C DTBO default CV = 4.45V => (4450 - 3900)/10 = 55
        assert_eq!(decode_rt9471_cv_mv(55), 4450);
        // Cap at 4700 mV
        assert_eq!(decode_rt9471_cv_mv(0x7f), 4700);
        // ICHG = 1500 mA => code 30
        assert_eq!(decode_rt9471_ichg_ma(30), 1500);
        // AICR = 500 mA => code 9 (9*50 + 50)
        assert_eq!(decode_rt9471_aicr_ma(9), 500);
        // MIVR = 4400 mV => code 5
        assert_eq!(decode_rt9471_mivr_mv(5), 4400);
        // IEOC = 200 mA => code 3 in bits [7:4]
        assert_eq!(decode_rt9471_ieoc_ma(3 << 4), 200);
    }

    #[test]
    fn usb_role_resolution() {
        assert_eq!(resolve_usb_role(false, false, false, false), UsbRole::Disconnected);
        assert_eq!(resolve_usb_role(false, false, true, false), UsbRole::Peripheral);
        assert_eq!(resolve_usb_role(true, false, false, false), UsbRole::HostOtg);
        assert_eq!(resolve_usb_role(false, false, false, true), UsbRole::HostOtg);
    }
}
