//! MT6768 Display Pipeline (`disp_ovl0`, `disp_rdma0`, `dsi0`, `disp_mutex0`) & LK Handoff
//! driver for the Infinix Hot 20 (X6826C).
//!
//! Verified against X6826C primary evidence (`stock/dtb`, `stock/dtbo`):
//! - Active panel: `td4160_hdp_dsi_vdo_hlt_boe_zal1878` (MIPI-DSI video mode, 60 Hz)
//! - Geometry: `720 x 1640`, 32 bpp `Bgrx`, LK stride `2944` bytes
//! - Bootloader VRAM: `0x787b_0000` (`0x0105_0000` bytes = 16.3 MiB)
//! - Hardware blocks:
//!   - `disp_mutex0@14001000` (GIC INTID `255`)
//!   - `disp_ovl0@1400b000` (GIC INTID `262`)
//!   - `disp_rdma0@1400d000` (GIC INTID `263`)
//!   - `dsi0@14014000` (GIC INTID `270`)
//! - Panel GPIOs: `LCM_RST` = GPIO 45 (`0x2d`), `DSI_TE` = GPIO 44 (`0x2c`),
//!   `LCD_BIAS_ENN` = GPIO 152 (`0x98`), `LCD_BIAS_ENP` = GPIO 153 (`0x99`)
//! - LCD Bias IC: `i2c_lcd_bias@3e` on `i2c6@1100d000`
//!
//! Safety policy:
//! - Preserves LK's working video-mode DSI pipeline and panel power/bias configuration.
//! - Does NOT invent or execute an unverified DSI panel DCS init sequence.
//! - Only adjusts OVL0 layer 0 per-pixel alpha bit (`OVL_L0_CON` bit 8) and hides LK's
//!   unlocked-bootloader warning layer (`OVL_SRC_CON & 1`) after confirming OVL0 is alive.

use core::fmt::Write;
use nc_device::DriverHost;

use crate::i2c::{self, I2cBus};
use crate::mmio::{dsb, rd32, wr32, LogBuf};

pub const DISP_MUTEX0_BASE: usize = 0x1400_1000;
pub const OVL0_BASE: usize = 0x1400_b000;
pub const RDMA0_BASE: usize = 0x1400_d000;
pub const DSI0_BASE: usize = 0x1401_4000;

const OVL_STA: usize = 0x000;
const OVL_SRC_CON: usize = 0x02c;
const OVL_L0_CON: usize = 0x030;
const OVL_L0_SRC_SIZE: usize = 0x038;
const OVL_L0_PITCH: usize = 0x044;
const OVL_L0_ADDR: usize = 0xf40;

const DISP_MUTEX_EN: usize = DISP_MUTEX0_BASE + 0x020;
const DISP_MUTEX_RUN: usize = DISP_MUTEX0_BASE + 0x024;

const RDMA_GLOBAL_CON: usize = 0x010;
const RDMA_SIZE_CON_0: usize = 0x014;
const RDMA_SIZE_CON_1: usize = 0x018;

const DSI_START: usize = 0x000;
const DSI_MODE_CTRL: usize = 0x014;
const DSI_VACT_NL: usize = 0x02c;

pub const LCD_BIAS_I2C_ADDR: u8 = 0x3e;

pub const LK_FB_BASE: u64 = 0x787b_0000;
pub const LK_FB_SIZE: u64 = 0x0105_0000;
pub const LK_FB_PITCH: u32 = 2944;
pub const PANEL_W: u32 = 720;
pub const PANEL_H: u32 = 1640;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramebufferGeometry {
    pub base: u64,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub size_bytes: u64,
}

/// Validates a probed framebuffer address/pitch/dimension tuple against LK's VRAM window
/// and integer overflow / alignment invariants.
pub fn validate_fb_geometry(
    addr: u64,
    width: u32,
    height: u32,
    pitch: u32,
) -> Option<FramebufferGeometry> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let min_pitch = width.checked_mul(4)?;
    if pitch < min_pitch || pitch > 8192 || (pitch & 3) != 0 {
        return None;
    }
    if addr == 0 || (addr & 7) != 0 {
        return None;
    }
    let size_bytes = (pitch as u64).checked_mul(height as u64)?;
    let end = addr.checked_add(size_bytes)?;
    let vram_end = LK_FB_BASE.checked_add(LK_FB_SIZE)?;
    if addr < LK_FB_BASE || end > vram_end {
        return None;
    }
    Some(FramebufferGeometry {
        base: addr,
        width,
        height,
        stride_bytes: pitch,
        size_bytes,
    })
}

