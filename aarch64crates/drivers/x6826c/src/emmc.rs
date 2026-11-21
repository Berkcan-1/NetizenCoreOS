//! MediaTek MT6768 MSDC0 eMMC Storage Driver (`mmc@11230000`) for Infinix Hot 20 (X6826C).
//!
//! Verified against:
//! - X6826C `stock/dtb` (`/mmc@11230000`: MSDC0 `0x1123_0000` + MSDC0_TOP `0x11cd_0000`,
//!   GIC INTID `132` [SPI 0x64], 8-bit bus, HS400, non-removable, `vmmc-supply = ldo_vemc`)
//! - Stock X6826C `lk_rafw.bin.c` & MT6768 Linux `drivers/mmc/host/mtk-sd.c`
//! - HakiOS MT6768 SoC-level PIO transfer reference (`mtkloader/msdc.h`, MIT license)
//!
//! Bootloader Handoff Contract:
//! - Preloader/LK powers `ldo_vemc`, configures the 8-bit bus/clocks/tuning, assigns
//!   RCA `0x0001`, and leaves the card in `tran` (transfer) state.
//! - During `init()`, this driver explicitly verifies card communication via `CMD13`
//!   (`SEND_STATUS`), selects the card with `CMD7` only if in `stby` state, stops an
//!   in-progress read with `CMD12` if in `data` state, and reads the real 512-byte
//!   `EXT_CSD` (`CMD8`) to obtain the true hardware sector count (`SEC_COUNT`).
//!
//! CRITICAL SAFETY RULE:
//! - Strictly **READ-ONLY**. Zero write/erase commands are implemented, and
//!   `BlockDevice::write_blocks` unconditionally returns `Err(())`.

use core::fmt::Write;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use nc_device::DriverHost;
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

use crate::mmio::{crc32_ieee, dsb, rd32, wr32, Deadline, IrqSpinLock, LogBuf};

pub const MSDC0_BASE: usize = 0x1123_0000;
pub const MSDC0_TOP_BASE: usize = 0x11cd_0000;
pub const SECTOR: usize = 512;
const EMMC_MAX_BLOCKS: u32 = 256;
const EMMC_DEFAULT_RCA: u32 = 0x0001;
const EMMC_MAX_RETRIES: u32 = 3;

const MSDC_CFG: usize = 0x00;
const MSDC_CFG_MODE: u32 = 1 << 0;
const MSDC_CFG_CKPDN: u32 = 1 << 1;
const MSDC_CFG_RST: u32 = 1 << 2;
const MSDC_CFG_PIO: u32 = 1 << 3;
const MSDC_PS: usize = 0x08;
const MSDC_INT: usize = 0x0c;
const MSDC_FIFOCS: usize = 0x14;
const MSDC_FIFOCS_CLR: u32 = 1 << 31;
const MSDC_RXDATA: usize = 0x1c;
const SDC_CFG: usize = 0x30;
const SDC_CMD: usize = 0x34;
const SDC_ARG: usize = 0x38;
const SDC_STS: usize = 0x3c;
const SDC_STS_BUSY: u32 = 3;
const SDC_RESP0: usize = 0x40;
const SDC_BLK_NUM: usize = 0x50;

const INT_CMDRDY: u32 = 1 << 8;
const INT_CMDTMO: u32 = 1 << 9;
const INT_RSPCRCERR: u32 = 1 << 10;
const INT_XFER_COMPL: u32 = 1 << 12;
const INT_DATTMO: u32 = 1 << 14;
const INT_DATCRCERR: u32 = 1 << 15;

const CMD_RSP_R1: u32 = 1 << 7;
const CMD_RSP_R1B: u32 = 6 << 7;
const CMD_DTYPE_SINGLE: u32 = 1 << 11;
const CMD_DTYPE_MULTI: u32 = 2 << 11;
const CMD_STOP: u32 = 1 << 14;

const MMC_SELECT_CARD: u32 = 7;
const MMC_SEND_EXT_CSD: u32 = 8;
const MMC_STOP_TRANSMISSION: u32 = 12;
const MMC_SEND_STATUS: u32 = 13;
const MMC_READ_SINGLE: u32 = 17;
const MMC_READ_MULTI: u32 = 18;

const CARD_STATE_STBY: u32 = 3;
const CARD_STATE_TRAN: u32 = 4;
const CARD_STATE_DATA: u32 = 5;
const CARD_STATE_PRG: u32 = 7;

