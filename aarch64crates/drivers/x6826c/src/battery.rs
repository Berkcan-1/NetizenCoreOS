//! MT6358 PMIC Battery, AUXADC, Fuel-Gauge (FGADC) & RT9471 Charger-Aware Monitor
//! for the Infinix Hot 20 (X6826C / MT6768).
//!
//! Verified directly from X6826C primary hardware evidence:
//! - `stock/dtb` `/pwrap@1000d000/mt6358-pmic/mt635x-auxadc`:
//!   - Ch 0 `batadc` (`resistance-ratio = <3 1>`, 15-bit) -> `0x1088`
//!   - Ch 2 `vcdt` (charger input divider, 12-bit) -> `0x108c`
//!   - Ch 3 `bat_temp` (`resistance-ratio = <2 1>`, 12-bit) -> `0x108e`
//!   - Ch 14 `vbif` (`resistance-ratio = <2 1>`, 12-bit, battery presence) -> `0x10a4`
//! - Linux `mt6359-auxadc.c` (`mt6358_auxadc_regs`):
//!   - `MT6358_AUXADC_ADC0 = 0x1088`
//!   - `MT6358_AUXADC_RQST0 = 0x110e` (FIXED from erroneous MT6359 `0x1108`!)
//! - `preloader_x6826c_h69c27.bin.c` L35962 & `lk_rafw.bin.c` L26227:
//!   - `MT6358_CHR_CON0 = 0x0a88`, `RGS_CHRDET = bit 5 (1 << 5)` (FIXED from erroneous bit 1!)
//! - `lk_rafw.bin.c` L71051–71124 & MT6358 FGADC register map:
//!   - `MT6358_FGADC_CON1 = 0x0d0a` (`SW_READ_PRE = bit 0`, `SW_CLEAR = bit 3`, `LATCHDATA_ST = bit 15`)
//!   - `MT6358_FGADC_CUR_CON0 = 0x0d8a` (signed 16-bit current, `UNIT_FGCURRENT = 381470`)
//! - `stock/dtb` `/battery` (`mediatek,bat_gm30`):
//!   - `R_FG_VALUE = 5` mOhm, `CAR_TUNE_VALUE = 100`, `FG_METER_RESISTANCE = 75`
//!   - All 5 calibrated temperature OCV + internal-resistance profiles:
//!     `T0 = 50°C`, `T1 = 25°C`, `T2 = 10°C`, `T3 = 0°C`, `T4 = -10°C` (5131.9 mAh HV cell)
//!
//! Safety & Honesty Policy:
//! - Strictly READ-ONLY (never modifies charging current/voltage/termination registers).
//! - Because X6826C's board-specific NTC pull-up resistor (`RBAT_PULL_UP_R`) and thermistor
//!   R-to-T table are not present in `stock/dtb` or `stock/dtbo`, `BAT_TEMP` raw millivolts
//!   are sampled for open/short fault checks while `temp_deci_c` honestly reports `i16::MIN`
//!   (unavailable) rather than fabricating a temperature.

use core::fmt::Write;
use nc_device::{BatteryStatus, DriverHost};

use crate::mmio::{Deadline, LogBuf};
use crate::pwrap;
use crate::usb;

/// MT6358 AUXADC register map (`mt6358_auxadc_regs`, verified against Linux `mt6359-auxadc.c`).
const MT6358_AUXADC_ADC0: u32 = 0x1088; // Ch 0: BATADC (15-bit, 3:1)
const MT6358_AUXADC_ADC2: u32 = 0x108c; // Ch 2: VCDT (12-bit)
const MT6358_AUXADC_ADC3: u32 = 0x108e; // Ch 3: BAT_TEMP (12-bit, 2:1)
const MT6358_AUXADC_ADC14: u32 = 0x10a4; // Ch 14: VBIF (12-bit, 2:1)
const MT6358_AUXADC_RQST0: u32 = 0x110e; // Software trigger bits [15:0]

const AUXADC_RDY_BIT: u16 = 1 << 15;
const AUXADC_DATA_15BIT: u16 = 0x7fff;
const AUXADC_DATA_12BIT: u16 = 0x0fff;
const AUXADC_TIMEOUT_US: u64 = 15_000;

