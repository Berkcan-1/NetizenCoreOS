//! MediaTek MT6768 TOPRGU Watchdog & Reboot-Mode driver (`toprgu@10007000`) for X6826C.
//!
//! Verified against X6826C `stock/dtb` (`/toprgu@10007000`, GIC INTID `188` [SPI `0x9c`],
//! `reboot-mode` subnode at offset `0x24`, mask `0x0f`).

use core::fmt::Write;
use nc_device::DriverHost;

use crate::mmio::{dsb, rd32, wr32, LogBuf};

pub const TOPRGU_BASE: usize = 0x1000_7000;
const TOPRGU_MODE: usize = 0x00;
const TOPRGU_LENGTH: usize = 0x04;
const TOPRGU_RESTART: usize = 0x08;
const TOPRGU_STATUS: usize = 0x0c;
const TOPRGU_SWRST: usize = 0x14;
/// `reboot-mode` register verified in `stock/dtb` `/toprgu@10007000/reboot-mode` (`offset = 0x24`).
const TOPRGU_REBOOT_MODE: usize = 0x24;

const TOPRGU_MODE_KEY: u32 = 0x2200_0000;
const TOPRGU_MODE_EN: u32 = 1 << 0;
const TOPRGU_RESTART_KEY: u32 = 0x1971;
const TOPRGU_SWRST_KEY: u32 = 0x1209;

pub const REBOOT_MODE_NORMAL: u32 = 0x00;
pub const REBOOT_MODE_CHARGER: u32 = 0x01;
pub const REBOOT_MODE_RECOVERY: u32 = 0x02;
pub const REBOOT_MODE_BOOTLOADER: u32 = 0x03;

/// Disarms the LK watchdog timer during early kernel bring-up while preserving
/// non-enable configuration flags in `TOPRGU_MODE`.
pub fn disable() {
    let cur = rd32(TOPRGU_BASE + TOPRGU_MODE);
    if cur == 0xffff_ffff {
        return;
    }
    wr32(TOPRGU_BASE + TOPRGU_RESTART, TOPRGU_RESTART_KEY);
    wr32(
        TOPRGU_BASE + TOPRGU_MODE,
        TOPRGU_MODE_KEY | (cur & 0x00ff_ffff & !TOPRGU_MODE_EN),
    );
    dsb();
}

pub fn kick() {
    wr32(TOPRGU_BASE + TOPRGU_RESTART, TOPRGU_RESTART_KEY);
    dsb();
}

pub fn set_reboot_mode(mode: u32) {
    let cur = rd32(TOPRGU_BASE + TOPRGU_REBOOT_MODE);
    if cur == 0xffff_ffff {
        return;
    }
    wr32(
        TOPRGU_BASE + TOPRGU_REBOOT_MODE,
        (cur & !0x0f) | (mode & 0x0f),
    );
    dsb();
}

pub fn warm_reset() -> ! {
    wr32(TOPRGU_BASE + TOPRGU_SWRST, TOPRGU_SWRST_KEY);
    dsb();
    loop {
        core::hint::spin_loop();
    }
}

pub fn init(host: &mut dyn DriverHost) {
    let before_mode = rd32(TOPRGU_BASE + TOPRGU_MODE);
    if before_mode == 0xffff_ffff {
        host.log("mtk-toprgu-wdt: denetleyici erisilemez");
        return;
    }
    let before_status = rd32(TOPRGU_BASE + TOPRGU_STATUS);
    let length = rd32(TOPRGU_BASE + TOPRGU_LENGTH);
    let reboot_mode = rd32(TOPRGU_BASE + TOPRGU_REBOOT_MODE) & 0x0f;
    disable();
    let after_mode = rd32(TOPRGU_BASE + TOPRGU_MODE);
    let mut msg = LogBuf::<192>::new();
    let _ = write!(
        msg,
        "mtk-toprgu-wdt: durduruldu (mode {:#x}->{:#x}, status {:#x}, len {:#x}, rmode {:#x})",
        before_mode, after_mode, before_status, length, reboot_mode
    );
    host.log(msg.as_str());
}