const LOCK_TIMEOUT_US: u64 = 100_000;
const IDLE_TIMEOUT_US: u64 = 250_000;
const CMD_TIMEOUT_US: u64 = 250_000;
const DATA_TIMEOUT_US: u64 = 1_500_000;
const RESET_TIMEOUT_US: u64 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmmcError {
    ControllerUnavailable,
    LockBusy,
    InvalidRange,
    BufferTooSmall,
    BusyTimeout,
    CmdTimeout,
    CmdCrc,
    DataTimeout,
    DataCrc,
    FifoStall,
    CardState(u32),
}

static EMMC_LOCK: IrqSpinLock = IrqSpinLock::new();
static INT_SEEN: AtomicU32 = AtomicU32::new(0);
static CARD_SECTORS: AtomicU64 = AtomicU64::new(0);
static IO_ERRORS: AtomicU32 = AtomicU32::new(0);

#[inline]
const fn cmd_blklen(n: u32) -> u32 {
    (n & 0xfff) << 16
}

#[inline]
pub const fn card_state_from_r1(r1: u32) -> u32 {
    (r1 >> 9) & 0x0f
}

/// Extracts eMMC bus width from `SDC_CFG` bits `[17:16]`: `0=1-bit`, `1=4-bit`, `2=8-bit`.
pub const fn decode_sdc_bus_width(sdc_cfg: u32) -> u8 {
    match (sdc_cfg >> 16) & 3 {
        0 => 1,
        1 => 4,
        2 => 8,
        _ => 0,
    }
}

fn wait_idle() -> Result<(), EmmcError> {
    let mut dl = Deadline::from_us(IDLE_TIMEOUT_US);
    loop {
        let sts = rd32(MSDC0_BASE + SDC_STS);
        if sts == 0xffff_ffff {
            return Err(EmmcError::ControllerUnavailable);
        }
        if (sts & SDC_STS_BUSY) == 0 {
            return Ok(());
        }
        if dl.expired() {
            return Err(EmmcError::BusyTimeout);
        }
    }
}

fn clear_fifo_and_int() {
    wr32(MSDC0_BASE + MSDC_CFG, rd32(MSDC0_BASE + MSDC_CFG) | MSDC_CFG_PIO);
    let pending = rd32(MSDC0_BASE + MSDC_INT);
    if pending != 0 {
        wr32(MSDC0_BASE + MSDC_INT, pending);
    }
    INT_SEEN.store(0, Ordering::Relaxed);
    wr32(
        MSDC0_BASE + MSDC_FIFOCS,
        rd32(MSDC0_BASE + MSDC_FIFOCS) | MSDC_FIFOCS_CLR,
    );
    dsb();
    let mut dl = Deadline::from_us(RESET_TIMEOUT_US);
    while (rd32(MSDC0_BASE + MSDC_FIFOCS) & MSDC_FIFOCS_CLR) != 0 {
        if dl.expired() {
            break;
        }
    }
}

/// Performs a bounded MSDC0 controller soft-reset (`MSDC_CFG_RST`) without disturbing
/// card power, bus width, or LK's HS400/HS200 pad tuning.
fn recover_controller() {
    let cfg = rd32(MSDC0_BASE + MSDC_CFG);
    if cfg == 0xffff_ffff {
        return;
    }
    wr32(MSDC0_BASE + MSDC_CFG, cfg | MSDC_CFG_RST);
    dsb();
    let mut dl = Deadline::from_us(RESET_TIMEOUT_US);
    while (rd32(MSDC0_BASE + MSDC_CFG) & MSDC_CFG_RST) != 0 {
        if dl.expired() {
            break;
        }
    }
    clear_fifo_and_int();
}

fn wait_int(want: u32, bad: u32, timeout_us: u64) -> Result<(), EmmcError> {
    let mut dl = Deadline::from_us(timeout_us);
    loop {
        let status = rd32(MSDC0_BASE + MSDC_INT);
        if status != 0 {
            wr32(MSDC0_BASE + MSDC_INT, status);
            INT_SEEN.fetch_or(status, Ordering::Relaxed);
        }
        let seen = INT_SEEN.load(Ordering::Relaxed);
        if (seen & bad) != 0 {
            INT_SEEN.store(0, Ordering::Relaxed);
            return Err(if (seen & INT_CMDTMO) != 0 {
                EmmcError::CmdTimeout
            } else if (seen & INT_RSPCRCERR) != 0 {
                EmmcError::CmdCrc
            } else if (seen & INT_DATTMO) != 0 {
                EmmcError::DataTimeout
            } else {
                EmmcError::DataCrc
            });
        }
        if (seen & want) != 0 {
            INT_SEEN.store(seen & !want, Ordering::Relaxed);
            return Ok(());
        }
        if dl.expired() {
            return Err(if (want & INT_CMDRDY) != 0 {
                EmmcError::CmdTimeout
            } else {
                EmmcError::DataTimeout
            });
        }
    }
}

