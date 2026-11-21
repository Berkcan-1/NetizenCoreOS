//! MediaTek MT6768 APMIXEDSYS CPU Clock & OPP telemetry driver (`apmixed@1000c000`).
//!
//! Verified against X6826C `stock/dtb` (`/apmixed@1000c000`, `/opp_table0`, `/opp_table1`)
//! and MT6768 PLL layout (`ARMPLL_L_CON1 = 0x20c` for Cortex-A75, `ARMPLL_LL_CON1 = 0x21c`
//! for Cortex-A55).
//!
//! Safety Policy:
//! - Strictly **READ-ONLY** during kernel boot. Does NOT perform speculative PLL changes
//!   or PMIC voltage scaling until a full thermal/clock/regulator framework is active.

use core::fmt::Write;
use nc_device::DriverHost;

use crate::mmio::{rd32, LogBuf};
use crate::pwrap::{
    self, MT6358_VPROC11_DBG0, MT6358_VPROC12_DBG0, MT6358_VSRAM_PROC11_DBG0,
    MT6358_VSRAM_PROC12_DBG0,
};

pub const APMIXED_BASE: usize = 0x1000_c000;
const ARMPLL_L_CON1: usize = 0x20c;
const ARMPLL_LL_CON1: usize = 0x21c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuClockSnapshot {
    pub little_a55_mhz: u32,
    pub big_a75_mhz: u32,
    pub vproc11_mv: Option<u32>,
    pub vproc12_mv: Option<u32>,
    pub vsram11_mv: Option<u32>,
    pub vsram12_mv: Option<u32>,
}

/// Decodes MT6768 ARMPLL `CON1` register (`26 MHz` crystal, 14-bit fractional PCW, 3-bit posdiv).
pub const fn pll_mhz(con1: u32) -> u32 {
    let shift = (con1 >> 24) & 7;
    if shift > 4 {
        return 0;
    }
    let posdiv = 1u32 << shift;
    let pcw = (con1 & 0x3f_ffff) as u64;
    (((pcw * 26) >> 14) as u32) / posdiv
}

pub fn read_clocks() -> Option<CpuClockSnapshot> {
    let little_con1 = rd32(APMIXED_BASE + ARMPLL_LL_CON1);
    let big_con1 = rd32(APMIXED_BASE + ARMPLL_L_CON1);
    if little_con1 == 0xffff_ffff || big_con1 == 0xffff_ffff {
        return None;
    }
    let (vproc11_mv, vproc12_mv, vsram11_mv, vsram12_mv) = if pwrap::is_ready() {
        (
            pwrap::read(MT6358_VPROC11_DBG0).map(|v| pwrap::millivolts(v & 0x7f)),
            pwrap::read(MT6358_VPROC12_DBG0).map(|v| pwrap::millivolts(v & 0x7f)),
            pwrap::read(MT6358_VSRAM_PROC11_DBG0).map(|v| pwrap::millivolts((v >> 8) & 0x7f)),
            pwrap::read(MT6358_VSRAM_PROC12_DBG0).map(|v| pwrap::millivolts((v >> 8) & 0x7f)),
        )
    } else {
        (None, None, None, None)
    };
    Some(CpuClockSnapshot {
        little_a55_mhz: pll_mhz(little_con1),
        big_a75_mhz: pll_mhz(big_con1),
        vproc11_mv,
        vproc12_mv,
        vsram11_mv,
        vsram12_mv,
    })
}

pub fn init(host: &mut dyn DriverHost) {
    let mut msg = LogBuf::<224>::new();
    match read_clocks() {
        Some(snap) => {
            let _ = write!(
                msg,
                "mtk-apmixed-cpufreq: A55={} MHz (Vproc12={} mV), A75={} MHz (Vproc11={} mV) [LK OPP korundu]",
                snap.little_a55_mhz,
                snap.vproc12_mv.unwrap_or(0),
                snap.big_a75_mhz,
                snap.vproc11_mv.unwrap_or(0)
            );
        }
        None => {
            let _ = write!(msg, "mtk-apmixed-cpufreq: APMIXEDSYS erisilemez");
        }
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armpll_frequency_decoding() {
        // 1176 MHz with posdiv = /2 (shift=1) => VCO = 2352 MHz => PCW = 2352 * 16384 / 26 = 1482043
        let pcw = (2352u64 * 16384 / 26) as u32;
        let con1 = (1u32 << 24) | (pcw & 0x3f_ffff);
        let mhz = pll_mhz(con1);
        assert!((1175..=1176).contains(&mhz), "got {mhz}");
        // Invalid posdiv shift returns 0 safely without panic
        assert_eq!(pll_mhz(7u32 << 24), 0);
    }
}