/// MT6358 Fuel Gauge ADC (`FGADC`) registers (`lk_rafw.bin.c` L71051–71124).
const MT6358_FGADC_CON1: u32 = 0x0d0a;
const MT6358_FGADC_CUR_CON0: u32 = 0x0d8a;
const FG_SW_READ_PRE: u16 = 1 << 0;
const FG_SW_CLEAR: u16 = 1 << 3;
const FG_LATCHDATA_ST: u16 = 1 << 15;
const FG_TIMEOUT_US: u64 = 20_000;

/// X6826C `/battery` fuel gauge calibration constants from `stock/dtb`.
const X6826C_R_FG_MOHM: i64 = 5; // `R_FG_VALUE = <0x05>`
const X6826C_CAR_TUNE: i64 = 100; // `CAR_TUNE_VALUE = <0x64>`
/// `UNIT_FGCURRENT = 0x5d21e = 381470` (verified in `lk_rafw.bin.c` L71105).
const MT6358_UNIT_FGCURRENT: i64 = 381_470;

/// Valid physical operating window for a single-cell 4.45V HV Li-Po battery (mV).
const VBAT_MIN_VALID_MV: u16 = 2700;
const VBAT_MAX_VALID_MV: u16 = 4650;

/// Calibrated `(ocv_mv, soc_percent, r_bat_mohm)` curves extracted from X6826C `stock/dtb`
/// `/battery` `battery0_profile_t0..t4` (`T0=50°C`, `T1=25°C`, `T2=10°C`, `T3=0°C`, `T4=-10°C`).
const X6826C_OCV_TEMPS_C: [i16; 5] = [50, 25, 10, 0, -10];

const X6826C_OCV_PROFILES: [[(u16, u8, u16); 21]; 5] = [
    // T0 = 50°C
    [
        (4438, 100, 97),  (4387, 95, 98),  (4333, 90, 97),  (4273, 85, 98),
        (4209, 80, 99),   (4147, 75, 101), (4088, 70, 102), (4033, 65, 106),
        (3981, 60, 110),  (3917, 55, 98),  (3877, 49, 97),  (3847, 44, 97),
        (3822, 39, 99),   (3802, 34, 102), (3777, 29, 100), (3749, 24, 98),
        (3726, 19, 99),   (3692, 14, 98),  (3678, 9, 106),  (3541, 4, 105),
        (2936, 0, 476),
    ],
    // T1 = 25°C (default nominal curve when temperature sensor calibration is unavailable)
    [
        (4447, 100, 124), (4394, 95, 125), (4344, 90, 125), (4286, 85, 125),
        (4223, 80, 126),  (4160, 75, 127), (4100, 70, 130), (4043, 65, 136),
        (3983, 60, 136),  (3923, 55, 119), (3882, 49, 116), (3851, 44, 117),
        (3825, 39, 119),  (3803, 34, 120), (3784, 29, 120), (3765, 24, 118),
        (3743, 19, 119),  (3712, 14, 123), (3689, 9, 130),  (3617, 4, 144),
        (3006, 0, 642),
    ],
    // T2 = 10°C
    [
        (4445, 100, 226), (4376, 95, 220), (4326, 90, 212), (4271, 85, 205),
        (4211, 80, 201),  (4151, 75, 200), (4092, 70, 201), (4032, 65, 202),
        (3969, 60, 189),  (3915, 55, 175), (3875, 49, 172), (3842, 44, 173),
        (3816, 39, 176),  (3794, 34, 180), (3779, 29, 185), (3765, 24, 199),
        (3744, 19, 223),  (3713, 14, 265), (3677, 9, 351),  (3626, 4, 633),
        (3386, 0, 1594),
    ],
    // T3 = 0°C
    [
        (4446, 100, 316), (4375, 95, 309), (4322, 90, 294), (4265, 85, 282),
        (4205, 80, 274),  (4145, 75, 270), (4086, 70, 267), (4027, 65, 269),
        (3964, 60, 243),  (3909, 55, 224), (3869, 49, 223), (3838, 44, 220),
        (3812, 39, 218),  (3792, 34, 222), (3780, 29, 214), (3765, 24, 268),
        (3742, 19, 327),  (3707, 14, 413), (3675, 9, 596),  (3601, 4, 1402),
        (3484, 0, 1845),
    ],
    // T4 = -10°C
    [
        (4444, 100, 748), (4323, 95, 695), (4237, 90, 637), (4166, 85, 605),
        (4103, 80, 587),  (4040, 75, 566), (3981, 70, 540), (3931, 65, 529),
        (3887, 60, 529),  (3850, 55, 539), (3821, 49, 563), (3804, 44, 610),
        (3792, 39, 705),  (3779, 34, 847), (3764, 29, 1004),(3748, 24, 1149),
        (3727, 19, 1265), (3700, 14, 1361),(3672, 9, 1519), (3401, 4, 1632),
        (3400, 0, 1629),
    ],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermistorFault {
    Ok,
    ShortToGnd,
    OpenCircuit,
    Unavailable,
}

/// Detailed read-only battery & fuel-gauge telemetry for X6826C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryTelemetry {
    pub available: bool,
    pub battery_present: Option<bool>,
    pub vbat_mv: u16,
    pub ocv_mv: u16,
    /// Signed instantaneous current from MT6358 FGADC in mA (`> 0` charging, `< 0` discharging).
    pub current_ma: Option<i32>,
    pub percent: u8,
    pub charger_connected: bool,
    pub charging_active: bool,
    pub full: bool,
    pub charger_fault: bool,
    pub temp_deci_c: i16,
    pub bat_temp_mv: Option<u16>,
    pub thermistor_state: ThermistorFault,
    pub vcdt_raw_mv: Option<u16>,
    pub vbif_mv: Option<u16>,
}