fn send_stop_unlocked() -> Result<(), EmmcError> {
    wr32(MSDC0_BASE + SDC_ARG, 0);
    wr32(
        MSDC0_BASE + SDC_CMD,
        MMC_STOP_TRANSMISSION | CMD_RSP_R1B | CMD_STOP,
    );
    dsb();
    wait_int(INT_CMDRDY, INT_CMDTMO | INT_RSPCRCERR, CMD_TIMEOUT_US)?;
    wait_idle()
}

fn send_status_unlocked(rca: u32) -> Result<u32, EmmcError> {
    wait_idle()?;
    clear_fifo_and_int();
    wr32(MSDC0_BASE + SDC_ARG, rca << 16);
    wr32(MSDC0_BASE + SDC_CMD, MMC_SEND_STATUS | CMD_RSP_R1);
    dsb();
    wait_int(INT_CMDRDY, INT_CMDTMO | INT_RSPCRCERR, CMD_TIMEOUT_US)?;
    Ok(rd32(MSDC0_BASE + SDC_RESP0))
}

fn select_card_unlocked(rca: u32) -> Result<(), EmmcError> {
    wait_idle()?;
    clear_fifo_and_int();
    wr32(MSDC0_BASE + SDC_ARG, rca << 16);
    wr32(MSDC0_BASE + SDC_CMD, MMC_SELECT_CARD | CMD_RSP_R1B);
    dsb();
    wait_int(INT_CMDRDY, INT_CMDTMO | INT_RSPCRCERR, CMD_TIMEOUT_US)?;
    wait_idle()
}

fn drain_pio_blocks(blocks: u32, dst: &mut [u8]) -> Result<(), EmmcError> {
    let total_words = (blocks as usize) * (SECTOR / 4);
    let mut word_idx = 0usize;
    while word_idx < total_words {
        let mut fifo = (rd32(MSDC0_BASE + MSDC_FIFOCS) & 0xff) as usize;
        let mut dl = Deadline::from_us(DATA_TIMEOUT_US);
        while fifo < 4 {
            let status = rd32(MSDC0_BASE + MSDC_INT);
            if (status & (INT_DATTMO | INT_DATCRCERR)) != 0 {
                wr32(MSDC0_BASE + MSDC_INT, status);
                if blocks > 1 {
                    let _ = send_stop_unlocked();
                }
                return Err(if (status & INT_DATTMO) != 0 {
                    EmmcError::DataTimeout
                } else {
                    EmmcError::DataCrc
                });
            }
            if dl.expired() {
                if blocks > 1 {
                    let _ = send_stop_unlocked();
                }
                return Err(EmmcError::FifoStall);
            }
            fifo = (rd32(MSDC0_BASE + MSDC_FIFOCS) & 0xff) as usize;
        }
        let avail_words = (fifo / 4).min(total_words - word_idx);
        for _ in 0..avail_words {
            let word = rd32(MSDC0_BASE + MSDC_RXDATA).to_le_bytes();
            let off = word_idx * 4;
            dst[off..off + 4].copy_from_slice(&word);
            word_idx += 1;
        }
    }
    wait_int(
        INT_XFER_COMPL,
        INT_DATTMO | INT_DATCRCERR,
        DATA_TIMEOUT_US,
    )
}

