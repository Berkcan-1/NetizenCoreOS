//! ARMv8-A CSPRNG and Hardware Entropy Pool (Feature 15).
//!
//! Security guarantees:
//! - Hardware TRNG is detected via `ID_AA64ISAR0_EL1.RNDR` (bits [63:60]).
//! - Cryptographic random requests (`fill_secure_bytes`, `try_secure_u64`)
//!   use hardware `RNDR` to seed and periodically reseed an RFC 8439 ChaCha20
//!   DRBG, and FAIL CLOSED (`Err(RngError::EntropyUnavailable)`) when no
//!   hardware TRNG is present.
//! - The non-cryptographic fallback (`next_u64` / `fill_non_crypto_bytes`) is
//!   NEVER used to satisfy cryptographic requests.

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

const RESEED_INTERVAL_BLOCKS: u64 = 1024;
const CHACHA_CONST: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyStatus {
    Unavailable = 0,
    HardwareTrng = 1,
    TestDeterministic = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RngError {
    EntropyUnavailable,
    HardwareFailure,
}

static NON_CRYPTO_STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
static DRBG_LOCK: AtomicBool = AtomicBool::new(false);
static DRBG_STATUS: AtomicU8 = AtomicU8::new(0);

struct DrbgState {
    initialized: bool,
    key: [u32; 8],
    nonce: [u32; 3],
    counter: u64,
    blocks_since_reseed: u64,
}

impl DrbgState {
    const fn new() -> Self {
        Self {
            initialized: false,
            key: [0; 8],
            nonce: [0; 3],
            counter: 0,
            blocks_since_reseed: 0,
        }
    }
}

static mut DRBG: DrbgState = DrbgState::new();

#[inline]
fn acquire_lock() {
    while DRBG_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

#[inline]
fn release_lock() {
    DRBG_LOCK.store(false, Ordering::Release);
}

#[inline]
fn qr(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(7);
}

fn chacha20_block(key: &[u32; 8], counter: u32, nonce: &[u32; 3]) -> [u8; 64] {
    let init: [u32; 16] = [
        CHACHA_CONST[0],
        CHACHA_CONST[1],
        CHACHA_CONST[2],
        CHACHA_CONST[3],
        key[0],
        key[1],
        key[2],
        key[3],
        key[4],
        key[5],
        key[6],
        key[7],
        counter,
        nonce[0],
        nonce[1],
        nonce[2],
    ];
    let mut x = init;
    for _ in 0..10 {
        qr(&mut x, 0, 4, 8, 12);
        qr(&mut x, 1, 5, 9, 13);
        qr(&mut x, 2, 6, 10, 14);
        qr(&mut x, 3, 7, 11, 15);
        qr(&mut x, 0, 5, 10, 15);
        qr(&mut x, 1, 6, 11, 12);
        qr(&mut x, 2, 7, 8, 13);
        qr(&mut x, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        let w = x[i].wrapping_add(init[i]).to_le_bytes();
        out[i * 4..i * 4 + 4].copy_from_slice(&w);
    }
    out
}

pub fn rndr_supported() -> bool {
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

fn read_hw_seed_u64() -> Result<u64, RngError> {
    if !rndr_supported() {
        return Err(RngError::EntropyUnavailable);
    }
    for _ in 0..16 {
        if let Some(v) = read_rndr() {
            return Ok(v);
        }
    }
    Err(RngError::HardwareFailure)
}

fn reseed_from_hardware(state: &mut DrbgState) -> Result<(), RngError> {
    let mut words = [0u64; 6];
    for slot in words.iter_mut() {
        *slot = read_hw_seed_u64()?;
    }
    for i in 0..4 {
        state.key[i * 2] ^= words[i] as u32;
        state.key[i * 2 + 1] ^= (words[i] >> 32) as u32;
    }
    state.nonce[0] ^= words[4] as u32;
    state.nonce[1] ^= (words[4] >> 32) as u32;
    state.nonce[2] ^= words[5] as u32;
    state.counter = state.counter.wrapping_add(1);
    state.blocks_since_reseed = 0;
    state.initialized = true;
    DRBG_STATUS.store(EntropyStatus::HardwareTrng as u8, Ordering::Release);
    Ok(())
}

pub struct HardwareRng;

impl HardwareRng {
    pub fn status() -> EntropyStatus {
        match DRBG_STATUS.load(Ordering::Acquire) {
            1 => EntropyStatus::HardwareTrng,
            2 => EntropyStatus::TestDeterministic,
            _ => {
                if rndr_supported() {
                    EntropyStatus::HardwareTrng
                } else {
                    EntropyStatus::Unavailable
                }
            }
        }
    }

    /// Cryptographically secure 64-bit value from hardware TRNG + ChaCha20 DRBG.
    /// Returns `None` (fail-closed) when no hardware entropy is available.
    pub fn try_secure_u64() -> Option<u64> {
        let mut buf = [0u8; 8];
        if Self::fill_secure_bytes_checked(&mut buf).is_ok() {
            Some(u64::from_le_bytes(buf))
        } else {
            None
        }
    }

    /// Fill `dest` with cryptographically secure bytes (fail-closed if no hardware TRNG).
    pub fn fill_secure_bytes_checked(dest: &mut [u8]) -> Result<(), RngError> {
        if dest.is_empty() {
            return Ok(());
        }
        acquire_lock();
        let res = unsafe {
            let state = &mut *core::ptr::addr_of_mut!(DRBG);
            if !state.initialized || state.blocks_since_reseed >= RESEED_INTERVAL_BLOCKS {
                if let Err(e) = reseed_from_hardware(state) {
                    release_lock();
                    return Err(e);
                }
            }
            let mut offset = 0usize;
            while offset < dest.len() {
                if state.blocks_since_reseed >= RESEED_INTERVAL_BLOCKS {
                    if let Err(e) = reseed_from_hardware(state) {
                        release_lock();
                        return Err(e);
                    }
                }
                let ctr = (state.counter & 0xFFFF_FFFF) as u32;
                let block = chacha20_block(&state.key, ctr, &state.nonce);
                state.counter = state.counter.wrapping_add(1);
                state.blocks_since_reseed += 1;
                let take = (dest.len() - offset).min(64);
                dest[offset..offset + take].copy_from_slice(&block[..take]);
                offset += take;
            }
            // Fast key erasure (backtracking resistance): overwrite key with next block.
            let erase = chacha20_block(&state.key, (state.counter & 0xFFFF_FFFF) as u32, &state.nonce);
            state.counter = state.counter.wrapping_add(1);
            for i in 0..8 {
                state.key[i] = u32::from_le_bytes([
                    erase[i * 4],
                    erase[i * 4 + 1],
                    erase[i * 4 + 2],
                    erase[i * 4 + 3],
                ]);
            }
            Ok(())
        };
        release_lock();
        res
    }

    pub fn fill_secure_bytes(dest: &mut [u8]) -> Result<(), ()> {
        Self::fill_secure_bytes_checked(dest).map_err(|_| ())
    }

    /// Non-cryptographic PRNG for jitter/non-security uses only.
    pub fn next_u64() -> u64 {
        if let Some(value) = Self::try_secure_u64() {
            return value;
        }
        let mut x = (NON_CRYPTO_STATE.load(Ordering::Relaxed) ^ crate::time::read_tsc()) | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        NON_CRYPTO_STATE.store(x, Ordering::Relaxed);
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
