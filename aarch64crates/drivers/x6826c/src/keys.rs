//! MT6768 Keypad (`kp@10010000`) & MT6358 Power Key driver for Infinix Hot 20 (X6826C).
//!
//! Verified against X6826C `stock/dtb` + `stock/dtbo` (`/kp@10010000`, GIC INTID `105` [SPI 0x49])
//! and `lk_rafw.bin.c` L26252–26282:
//! - `KP_MEM1` (`0x1001_0004`): active-low bit 0 = Volume Up (`0x73`), bit 1 = Volume Down (`0x72`)
//! - `MT6358_TOPSTATUS` (`0x0028`): active-low bit 1 (`PWRKEY_DEB`) = Power Key (`0x74`)

use core::fmt::Write;
use core::ptr::addr_of_mut;
use nc_device::DriverHost;
use nc_hal::{InputDevice, KeyCode, MouseState};

use crate::mmio::{rd32, LogBuf};
use crate::pwrap::{self, MT6358_TOPSTATUS, PWRKEY_DEB};

pub const KPD_BASE: usize = 0x1001_0000;
const KP_MEM1: usize = 0x0004;

pub struct KeysDevice {
    kpd_ok: bool,
    pmic_ok: bool,
    seen: [bool; 3],
    down: [bool; 3],
    tick: u8,
    last_top: u16,
}

impl KeysDevice {
    pub const fn new() -> Self {
        Self {
            kpd_ok: false,
            pmic_ok: false,
            seen: [false; 3],
            down: [false; 3],
            tick: 0,
            last_top: PWRKEY_DEB,
        }
    }

    /// Two-sample debounce step over `[vol_up, vol_down, pwrkey]`.
    pub fn step_debounce(&mut self, now: [bool; 3]) -> Option<KeyCode> {
        const CODES: [KeyCode; 3] = [KeyCode::Up, KeyCode::Down, KeyCode::Escape];
        let mut emitted = None;
        for i in 0..3 {
            if now[i] == self.seen[i] && now[i] != self.down[i] {
                self.down[i] = now[i];
                if now[i] && emitted.is_none() {
                    emitted = Some(CODES[i]);
                }
            }
            self.seen[i] = now[i];
        }
        emitted
    }
}

impl Default for KeysDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDevice for KeysDevice {
    fn name(&self) -> &'static str {
        "mtk-kpd-keys"
    }

    fn poll(&mut self, _width: i32, _height: i32) -> (MouseState, Option<KeyCode>) {
        let empty_mouse = MouseState {
            x: 0,
            y: 0,
            left_down: false,
            left_clicked: false,
            right_down: false,
            right_clicked: false,
        };
        if !self.kpd_ok && !self.pmic_ok {
            return (empty_mouse, None);
        }

        let (vol_up, vol_down) = if self.kpd_ok {
            let mem = rd32(KPD_BASE + KP_MEM1);
            if mem != 0xffff_ffff {
                ((mem & (1 << 0)) == 0, (mem & (1 << 1)) == 0)
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };

        if self.pmic_ok {
            self.tick = self.tick.wrapping_add(1);
            if (self.tick & 1) == 0 {
                if let Some(top) = pwrap::read(MT6358_TOPSTATUS) {
                    self.last_top = top;
                }
            }
        }
        let pwr = self.pmic_ok && (self.last_top & PWRKEY_DEB) == 0;
        let emitted = self.step_debounce([vol_up, vol_down, pwr]);
        (empty_mouse, emitted)
    }
}

static mut KEYS_DEV: KeysDevice = KeysDevice::new();

pub fn init(host: &mut dyn DriverHost) {
    let mem1 = rd32(KPD_BASE + KP_MEM1);
    let kpd_ok = mem1 != 0 && mem1 != 0xffff_ffff;
    let pwrap_top = if pwrap::is_ready() {
        pwrap::read(MT6358_TOPSTATUS)
    } else {
        None
    };
    let pmic_ok = pwrap_top.is_some();

    let mut msg = LogBuf::<192>::new();
    if kpd_ok || pmic_ok {
        unsafe {
            let dev = &mut *addr_of_mut!(KEYS_DEV);
            dev.kpd_ok = kpd_ok;
            dev.pmic_ok = pmic_ok;
            if let Some(top) = pwrap_top {
                dev.last_top = top;
            }
            host.add_input_device(dev);
        }
        let _ = write!(
            msg,
            "mtk-kpd-keys: hazir (kpd={} KP_MEM1={:#06x}, pmic_pwr={} TOPSTATUS={:#06x})",
            kpd_ok as u8,
            mem1 & 0xffff,
            pmic_ok as u8,
            pwrap_top.unwrap_or(0)
        );
    } else {
        let _ = write!(
            msg,
            "mtk-kpd-keys: tus takimi ve PMIC erisilemez (kayit yapilmadi)"
        );
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sample_debounce_filters_single_glitch() {
        let mut dev = KeysDevice::new();
        // Single-sample bounce ignored
        assert_eq!(dev.step_debounce([true, false, false]), None);
        assert_eq!(dev.step_debounce([false, false, false]), None);
        // Two consecutive presses emit KeyCode::Up once
        assert_eq!(dev.step_debounce([true, false, false]), None);
        assert_eq!(dev.step_debounce([true, false, false]), Some(KeyCode::Up));
        // Held key does not re-emit
        assert_eq!(dev.step_debounce([true, false, false]), None);
        // Release requires two samples then allows next press
        assert_eq!(dev.step_debounce([false, false, false]), None);
        assert_eq!(dev.step_debounce([false, false, false]), None);
        assert_eq!(dev.step_debounce([false, false, true]), None);
        assert_eq!(dev.step_debounce([false, false, true]), Some(KeyCode::Escape));
    }
}