/// Reads the 512-byte eMMC `EXT_CSD` register via `CMD8` (strictly read-only).
fn read_ext_csd_unlocked(dst: &mut [u8; SECTOR]) -> Result<(), EmmcError> {
    wait_idle()?;
    clear_fifo_and_int();
    wr32(MSDC0_BASE + SDC_BLK_NUM, 1);
    wr32(MSDC0_BASE + SDC_ARG, 0);
    wr32(
        MSDC0_BASE + SDC_CMD,
        MMC_SEND_EXT_CSD | CMD_RSP_R1 | CMD_DTYPE_SINGLE | cmd_blklen(SECTOR as u32),
    );
    dsb();
    wait_int(INT_CMDRDY, INT_CMDTMO | INT_RSPCRCERR, CMD_TIMEOUT_US)?;
    drain_pio_blocks(1, dst)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtCsdInfo {
    pub sec_count: u64,
    pub rev: u8,
    pub bus_width_code: u8,
    pub hs_timing: u8,
    pub device_type: u8,
}

/// Parses key read-only geometry/capability fields from a 512-byte eMMC `EXT_CSD`.
pub fn parse_ext_csd(buf: &[u8; SECTOR]) -> Option<ExtCsdInfo> {
    let sec_count = u32::from_le_bytes([buf[212], buf[213], buf[214], buf[215]]) as u64;
    if sec_count == 0 {
        return None;
    }
    Some(ExtCsdInfo {
        sec_count,
        rev: buf[192],
        bus_width_code: buf[183],
        hs_timing: buf[185],
        device_type: buf[196],
    })
}

fn read_chunk_unlocked(lba: u64, blocks: u32, dst: &mut [u8]) -> Result<(), EmmcError> {
    if blocks == 0 {
        return Ok(());
    }
    let needed = (blocks as usize)
        .checked_mul(SECTOR)
        .ok_or(EmmcError::InvalidRange)?;
    if dst.len() < needed {
        return Err(EmmcError::BufferTooSmall);
    }
    if lba > (u32::MAX as u64) {
        return Err(EmmcError::InvalidRange);
    }
    let cap = CARD_SECTORS.load(Ordering::Acquire);
    if cap > 0 {
        let end = lba
            .checked_add(blocks as u64)
            .ok_or(EmmcError::InvalidRange)?;
        if end > cap {
            return Err(EmmcError::InvalidRange);
        }
    }

    wait_idle()?;
    clear_fifo_and_int();
    wr32(MSDC0_BASE + SDC_BLK_NUM, blocks);
    wr32(MSDC0_BASE + SDC_ARG, lba as u32);
    let cmd = if blocks == 1 {
        MMC_READ_SINGLE | CMD_RSP_R1 | CMD_DTYPE_SINGLE | cmd_blklen(SECTOR as u32)
    } else {
        MMC_READ_MULTI | CMD_RSP_R1 | CMD_DTYPE_MULTI | cmd_blklen(SECTOR as u32)
    };
    wr32(MSDC0_BASE + SDC_CMD, cmd);
    dsb();
    wait_int(INT_CMDRDY, INT_CMDTMO | INT_RSPCRCERR, CMD_TIMEOUT_US)?;
    drain_pio_blocks(blocks, dst)?;
    if blocks > 1 {
        send_stop_unlocked()?;
    }
    Ok(())
}

pub fn read_retrying_checked(lba: u64, blocks: u32, dst: &mut [u8]) -> Result<(), EmmcError> {
    let _guard = EMMC_LOCK
        .try_lock_timeout_us(LOCK_TIMEOUT_US)
        .ok_or(EmmcError::LockBusy)?;

    let mut last_err = EmmcError::BusyTimeout;
    for attempt in 0..EMMC_MAX_RETRIES {
        match read_chunk_unlocked(lba, blocks, dst) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err;
                IO_ERRORS.fetch_add(1, Ordering::Relaxed);
                if attempt + 1 < EMMC_MAX_RETRIES {
                    recover_controller();
                    let _ = send_stop_unlocked();
                }
            }
        }
    }
    Err(last_err)
}

pub fn read_retrying(lba: u64, blocks: u32, dst: &mut [u8]) -> bool {
    read_retrying_checked(lba, blocks, dst).is_ok()
}

pub fn card_capacity_sectors() -> u64 {
    CARD_SECTORS.load(Ordering::Acquire)
}

pub fn io_error_count() -> u32 {
    IO_ERRORS.load(Ordering::Relaxed)
}

