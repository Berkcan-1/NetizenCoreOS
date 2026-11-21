use crate::{mmio_read32, mmio_read8, mmio_write32, mmio_write8};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

pub const KIND_NONE: u8 = 0;
pub const KIND_PL011: u8 = 1;
pub const KIND_NS16550: u8 = 2;
pub const KIND_NS16550_32: u8 = 3;

static BASE: AtomicUsize = AtomicUsize::new(0);
static KIND: AtomicU8 = AtomicU8::new(KIND_NONE);
static SHIFT: AtomicU8 = AtomicU8::new(0);

const READY_SPINS: u32 = 200_000;

pub fn configure(kind: u8, base: usize, reg_shift: u8) {
    BASE.store(base, Ordering::Relaxed);
    SHIFT.store(reg_shift, Ordering::Relaxed);
    KIND.store(kind, Ordering::Release);
}

pub fn init() {
    let base = BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    match KIND.load(Ordering::Acquire) {
        KIND_PL011 => unsafe {
            mmio_write32(base + 0x30, 0);
            mmio_write32(base + 0x44, 0x7FF);
            mmio_write32(base + 0x2C, 0x70);
            mmio_write32(base + 0x30, 0x301);
        },
        _ => {}
    }
}

pub fn present() -> bool {
    BASE.load(Ordering::Relaxed) != 0 && KIND.load(Ordering::Acquire) != KIND_NONE
}

fn reg16550(index: usize) -> usize {
    BASE.load(Ordering::Relaxed) + (index << SHIFT.load(Ordering::Relaxed))
}

pub fn write_byte(byte: u8) {
    let base = BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let mut spins = 0u32;
    match KIND.load(Ordering::Acquire) {
        KIND_PL011 => unsafe {
            while mmio_read32(base + 0x18) & (1 << 5) != 0 {
                spins += 1;
                if spins > READY_SPINS {
                    return;
                }
            }
            mmio_write32(base, byte as u32);
        },
        KIND_NS16550 => unsafe {
            while mmio_read8(reg16550(5)) & 0x20 == 0 {
                spins += 1;
                if spins > READY_SPINS {
                    return;
                }
            }
            mmio_write8(reg16550(0), byte);
        },
        KIND_NS16550_32 => unsafe {
            while mmio_read32(reg16550(5)) & 0x20 == 0 {
                spins += 1;
                if spins > READY_SPINS {
                    return;
                }
            }
            mmio_write32(reg16550(0), byte as u32);
        },
        _ => {}
    }
}

pub fn write_str(text: &str) {
    for byte in text.bytes() {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}
