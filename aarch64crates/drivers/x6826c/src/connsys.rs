//! MediaTek MT6768 CONNSYS (MT6631 Wi-Fi/BT/GPS) & BTIF read-only power/status probe
//! (`consys@18002000`, GIC INTID `316` [SPI `0x11c`]; `btif@1100c000`, GIC INTID `165`).
//!
//! Safety & Honesty Policy:
//! - Strictly READ-ONLY. Never reads `0x1800_2000` or `0x180b_1000` unless SPM confirms
//!   BOTH `CONN` (bit 1) and `CONN_TOP2` (bit 30) power domains are active (preventing a
//!   bus hang on powered-off CONNSYS).
//! - Does NOT claim functional Wi-Fi or Bluetooth without vendor firmware & WMT/STP bring-up.

use core::fmt::Write;
use nc_device::DriverHost;

use crate::mmio::{rd32, LogBuf};
use crate::pwrap::{
    self, MT6358_VCN18_CON0, MT6358_VCN28_CON0, MT6358_VCN33_BT_CON0, MT6358_VCN33_WIFI_CON0,
};

const SPM_BASE: usize = 0x1000_6000;
const SPM_PWR_STATUS: usize = 0x180;
const SPM_PWR_STATUS_2ND: usize = 0x184;
const PWR_STATUS_CONN: u32 = 1 << 1;
const PWR_STATUS_CONN_TOP2: u32 = 1 << 30;

pub const MCU_BASE: usize = 0x1800_2000;
pub const MCU_TOP_MISC_OFF: usize = 0x180b_1000;
pub const BTIF_BASE: usize = 0x1100_c000;

const CONSYS_HW_ID: usize = 0x00;
const CONSYS_FW_ID: usize = 0x04;
const CONSYS_IP_VER: usize = 0x10;
const BTIF_LSR: usize = 0x14;

pub fn is_powered() -> bool {
    let st1 = rd32(SPM_BASE + SPM_PWR_STATUS);
    let st2 = rd32(SPM_BASE + SPM_PWR_STATUS_2ND);
    if st1 == 0xffff_ffff || st2 == 0xffff_ffff {
        return false;
    }
    let mask = PWR_STATUS_CONN | PWR_STATUS_CONN_TOP2;
    (st1 & mask) == mask && (st2 & mask) == mask
}

pub fn init(host: &mut dyn DriverHost) {
    let st1 = rd32(SPM_BASE + SPM_PWR_STATUS);
    if st1 == 0xffff_ffff {
        host.log("mtk-consys-mt6631: SPM/CONNSYS erisilemez");
        return;
    }
    let (vcn18, vcn28, vcn33_bt, vcn33_wifi) = if pwrap::is_ready() {
        (
            (pwrap::read(MT6358_VCN18_CON0).unwrap_or(0) & 1) != 0,
            (pwrap::read(MT6358_VCN28_CON0).unwrap_or(0) & 1) != 0,
            (pwrap::read(MT6358_VCN33_BT_CON0).unwrap_or(0) & 1) != 0,
            (pwrap::read(MT6358_VCN33_WIFI_CON0).unwrap_or(0) & 1) != 0,
        )
    } else {
        (false, false, false, false)
    };
    let btif_lsr = rd32(BTIF_BASE + BTIF_LSR) & 0xff;

    let mut msg = LogBuf::<224>::new();
    if is_powered() {
        let ip_ver = rd32(MCU_TOP_MISC_OFF + CONSYS_IP_VER);
        let hw_id = rd32(MCU_BASE + CONSYS_HW_ID);
        let fw_id = rd32(MCU_BASE + CONSYS_FW_ID);
        let _ = write!(
            msg,
            "mtk-consys-mt6631: CONNSYS acik (IP={:#x}, HW={:#x}, FW={:#x}, BTIF_LSR={:#x}; Wi-Fi/BT FW yuklu degil)",
            ip_ver, hw_id, fw_id, btif_lsr
        );
    } else {
        let _ = write!(
            msg,
            "mtk-consys-mt6631: CONNSYS kapali/beklemede (SPM={:#x}, VCN18={} VCN28={} BT={} WIFI={}, BTIF={:#x})",
            st1,
            vcn18 as u8,
            vcn28 as u8,
            vcn33_bt as u8,
            vcn33_wifi as u8,
            btif_lsr
        );
    }
    host.log(msg.as_str());
}
