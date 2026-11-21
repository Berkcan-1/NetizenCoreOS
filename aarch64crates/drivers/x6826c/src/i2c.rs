//! MediaTek MT6768 I2C Master FIFO driver for Infinix Hot 20 (X6826C).
//!
//! Verified directly from:
//! - X6826C `stock/dtb` + `stock/dtbo`:
//!   - `i2c5@11016000` (IRQ SPI 0x93 -> GIC INTID 179, `clock-div = 5`, open-drain,
//!     SCL=GPIO48, SDA=GPIO49): hosts `rt-swchg@53` (Richtek RT9471 primary charger)
//!   - `i2c6@1100d000` (IRQ SPI 0x94 -> GIC INTID 180, `clock-div = 5`, open-drain):
//!     hosts `i2c_lcd_bias@3e`
//! - X6826C stock `lk_rafw.bin.c` (`FUN_000015b4`, `FUN_00001a10`, `FUN_00001cc8`):
//!   verified register offsets, `I2C_CLOCK_DIV = 0x404`, `I2C_IO_CONFIG = 3`,
//!   `I2C_CONTROL` flags (`RS_STOP | CLK_EXT_EN | DIR_CHANGE | ACKERR_DET_EN`).
//!
//! Strictly supports read-only register probing/reading (`write_read` for reg address
//! pointer + data read) to avoid unintended writes to PMU/charger/bias ICs.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use nc_device::DriverHost;

use crate::mmio::{dsb, rd16, rd32, rd8, wr16, wr8, Deadline, IrqSpinLock, LogBuf};

pub const I2C5_BASE: usize = 0x1101_6000;
pub const I2C6_BASE: usize = 0x1100_d000;

/// Verified MT6768 I2C controller register offsets (from X6826C `lk_rafw.bin.c`).
const REG_DATA_PORT: usize = 0x00;
const REG_SLAVE_ADDR: usize = 0x04;
const REG_INTR_MASK: usize = 0x08;
const REG_INTR_STAT: usize = 0x0c;
const REG_CONTROL: usize = 0x10;
const REG_TRANSFER_LEN: usize = 0x14;
const REG_TRANSAC_LEN: usize = 0x18;
const REG_DELAY_LEN: usize = 0x1c;
const REG_TIMING: usize = 0x20;
const REG_START: usize = 0x24;
const REG_EXT_CONF: usize = 0x28;
const REG_LTIMING: usize = 0x2c;
const REG_HS: usize = 0x30;
const REG_IO_CONFIG: usize = 0x34;
const REG_FIFO_ADDR_CLR: usize = 0x38;
const REG_TRANSFER_LEN_AUX: usize = 0x44;
const REG_CLOCK_DIV: usize = 0x48;
const REG_SOFTRESET: usize = 0x50;

const INTR_TRANSAC_COMP: u16 = 1 << 0;
const INTR_ACKERR: u16 = 1 << 1;
const INTR_HS_NACKERR: u16 = 1 << 2;
const INTR_ALL_ERR: u16 = INTR_ACKERR | INTR_HS_NACKERR;
const INTR_ALL: u16 = INTR_TRANSAC_COMP | INTR_ALL_ERR;

/// `I2C_CONTROL` bits verified from X6826C `lk_rafw.bin.c` L9443–9452:
/// base `0x00a8` (RS/ACKERR_DET_EN/CLK_EXT_EN), plus `0x10` (DIR_CHANGE) and `0x02` (RS_STOP).
const CTRL_RS: u16 = 1 << 1;
const CTRL_CLK_EXT_EN: u16 = 1 << 3;
const CTRL_DIR_CHANGE: u16 = 1 << 4;
const CTRL_ACKERR_DET_EN: u16 = 1 << 5;
const CTRL_TRANSFER_LEN_CHANGE: u16 = 1 << 7;

/// Safe 100 kHz fallback timing for MT6768 I2C when LK has not configured a bus.
const DEFAULT_CLOCK_DIV: u16 = 0x0404; // div 5, matches DTS `clock-div = <5>` & LK `0x404`
const DEFAULT_EXT_CONF_100K: u16 = 0x8001;
const DEFAULT_TIMING_100K: u16 = 0x1320;
const DEFAULT_LTIMING_100K: u16 = 0x1320;
const IO_CONFIG_OPEN_DRAIN: u16 = 0x0003;

const FIFO_MAX_BYTES: usize = 8;
const I2C_LOCK_TIMEOUT_US: u64 = 10_000;
const I2C_TX_TIMEOUT_US: u64 = 15_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cError {
    BusUnavailable,
    LockTimeout,
    InvalidLength,
    AckError,
    Timeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cBus {
    Bus5,
    Bus6,
}

impl I2cBus {
    #[inline]
    pub const fn base(self) -> usize {
        match self {
            I2cBus::Bus5 => I2C5_BASE,
            I2cBus::Bus6 => I2C6_BASE,
        }
    }

    fn lock(self) -> &'static IrqSpinLock {
        match self {
            I2cBus::Bus5 => &I2C5_LOCK,
            I2cBus::Bus6 => &I2C6_LOCK,
        }
    }

    fn ready_flag(self) -> &'static AtomicBool {
        match self {
            I2cBus::Bus5 => &I2C5_READY,
            I2cBus::Bus6 => &I2C6_READY,
        }
    }
}

static I2C5_LOCK: IrqSpinLock = IrqSpinLock::new();
static I2C6_LOCK: IrqSpinLock = IrqSpinLock::new();
static I2C5_READY: AtomicBool = AtomicBool::new(false);
static I2C6_READY: AtomicBool = AtomicBool::new(false);

