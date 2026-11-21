//! MT6768 Display Backlight PWM driver (`disp_pwm@1100e000`) for Infinix Hot 20 (X6826C).
//!
//! Verified against X6826C `stock/dtb` + `stock/dtbo`:
//! - `/disp_pwm@1100e000`: `compatible = "mediatek,disp_pwm"`, reg `0x1100_e000` (`0x1000`)
//! - `/odm/led@6`: `compatible = "mediatek,lcd-backlight"`, `led_mode = <0x05>`
//!   (`MT65XX_LED_MODE_CUST_BLS_PWM`), `pwm_config = <0x01 0x00 0x00 0x00 0x00>`
//!
//! Preserves LK's `CON0` clock divider and `CON1` period (`bits [12:0]`) without
//! disabling or flashing the backlight during kernel initialization.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use nc_device::DriverHost;

use crate::mmio::{dsb, rd32, wr32, LogBuf};

pub const DISP_PWM0_BASE: usize = 0x1100_e000;
const DISP_PWM_EN: usize = 0x00;
const DISP_PWM_COMMIT: usize = 0x08;
const DISP_PWM_CON_0: usize = 0x10;
const DISP_PWM_CON_1: usize = 0x14;

const PERIOD_MASK: u32 = 0x1fff;
const DEFAULT_PERIOD: u32 = 1023;
const MIN_NONZERO_DUTY: u32 = 32;

static BACKLIGHT_READY: AtomicBool = AtomicBool::new(false);
static LAST_PERCENT: AtomicU8 = AtomicU8::new(100);

/// Computes `(duty, period, con1)` for a requested `0..=100` percentage while preserving
/// the hardware PWM period from `CON1`.
pub const fn compute_pwm_con1(percent: u8, existing_con1: u32) -> (u32, u32, u32) {
    let raw_period = existing_con1 & PERIOD_MASK;
    let period = if raw_period >= 255 {
        raw_period
    } else {
        DEFAULT_PERIOD
    };
    let clamped_pct = if percent > 100 { 100 } else { percent } as u32;
    let duty = if clamped_pct == 0 {
        0
    } else {
        let scaled = (clamped_pct * period) / 100;
        if scaled < MIN_NONZERO_DUTY {
            MIN_NONZERO_DUTY
        } else if scaled > period {
            period
        } else {
            scaled
        }
    };
    let next_con1 = (duty << 16) | period;
    (duty, period, next_con1)
}

pub fn current_percent() -> u8 {
    LAST_PERCENT.load(Ordering::Acquire)
}

pub fn set_brightness(percent: u8) -> bool {
    if !BACKLIGHT_READY.load(Ordering::Acquire) {
        let probe = rd32(DISP_PWM0_BASE + DISP_PWM_CON_1);
        if probe == 0xffff_ffff {
            return false;
        }
    }
    let con1 = rd32(DISP_PWM0_BASE + DISP_PWM_CON_1);
    if con1 == 0xffff_ffff {
        return false;
    }
    let clamped = percent.min(100);
    // Keep a safe visible floor (10%) for non-zero brightness requests from UI sliders.
    let effective = if clamped == 0 { 0 } else { clamped.max(10) };
    let (duty, _period, next_con1) = compute_pwm_con1(effective, con1);

    wr32(DISP_PWM0_BASE + DISP_PWM_CON_1, next_con1);
    wr32(DISP_PWM0_BASE + DISP_PWM_EN, if duty == 0 { 0 } else { 1 });
    wr32(DISP_PWM0_BASE + DISP_PWM_COMMIT, 1);
    dsb();

    LAST_PERCENT.store(effective, Ordering::Release);
    true
}

pub fn init(host: &mut dyn DriverHost) {
    let en = rd32(DISP_PWM0_BASE + DISP_PWM_EN);
    let con0 = rd32(DISP_PWM0_BASE + DISP_PWM_CON_0);
    let con1 = rd32(DISP_PWM0_BASE + DISP_PWM_CON_1);
    let mut msg = LogBuf::<192>::new();
    if con1 == 0xffff_ffff || en == 0xffff_ffff {
        BACKLIGHT_READY.store(false, Ordering::Release);
        let _ = write!(msg, "mtk-disp-pwm0: denetleyici erisilemez");
        host.log(msg.as_str());
        return;
    }

    let period = (con1 & PERIOD_MASK).max(1);
    let duty = (con1 >> 16) & PERIOD_MASK;
    let init_pct = ((duty * 100) / period).min(100) as u8;
    LAST_PERCENT.store(init_pct, Ordering::Release);
    BACKLIGHT_READY.store(true, Ordering::Release);

    let _ = write!(
        msg,
        "mtk-disp-pwm0: hazir (EN={:#x} CON0={:#x} duty={}/{} ~{}%, LK parlakligi korundu)",
        en, con0, duty, period, init_pct
    );
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwm_duty_scaling_and_clamping() {
        let (d0, p0, _) = compute_pwm_con1(0, 1023);
        assert_eq!(d0, 0);
        assert_eq!(p0, 1023);

        let (d10, p10, _) = compute_pwm_con1(10, 1023);
        assert_eq!(p10, 1023);
        assert_eq!(d10, 102);

        let (d100, p100, con1) = compute_pwm_con1(100, 2047);
        assert_eq!(p100, 2047);
        assert_eq!(d100, 2047);
        assert_eq!(con1, (2047 << 16) | 2047);

        // Over-100 input safely clamps to 100%
        let (d200, _, _) = compute_pwm_con1(200, 1023);
        assert_eq!(d200, 1023);

        // Tiny non-zero input uses safe visible floor
        let (d1, _, _) = compute_pwm_con1(1, 1023);
        assert_eq!(d1, MIN_NONZERO_DUTY);
    }
}