/// Validates a Protective MBR at LBA 0 (0x55AA signature + partition type `0xEE`).
pub fn is_protective_mbr(mbr: &[u8; SECTOR]) -> bool {
    if mbr[510] != 0x55 || mbr[511] != 0xaa {
        return false;
    }
    for i in 0..4 {
        let off = 446 + i * 16;
        if mbr[off + 4] == 0xee {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GptHeader {
    pub revision: u32,
    pub header_size: u32,
    pub my_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub entries_lba: u64,
    pub entry_count: u32,
    pub entry_size: u32,
    pub entries_crc32: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    BadSignature,
    BadHeaderSize,
    BadHeaderCrc,
    BadMyLba,
    BadUsableRange,
    ExceedsCapacity,
    UnsupportedEntryLayout,
    BadEntriesCrc,
}

/// Validates a 512-byte primary GPT header sector including IEEE CRC-32 and capacity bounds.
pub fn parse_gpt_header(hdr: &[u8; SECTOR], card_sectors: Option<u64>) -> Result<GptHeader, GptError> {
    if &hdr[0..8] != b"EFI PART" {
        return Err(GptError::BadSignature);
    }
    let revision = u32::from_le_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]);
    let header_size = u32::from_le_bytes([hdr[12], hdr[13], hdr[14], hdr[15]]);
    if !(92..=SECTOR as u32).contains(&header_size) {
        return Err(GptError::BadHeaderSize);
    }
    let stored_crc = u32::from_le_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]);
    let mut tmp = [0u8; SECTOR];
    let hlen = header_size as usize;
    tmp[..hlen].copy_from_slice(&hdr[..hlen]);
    tmp[16..20].fill(0);
    if crc32_ieee(&tmp[..hlen]) != stored_crc {
        return Err(GptError::BadHeaderCrc);
    }

    let my_lba = u64::from_le_bytes(hdr[24..32].try_into().unwrap());
    if my_lba != 1 {
        return Err(GptError::BadMyLba);
    }
    let first_usable_lba = u64::from_le_bytes(hdr[40..48].try_into().unwrap());
    let last_usable_lba = u64::from_le_bytes(hdr[48..56].try_into().unwrap());
    if first_usable_lba < 2 || last_usable_lba < first_usable_lba {
        return Err(GptError::BadUsableRange);
    }
    if let Some(cap) = card_sectors {
        if last_usable_lba >= cap {
            return Err(GptError::ExceedsCapacity);
        }
    }

    let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().unwrap());
    let entry_count = u32::from_le_bytes(hdr[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(hdr[84..88].try_into().unwrap());
    let entries_crc32 = u32::from_le_bytes(hdr[88..92].try_into().unwrap());

    if entries_lba < 2
        || entry_count == 0
        || entry_count > 128
        || entry_size < 128
        || (entry_size & 7) != 0
        || (entry_count as usize).saturating_mul(entry_size as usize) > 128 * 128
    {
        return Err(GptError::UnsupportedEntryLayout);
    }

    Ok(GptHeader {
        revision,
        header_size,
        my_lba,
        first_usable_lba,
        last_usable_lba,
        entries_lba,
        entry_count,
        entry_size,
        entries_crc32,
    })
}

pub fn verify_gpt_entries_crc(hdr: &GptHeader, entries_buf: &[u8]) -> Result<(), GptError> {
    let bytes = (hdr.entry_count as usize)
        .checked_mul(hdr.entry_size as usize)
        .ok_or(GptError::UnsupportedEntryLayout)?;
    if entries_buf.len() < bytes {
        return Err(GptError::UnsupportedEntryLayout);
    }
    if crc32_ieee(&entries_buf[..bytes]) != hdr.entries_crc32 {
        return Err(GptError::BadEntriesCrc);
    }
    Ok(())
}

fn utf16le_matches(entry: &[u8], name: &str) -> bool {
    let bytes = name.as_bytes();
    // GPT partition name is 72 bytes (36 UTF-16LE code units) at offset 56..128.
    if bytes.len() > 36 || 56 + bytes.len() * 2 > entry.len() {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if entry[56 + i * 2] != b || entry[56 + i * 2 + 1] != 0 {
            return false;
        }
    }
    if bytes.len() == 36 {
        true
    } else {
        entry[56 + bytes.len() * 2] == 0 && entry[56 + bytes.len() * 2 + 1] == 0
    }
}

/// Finds a named GPT partition and validates its LBA span strictly within `[first_usable_lba..=last_usable_lba]`
/// and real eMMC card capacity (if known) with checked arithmetic.
pub fn find_gpt_entry_checked(
    hdr: &GptHeader,
    entries: &[u8],
    wanted: &str,
    card_sectors: Option<u64>,
) -> Option<(u64, u64)> {
    let count = hdr.entry_count as usize;
    let size = hdr.entry_size as usize;
    if entries.len() < count.checked_mul(size)? {
        return None;
    }
    for i in 0..count {
        let off = i * size;
        let e = &entries[off..off + size];
        // Unused entry has an all-zero Type GUID.
        if e[0..16].iter().all(|&b| b == 0) {
            continue;
        }
        if utf16le_matches(e, wanted) {
            let first = u64::from_le_bytes(e[32..40].try_into().ok()?);
            let last = u64::from_le_bytes(e[40..48].try_into().ok()?);
            if first < hdr.first_usable_lba || last < first || last > hdr.last_usable_lba {
                return None;
            }
            if let Some(cap) = card_sectors {
                if last >= cap {
                    return None;
                }
            }
            let blocks = last.checked_sub(first)?.checked_add(1)?;
            return Some((first, blocks));
        }
    }
    None
}

pub struct EmmcPartition {
    channel: u8,
    model: [u8; 40],
    model_len: usize,
    first_lba: u64,
    sectors: u64,
}

impl EmmcPartition {
    const fn empty(channel: u8) -> Self {
        Self {
            channel,
            model: [0u8; 40],
            model_len: 0,
            first_lba: 0,
            sectors: 0,
        }
    }

    fn configure(&mut self, label: &str, first_lba: u64, sectors: u64) {
        let bytes = label.as_bytes();
        let n = bytes.len().min(self.model.len());
        self.model = [0u8; 40];
        self.model[..n].copy_from_slice(&bytes[..n]);
        self.model_len = n;
        self.first_lba = first_lba;
        self.sectors = sectors;
    }

    pub fn validate_read(&self, lba: u64, count: u32, buf_len: usize) -> Option<u64> {
        let total_bytes = (count as usize).checked_mul(SECTOR)?;
        if buf_len < total_bytes {
            return None;
        }
        let end_lba = lba.checked_add(count as u64)?;
        if end_lba > self.sectors {
            return None;
        }
        self.first_lba.checked_add(lba)
    }
}

impl BlockDevice for EmmcPartition {
    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            channel: self.channel,
            model: self.model,
            model_len: self.model_len,
            block_count: self.sectors,
            block_size: SECTOR as u32,
            device_type: DeviceType::HardDisk,
        }
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        let mut cur_lba = self.validate_read(lba, count, buffer.len()).ok_or(())?;
        let mut left = count;
        let mut offset = 0usize;
        while left > 0 {
            let chunk = left.min(EMMC_MAX_BLOCKS);
            let bytes = (chunk as usize) * SECTOR;
            if read_retrying_checked(cur_lba, chunk, &mut buffer[offset..offset + bytes]).is_err() {
                return Err(());
            }
            cur_lba = cur_lba.checked_add(chunk as u64).ok_or(())?;
            left -= chunk;
            offset += bytes;
        }
        Ok(())
    }

    fn write_blocks(&mut self, _lba: u64, _count: u32, _buffer: &[u8]) -> Result<(), ()> {
        // CRITICAL SAFETY RULE: X6826C eMMC driver is strictly read-only.
        Err(())
    }
}