/// Converts raw 15-bit MT6358 BATADC sample (`3:1` divider, `1.8 V` ref) to millivolts.
pub const fn auxadc_batadc_to_mv(raw_15bit: u16) -> u16 {
    let adc = (raw_15bit & AUXADC_DATA_15BIT) as u32;
    ((adc * 5400) >> 15) as u16
}

/// Converts raw 12-bit MT6358 AUXADC sample (`num:den` ratio, `1.8 V` ref) to millivolts.
pub const fn auxadc_12bit_to_mv(raw_12bit: u16, ratio_num: u32, ratio_den: u32) -> u16 {
    let adc = (raw_12bit & AUXADC_DATA_12BIT) as u32;
    ((adc * 1800 * ratio_num) / (4096 * ratio_den)) as u16
}

/// Converts a 16-bit signed MT6358 `FG_CURRENT_OUT` raw value to signed milliamps
/// (`+` = charging, `-` = discharging) using X6826C's `R_FG_VALUE = 5` mOhm & `CAR_TUNE = 100`.
pub const fn decode_fg_current_ma(raw_u16: u16) -> i32 {
    let signed = raw_u16 as i16 as i64;
    if signed == 0 {
        return 0;
    }
    let mag = if signed < 0 { -signed } else { signed };
    // Base scaling from `lk_rafw.bin.c` L71105..71122:
    //   base_0p1ma = mag * UNIT_FGCURRENT / 100_000 (at 10 mOhm reference)
    //   adjusted for R_FG_VALUE (5 mOhm => 2x) and CAR_TUNE (100%):
    //   ma = mag * 381470 * (10 / R_FG_VALUE) * CAR_TUNE / (100_000 * 100 * 10)
    let ma = (mag * MT6358_UNIT_FGCURRENT * 10 * X6826C_CAR_TUNE)
        / (100_000 * X6826C_R_FG_MOHM * 100 * 10);
    if signed < 0 { -(ma as i32) } else { ma as i32 }
}

/// Classifies `BAT_TEMP` ADC voltage (`0..3600 mV` with 2:1 ratio on 1.8V pull-up)
/// for open-circuit or short-to-ground sensor faults.
pub const fn classify_thermistor_mv(mv: u16) -> ThermistorFault {
    if mv < 60 {
        ThermistorFault::ShortToGnd
    } else if mv > 1750 {
        ThermistorFault::OpenCircuit
    } else {
        ThermistorFault::Ok
    }
}

