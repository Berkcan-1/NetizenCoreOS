use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

fn rndr_supported() -> bool {
    let isar0: u64;
    unsafe {
        asm!("mrs {}, id_aa64isar0_el1", out(reg) isar0, options(nomem, nostack, preserves_flags));
    }
    ((isar0 >> 60) & 0xF) != 0
}

fn read_rndr() -> Option<u64> {
    let value: u64;
    let failed: u64;
    unsafe {
        asm!(
            "mrs {value}, s3_3_c2_c4_0",
            "cset {failed}, eq",
            value = out(reg) value,
            failed = out(reg) failed,
            options(nomem, nostack)
        );
    }
    if failed == 0 {
        Some(value)
    } else {
        None
    }
}

pub struct HardwareRng;

impl HardwareRng {
    pub fn try_secure_u64() -> Option<u64> {
        if !rndr_supported() {
            return None;
        }
        for _ in 0..8 {
            if let Some(value) = read_rndr() {
                return Some(value);
            }
        }
        None
    }

    pub fn fill_secure_bytes(dest: &mut [u8]) -> Result<(), ()> {
        let mut offset = 0;
        while offset < dest.len() {
            let word = Self::try_secure_u64().ok_or(())?.to_le_bytes();
            let take = (dest.len() - offset).min(8);
            dest[offset..offset + take].copy_from_slice(&word[..take]);
            offset += take;
        }
        Ok(())
    }

    pub fn next_u64() -> u64 {
        if let Some(value) = Self::try_secure_u64() {
            return value;
        }
        let mut x = (STATE.load(Ordering::Relaxed) ^ crate::time::read_tsc()) | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        STATE.store(x, Ordering::Relaxed);
        x
    }

    pub fn fill_bytes(dest: &mut [u8]) {
        let mut offset = 0;
        while offset < dest.len() {
            let word = Self::next_u64().to_le_bytes();
            let take = (dest.len() - offset).min(8);
            dest[offset..offset + take].copy_from_slice(&word[..take]);
            offset += take;
        }
    }
}
