//! OmniVision / Synaptics TD4160 TCM Touchscreen driver over MT6768 SPI2 (`spi2@11012000`)
//! for the Infinix Hot 20 (X6826C).
//!
//! Verified against X6826C primary hardware evidence:
//! - `stock/dtb` + `stock/dtbo` `/spi2@11012000/touch@0` (`mediatek,cap_touch_2`):
//!   - SPI2 base `0x1101_2000`, GIC INTID `177` (SPI `0x91`), `pad-select = 0`
//!   - `omnivision,spi-mode = 3` (CPOL=1, CPHA=1), `spi-max-frequency = 8_000_000`
//!   - `omnivision,ubl-byte-delay-us = 20`, `reset-active-ms = 20`, `reset-delay-ms = 200`
//!   - `omnivision,reset-on-state = 0` (active low), `irq-on-state = 0` (active low)
//! - `stock/dtbo` `/touch` + `/pinctrl`:
//!   - `RST` = GPIO 0 (`pinmux = 0x00`), `IRQ` = GPIO 1 (`pinmux = 0x100`, EINT 1)
//!   - `vtouch-supply` = `ldo_vldo28`
//! - `stock/hdl_firmware.img` vs `firmware/td4160_romboot.bin`:
//!   - Exact byte-for-byte match to `ROMBOOT_APP_CODE` section (102,400 bytes, IEEE CRC32
//!     `0xeb776dc0`, header magic `0x55 0xaa`).
//!   - **Volatile SRAM host-download only**: TD4160 TDDI has no onboard flash; ROMBOOT
//!     download loads RAM firmware cleanly and does NOT permanently modify any NVM/flash.
//! - MT6768 silicon clock gate:
//!   - `TOPCKGEN CLK_CFG_2_CLR` (`0x1000_0068`): `spi_sel` (`3 << 24`) + `PDN` (`1 << 31`)
//!   - `INFRACFG_AO IFR4_CG_CLR` (`0x1000_10a8`): single verified bit **`1 << 9`**
//!     (`CLK_IFR_SPI2`, from `clk-mt6765.c` / `clk-mt6768.c`) — no blind 32-bit unmasking!

use core::fmt::Write;
use core::ptr::addr_of_mut;
use nc_device::DriverHost;
use nc_hal::{InputDevice, KeyCode, MouseState};

use crate::mmio::{crc32_ieee, delay_ms, delay_us, dsb, rd32, wr32, Deadline, LogBuf};

pub const SPI2_BASE: usize = 0x1101_2000;
const GPIO_BASE: usize = 0x1000_5000;
const TOPCKGEN_BASE: usize = 0x1000_0000;
const INFRACFG_BASE: usize = 0x1000_1000;

const CLK_CFG_2_CLR: usize = 0x068;
const SPI_SEL_FIELD: u32 = 3 << 24;
const SPI_SEL_PDN: u32 = 1 << 31;
/// `GATE_IFR4(CLK_IFR_SPI2, "ifr_spi2", "spi_ck", 9)` -> `INFRACFG_AO + 0xA8` bit 9.
const INFRA_CG4_CLR: usize = 0x0a8;
const CLK_IFR_SPI2_BIT: u32 = 1 << 9;

const GPIO_DIR0_SET: usize = 0x004;
const GPIO_DIR0_CLR: usize = 0x008;
const GPIO_DOUT0_SET: usize = 0x104;
const GPIO_DOUT0_CLR: usize = 0x108;
const GPIO_DIN0: usize = 0x200;
const GPIO_MODE3: usize = 0x330;

const PIN_RESET: u32 = 0;
const PIN_IRQ: u32 = 1;
const PIN_CS: u32 = 25;
const PIN_MO: u32 = 26;
const PIN_CLK: u32 = 27;
const PIN_MI: u32 = 28;
const PIN_FUNC_SPI2: u32 = 2;

const SPI_CFG0: usize = 0x00;
const SPI_CFG1: usize = 0x04;
const SPI_TX_DATA: usize = 0x10;
const SPI_RX_DATA: usize = 0x14;
const SPI_CMD: usize = 0x18;
const SPI_STATUS0: usize = 0x1c;
const SPI_PAD_SEL: usize = 0x24;
const SPI_CFG2: usize = 0x28;

