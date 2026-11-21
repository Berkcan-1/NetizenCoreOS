//! Thin Rust adapter over the authoritative SPARK Ada Lock & Credential module (`NC_Security_Lock`).
//!
//! All credential validation, PBKDF2 derivation, sector integrity hashing,
//! constant-time verification, and fail-closed state evaluation execute in SPARK Ada.
use crate::{constant_time_eq, pbkdf2_hmac_sha256, sha256};

pub const KIND_PIN: u8 = 1;
pub const KIND_PASSWORD: u8 = 2;
pub const DEFAULT_ITERATIONS: u32 = 40_000;
pub const PIN_LENGTH: usize = 4;
pub const PASSWORD_MIN: usize = 6;
pub const PASSWORD_MAX: usize = 32;

const CREDENTIAL_MAGIC: &[u8; 8] = b"NCLOCKV1";
const MARKER_MAGIC: &[u8; 8] = b"NCLOCKMK";

pub fn secret_is_valid(kind: u8, secret: &[u8]) -> bool {
    unsafe { nc_spark_sec::nc_sec_lock_secret_valid(kind, secret.as_ptr(), secret.len()) != 0 }
}

pub fn lockout_seconds_for(fails: u32) -> u32 {
    unsafe { nc_spark_sec::nc_sec_lock_lockout_secs(fails) }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LockRecord {
    pub kind: u8,
    pub iterations: u32,
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

impl LockRecord {
    pub fn derive(kind: u8, secret: &[u8], salt: [u8; 16], iterations: u32) -> LockRecord {
        let mut hash = [0u8; 32];
        pbkdf2_hmac_sha256(secret, &salt, iterations, &mut hash);
        LockRecord { kind, iterations, salt, hash }
    }

    pub fn verify(&self, secret: &[u8]) -> bool {
        if !secret_is_valid(self.kind, secret) {
            return false;
        }
        let mut hash = [0u8; 32];
        pbkdf2_hmac_sha256(secret, &self.salt, self.iterations, &mut hash);
        let ok = constant_time_eq(&hash, &self.hash);
        crate::secure_zero(&mut hash);
        ok
    }

    pub fn to_sector(&self) -> [u8; 512] {
        let mut sector = [0u8; 512];
        sector[0..8].copy_from_slice(CREDENTIAL_MAGIC);
        sector[8] = 1;
        sector[9] = self.kind;
        sector[12..16].copy_from_slice(&self.iterations.to_le_bytes());
        sector[16..32].copy_from_slice(&self.salt);
        sector[32..64].copy_from_slice(&self.hash);
        let tail = sha256(&sector[0..480]);
        sector[480..512].copy_from_slice(&tail);
        sector
    }

    pub fn from_sector(sector: &[u8]) -> Option<LockRecord> {
        if sector.len() != 512 {
            return None;
        }
        let mut cred = [0u8; 512];
        cred.copy_from_slice(sector);
        let marker = Marker {
            enabled: true,
            fails: 0,
            credential_digest: Self::sector_digest(&cred),
        }
        .to_sector();
        match evaluate(&cred, &marker) {
            LockState::Enabled { record, .. } => Some(record),
            _ => None,
        }
    }

    pub fn sector_digest(sector: &[u8; 512]) -> [u8; 32] {
        sha256(sector)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub enabled: bool,
    pub fails: u32,
    pub credential_digest: [u8; 32],
}

impl Marker {
    pub fn to_sector(&self) -> [u8; 512] {
        let mut sector = [0u8; 512];
        sector[0..8].copy_from_slice(MARKER_MAGIC);
        sector[8] = 1;
        sector[9] = if self.enabled { 1 } else { 0 };
        sector[12..16].copy_from_slice(&self.fails.to_le_bytes());
        sector[16..48].copy_from_slice(&self.credential_digest);
        let tail = sha256(&sector[0..480]);
        sector[480..512].copy_from_slice(&tail);
        sector
    }

    pub fn from_sector(sector: &[u8]) -> Option<Marker> {
        if sector.len() != 512 || &sector[0..8] != MARKER_MAGIC || sector[8] != 1 {
            return None;
        }
        let tail = sha256(&sector[0..480]);
        if !constant_time_eq(&tail, &sector[480..512]) {
            return None;
        }
        if sector[9] != 0 && sector[9] != 1 {
            return None;
        }
        let fails = u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]);
        let mut credential_digest = [0u8; 32];
        credential_digest.copy_from_slice(&sector[16..48]);
        Some(Marker { enabled: sector[9] == 1, fails, credential_digest })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    Disabled,
    Enabled { record: LockRecord, fails: u32 },
    Destroyed,
}

/// Authoritative sector evaluation delegated to SPARK Ada (`nc_sec_lock_evaluate_sectors`).
pub fn evaluate(credential_sector: &[u8; 512], marker_sector: &[u8; 512]) -> LockState {
    let mut kind = 0u8;
    let mut iterations = 0u32;
    let mut fails = 0u32;
    let mut salt = [0u8; 16];
    let mut hash = [0u8; 32];
    let status = unsafe {
        nc_spark_sec::nc_sec_lock_evaluate_sectors(
            credential_sector.as_ptr(),
            marker_sector.as_ptr(),
            &mut kind,
            &mut iterations,
            &mut fails,
            salt.as_mut_ptr(),
            hash.as_mut_ptr(),
        )
    };
    match status {
        nc_spark_sec::LOCK_DISABLED => LockState::Disabled,
        nc_spark_sec::LOCK_ENABLED => LockState::Enabled {
            record: LockRecord { kind, iterations, salt, hash },
            fails,
        },
        _ => LockState::Destroyed,
    }
}