pub fn is_ready(bus: I2cBus) -> bool {
    bus.ready_flag().load(Ordering::Acquire)
}

fn soft_reset(base: usize) {
    wr16(base + REG_SOFTRESET, 1);
    dsb();
    wr16(base + REG_SLAVE_ADDR, 0);
    dsb();
    wr16(base + REG_INTR_STAT, INTR_ALL);
    dsb();
    wr16(base + REG_FIFO_ADDR_CLR, 5);
    dsb();
}

/// Reads `out.len()` (1..=8) bytes starting at register `reg` from 7-bit `addr7`
/// using an atomic write-then-read repeated-start FIFO transaction.
pub fn read_regs(bus: I2cBus, addr7: u8, reg: u8, out: &mut [u8]) -> Result<(), I2cError> {
    if !is_ready(bus) {
        return Err(I2cError::BusUnavailable);
    }
    if out.is_empty() || out.len() > FIFO_MAX_BYTES {
        return Err(I2cError::InvalidLength);
    }
    let _guard = bus
        .lock()
        .try_lock_timeout_us(I2C_LOCK_TIMEOUT_US)
        .ok_or(I2cError::LockTimeout)?;

    let base = bus.base();
    // Preserve LK's timing if already configured, otherwise set conservative 100 kHz open-drain.
    let existing_timing = rd16(base + REG_TIMING);
    let existing_ltiming = rd16(base + REG_LTIMING);
    let existing_div = rd16(base + REG_CLOCK_DIV);
    wr16(
        base + REG_CLOCK_DIV,
        if existing_div == 0 { DEFAULT_CLOCK_DIV } else { existing_div },
    );
    wr16(base + REG_EXT_CONF, DEFAULT_EXT_CONF_100K);
    wr16(
        base + REG_TIMING,
        if existing_timing == 0 { DEFAULT_TIMING_100K } else { existing_timing },
    );
    wr16(
        base + REG_LTIMING,
        if existing_ltiming == 0 { DEFAULT_LTIMING_100K } else { existing_ltiming },
    );
    wr16(base + REG_HS, 0);
    wr16(base + REG_DELAY_LEN, 2);
    wr16(base + REG_IO_CONFIG, IO_CONFIG_OPEN_DRAIN);

    // Write-then-read with repeated start (`RS | CLK_EXT_EN | DIR_CHANGE | ACKERR_DET_EN | LEN_CHANGE`).
    let ctrl = CTRL_RS | CTRL_CLK_EXT_EN | CTRL_DIR_CHANGE | CTRL_ACKERR_DET_EN | CTRL_TRANSFER_LEN_CHANGE;
    wr16(base + REG_CONTROL, ctrl);
    wr16(base + REG_SLAVE_ADDR, ((addr7 & 0x7f) as u16) << 1);
    wr16(base + REG_INTR_STAT, INTR_ALL);
    wr16(base + REG_FIFO_ADDR_CLR, 5);
    let mask = rd16(base + REG_INTR_MASK);
    wr16(base + REG_INTR_MASK, mask & !INTR_ALL);
    wr16(base + REG_TRANSFER_LEN, 1);
    wr16(base + REG_TRANSFER_LEN_AUX, out.len() as u16);
    wr16(base + REG_TRANSAC_LEN, 2);
    dsb();

    wr8(base + REG_DATA_PORT, reg);
    dsb();
    wr16(base + REG_START, 1);
    dsb();

    let mut dl = Deadline::from_us(I2C_TX_TIMEOUT_US);
    loop {
        let stat = rd16(base + REG_INTR_STAT);
        if (stat & INTR_ALL_ERR) != 0 {
            soft_reset(base);
            return Err(I2cError::AckError);
        }
        if (stat & INTR_TRANSAC_COMP) != 0 {
            wr16(base + REG_INTR_STAT, stat & INTR_ALL);
            dsb();
            break;
        }
        if dl.expired() {
            soft_reset(base);
            return Err(I2cError::Timeout);
        }
    }

    for slot in out.iter_mut() {
        *slot = rd8(base + REG_DATA_PORT);
    }
    dsb();
    Ok(())
}

/// Convenience helper to read a single 8-bit register from `addr7`.
pub fn read_reg(bus: I2cBus, addr7: u8, reg: u8) -> Result<u8, I2cError> {
    let mut byte = [0u8; 1];
    read_regs(bus, addr7, reg, &mut byte)?;
    Ok(byte[0])
}

fn probe_bus(bus: I2cBus) -> (bool, u16, u16) {
    let base = bus.base();
    let probe = rd32(base + REG_INTR_MASK);
    if probe == 0xffff_ffff {
        bus.ready_flag().store(false, Ordering::Release);
        return (false, 0, 0);
    }
    let div = rd16(base + REG_CLOCK_DIV);
    let timing = rd16(base + REG_TIMING);
    bus.ready_flag().store(true, Ordering::Release);
    (true, div, timing)
}

pub fn init(host: &mut dyn DriverHost) {
    let (ok5, div5, tim5) = probe_bus(I2cBus::Bus5);
    let (ok6, div6, tim6) = probe_bus(I2cBus::Bus6);
    let mut msg = LogBuf::<192>::new();
    let _ = write!(
        msg,
        "mtk-i2c: i2c5={} (div={:#x} tim={:#x}), i2c6={} (div={:#x} tim={:#x})",
        if ok5 { "hazir" } else { "yok" },
        div5,
        tim5,
        if ok6 { "hazir" } else { "yok" },
        div6,
        tim6
    );
    host.log(msg.as_str());
}