pub fn probe_lfb() -> FramebufferGeometry {
    let addr = rd32(OVL0_BASE + OVL_L0_ADDR) as u64;
    let pitch = rd32(OVL0_BASE + OVL_L0_PITCH) & 0xffff;
    let src_size = rd32(OVL0_BASE + OVL_L0_SRC_SIZE);
    let hw_w = src_size & 0x1fff;
    let hw_h = (src_size >> 16) & 0x1fff;
    let (w, h) = if hw_w == PANEL_W && hw_h == PANEL_H {
        (hw_w, hw_h)
    } else {
        (PANEL_W, PANEL_H)
    };
    validate_fb_geometry(addr, w, h, pitch).unwrap_or(FramebufferGeometry {
        base: LK_FB_BASE,
        width: PANEL_W,
        height: PANEL_H,
        stride_bytes: LK_FB_PITCH,
        size_bytes: (LK_FB_PITCH as u64) * (PANEL_H as u64),
    })
}

pub fn configure_ovl0() -> (FramebufferGeometry, bool, u32, u32) {
    let geom = probe_lfb();
    let sta = rd32(OVL0_BASE + OVL_STA);
    let con_before = rd32(OVL0_BASE + OVL_L0_CON);
    let src_before = rd32(OVL0_BASE + OVL_SRC_CON);
    if sta == 0xffff_ffff || (con_before == 0 && src_before == 0) {
        return (geom, false, con_before, src_before);
    }

    // Clear per-pixel alpha enable (bit 8) so BGRX opaque pixels render cleanly,
    // and keep only Layer 0 enabled (hiding LK's unlocked-state overlay on Layer 3).
    let next_con = con_before & !(1u32 << 8);
    let next_src = src_before & 1u32;
    if next_con != con_before || next_src != src_before {
        wr32(OVL0_BASE + OVL_L0_CON, next_con);
        wr32(OVL0_BASE + OVL_SRC_CON, next_src);
        wr32(DISP_MUTEX_EN, 1);
        wr32(DISP_MUTEX_RUN, 1);
        dsb();
    }
    let con_after = rd32(OVL0_BASE + OVL_L0_CON);
    let src_after = rd32(OVL0_BASE + OVL_SRC_CON);
    (geom, true, con_after, src_after)
}

pub fn init(host: &mut dyn DriverHost) {
    let (geom, ovl_ok, con_after, src_after) = configure_ovl0();
    let rdma_con = rd32(RDMA0_BASE + RDMA_GLOBAL_CON);
    let rdma_w = rd32(RDMA0_BASE + RDMA_SIZE_CON_0) & 0x1fff;
    let rdma_h = rd32(RDMA0_BASE + RDMA_SIZE_CON_1) & 0xfffff;
    let dsi_mode = rd32(DSI0_BASE + DSI_MODE_CTRL) & 0x3;
    let dsi_vact = rd32(DSI0_BASE + DSI_VACT_NL) & 0x7fff;
    let dsi_start = rd32(DSI0_BASE + DSI_START);
    let bias_vpos = i2c::read_reg(I2cBus::Bus6, LCD_BIAS_I2C_ADDR, 0x00).ok();

    let mut msg = LogBuf::<224>::new();
    if ovl_ok {
        let _ = write!(
            msg,
            "mtk-disp-ovl0: {}x{} @ {:#x} (pitch {}, L0={:#x} SRC={:#x} | RDMA={}x{} con={:#x} | DSI mode={} vact={} st={:#x} | bias={:?})",
            geom.width,
            geom.height,
            geom.base,
            geom.stride_bytes,
            con_after,
            src_after,
            rdma_w,
            rdma_h,
            rdma_con & 1,
            dsi_mode,
            dsi_vact,
            dsi_start & 1,
            bias_vpos
        );
    } else {
        let _ = write!(
            msg,
            "mtk-disp-ovl0: OVL0 erisilemez veya pasif; LK tamponu korunuyor ({}x{} @ {:#x}, pitch {})",
            geom.width, geom.height, geom.base, geom.stride_bytes
        );
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_x6826c_framebuffer_geometry() {
        let g = validate_fb_geometry(LK_FB_BASE, 720, 1640, 2944).expect("valid LK geometry");
        assert_eq!(g.size_bytes, 2944 * 1640);
        // Pitch smaller than width * 4 must be rejected
        assert!(validate_fb_geometry(LK_FB_BASE, 720, 1640, 2800).is_none());
        // Unaligned address or pitch rejected
        assert!(validate_fb_geometry(LK_FB_BASE + 4, 720, 1640, 2944).is_none());
        assert!(validate_fb_geometry(LK_FB_BASE, 720, 1640, 2942).is_none());
        // Overflow past VRAM window rejected
        assert!(validate_fb_geometry(LK_FB_BASE + LK_FB_SIZE - 4096, 720, 1640, 2944).is_none());
        // Outside VRAM window rejected
        assert!(validate_fb_geometry(0x4000_0000, 720, 1640, 2944).is_none());
    }
}