const SPI_CMD_ACT: u32 = 1 << 0;
const SPI_CMD_RST: u32 = 1 << 2;
const SPI_CMD_CPHA: u32 = 1 << 8;
const SPI_CMD_CPOL: u32 = 1 << 9;
const SPI_CMD_TXMSBF: u32 = 1 << 12;
const SPI_CMD_RXMSBF: u32 = 1 << 13;
const SPI_STATUS0_DONE: u32 = 1 << 0;
const SPI_FIFO_BYTES: usize = 32;
const SPI_TICKS: u32 = 2; // 26 MHz / 4 = 6.5 MHz <= 8 MHz max
const SPI_XFER_TIMEOUT_US: u64 = 10_000;

const TCM_MARKER: u8 = 0xa5;
const TCM_STATUS_OK: u8 = 0x01;
const TCM_STATUS_CONTINUED: u8 = 0x03;

const TCM_CMD_CONTINUE_WRITE: u8 = 0x01;
const TCM_CMD_ENABLE_REPORT: u8 = 0x05;
const TCM_CMD_ROMBOOT_RUN: u8 = 0x42;
const TCM_CMD_ROMBOOT_DOWNLOAD: u8 = 0x45;

const TCM_REPORT_IDENTIFY: u8 = 0x10;
const TCM_REPORT_TOUCH: u8 = 0x11;

const TCM_MODE_APPLICATION: u8 = 0x01;
const TCM_MODE_HOSTDOWNLOAD: u8 = 0x02;
const TCM_MODE_ROMBOOT: u8 = 0x04;

const TCM_MESSAGE_HEAD: usize = 3;
const TCM_ROMBOOT_HEADER: usize = 14;
const READ_GAP_US: u32 = 20;

const FINGER_BITS: usize = 88;
pub const MAX_FINGERS: usize = 10;
pub const PANEL_W: i32 = 720;
pub const PANEL_H: i32 = 1640;

/// Verified X6826C TD4160 `ROMBOOT_APP_CODE` image (`stock/hdl_firmware.img` section 1).
static TOUCH_FW: &[u8] = include_bytes!("../firmware/td4160_romboot.bin");
pub const EXPECTED_FW_LEN: usize = 102_400;
pub const EXPECTED_FW_CRC32: u32 = 0xeb77_6dc0;

