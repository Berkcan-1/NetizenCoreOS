use crate::{mmio_read32, mmio_read8, mmio_write32, mmio_write8};
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TIMER_INTID: u32 = 27;

const GICD_CTLR: usize = 0x000;
const GICR_WAKER: usize = 0x014;
const SGI_FRAME: usize = 0x10000;
const GICR_IGROUPR0: usize = 0x080;
const GICR_ISENABLER0: usize = 0x100;
const GICR_ICENABLER0: usize = 0x180;
const GICR_IPRIORITYR: usize = 0x400;
const SPINS: u32 = 1_000_000;

static READY: AtomicBool = AtomicBool::new(false);

pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}

unsafe fn write_sre(value: u64) {
    asm!("msr s3_0_c12_c12_5, {}", "isb", in(reg) value, options(nomem, nostack));
}

unsafe fn read_sre() -> u64 {
    let value: u64;
    asm!("mrs {}, s3_0_c12_c12_5", out(reg) value, options(nomem, nostack, preserves_flags));
    value
}

pub fn init(distributor: usize, redistributor: usize) -> bool {
    if distributor == 0 || redistributor == 0 {
        return false;
    }
    unsafe {
        write_sre(read_sre() | 1);
        if read_sre() & 1 == 0 {
            return false;
        }

        mmio_write32(distributor + GICD_CTLR, 0);
        let mut spins = 0u32;
        while mmio_read32(distributor + GICD_CTLR) & (1 << 31) != 0 {
            spins += 1;
            if spins > SPINS {
                return false;
            }
        }
        mmio_write32(distributor + GICD_CTLR, (1 << 4) | (1 << 1));
        spins = 0;
        while mmio_read32(distributor + GICD_CTLR) & (1 << 31) != 0 {
            spins += 1;
            if spins > SPINS {
                return false;
            }
        }

        let waker = mmio_read32(redistributor + GICR_WAKER);
        mmio_write32(redistributor + GICR_WAKER, waker & !(1 << 1));
        spins = 0;
        while mmio_read32(redistributor + GICR_WAKER) & (1 << 2) != 0 {
            spins += 1;
            if spins > SPINS {
                return false;
            }
        }

        let sgi = redistributor + SGI_FRAME;
        mmio_write32(sgi + GICR_IGROUPR0, 0xFFFF_FFFF);
        mmio_write32(sgi + GICR_ICENABLER0, 0xFFFF_FFFF);
        mmio_write8(sgi + GICR_IPRIORITYR + TIMER_INTID as usize, 0x80);
        let _ = mmio_read8(sgi + GICR_IPRIORITYR + TIMER_INTID as usize);
        mmio_write32(sgi + GICR_ISENABLER0, 1 << TIMER_INTID);

        asm!(
            "msr s3_0_c4_c6_0, {pmr}",
            "msr s3_0_c12_c12_3, xzr",
            "msr s3_0_c12_c12_7, {one}",
            "isb",
            pmr = in(reg) 0xFFu64,
            one = in(reg) 1u64,
            options(nomem, nostack)
        );
    }
    READY.store(true, Ordering::Release);
    true
}

pub fn acknowledge() -> u32 {
    let value: u64;
    unsafe {
        asm!("mrs {}, s3_0_c12_c12_0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    (value & 0xFF_FFFF) as u32
}

pub fn end(intid: u32) {
    unsafe {
        asm!("msr s3_0_c12_c12_1, {}", "isb", in(reg) intid as u64, options(nomem, nostack));
    }
}
