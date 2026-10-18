// crates/nc-arch/src/rand.rs
use core::arch::asm;
use core::arch::x86_64::{__cpuid, __cpuid_count};
use core::sync::atomic::{AtomicU64, Ordering};

pub struct HardwareRng;

static FALLBACK_STATE: AtomicU64 = AtomicU64::new(0x853C_49E6_748F_EA9B);

#[inline]
fn has_rdrand() -> bool {
    (__cpuid(1).ecx & (1 << 30)) != 0
}

#[inline]
fn has_rdseed() -> bool {
    if __cpuid(0).eax < 7 {
        return false;
    }
    (__cpuid_count(7, 0).ebx & (1 << 18)) != 0
}

impl HardwareRng {
    /// Bulgu #12: Yalnizca gercek donanim entropisi (RDSEED veya RDRAND) basariliysa doner.
    /// Anahtar, nonce ve guvenlik belirtecleri yalnizca bu fonksiyonu veya `fill_secure_bytes`i kullanmalidir.
    pub fn try_secure_u64() -> Option<u64> {
        if has_rdseed() {
            for _ in 0..16 {
                let mut val: u64;
                let mut success: u8;
                unsafe {
                    asm!(
                        "rdseed {0}",
                        "setc {1}",
                        out(reg) val,
                        out(reg_byte) success,
                        options(nomem, nostack)
                    );
                }
                if success == 1 {
                    return Some(val);
                }
            }
        }
        if has_rdrand() {
            for _ in 0..16 {
                let mut val: u64;
                let mut success: u8;
                unsafe {
                    asm!(
                        "rdrand {0}",
                        "setc {1}",
                        out(reg) val,
                        out(reg_byte) success,
                        options(nomem, nostack)
                    );
                }
                if success == 1 {
                    return Some(val);
                }
            }
        }
        None
    }

    /// Kriptografik kullanim icin: Donanim entropisi yoksa `Err(())` doner, PRNG fallback'e dusmez.
    pub fn fill_secure_bytes(dest: &mut [u8]) -> Result<(), ()> {
        let mut chunks = dest.chunks_exact_mut(8);
        for chunk in chunks.by_ref() {
            let val = Self::try_secure_u64().ok_or(())?;
            chunk.copy_from_slice(&val.to_le_bytes());
        }
        let remainder = chunks.into_remainder();
        if !remainder.is_empty() {
            let val = Self::try_secure_u64().ok_or(())?;
            remainder.copy_from_slice(&val.to_le_bytes()[..remainder.len()]);
        }
        Ok(())
    }

    /// Yalnizca UI/efekt/genel amacli hizli rastgele sayi uretimi (Kriptografik DEGILDIR)
    pub fn next_u64() -> u64 {
        if let Some(hw) = Self::try_secure_u64() {
            return hw;
        }
        let tsc = crate::time::read_tsc();
        let prev = FALLBACK_STATE.fetch_add(
            0x9E37_79B9_7F4A_7C15u64.wrapping_add(tsc),
            Ordering::Relaxed,
        );
        let mut z = prev.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn fill_bytes(dest: &mut [u8]) {
        let mut chunks = dest.chunks_exact_mut(8);
        for chunk in chunks.by_ref() {
            chunk.copy_from_slice(&Self::next_u64().to_le_bytes());
        }
        let remainder = chunks.into_remainder();
        if !remainder.is_empty() {
            let bytes = Self::next_u64().to_le_bytes();
            remainder.copy_from_slice(&bytes[..remainder.len()]);
        }
    }
}
