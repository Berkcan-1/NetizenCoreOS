//! MediaTek MT6768 SCP ("tinysys" Cortex-M4 Sensor Hub) diagnostic & wake-lock driver
//! (`scp@10500000`, GIC INTID `237` [SPI `0xcd`]) for Infinix Hot 20 (X6826C).
//!
//! Verified against X6826C `stock/dtb` (`/scp@10500000`: SRAM `0x1050_0000`,
//! CFG `0x105c_0000`, CLKCTRL `0x105c_4000`, `scp_sramSize = 0x80000`) and
//! HakiOS SoC-level `redwood_scp.c`.
//!
//! Safety & Honesty Policy:
//! - Read-only status inspection at boot; does NOT fabricate sensor readings.

use core::fmt::Write;
use nc_device::DriverHost;

use crate::mmio::{delay_us, dsb, rd32, wr32, Deadline, LogBuf};

pub const SCP_SRAM_BASE: usize = 0x1050_0000;
pub const SCP_CFG_BASE: usize = 0x105c_0000;
pub const SCP_CLKCTRL_BASE: usize = 0x105c_4000;
const INFRACFG_BASE: usize = 0x1000_1000;

const MODULE_RESET_STATUS: usize = 0x148;
const SCP_RESET_BIT: u32 = 1 << 3;
const INFRA_IRQ_SET: usize = 0xb14;
const INFRA_IRQ_CLEAR: usize = 0xb18;

const SCP_A_TO_HOST_REG: usize = 0x01c;
const SCP_GIPC_IN_REG: usize = 0x028;
const SCP_A_DEBUG_LR_REG: usize = 0x0ac;
const SCP_A_DEBUG_PC_REG: usize = 0x0b4;
const SCP_SLEEP_STATUS_REG: usize = 0x114;
const SCP_SRAM_PDN: usize = 0x02c;

pub const SCP_SEND_OBJ: usize = 0x06e0;
pub const SCP_RECV_OBJ: usize = 0x05c0;
pub const IPI_SENSOR: u32 = 8;

pub fn is_out_of_reset() -> bool {
    let rst = rd32(INFRACFG_BASE + MODULE_RESET_STATUS);
    rst != 0xffff_ffff && (rst & SCP_RESET_BIT) == 0
}

pub fn awake_lock(lock: bool) -> bool {
    if !is_out_of_reset() {
        return false;
    }
    let which = if lock { 0u32 } else { 1u32 };
    wr32(INFRACFG_BASE + INFRA_IRQ_SET, 0xa0 | (1u32 << which));
    dsb();
    let mut taken = false;
    let mut dl = Deadline::from_ms(100);
    while !dl.expired() {
        let v = rd32(INFRACFG_BASE + INFRA_IRQ_SET);
        if (v & 0xf0) != 0xa0 || (v & (1u32 << which)) == 0 {
            taken = (v & 0xf0) == 0xa0;
            break;
        }
        delay_us(10);
    }
    let cur = rd32(INFRACFG_BASE + INFRA_IRQ_SET);
    wr32(INFRACFG_BASE + INFRA_IRQ_CLEAR, cur);
    dsb();
    taken
}

pub fn trigger_gipc() {
    if !is_out_of_reset() {
        return;
    }
    wr32(SCP_CFG_BASE + SCP_GIPC_IN_REG, 1);
    dsb();
}

pub fn init(host: &mut dyn DriverHost) {
    let rst = rd32(INFRACFG_BASE + MODULE_RESET_STATUS);
    if rst == 0xffff_ffff {
        host.log("mtk-scp-sensorhub: INFRACFG/SCP erisilemez");
        return;
    }
    let out_of_reset = (rst & SCP_RESET_BIT) == 0;
    let pc0 = rd32(SCP_CFG_BASE + SCP_A_DEBUG_PC_REG);
    delay_us(200);
    let pc1 = rd32(SCP_CFG_BASE + SCP_A_DEBUG_PC_REG);
    let lr = rd32(SCP_CFG_BASE + SCP_A_DEBUG_LR_REG);
    let sleep = rd32(SCP_CFG_BASE + SCP_SLEEP_STATUS_REG);
    let to_host = rd32(SCP_CFG_BASE + SCP_A_TO_HOST_REG);
    let sram_pdn = rd32(SCP_CLKCTRL_BASE + SCP_SRAM_PDN);

    let mut msg = LogBuf::<224>::new();
    let _ = write!(
        msg,
        "mtk-scp-sensorhub: Cortex-M4 reset={} aktif={} (PC {:#x}->{:#x} LR={:#x}, uyku={:#x}, host={:#x}, sram_pdn={:#x})",
        if out_of_reset { "acik" } else { "tutulu" },
        if out_of_reset && pc0 != pc1 { "evet" } else { "hayir" },
        pc0,
        pc1,
        lr,
        sleep,
        to_host,
        sram_pdn
    );
    host.log(msg.as_str());
}