static mut SUPER_PART: EmmcPartition = EmmcPartition::empty(0);
static mut DATA_PART: EmmcPartition = EmmcPartition::empty(1);
static mut GPT_BUF: [u8; 128 * 128] = [0u8; 128 * 128];

/// Verifies bootloader card handoff state (CMD13 -> optional CMD7/CMD12 -> CMD8 EXT_CSD).
fn probe_card_handoff() -> Result<(u32, Option<ExtCsdInfo>), EmmcError> {
    let _guard = EMMC_LOCK
        .try_lock_timeout_us(LOCK_TIMEOUT_US)
        .ok_or(EmmcError::LockBusy)?;

    let mut r1 = send_status_unlocked(EMMC_DEFAULT_RCA)?;
    let mut state = card_state_from_r1(r1);
    if state == CARD_STATE_STBY {
        select_card_unlocked(EMMC_DEFAULT_RCA)?;
        r1 = send_status_unlocked(EMMC_DEFAULT_RCA)?;
        state = card_state_from_r1(r1);
    } else if state == CARD_STATE_DATA {
        let _ = send_stop_unlocked();
        r1 = send_status_unlocked(EMMC_DEFAULT_RCA)?;
        state = card_state_from_r1(r1);
    } else if state == CARD_STATE_PRG {
        let mut dl = Deadline::from_ms(500);
        while state == CARD_STATE_PRG && !dl.expired() {
            r1 = send_status_unlocked(EMMC_DEFAULT_RCA)?;
            state = card_state_from_r1(r1);
        }
    }
    if state != CARD_STATE_TRAN {
        return Err(EmmcError::CardState(state));
    }

    let mut ext_csd = [0u8; SECTOR];
    let info = match read_ext_csd_unlocked(&mut ext_csd) {
        Ok(()) => parse_ext_csd(&ext_csd),
        Err(_) => {
            recover_controller();
            None
        }
    };
    Ok((state, info))
}