/// Verifies embedded volatile ROMBOOT firmware integrity before any download attempt.
pub fn verify_firmware_integrity(fw: &[u8]) -> bool {
    fw.len() == EXPECTED_FW_LEN
        && fw.len() >= 2
        && fw[0] == 0x55
        && fw[1] == 0xaa
        && crc32_ieee(fw) == EXPECTED_FW_CRC32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcmIdentify {
    pub version: u8,
    pub mode: u8,
    pub part_number: [u8; 16],
    pub part_len: usize,
    pub build_id: u32,
}

impl TcmIdentify {
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < 2 {
            return None;
        }
        let version = payload[0];
        let mode = payload[1];
        let mut part_number = [0u8; 16];
        let mut part_len = 0usize;
        if payload.len() >= 18 {
            for &b in &payload[2..18] {
                if b == 0 {
                    break;
                }
                if (0x20..=0x7e).contains(&b) && part_len < part_number.len() {
                    part_number[part_len] = b;
                    part_len += 1;
                }
            }
        }
        let build_id = if payload.len() >= 22 {
            u32::from_le_bytes([payload[18], payload[19], payload[20], payload[21]])
        } else {
            0
        };
        Some(Self {
            version,
            mode,
            part_number,
            part_len,
            build_id,
        })
    }

    pub fn part_str(&self) -> &str {
        core::str::from_utf8(&self.part_number[..self.part_len]).unwrap_or("unknown")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchContact {
    pub id: u8,
    pub active: bool,
    pub kind: u8,
    pub raw_x: i32,
    pub raw_y: i32,
    pub pressure: u8,
    pub width_x: u8,
    pub width_y: u8,
}

impl TouchContact {
    pub const EMPTY: Self = Self {
        id: 0,
        active: false,
        kind: 0,
        raw_x: 0,
        raw_y: 0,
        pressure: 0,
        width_x: 0,
        width_y: 0,
    };
}

#[inline]
fn pin_set(pin: u32, high: bool) {
    let reg = if high { GPIO_DOUT0_SET } else { GPIO_DOUT0_CLR };
    wr32(GPIO_BASE + reg, 1u32 << pin);
}

#[inline]
fn pin_get(pin: u32) -> bool {
    ((rd32(GPIO_BASE + GPIO_DIN0) >> pin) & 1) != 0
}

#[inline]
fn pin_settle() {
    let _ = rd32(GPIO_BASE + GPIO_DIN0);
    let _ = rd32(GPIO_BASE + GPIO_DIN0);
}

fn pins_to_spi2() {
    let mut mode = rd32(GPIO_BASE + GPIO_MODE3);
    for pin in PIN_CS..=PIN_MI {
        let shift = (pin % 8) * 4;
        mode = (mode & !(0xf << shift)) | (PIN_FUNC_SPI2 << shift);
    }
    wr32(GPIO_BASE + GPIO_MODE3, mode);
    dsb();
}

fn pins_to_gpio() {
    let mut mode = rd32(GPIO_BASE + GPIO_MODE3);
    for pin in PIN_CS..=PIN_MI {
        let shift = (pin % 8) * 4;
        mode &= !(0xf << shift);
    }
    wr32(GPIO_BASE + GPIO_MODE3, mode);
    wr32(
        GPIO_BASE + GPIO_DIR0_SET,
        (1u32 << PIN_MO) | (1u32 << PIN_CLK) | (1u32 << PIN_CS),
    );
    wr32(GPIO_BASE + GPIO_DIR0_CLR, 1u32 << PIN_MI);
    pin_set(PIN_CS, true);
    pin_set(PIN_CLK, true);
    dsb();
    pin_settle();
}

fn spi_setup() {
    // Select 26 MHz crystal source for spi_sel and clear PDN; ungate ONLY CLK_IFR_SPI2 (bit 9).
    wr32(TOPCKGEN_BASE + CLK_CFG_2_CLR, SPI_SEL_FIELD | SPI_SEL_PDN);
    wr32(INFRACFG_BASE + INFRA_CG4_CLR, CLK_IFR_SPI2_BIT);
    dsb();
    wr32(SPI2_BASE + SPI_CMD, SPI_CMD_RST);
    wr32(SPI2_BASE + SPI_CMD, 0);
    wr32(
        SPI2_BASE + SPI_CFG2,
        (SPI_TICKS - 1) | ((SPI_TICKS - 1) << 16),
    );
    wr32(
        SPI2_BASE + SPI_CFG0,
        (SPI_TICKS - 1) | ((SPI_TICKS - 1) << 16),
    );
    wr32(SPI2_BASE + SPI_PAD_SEL, 0);
}

fn spi_fifo_write(tx: &[u8]) -> bool {
    if tx.is_empty() || tx.len() > SPI_FIFO_BYTES {
        return false;
    }
    let words = tx.len().div_ceil(4);
    pins_to_spi2();
    spi_setup();
    wr32(
        SPI2_BASE + SPI_CMD,
        SPI_CMD_CPHA | SPI_CMD_CPOL | SPI_CMD_TXMSBF | SPI_CMD_RXMSBF,
    );
    wr32(
        SPI2_BASE + SPI_CFG1,
        (((tx.len() as u32) - 1) << 16) | (SPI_TICKS - 1),
    );
    for w in 0..words {
        let mut val = 0u32;
        for b in 0..4 {
            let idx = w * 4 + b;
            let byte = if idx < tx.len() { tx[idx] } else { 0xff };
            val |= (byte as u32) << (8 * b);
        }
        wr32(SPI2_BASE + SPI_TX_DATA, val);
    }
    wr32(SPI2_BASE + SPI_CMD, rd32(SPI2_BASE + SPI_CMD) | SPI_CMD_ACT);
    dsb();
    let mut dl = Deadline::from_us(SPI_XFER_TIMEOUT_US);
    loop {
        if (rd32(SPI2_BASE + SPI_STATUS0) & SPI_STATUS0_DONE) != 0 {
            for _ in 0..words {
                let _ = rd32(SPI2_BASE + SPI_RX_DATA);
            }
            return true;
        }
        if dl.expired() {
            return false;
        }
    }
}

fn bitbang_clock_byte(out_byte: u8) -> u8 {
    let mut in_byte = 0u8;
    for bit in (0..8).rev() {
        pin_set(PIN_MO, ((out_byte >> bit) & 1) != 0);
        pin_set(PIN_CLK, false);
        pin_settle();
        pin_set(PIN_CLK, true);
        pin_settle();
        in_byte = (in_byte << 1) | (if pin_get(PIN_MI) { 1 } else { 0 });
    }
    in_byte
}

fn spi_bitbang_read(rx: &mut [u8], gap_us: u32) {
    pins_to_gpio();
    pin_set(PIN_CLK, true);
    pin_set(PIN_CS, false);
    pin_settle();
    for slot in rx.iter_mut() {
        *slot = bitbang_clock_byte(0xff);
        if gap_us != 0 {
            delay_us(gap_us);
        }
    }
    pin_set(PIN_CS, true);
    pin_settle();
}

fn spi_bitbang_write(tx: &[u8]) {
    pins_to_gpio();
    pin_set(PIN_CLK, true);
    pin_set(PIN_CS, false);
    pin_settle();
    for &b in tx {
        let _ = bitbang_clock_byte(b);
    }
    pin_set(PIN_CS, true);
    pin_settle();
}

fn tcm_write_chunk(tx: &[u8]) {
    if !spi_fifo_write(tx) {
        spi_bitbang_write(tx);
    }
}

#[inline]
fn tcm_irq() -> bool {
    !pin_get(PIN_IRQ)
}

fn tcm_wait_irq(ms: u32) -> bool {
    let mut dl = Deadline::from_ms(ms as u64);
    loop {
        if tcm_irq() {
            return true;
        }
        if dl.expired() {
            return false;
        }
    }
}

fn tcm_reset() {
    wr32(GPIO_BASE + GPIO_DIR0_SET, 1u32 << PIN_RESET);
    wr32(GPIO_BASE + GPIO_DIR0_CLR, 1u32 << PIN_IRQ);
    pin_set(PIN_RESET, false);
    dsb();
    delay_ms(20); // `omnivision,reset-active-ms = <0x14>`
    pin_set(PIN_RESET, true);
    dsb();
    delay_ms(200); // `omnivision,reset-delay-ms = <0xc8>`
}

fn tcm_write(cmd: u8, payload: &[u8]) -> bool {
    let mut chunk = [0u8; SPI_FIFO_BYTES];
    let space = SPI_FIFO_BYTES - 1;
    let len = payload.len();
    let mut remaining = len + 2;
    let mut sent = 0usize;
    let mut first = true;

    while remaining > 0 {
        let xfer = remaining.min(space);
        let mut n = 0usize;
        if first {
            chunk[n] = cmd;
            n += 1;
            chunk[n] = (len & 0xff) as u8;
            n += 1;
            chunk[n] = ((len >> 8) & 0xff) as u8;
            n += 1;
            for _ in 2..xfer {
                chunk[n] = payload[sent];
                n += 1;
                sent += 1;
            }
            first = false;
        } else {
            chunk[n] = TCM_CMD_CONTINUE_WRITE;
            n += 1;
            for _ in 0..xfer {
                chunk[n] = payload[sent];
                n += 1;
                sent += 1;
            }
        }
        tcm_write_chunk(&chunk[..n]);
        remaining -= xfer;
    }
    true
}

fn tcm_read(buf: &mut [u8]) -> Option<(u8, usize)> {
    let mut head = [0u8; 4];
    spi_bitbang_read(&mut head, READ_GAP_US);
    if head[0] != TCM_MARKER {
        return None;
    }
    let code = head[1];
    let len = (head[2] as usize) | ((head[3] as usize) << 8);
    if len > 4096 {
        return None;
    }
    let mut left = if len == 0 { 1 } else { len + 1 };
    let mut at = 0usize;
    let mut piece = [0u8; SPI_FIFO_BYTES];
    let room = SPI_FIFO_BYTES - 2;

    while left > 0 {
        let want = left.min(room);
        spi_bitbang_read(&mut piece[..want + 2], READ_GAP_US);
        if piece[0] != TCM_MARKER || piece[1] != TCM_STATUS_CONTINUED {
            return None;
        }
        for i in 0..want {
            if at + i < buf.len() {
                buf[at + i] = piece[2 + i];
            }
        }
        at += want;
        left -= want;
    }
    Some((code, len.min(buf.len())))
}

fn tcm_read_patiently(buf: &mut [u8]) -> Option<(u8, usize)> {
    for _ in 0..6 {
        let _ = tcm_wait_irq(100);
        if let Some(res) = tcm_read(buf) {
            return Some(res);
        }
        delay_ms(20);
    }
    None
}

/// Downloads verified `ROMBOOT_APP_CODE` into TD4160 volatile SRAM over a single unbroken
/// CS-low transfer (required by TD4160 ROMBOOT protocol; never touches flash).
fn tcm_load_volatile_ram_firmware() -> bool {
    if !verify_firmware_integrity(TOUCH_FW) {
        return false;
    }
    let fw_len = TOUCH_FW.len();
    let payload_len = TCM_ROMBOOT_HEADER + fw_len;
    let mut head = [0u8; TCM_MESSAGE_HEAD + TCM_ROMBOOT_HEADER];
    head[0] = TCM_CMD_ROMBOOT_DOWNLOAD;
    head[1] = (payload_len & 0xff) as u8;
    head[2] = ((payload_len >> 8) & 0xff) as u8;
    head[TCM_MESSAGE_HEAD] = ((fw_len >> 16) & 0xff) as u8;

    pins_to_gpio();
    pin_set(PIN_CLK, true);
    pin_set(PIN_CS, false);
    pin_settle();
    for &b in head.iter() {
        let _ = bitbang_clock_byte(b);
    }
    for &b in TOUCH_FW.iter() {
        let _ = bitbang_clock_byte(b);
    }
    pin_set(PIN_CS, true);
    pin_settle();
    true
}

fn tcm_run_firmware() {
    delay_ms(100);
    let _ = tcm_write(TCM_CMD_ROMBOOT_RUN, &[]);
}

fn bring_up_once() -> Option<TcmIdentify> {
    pins_to_spi2();
    spi_setup();

    let mut buf = [0u8; 64];
    if tcm_irq() {
        if let Some((code, got)) = tcm_read(&mut buf) {
            if code == TCM_REPORT_IDENTIFY {
                if let Some(id) = TcmIdentify::parse(&buf[..got]) {
                    if id.mode == TCM_MODE_APPLICATION || id.mode == TCM_MODE_HOSTDOWNLOAD {
                        let _ = tcm_write(TCM_CMD_ENABLE_REPORT, &[TCM_REPORT_TOUCH]);
                        return Some(id);
                    }
                }
            }
        }
    }

    tcm_reset();
    let (code, got) = tcm_read_patiently(&mut buf)?;
    if code != TCM_REPORT_IDENTIFY {
        return None;
    }
    let id = TcmIdentify::parse(&buf[..got])?;
    if id.mode == TCM_MODE_APPLICATION || id.mode == TCM_MODE_HOSTDOWNLOAD {
        let _ = tcm_write(TCM_CMD_ENABLE_REPORT, &[TCM_REPORT_TOUCH]);
        return Some(id);
    }
    if id.mode != TCM_MODE_ROMBOOT {
        return None;
    }

    if !tcm_load_volatile_ram_firmware() {
        return None;
    }
    let (ans_code, _) = tcm_read_patiently(&mut buf)?;
    if ans_code != TCM_STATUS_OK {
        return None;
    }

    tcm_run_firmware();
    let mut dl = Deadline::from_ms(3_000);
    while !dl.expired() {
        if !tcm_irq() {
            delay_ms(10);
            continue;
        }
        if let Some((code, n)) = tcm_read(&mut buf) {
            if code == TCM_REPORT_IDENTIFY {
                if let Some(run_id) = TcmIdentify::parse(&buf[..n]) {
                    if run_id.mode == TCM_MODE_APPLICATION
                        || run_id.mode == TCM_MODE_HOSTDOWNLOAD
                    {
                        let _ = tcm_write(TCM_CMD_ENABLE_REPORT, &[TCM_REPORT_TOUCH]);
                        let _ = tcm_read_patiently(&mut buf);
                        return Some(run_id);
                    }
                }
            }
        } else {
            delay_ms(10);
        }
    }
    None
}

pub fn extract_bits(buffer: &[u8], bit_offset: usize, count: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..count {
        let bit = bit_offset + i;
        let byte_idx = bit / 8;
        if byte_idx < buffer.len() {
            let bit_val = ((buffer[byte_idx] >> (bit % 8)) & 1) as u32;
            value |= bit_val << i;
        }
    }
    value
}

pub fn scale_coord(raw: i32, panel_extent: i32, screen_extent: i32) -> i32 {
    if panel_extent <= 1 || screen_extent <= 1 {
        return 0;
    }
    let clamped = raw.clamp(0, panel_extent - 1);
    ((clamped as i64) * ((screen_extent - 1) as i64) / ((panel_extent - 1) as i64)) as i32
}

pub struct TouchDevice {
    ready: bool,
    contacts: [TouchContact; MAX_FINGERS],
    active_count: u8,
    primary_id: Option<u8>,
    x: i32,
    y: i32,
    down: bool,
}

impl TouchDevice {
    pub const fn new() -> Self {
        Self {
            ready: false,
            contacts: [TouchContact::EMPTY; MAX_FINGERS],
            active_count: 0,
            primary_id: None,
            x: PANEL_W / 2,
            y: PANEL_H / 2,
            down: false,
        }
    }

    pub fn active_contacts(&self) -> (&[TouchContact; MAX_FINGERS], u8) {
        (&self.contacts, self.active_count)
    }

    /// Decodes a full TCM touch report (`0x11`), updating all 10 multitouch contact slots
    /// and tracking primary pointer continuity for NetizenCore's `InputDevice` pipeline.
    pub fn decode_report(&mut self, report: &[u8], screen_w: i32, screen_h: i32) -> (bool, bool) {
        let mut seen = [false; MAX_FINGERS];
        let mut at = 8usize; // First byte = button bitmask
        let total_bits = report.len() * 8;

        while at + FINGER_BITS <= total_bits {
            let which = extract_bits(report, at, 4) as usize;
            let kind = extract_bits(report, at + 4, 4) as u8;
            let raw_x = extract_bits(report, at + 8, 12) as i32;
            let raw_y = extract_bits(report, at + 20, 12) as i32;
            let pressure = extract_bits(report, at + 32, 8) as u8;
            let width_x = extract_bits(report, at + 40, 8) as u8;
            let width_y = extract_bits(report, at + 48, 8) as u8;
            at += FINGER_BITS;

            if which >= MAX_FINGERS || kind == 0 {
                continue;
            }
            seen[which] = true;
            self.contacts[which] = TouchContact {
                id: which as u8,
                active: true,
                kind,
                raw_x: raw_x.clamp(0, PANEL_W - 1),
                raw_y: raw_y.clamp(0, PANEL_H - 1),
                pressure,
                width_x,
                width_y,
            };
        }

        let mut count = 0u8;
        let mut first_active: Option<usize> = None;
        for (i, &is_seen) in seen.iter().enumerate() {
            if is_seen {
                count += 1;
                if first_active.is_none() {
                    first_active = Some(i);
                }
            } else {
                self.contacts[i].active = false;
            }
        }
        self.active_count = count;

        // Keep tracking the same primary finger until it lifts; only switch if lifted.
        let chosen = match self.primary_id {
            Some(pid) if (pid as usize) < MAX_FINGERS && self.contacts[pid as usize].active => {
                Some(pid as usize)
            }
            _ => first_active,
        };

        let active = chosen.is_some();
        let clicked = active && !self.down;
        if let Some(idx) = chosen {
            let c = self.contacts[idx];
            self.primary_id = Some(idx as u8);
            self.x = scale_coord(c.raw_x, PANEL_W, screen_w.max(1));
            self.y = scale_coord(c.raw_y, PANEL_H, screen_h.max(1));
        } else {
            self.primary_id = None;
        }
        self.down = active;
        (active, clicked)
    }
}

impl Default for TouchDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDevice for TouchDevice {
    fn name(&self) -> &'static str {
        "td4160-spi2-touch"
    }

    fn poll(&mut self, width: i32, height: i32) -> (MouseState, Option<KeyCode>) {
        if !self.ready {
            return (
                MouseState {
                    x: self.x,
                    y: self.y,
                    left_down: false,
                    left_clicked: false,
                    right_down: false,
                    right_clicked: false,
                },
                None,
            );
        }

        let mut any_clicked = false;
        let mut buf = [0u8; 192];
        let mut budget = 4u8;
        while budget > 0 && tcm_irq() {
            budget -= 1;
            let Some((code, len)) = tcm_read(&mut buf) else {
                break;
            };
            if code == TCM_REPORT_TOUCH && len >= 1 {
                let (_down, clicked) = self.decode_report(&buf[..len], width, height);
                if clicked {
                    any_clicked = true;
                }
            }
        }

        (
            MouseState {
                x: self.x,
                y: self.y,
                left_down: self.down,
                left_clicked: any_clicked,
                right_down: false,
                right_clicked: false,
            },
            None,
        )
    }
}

static mut TOUCH_DEV: TouchDevice = TouchDevice::new();

pub fn init(host: &mut dyn DriverHost) {
    let gpio_probe = rd32(GPIO_BASE + GPIO_MODE3);
    if gpio_probe == 0xffff_ffff {
        host.log("td4160-spi2-touch: GPIO/SPI2 erisilemez");
        return;
    }

    let fw_valid = verify_firmware_integrity(TOUCH_FW);
    if !fw_valid {
        host.log("td4160-spi2-touch: gomulu ROMBOOT aygit yazilimi CRC32 dogrulamasi basarisiz");
        return;
    }

    let mut identified: Option<TcmIdentify> = None;
    for attempt in 1..=2 {
        if let Some(id) = bring_up_once() {
            identified = Some(id);
            break;
        }
        if attempt < 2 {
            delay_ms(50);
        }
    }

    let mut msg = LogBuf::<224>::new();
    if let Some(id) = identified {
        unsafe {
            let dev = &mut *addr_of_mut!(TOUCH_DEV);
            dev.ready = true;
            host.add_input_device(dev);
        }
        let _ = write!(
            msg,
            "td4160-spi2-touch: '{}' hazir (SPI2 {:#x}, mod={:#x}, v={}, build={}, fw={}B CRC32 ok)",
            id.part_str(),
            SPI2_BASE,
            id.mode,
            id.version,
            id.build_id,
            TOUCH_FW.len()
        );
    } else {
        // Honest registration: do NOT register a non-responding input device.
        let _ = write!(
            msg,
            "td4160-spi2-touch: SPI2 {:#x} yanit vermedi (sensor bulunamadi, kayit yapilmadi)",
            SPI2_BASE
        );
    }
    host.log(msg.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_finger(buf: &mut [u8], bit_off: usize, id: u8, kind: u8, x: u16, y: u16, p: u8) {
        let mut write_bits = |off: usize, val: u32, count: usize| {
            for i in 0..count {
                let bit = off + i;
                if ((val >> i) & 1) != 0 {
                    buf[bit / 8] |= 1 << (bit % 8);
                }
            }
        };
        write_bits(bit_off, id as u32, 4);
        write_bits(bit_off + 4, kind as u32, 4);
        write_bits(bit_off + 8, x as u32, 12);
        write_bits(bit_off + 20, y as u32, 12);
        write_bits(bit_off + 32, p as u32, 8);
    }

    #[test]
    fn embedded_romboot_firmware_integrity_matches_stock_hdl() {
        assert!(verify_firmware_integrity(TOUCH_FW));
    }

    #[test]
    fn tcm_identify_parser() {
        let mut pkt = [0u8; 24];
        pkt[0] = 2; // version
        pkt[1] = TCM_MODE_APPLICATION;
        pkt[2..8].copy_from_slice(b"td4160");
        pkt[18..22].copy_from_slice(&12345u32.to_le_bytes());
        let id = TcmIdentify::parse(&pkt).unwrap();
        assert_eq!(id.version, 2);
        assert_eq!(id.mode, TCM_MODE_APPLICATION);
        assert_eq!(id.part_str(), "td4160");
        assert_eq!(id.build_id, 12345);
    }

    #[test]
    fn multitouch_report_decoding_and_primary_continuity() {
        let mut dev = TouchDevice::new();
        // Frame 1: finger 0 lands at (360, 820)
        let mut r1 = [0u8; 1 + 11];
        encode_finger(&mut r1, 8, 0, 1, 360, 820, 80);
        let (down1, click1) = dev.decode_report(&r1, 720, 1640);
        assert!(down1 && click1);
        assert_eq!(dev.active_contacts().1, 1);
        assert_eq!((dev.x, dev.y), (360, 820));

        // Frame 2: finger 0 moves to (370, 830) AND finger 1 lands at (100, 200);
        // primary pointer must continue tracking finger 0 without jumping to finger 1!
        let mut r2 = [0u8; 1 + 22];
        encode_finger(&mut r2, 8, 1, 1, 100, 200, 60);
        encode_finger(&mut r2, 8 + 88, 0, 1, 370, 830, 85);
        let (down2, click2) = dev.decode_report(&r2, 720, 1640);
        assert!(down2 && !click2);
        assert_eq!(dev.active_contacts().1, 2);
        assert_eq!((dev.x, dev.y), (370, 830));

        // Frame 3: all fingers lifted
        let r3 = [0u8; 1];
        let (down3, click3) = dev.decode_report(&r3, 720, 1640);
        assert!(!down3 && !click3);
        assert_eq!(dev.active_contacts().1, 0);
    }
}