/// Generic NTC thermistor conversion (`R_ntc` -> tenths of °C) using a caller-supplied
/// calibrated `(temp_c, resistance_ohm)` descending-R lookup table and pull-up resistor.
///
/// Only invoked when verified board thermistor parameters are supplied.
pub fn ntc_mv_to_deci_c(
    v_adc_mv: u16,
    v_pullup_mv: u16,
    r_pullup_ohm: u32,
    table: &[(i16, u32)],
) -> Option<i16> {
    if table.len() < 2 || v_adc_mv == 0 || v_adc_mv >= v_pullup_mv || r_pullup_ohm == 0 {
        return None;
    }
    let num = (v_adc_mv as u64).checked_mul(r_pullup_ohm as u64)?;
    let den = (v_pullup_mv - v_adc_mv) as u64;
    let r_ntc = (num / den) as u32;

    if r_ntc >= table[0].1 {
        return Some(table[0].0.saturating_mul(10));
    }
    let last = table[table.len() - 1];
    if r_ntc <= last.1 {
        return Some(last.0.saturating_mul(10));
    }
    for w in table.windows(2) {
        let (t_lo, r_hi) = w[0];
        let (t_hi, r_lo) = w[1];
        if r_ntc <= r_hi && r_ntc >= r_lo && r_hi > r_lo {
            let span_r = (r_hi - r_lo) as i64;
            let span_t = ((t_hi - t_lo) as i64) * 10;
            let delta_r = (r_hi - r_ntc) as i64;
            let deci = (t_lo as i64) * 10 + (delta_r * span_t) / span_r;
            return Some(deci as i16);
        }
    }
    None
}

fn lookup_curve(curve: &[(u16, u8, u16); 21], ocv_mv: u16) -> (u8, u16) {
    if ocv_mv >= curve[0].0 {
        return (100, curve[0].2);
    }
    let last = curve[curve.len() - 1];
    if ocv_mv <= last.0 {
        return (0, last.2);
    }
    for w in curve.windows(2) {
        let (v_hi, p_hi, r_hi) = w[0];
        let (v_lo, p_lo, r_lo) = w[1];
        if ocv_mv <= v_hi && ocv_mv >= v_lo && v_hi > v_lo {
            let span_v = (v_hi - v_lo) as u32;
            let delta = (ocv_mv - v_lo) as u32;
            let p = p_lo as u32 + (delta * (p_hi - p_lo) as u32) / span_v;
            let r = ((r_lo as u32) * (span_v - delta) + (r_hi as u32) * delta) / span_v;
            return (p.min(100) as u8, r as u16);
        }
    }
    (0, last.2)
}

/// Temperature-aware OCV -> (SoC %, estimated R_bat mOhm) lookup across X6826C's 5 DTS profiles.
/// When `temp_deci_c == i16::MIN` (unavailable), uses the verified 25°C (`T1`) profile.
pub fn ocv_to_soc_and_r(ocv_mv: u16, temp_deci_c: i16) -> (u8, u16) {
    if temp_deci_c == i16::MIN {
        return lookup_curve(&X6826C_OCV_PROFILES[1], ocv_mv);
    }
    let temp_c = temp_deci_c / 10;
    if temp_c >= X6826C_OCV_TEMPS_C[0] {
        return lookup_curve(&X6826C_OCV_PROFILES[0], ocv_mv);
    }
    let last_idx = X6826C_OCV_TEMPS_C.len() - 1;
    if temp_c <= X6826C_OCV_TEMPS_C[last_idx] {
        return lookup_curve(&X6826C_OCV_PROFILES[last_idx], ocv_mv);
    }
    for i in 0..last_idx {
        let t_hi = X6826C_OCV_TEMPS_C[i];
        let t_lo = X6826C_OCV_TEMPS_C[i + 1];
        if temp_c <= t_hi && temp_c >= t_lo && t_hi > t_lo {
            let (p_hi, r_hi) = lookup_curve(&X6826C_OCV_PROFILES[i], ocv_mv);
            let (p_lo, r_lo) = lookup_curve(&X6826C_OCV_PROFILES[i + 1], ocv_mv);
            let span = (t_hi - t_lo) as i32;
            let w_hi = (temp_c - t_lo) as i32;
            let p = ((p_lo as i32) * (span - w_hi) + (p_hi as i32) * w_hi) / span;
            let r = ((r_lo as i32) * (span - w_hi) + (r_hi as i32) * w_hi) / span;
            return (p.clamp(0, 100) as u8, r.max(1) as u16);
        }
    }
    lookup_curve(&X6826C_OCV_PROFILES[1], ocv_mv)
}

