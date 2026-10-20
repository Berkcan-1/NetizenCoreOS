#![no_std]
extern crate alloc;

pub mod crypto;
pub mod manifest;
pub mod dysonsphere;
pub use dysonsphere::{
    authorize_active, authorize_app, clear_active_caller, init_dysonsphere, set_active_caller,
    AuthDecision, Capability, DysonSphereEngine,
};

//use alloc::string::String;
use alloc::vec::Vec;
use crypto::{compute_sha256, decode_hex};
pub use manifest::PackageManifest;

pub const NTZC_MAGIC: [u8; 4] = *b"NTZC";
pub const NTZC_MAGIC_V2: [u8; 4] = *b"NTZ2";

pub const PERM_SYSTEM_SETTINGS: u32 = 1 << 0;
pub const PERM_POWER_CONTROL: u32 = 1 << 1;
pub const PERM_STORAGE_FULL: u32 = 1 << 2;

pub static TRUSTED_PUBLISHERS: &[[u8; 32]] = &[
    [
        0x3b, 0x6a, 0x27, 0xbc, 0xce, 0xb6, 0xa4, 0x2d,
        0x62, 0xa3, 0xa8, 0xd0, 0x2a, 0x6f, 0x0d, 0x73,
        0x65, 0x32, 0x15, 0x77, 0x1d, 0xe2, 0x43, 0xa6,
        0x3a, 0xc0, 0x48, 0xa1, 0x8b, 0x59, 0xda, 0x29,
    ],
];

#[derive(Clone, Debug)]
pub struct NtzcPackage {
    pub uid: u32,
    pub permissions: u32,
    pub manifest: PackageManifest,
    pub payload: Vec<u8>,
    pub is_verified: bool,
}

impl NtzcPackage {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 28 {
            return None;
        }

        // NTZ2 (v2) Korumalı Format
        if &bytes[0..4] == &NTZC_MAGIC_V2 {
            if bytes.len() < 64 {
                return None;
            }
            let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            if version != 2 {
                return None;
            }
            let m_off = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
            let m_len = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
            let p_off = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;
            let p_len = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]) as usize;

            let total_m = m_off.checked_add(m_len)?;
            let total_p = p_off.checked_add(p_len)?;
            if total_m > bytes.len() || total_p > bytes.len() {
                return None;
            }

            let expected_hash = &bytes[32..64];
            let payload_bytes = &bytes[p_off..p_off + p_len];
            let actual_hash = compute_sha256(payload_bytes);

            if expected_hash != actual_hash {
                return None;
            }

            let manifest_text = core::str::from_utf8(&bytes[m_off..m_off + m_len]).ok()?;
            let manifest = PackageManifest::parse(manifest_text)?;

            let mut pubkey = [0u8; 32];
            let key_hex = manifest.publisher_key_id.trim_start_matches("ed25519:");
            if decode_hex(key_hex, &mut pubkey).is_err() {
                return None;
            }
            if !TRUSTED_PUBLISHERS.iter().any(|k| k == &pubkey) {
                return None;
            }

            let mut payload = Vec::with_capacity(p_len);
            payload.extend_from_slice(payload_bytes);

            let _ = dysonsphere::init_dysonsphere();
            return Some(NtzcPackage {
                uid: 1001,
                permissions: 0,
                manifest,
                payload,
                is_verified: true,
            });
        }

        // NTZC (v1) Klasik Format
        if &bytes[0..4] == &NTZC_MAGIC {
            let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            if version != 1 {
                return None;
            }
            let uid = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            let permissions = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
            let manifest_len = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
            let payload_offset = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
            let payload_len = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;

            let total_m = 28usize.checked_add(manifest_len)?;
            let total_p = payload_offset.checked_add(payload_len)?;
            if total_m > bytes.len() || total_p > bytes.len() {
                return None;
            }

            let manifest_text = core::str::from_utf8(&bytes[28..28 + manifest_len]).ok()?;
            let manifest = PackageManifest::parse(manifest_text)?;

            let mut payload = Vec::with_capacity(payload_len);
            payload.extend_from_slice(&bytes[payload_offset..payload_offset + payload_len]);

            return Some(NtzcPackage {
                uid,
                permissions,
                manifest,
                payload,
                is_verified: false,
            });
        }

        None
    }

    pub fn is_system(&self) -> bool {
        self.uid == 1000
    }

    pub fn is_trusted_builtin(app_id: &str) -> bool {
        matches!(
            app_id,
            "nc.settings" | "nc.files" | "nc.system" | "nc.stopwatch" | "nc.browser"
        )
    }

    pub fn role_label(&self) -> &'static str {
        if self.is_system() || Self::is_trusted_builtin(&self.manifest.id) {
            "Sistem Uygulamasi"
        } else if self.is_verified {
            "Dogrulanmis Uygulama"
        } else {
            "Kullanici Uygulamasi"
        }
    }

    pub fn permission_labels(&self) -> Vec<&'static str> {
        let mut list = Vec::new();
        if (self.permissions & PERM_SYSTEM_SETTINGS) != 0 {
            list.push("Gorunum ve Sistem Ayarlarini Degistirme");
        }
        if (self.permissions & PERM_POWER_CONTROL) != 0 {
            list.push("Cihazi Kapatma ve Yeniden Baslatma");
        }
        if (self.permissions & PERM_STORAGE_FULL) != 0 {
            list.push("Depolama Alanindaki Dosyalara Erisim");
        }
        if list.is_empty() {
            list.push("Temel Uygulama Izinleri (Korumali)");
        }
        list
    }
}
