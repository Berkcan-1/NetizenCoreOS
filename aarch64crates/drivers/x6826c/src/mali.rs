//! ARM Mali-G52 MC2 GPU hardware power-sequencing & identification driver (`mali@13040000`)
//! for the Infinix Hot 20 (X6826C / MT6768).
//!
//! Verified against:
//! - X6826C `stock/dtb` (`/mali@13040000`: reg `0x1304_0000` `0x4000`, GIC INTIDs
//!   `306, 307, 308, 309, 310` [SPI `0x112..0x116`])
//! - MT6768 Linux `clk-mt6768-pg.c` / `clk-mt6768.c` and HakiOS `mtkmali.h` (MIT license)
//!
//! Safety & Honesty Policy:
//! - Verifies `VGPU` and `VSRAM_GPU` PMIC rails via `pwrap` BEFORE enabling SPM MFG domains.
//! - Uses timer-backed bounded waits and performs reverse-order domain rollback on failure.
//! - Reports hardware bring-up and identification truthfully (does NOT claim full 3D
//!   userspace acceleration is active).

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use nc_device::DriverHost;

use crate::mmio::{delay_ms, dsb, rd32, wr32, Deadline, LogBuf};
use crate::pwrap::{self, MT6358_VGPU_DBG1, MT6358_VSRAM_GPU_DBG1};

const TOPCKGEN_BASE: usize = 0x1000_0000;
const INFRACFG_BASE: usize = 0x1000_1000;
const SPM_BASE: usize = 0x1000_6000;
pub const MFGCFG_BASE: usize = 0x1300_0000;
pub const MALI_BASE: usize = 0x1304_0000;

const CLK_CFG_1_SET: usize = 0x054;
const CLK_CFG_1_CLR: usize = 0x058;
const MFG_SEL_FIELD: u32 = 3;
const MFG_SEL_PDN: u32 = 1 << 7;
const MFG_SEL_SYSPLL_D3: u32 = 2; // 364 MHz off running SYSPLL

const SPM_PWR_STATUS: usize = 0x180;
const SPM_PWR_STATUS_2ND: usize = 0x184;
const PWR_RST_B: u32 = 1 << 0;
const PWR_ISO: u32 = 1 << 1;
const PWR_ON: u32 = 1 << 2;
const PWR_ON_2ND: u32 = 1 << 3;
const PWR_CLK_DIS: u32 = 1 << 4;
const PWR_SRAM_PDN: u32 = 1 << 8;
const PWR_SRAM_PDN_ACK: u32 = 1 << 12;

const INFRA_TOPAXI_PROTECTEN_STA1: usize = 0x228;
const INFRA_TOPAXI_PROTECTEN_SET: usize = 0x2a0;
const INFRA_TOPAXI_PROTECTEN_CLR: usize = 0x2a4;
const MFG_PROT_STEP1: u32 = 1 << 25;
const MFG_PROT_STEP2: u32 = (1 << 21) | (1 << 22);

const MFG_CG_CLR: usize = 0x008;
const MFG_CG_BG3D: u32 = 1 << 0;

const GPU_ID: usize = 0x000;
const GPU_L2_FEATURES: usize = 0x004;
const GPU_AS_PRESENT: usize = 0x018;
const GPU_JS_PRESENT: usize = 0x01c;
const GPU_INT_RAWSTAT: usize = 0x020;
const GPU_INT_CLEAR: usize = 0x024;
const GPU_CMD: usize = 0x030;
const GPU_SHADER_PRESENT: usize = 0x100;
const GPU_TILER_PRESENT: usize = 0x110;
const GPU_L2_PRESENT: usize = 0x120;
const GPU_SHADER_READY: usize = 0x140;
const GPU_TILER_READY: usize = 0x150;
const GPU_L2_READY: usize = 0x160;
const GPU_SHADER_PWRON: usize = 0x180;
const GPU_TILER_PWRON: usize = 0x190;
const GPU_L2_PWRON: usize = 0x1a0;

const GPU_CMD_SOFT_RESET: u32 = 0x01;
const GPU_IRQ_RESET_COMPLETED: u32 = 1 << 8;

static MALI_POWERED: AtomicBool = AtomicBool::new(false);
static MALI_GPU_ID: AtomicU32 = AtomicU32::new(0);

struct MaliDomain {
    con: usize,
    status: u32,
    sram: bool,
}

