//! Ekran kilidi (arayüz kilidi) kimlik bilgisi kaydı.
//!
//! GÜVENLİK SINIRI (F-10): 4 haneli PIN yalnızca 10^4 olasılıktır. Kayıt sektörünü okuyabilen bir
//! saldırgan PBKDF2'ye rağmen çevrimdışı olarak saniyeler içinde PIN'i bulur. Bu kilit, başı boş
//! bir kişiye ve yanlışlıkla erişime karşıdır; DİSK ŞİFRELEMESİ veya veri koruması DEĞİLDİR ve
//! öyle sunulmamalıdır. Gerçek koruma için: parola tabanlı disk anahtarı türetme + uzun parola +
//! (varsa) TPM/donanım yavaşlatma gerekir.
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
    match kind {
        KIND_PIN => secret.len() == PIN_LENGTH && secret.iter().all(|b| b.is_ascii_digit()),
        KIND_PASSWORD => secret.len() >= PASSWORD_MIN && secret.len() <= PASSWORD_MAX,
        _ => false,
    }
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
        constant_time_eq(&hash, &self.hash)
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
        if sector.len() != 512 || &sector[0..8] != CREDENTIAL_MAGIC || sector[8] != 1 {
            return None;
        }
        let tail = sha256(&sector[0..480]);
        if !constant_time_eq(&tail, &sector[480..512]) {
            return None;
        }
        let kind = sector[9];
        if kind != KIND_PIN && kind != KIND_PASSWORD {
            return None;
        }
        let iterations = u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]);
        if iterations == 0 {
            return None;
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&sector[16..32]);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&sector[32..64]);
        Some(LockRecord { kind, iterations, salt, hash })
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

pub fn evaluate(credential_sector: &[u8; 512], marker_sector: &[u8; 512]) -> LockState {
    let credential_blank = credential_sector.iter().all(|b| *b == 0);
    let marker_blank = marker_sector.iter().all(|b| *b == 0);
    if credential_blank && marker_blank {
        return LockState::Disabled;
    }
    let marker = match Marker::from_sector(marker_sector) {
        Some(m) => m,
        None => return LockState::Destroyed,
    };
    if !marker.enabled {
        return if credential_blank { LockState::Disabled } else { LockState::Destroyed };
    }
    let record = match LockRecord::from_sector(credential_sector) {
        Some(r) => r,
        None => return LockState::Destroyed,
    };
    if !constant_time_eq(&LockRecord::sector_digest(credential_sector), &marker.credential_digest) {
        return LockState::Destroyed;
    }
    LockState::Enabled { record, fails: marker.fails }
}