/// Computes IR-drop compensated open-circuit voltage (`V_ocv = V_bat - I_bat * R_bat`)
/// clamped to plausible cell bounds.
pub fn compensate_ir_ocv(vbat_mv: u16, current_ma: Option<i32>, r_bat_mohm: u16) -> u16 {
    let Some(i_ma) = current_ma else {
        return vbat_mv;
    };
    // Guard against absurd current spikes distorting OCV by > 350 mV.
    let i_clamped = i_ma.clamp(-3500, 3500) as i64;
    let delta_mv = ((i_clamped * (r_bat_mohm as i64)) / 1000).clamp(-350, 350);
    let ocv = (vbat_mv as i64 - delta_mv).clamp(VBAT_MIN_VALID_MV as i64, VBAT_MAX_VALID_MV as i64);
    ocv as u16
}

fn read_auxadc_channel(channel: u8, reg: u32) -> Option<u16> {
    if !pwrap::is_ready() || channel > 15 {
        return None;
    }
    if !pwrap::write(MT6358_AUXADC_RQST0, 1u16 << channel) {
        return None;
    }
    let mut dl = Deadline::from_us(AUXADC_TIMEOUT_US);
    loop {
        let raw = pwrap::read(reg)?;
        if (raw & AUXADC_RDY_BIT) != 0 {
            return Some(raw);
        }
        if dl.expired() {
            return None;
        }
    }
}

fn read_vbat_mv() -> Option<u16> {
    let raw = read_auxadc_channel(0, MT6358_AUXADC_ADC0)?;
    let mv = auxadc_batadc_to_mv(raw);
    if (VBAT_MIN_VALID_MV..=VBAT_MAX_VALID_MV).contains(&mv) {
        Some(mv)
    } else {
        None
    }
}

fn read_vcdt_raw_mv() -> Option<u16> {
    let raw = read_auxadc_channel(2, MT6358_AUXADC_ADC2)?;
    Some(auxadc_12bit_to_mv(raw, 1, 1))
}

fn read_bat_temp_mv() -> Option<u16> {
    let raw = read_auxadc_channel(3, MT6358_AUXADC_ADC3)?;
    // DTS `bat_temp`: `resistance-ratio = <2 1>`
    Some(auxadc_12bit_to_mv(raw, 2, 1))
}

fn read_vbif_mv() -> Option<u16> {
    let raw = read_auxadc_channel(14, MT6358_AUXADC_ADC14)?;
    // DTS `vbif`: `resistance-ratio = <2 1>`
    Some(auxadc_12bit_to_mv(raw, 2, 1))
}

/// Latches and reads instantaneous battery current from MT6358 FGADC (`lk_rafw.bin.c` L71080–71124).
fn read_fg_current_ma() -> Option<i32> {
    if !pwrap::is_ready() {
        return None;
    }
    pwrap::update_field(MT6358_FGADC_CON1, 0x0f, 0, FG_SW_READ_PRE).ok()?;
    let mut dl = Deadline::from_us(FG_TIMEOUT_US);
    loop {
        let con1 = pwrap::read(MT6358_FGADC_CON1)?;
        if (con1 & FG_LATCHDATA_ST) != 0 {
            break;
        }
        if dl.expired() {
            let _ = pwrap::update_field(MT6358_FGADC_CON1, 0x0f, 0, 0);
            return None;
        }
    }
    let raw_cur = pwrap::read(MT6358_FGADC_CUR_CON0)?;
    let _ = pwrap::update_field(MT6358_FGADC_CON1, 0x0f, 0, FG_SW_CLEAR);
    let mut clear_dl = Deadline::from_us(FG_TIMEOUT_US);
    while !clear_dl.expired() {
        if let Some(con1) = pwrap::read(MT6358_FGADC_CON1) {
            if (con1 & FG_LATCHDATA_ST) == 0 {
                break;
            }
        }
    }
    let _ = pwrap::update_field(MT6358_FGADC_CON1, 0x0f, 0, 0);
    Some(decode_fg_current_ma(raw_cur))
}