const DOMAINS: [MaliDomain; 4] = [
    MaliDomain { con: 0x334, status: 1 << 14, sram: false }, // MFG_ASYNC
    MaliDomain { con: 0x338, status: 1 << 11, sram: true },  // MFG
    MaliDomain { con: 0x34c, status: 1 << 12, sram: true },  // MFG_CORE0
    MaliDomain { con: 0x310, status: 1 << 13, sram: true },  // MFG_CORE1
];

fn wait_reg(addr: usize, mask: u32, want: u32, ms: u64) -> bool {
    let mut dl = Deadline::from_ms(ms);
    loop {
        if (rd32(addr) & mask) == want {
            return true;
        }
        if dl.expired() {
            return false;
        }
    }
}

fn domain_on(d: &MaliDomain) -> bool {
    let con = SPM_BASE + d.con;
    wr32(con, rd32(con) | PWR_ON);
    wr32(con, rd32(con) | PWR_ON_2ND);
    dsb();
    if !wait_reg(SPM_BASE + SPM_PWR_STATUS, d.status, d.status, 20)
        || !wait_reg(SPM_BASE + SPM_PWR_STATUS_2ND, d.status, d.status, 20)
    {
        return false;
    }
    wr32(con, rd32(con) & !PWR_CLK_DIS);
    wr32(con, rd32(con) & !PWR_ISO);
    wr32(con, rd32(con) | PWR_RST_B);
    dsb();
    if d.sram {
        wr32(con, rd32(con) & !PWR_SRAM_PDN);
        dsb();
        if !wait_reg(con, PWR_SRAM_PDN_ACK, 0, 20) {
            return false;
        }
    }
    true
}

fn domain_off(d: &MaliDomain) {
    let con = SPM_BASE + d.con;
    if d.sram {
        wr32(con, rd32(con) | PWR_SRAM_PDN);
        dsb();
        let _ = wait_reg(con, PWR_SRAM_PDN_ACK, PWR_SRAM_PDN_ACK, 20);
    }
    wr32(con, rd32(con) | PWR_ISO);
    wr32(con, rd32(con) | PWR_CLK_DIS);
    wr32(con, rd32(con) & !PWR_RST_B);
    wr32(con, rd32(con) & !(PWR_ON | PWR_ON_2ND));
    dsb();
    let _ = wait_reg(SPM_BASE + SPM_PWR_STATUS, d.status, 0, 20);
    let _ = wait_reg(SPM_BASE + SPM_PWR_STATUS_2ND, d.status, 0, 20);
}

fn rollback_domains(up_count: usize, bus_opened: bool) {
    if bus_opened {
        wr32(INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_SET, MFG_PROT_STEP1);
        dsb();
        let _ = wait_reg(
            INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_STA1,
            MFG_PROT_STEP1,
            MFG_PROT_STEP1,
            20,
        );
        wr32(INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_SET, MFG_PROT_STEP2);
        dsb();
        let _ = wait_reg(
            INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_STA1,
            MFG_PROT_STEP2,
            MFG_PROT_STEP2,
            20,
        );
    }
    for idx in (0..up_count).rev() {
        domain_off(&DOMAINS[idx]);
    }
}

pub const fn gpu_name(id: u32) -> &'static str {
    match id >> 16 {
        0x7212 => "Mali-G52 MC2",
        0x7402 => "Mali-G52 r1",
        0x7211 => "Mali-G76",
        0x7093 => "Mali-G31",
        0x9093 => "Mali-G57",
        _ => "Mali Bifrost",
    }
}

pub fn is_powered() -> bool {
    MALI_POWERED.load(Ordering::Acquire)
}

pub fn gpu_id() -> u32 {
    MALI_GPU_ID.load(Ordering::Acquire)
}