pub fn init(host: &mut dyn DriverHost) {
    let cfg = rd32(MSDC0_BASE + MSDC_CFG);
    let sts = rd32(MSDC0_BASE + SDC_STS);
    let sdc_cfg = rd32(MSDC0_BASE + SDC_CFG);
    let ps = rd32(MSDC0_BASE + MSDC_PS);
    if cfg == 0xffff_ffff || sts == 0xffff_ffff {
        host.log("mtk-msdc0-emmc: denetleyici erisilemez");
        return;
    }
    let bus_width = decode_sdc_bus_width(sdc_cfg);
    let clk_active = (cfg & MSDC_CFG_MODE) != 0 && (cfg & MSDC_CFG_CKPDN) != 0;

    let card_sectors = match probe_card_handoff() {
        Ok((state, Some(ext))) => {
            CARD_SECTORS.store(ext.sec_count, Ordering::Release);
            let mut msg = LogBuf::<224>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: kart tran({}) EXT_CSD rev={} {} MiB ({} sektor, {}-bit, hs={:#x}, clk={})",
                state,
                ext.rev,
                (ext.sec_count * SECTOR as u64) >> 20,
                ext.sec_count,
                bus_width,
                ext.hs_timing,
                clk_active as u8
            );
            host.log(msg.as_str());
            Some(ext.sec_count)
        }
        Ok((state, None)) => {
            let mut msg = LogBuf::<192>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: kart tran({}) fakat EXT_CSD okunamadi (bus={}-bit, PS={:#x}); GPT sinirlari kullanilacak",
                state, bus_width, ps
            );
            host.log(msg.as_str());
            None
        }
        Err(err) => {
            let mut msg = LogBuf::<192>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: CMD13 kart el-sikismasi basarisiz ({:?}, CFG={:#x}, STS={:#x}); dogrudan GPT deneniyor",
                err, cfg, sts
            );
            host.log(msg.as_str());
            None
        }
    };

    // Check Protective MBR at LBA 0 (informational + integrity check).
    let mut mbr = [0u8; SECTOR];
    let pmbr_ok = read_retrying_checked(0, 1, &mut mbr).is_ok() && is_protective_mbr(&mbr);

    let mut hdr_buf = [0u8; SECTOR];
    if let Err(err) = read_retrying_checked(1, 1, &mut hdr_buf) {
        let mut msg = LogBuf::<160>::new();
        let _ = write!(
            msg,
            "mtk-msdc0-emmc: LBA 1 GPT basligi okunamadi ({:?})",
            err
        );
        host.log(msg.as_str());
        return;
    }

    let gpt = match parse_gpt_header(&hdr_buf, card_sectors) {
        Ok(g) => g,
        Err(err) => {
            let mut msg = LogBuf::<160>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: gecersiz GPT basligi ({:?}, pmbr={})",
                err, pmbr_ok as u8
            );
            host.log(msg.as_str());
            return;
        }
    };

    // If EXT_CSD did not report capacity, use GPT's last_usable_lba + 1 as a safe upper bound.
    if card_sectors.is_none() {
        CARD_SECTORS.store(gpt.last_usable_lba.saturating_add(1), Ordering::Release);
    }

    let entry_bytes = (gpt.entry_count as usize) * (gpt.entry_size as usize);
    let gpt_sectors = entry_bytes.div_ceil(SECTOR) as u32;
    let read_bytes = (gpt_sectors as usize) * SECTOR;
    let gpt_slice = unsafe {
        core::slice::from_raw_parts_mut(addr_of_mut!(GPT_BUF) as *mut u8, read_bytes)
    };
    if let Err(err) = read_retrying_checked(gpt.entries_lba, gpt_sectors, gpt_slice) {
        let mut msg = LogBuf::<160>::new();
        let _ = write!(
            msg,
            "mtk-msdc0-emmc: GPT giris tablosu okunamadi ({:?})",
            err
        );
        host.log(msg.as_str());
        return;
    }
    if let Err(err) = verify_gpt_entries_crc(&gpt, gpt_slice) {
        let mut msg = LogBuf::<160>::new();
        let _ = write!(
            msg,
            "mtk-msdc0-emmc: GPT giris CRC32 dogrulamasi basarisiz ({:?}); bolumler baglanmadi",
            err
        );
        host.log(msg.as_str());
        return;
    }

    let sys_candidates = ["super", "system_a", "system"];
    for name in sys_candidates {
        if let Some((first, blocks)) = find_gpt_entry_checked(&gpt, gpt_slice, name, card_sectors) {
            unsafe {
                let part = &mut *addr_of_mut!(SUPER_PART);
                part.configure("mtk-emmc-super", first, blocks);
                host.add_block_device(part);
            }
            let mut msg = LogBuf::<192>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: GPT '{}' baglandi (LBA {:#x}, {} MiB, CRC32 ok, pmbr={}, salt-okunur)",
                name,
                first,
                (blocks * SECTOR as u64) >> 20,
                pmbr_ok as u8
            );
            host.log(msg.as_str());
            break;
        }
    }

    let data_candidates = ["userdata", "ncdata"];
    for name in data_candidates {
        if let Some((first, blocks)) = find_gpt_entry_checked(&gpt, gpt_slice, name, card_sectors) {
            unsafe {
                let part = &mut *addr_of_mut!(DATA_PART);
                part.configure("mtk-emmc-userdata", first, blocks);
                host.add_block_device(part);
            }
            let mut msg = LogBuf::<192>::new();
            let _ = write!(
                msg,
                "mtk-msdc0-emmc: GPT '{}' baglandi (LBA {:#x}, {} MiB, CRC32 ok, salt-okunur)",
                name,
                first,
                (blocks * SECTOR as u64) >> 20
            );
            host.log(msg.as_str());
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_valid_gpt(entries: &[(&str, u64, u64)], last_usable: u64) -> ([u8; SECTOR], [u8; 128 * 128]) {
        let mut table = [0u8; 128 * 128];
        for (idx, &(name, first, last)) in entries.iter().enumerate() {
            let off = idx * 128;
            table[off] = 0xaf; // non-zero Type GUID
            table[off + 32..off + 40].copy_from_slice(&first.to_le_bytes());
            table[off + 40..off + 48].copy_from_slice(&last.to_le_bytes());
            for (ci, b) in name.bytes().enumerate() {
                table[off + 56 + ci * 2] = b;
                table[off + 56 + ci * 2 + 1] = 0;
            }
        }
        let entries_crc = crc32_ieee(&table[..128 * 128]);
        let mut hdr = [0u8; SECTOR];
        hdr[0..8].copy_from_slice(b"EFI PART");
        hdr[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        hdr[12..16].copy_from_slice(&92u32.to_le_bytes());
        hdr[24..32].copy_from_slice(&1u64.to_le_bytes());
        hdr[40..48].copy_from_slice(&34u64.to_le_bytes());
        hdr[48..56].copy_from_slice(&last_usable.to_le_bytes());
        hdr[72..80].copy_from_slice(&2u64.to_le_bytes());
        hdr[80..84].copy_from_slice(&128u32.to_le_bytes());
        hdr[84..88].copy_from_slice(&128u32.to_le_bytes());
        hdr[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let hdr_crc = crc32_ieee(&hdr[..92]);
        hdr[16..20].copy_from_slice(&hdr_crc.to_le_bytes());
        (hdr, table)
    }

    #[test]
    fn gpt_crc_and_partition_lookup() {
        let cap = 250_000_000u64; // ~128 GB
        let (hdr_buf, table) = build_valid_gpt(
            &[("boot", 1024, 66559), ("super", 100_000, 8_488_607), ("userdata", 9_000_000, 200_000_000)],
            cap - 34,
        );
        let hdr = parse_gpt_header(&hdr_buf, Some(cap)).expect("valid GPT header");
        assert!(verify_gpt_entries_crc(&hdr, &table).is_ok());
        assert_eq!(find_gpt_entry_checked(&hdr, &table, "super", Some(cap)), Some((100_000, 8_388_608)));
        assert_eq!(find_gpt_entry_checked(&hdr, &table, "userdata", Some(cap)), Some((9_000_000, 191_000_001)));
        assert_eq!(find_gpt_entry_checked(&hdr, &table, "missing", Some(cap)), None);
    }

    #[test]
    fn gpt_rejects_corrupt_header_and_oob_partition() {
        let cap = 1_000_000u64;
        let (mut hdr_buf, mut table) = build_valid_gpt(&[("super", 100, 999_999)], cap - 34);
        // Out-of-bounds partition exceeds last_usable_lba
        let hdr = parse_gpt_header(&hdr_buf, Some(cap)).unwrap();
        assert_eq!(find_gpt_entry_checked(&hdr, &table, "super", Some(cap)), None);

        // Corrupt entry CRC
        table[0] ^= 0xff;
        assert_eq!(verify_gpt_entries_crc(&hdr, &table), Err(GptError::BadEntriesCrc));

        // Corrupt header CRC
        hdr_buf[40] ^= 1;
        assert_eq!(parse_gpt_header(&hdr_buf, Some(cap)), Err(GptError::BadHeaderCrc));
    }

    #[test]
    fn partition_bounds_and_read_only_enforced() {
        let mut p = EmmcPartition::empty(0);
        p.configure("super", 1000, 500);
        assert_eq!(p.validate_read(0, 1, 512), Some(1000));
        assert_eq!(p.validate_read(499, 1, 512), Some(1499));
        assert_eq!(p.validate_read(500, 1, 512), None);
        assert_eq!(p.validate_read(u64::MAX, 1, 512), None);
        assert_eq!(p.validate_read(0, 2, 512), None); // buffer too small
        assert!(p.write_blocks(0, 1, &[0u8; 512]).is_err());
    }

    #[test]
    fn ext_csd_and_pmbr_parsing() {
        let mut ext = [0u8; SECTOR];
        ext[192] = 8; // eMMC 5.1
        ext[212..216].copy_from_slice(&249_737_216u32.to_le_bytes());
        let info = parse_ext_csd(&ext).unwrap();
        assert_eq!(info.sec_count, 249_737_216);
        assert_eq!(info.rev, 8);

        let mut mbr = [0u8; SECTOR];
        assert!(!is_protective_mbr(&mbr));
        mbr[446 + 4] = 0xee;
        mbr[510] = 0x55;
        mbr[511] = 0xaa;
        assert!(is_protective_mbr(&mbr));
    }
}