pub fn read_telemetry() -> BatteryTelemetry {
    if !pwrap::is_ready() {
        return BatteryTelemetry {
            available: false,
            battery_present: None,
            vbat_mv: 0,
            ocv_mv: 0,
            current_ma: None,
            percent: 0,
            charger_connected: false,
            charging_active: false,
            full: false,
            charger_fault: false,
            temp_deci_c: i16::MIN,
            bat_temp_mv: None,
            thermistor_state: ThermistorFault::Unavailable,
            vcdt_raw_mv: None,
            vbif_mv: None,
        };
    }

    let Some(vbat_mv) = read_vbat_mv() else {
        return BatteryTelemetry {
            available: false,
            battery_present: None,
            vbat_mv: 0,
            ocv_mv: 0,
            current_ma: None,
            percent: 0,
            charger_connected: false,
            charging_active: false,
            full: false,
            charger_fault: false,
            temp_deci_c: i16::MIN,
            bat_temp_mv: None,
            thermistor_state: ThermistorFault::Unavailable,
            vcdt_raw_mv: None,
            vbif_mv: None,
        };
    };

    let current_ma = read_fg_current_ma();
    let bat_temp_mv = read_bat_temp_mv();
    let thermistor_state = match bat_temp_mv {
        Some(mv) => classify_thermistor_mv(mv),
        None => ThermistorFault::Unavailable,
    };
    let vcdt_raw_mv = read_vcdt_raw_mv();
    let vbif_mv = read_vbif_mv();
    // On MT6358, VBIF is pulled up to ~2.8V when open (no battery); < 2500 mV indicates battery present.
    let battery_present = vbif_mv.map(|mv| mv < 2500);

    // Do not fabricate battery temperature without verified X6826C NTC pull-up R & R-T curve.
    let temp_deci_c = i16::MIN;

    // Two-pass IR-drop compensated OCV -> SoC using X6826C 25°C (T1) profile.
    let (_rough_soc, est_r) = ocv_to_soc_and_r(vbat_mv, temp_deci_c);
    let ocv_mv = compensate_ir_ocv(vbat_mv, current_ma, est_r);
    let (percent, _) = ocv_to_soc_and_r(ocv_mv, temp_deci_c);

    // Separate physical charger attachment from active charging & charge termination!
    let usb_st = usb::read_status();
    let charger_connected = usb_st.vbus_present;
    let (charging_active, full, charger_fault) = if usb_st.charger.present {
        let state = usb_st.charger.ic_state;
        (
            state.is_actively_charging(),
            state.is_full(),
            state.is_fault() || usb_st.charger.fault_otp || usb_st.charger.fault_vac_ov,
        )
    } else {
        // Fallback when RT9471 is unreachable: require positive FGADC current (if available)
        // in addition to RGS_CHRDET so an attached/idle cable is not misreported as actively charging.
        let active = match current_ma {
            Some(i_ma) => charger_connected && i_ma > 20,
            None => charger_connected,
        };
        let is_full =
            charger_connected && percent >= 99 && current_ma.is_none_or(|i| i.abs() <= 200);
        (active, is_full, false)
    };

    BatteryTelemetry {
        available: true,
        battery_present,
        vbat_mv,
        ocv_mv,
        current_ma,
        percent: if full { 100 } else { percent },
        charger_connected,
        charging_active,
        full,
        charger_fault,
        temp_deci_c,
        bat_temp_mv,
        thermistor_state,
        vcdt_raw_mv,
        vbif_mv,
    }
}