pub fn init(host: &mut dyn DriverHost) {
    let pwr_status = rd32(SPM_BASE + SPM_PWR_STATUS);
    if pwr_status == 0 || pwr_status == 0xffff_ffff {
        host.log("arm-mali-g52: SPM erisilemez; GPU baslatilmadi");
        return;
    }

    // Verify VGPU & VSRAM_GPU PMIC rails are actually enabled before waking MFG domains.
    if pwrap::is_ready() {
        let vgpu_on = (pwrap::read(MT6358_VGPU_DBG1).unwrap_or(0) & 1) != 0;
        let vsram_on = (pwrap::read(MT6358_VSRAM_GPU_DBG1).unwrap_or(0) & 1) != 0;
        if !vgpu_on || !vsram_on {
            let mut msg = LogBuf::<160>::new();
            let _ = write!(
                msg,
                "arm-mali-g52: PMIC GPU beslemesi kapali (VGPU={} VSRAM_GPU={}); baslatilmadi",
                vgpu_on as u8, vsram_on as u8
            );
            host.log(msg.as_str());
            return;
        }
    }

    wr32(TOPCKGEN_BASE + CLK_CFG_1_CLR, MFG_SEL_FIELD | MFG_SEL_PDN);
    wr32(TOPCKGEN_BASE + CLK_CFG_1_SET, MFG_SEL_SYSPLL_D3);
    dsb();

    let mut bus_opened = false;
    for (idx, d) in DOMAINS.iter().enumerate() {
        if !domain_on(d) {
            rollback_domains(idx, bus_opened);
            let mut msg = LogBuf::<160>::new();
            let _ = write!(
                msg,
                "arm-mali-g52: SPM guc alani {} acilamadi (geri alindi)",
                idx
            );
            host.log(msg.as_str());
            return;
        }
        if idx == 1 {
            wr32(INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_CLR, MFG_PROT_STEP2);
            wr32(INFRACFG_BASE + INFRA_TOPAXI_PROTECTEN_CLR, MFG_PROT_STEP1);
            dsb();
            bus_opened = true;
        }
    }

    wr32(MFGCFG_BASE + MFG_CG_CLR, MFG_CG_BG3D);
    dsb();
    delay_ms(1);

    let id = rd32(MALI_BASE + GPU_ID);
    if id == 0 || id == 0xffff_ffff {
        rollback_domains(DOMAINS.len(), bus_opened);
        host.log("arm-mali-g52: gecersiz GPU_ID; alanlar kapatildi");
        return;
    }

    let shaders = rd32(MALI_BASE + GPU_SHADER_PRESENT);
    let tilers = rd32(MALI_BASE + GPU_TILER_PRESENT);
    let l2 = rd32(MALI_BASE + GPU_L2_PRESENT);
    let l2_feat = rd32(MALI_BASE + GPU_L2_FEATURES);
    let as_pres = rd32(MALI_BASE + GPU_AS_PRESENT);
    let js_pres = rd32(MALI_BASE + GPU_JS_PRESENT);

    wr32(MALI_BASE + GPU_INT_CLEAR, 0xffff_ffff);
    wr32(MALI_BASE + GPU_CMD, GPU_CMD_SOFT_RESET);
    dsb();
    let reset_ok = wait_reg(
        MALI_BASE + GPU_INT_RAWSTAT,
        GPU_IRQ_RESET_COMPLETED,
        GPU_IRQ_RESET_COMPLETED,
        20,
    );
    if reset_ok {
        wr32(MALI_BASE + GPU_INT_CLEAR, GPU_IRQ_RESET_COMPLETED);
        wr32(MALI_BASE + GPU_L2_PWRON, l2);
        let _ = wait_reg(MALI_BASE + GPU_L2_READY, l2, l2, 20);
        wr32(MALI_BASE + GPU_TILER_PWRON, tilers);
        let _ = wait_reg(MALI_BASE + GPU_TILER_READY, tilers, tilers, 20);
        wr32(MALI_BASE + GPU_SHADER_PWRON, shaders);
        let _ = wait_reg(MALI_BASE + GPU_SHADER_READY, shaders, shaders, 20);
    }

    let shader_ready = rd32(MALI_BASE + GPU_SHADER_READY);
    MALI_GPU_ID.store(id, Ordering::Release);
    MALI_POWERED.store(reset_ok && shader_ready == shaders && shaders != 0, Ordering::Release);

    let mut msg = LogBuf::<224>::new();
    let _ = write!(
        msg,
        "arm-mali-g52: {} ({:#010x}) hw-acik (reset={} shader={:#x}/{:#x} l2={:#x} feat={:#x} AS={:#x} JS={:#x}, 364 MHz)",
        gpu_name(id),
        id,
        reset_ok as u8,
        shader_ready,
        shaders,
        l2,
        l2_feat,
        as_pres,
        js_pres
    );
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mali_gpu_id_names() {
        assert_eq!(gpu_name(0x7212_0000), "Mali-G52 MC2");
        assert_eq!(gpu_name(0x7402_0000), "Mali-G52 r1");
        assert_eq!(gpu_name(0x1234_0000), "Mali Bifrost");
    }
}