/// NetizenCore `HardwareOps::battery` callback returning honest hardware `BatteryStatus`.
pub fn read_status() -> BatteryStatus {
    let t = read_telemetry();
    if !t.available {
        return BatteryStatus::UNAVAILABLE;
    }
    BatteryStatus {
        available: true,
        percent: t.percent,
        charging: t.charging_active,
        full: t.full,
        voltage_mv: t.vbat_mv,
        temp_deci_c: t.temp_deci_c,
    }
}

pub fn init(host: &mut dyn DriverHost) {
    let t = read_telemetry();
    let mut msg = LogBuf::<224>::new();
    if t.available {
        let cur = t.current_ma.unwrap_or(0);
        let _ = write!(
            msg,
            "mt6358-battery: {}% (Vbat={}mV OCV={}mV I={}mA chg_in={} active={} full={} fault={} ntc={:?})",
            t.percent,
            t.vbat_mv,
            t.ocv_mv,
            cur,
            t.charger_connected as u8,
            t.charging_active as u8,
            t.full as u8,
            t.charger_fault as u8,
            t.thermistor_state
        );
    } else {
        let _ = write!(
            msg,
            "mt6358-battery: MT6358 AUXADC/PMIC yanit vermedi (unavailable)"
        );
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auxadc_conversions() {
        // Half-scale 15-bit BATADC => 2700 mV; 4447 mV => raw ~26984
        assert_eq!(auxadc_batadc_to_mv(0x4000), 2700);
        assert_eq!(auxadc_batadc_to_mv(26984), 4446);
        // 12-bit 2:1 channel half-scale => 1800 mV
        assert_eq!(auxadc_12bit_to_mv(2048, 2, 1), 1800);
    }

    #[test]
    fn fgadc_signed_current_decoding() {
        assert_eq!(decode_fg_current_ma(0), 0);
        // Positive raw => charging (+mA)
        let chg = decode_fg_current_ma(1000);
        assert!((760..=765).contains(&chg), "got {chg}");
        // Two's complement negative => discharging (-mA)
        let dis = decode_fg_current_ma((-1000i16) as u16);
        assert_eq!(dis, -chg);
    }

    #[test]
    fn ocv_curves_and_ir_compensation() {
        // 25°C (T1) endpoints & interpolation
        assert_eq!(ocv_to_soc_and_r(4447, i16::MIN).0, 100);
        assert_eq!(ocv_to_soc_and_r(3006, i16::MIN).0, 0);
        let (mid_soc, mid_r) = ocv_to_soc_and_r(3882, i16::MIN);
        assert_eq!(mid_soc, 49);
        assert_eq!(mid_r, 116);

        // Temperature interpolation between 0°C and 25°C
        let (soc_cold, r_cold) = ocv_to_soc_and_r(3800, 0);
        let (soc_warm, r_warm) = ocv_to_soc_and_r(3800, 250);
        assert!(r_cold > r_warm);
        assert!(soc_cold <= 45 && soc_warm <= 45);

        // Under 1000 mA charging load with 120 mOhm internal resistance, OCV is 120 mV below Vbat
        assert_eq!(compensate_ir_ocv(4200, Some(1000), 120), 4080);
        // Under 1000 mA discharge load, OCV is 120 mV above Vbat
        assert_eq!(compensate_ir_ocv(3800, Some(-1000), 120), 3920);
    }

    #[test]
    fn ntc_table_and_fault_detection() {
        assert_eq!(classify_thermistor_mv(20), ThermistorFault::ShortToGnd);
        assert_eq!(classify_thermistor_mv(1790), ThermistorFault::OpenCircuit);
        assert_eq!(classify_thermistor_mv(900), ThermistorFault::Ok);

        // Simple synthetic 10k NTC curve for unit testing interpolation
        let table = [(0, 32650), (25, 10000), (50, 3603)];
        // Half of Vpullup with 10k pullup => R_ntc == 10k => 25.0°C (250 deci-C)
        assert_eq!(ntc_mv_to_deci_c(900, 1800, 10_000, &table), Some(250));
        assert_eq!(ntc_mv_to_deci_c(0, 1800, 10_000, &table), None);
        assert_eq!(ntc_mv_to_deci_c(1800, 1800, 10_000, &table), None);
    }
}
