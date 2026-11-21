//! Isolated Per-Application Key Store (Feature 16).
//!
//! Backed exclusively by the authoritative SPARK Ada cryptographic core
//! (`nc_crypto::aead_encrypt`, `aead_decrypt`, `hmac_sha256`, `secure_zero`).
//! Raw key material is NEVER returned to EL0 applications; callers invoke
//! scoped encrypt/decrypt/sign/rotate/revoke operations authenticated by
//! kernel-verified `caller_app_id`.

pub const MAX_KEYSTORE_ENTRIES: usize = 64;
pub const MAX_KEY_NAME: usize = 32;
pub const MAX_APP_ID: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStoreError {
    TableFull,
    InvalidInput,
    NotFound,
    PermissionDenied,
    DeviceLocked,
    KeyRevoked,
    CryptoFailed,
    AlreadyExists,
}

#[derive(Clone, Copy)]
struct KeyEntry {
    in_use: bool,
    revoked: bool,
    auth_required: bool,
    version: u32,
    owner_len: u8,
    name_len: u8,
    owner_id: [u8; MAX_APP_ID],
    key_name: [u8; MAX_KEY_NAME],
    key_material: [u8; 32],
}

impl KeyEntry {
    const EMPTY: Self = Self {
        in_use: false,
        revoked: false,
        auth_required: true,
        version: 0,
        owner_len: 0,
        name_len: 0,
        owner_id: [0; MAX_APP_ID],
        key_name: [0; MAX_KEY_NAME],
        key_material: [0; 32],
    };

    fn matches_name(&self, name: &str) -> bool {
        let b = name.as_bytes();
        self.name_len as usize == b.len() && &self.key_name[..b.len()] == b
    }

    fn matches_owner(&self, app_id: &str) -> bool {
        let b = app_id.as_bytes();
        self.owner_len as usize == b.len() && &self.owner_id[..b.len()] == b
    }
}

pub struct AppKeyStore {
    entries: [KeyEntry; MAX_KEYSTORE_ENTRIES],
}

impl AppKeyStore {
    pub const fn new() -> Self {
        Self {
            entries: [KeyEntry::EMPTY; MAX_KEYSTORE_ENTRIES],
        }
    }

    pub fn create_key(
        &mut self,
        caller_app_id: &str,
        key_name: &str,
        raw_key: &[u8; 32],
        auth_required: bool,
    ) -> Result<u32, KeyStoreError> {
        let app_b = caller_app_id.as_bytes();
        let name_b = key_name.as_bytes();
        if app_b.is_empty() || app_b.len() > MAX_APP_ID || name_b.is_empty() || name_b.len() > MAX_KEY_NAME {
            return Err(KeyStoreError::InvalidInput);
        }
        if self.entries.iter().any(|e| e.in_use && e.matches_owner(caller_app_id) && e.matches_name(key_name)) {
            return Err(KeyStoreError::AlreadyExists);
        }
        let slot = self
            .entries
            .iter_mut()
            .find(|e| !e.in_use)
            .ok_or(KeyStoreError::TableFull)?;
        *slot = KeyEntry::EMPTY;
        slot.in_use = true;
        slot.auth_required = auth_required;
        slot.version = 1;
        slot.owner_len = app_b.len() as u8;
        slot.name_len = name_b.len() as u8;
        slot.owner_id[..app_b.len()].copy_from_slice(app_b);
        slot.key_name[..name_b.len()].copy_from_slice(name_b);
        slot.key_material.copy_from_slice(raw_key);
        Ok(1)
    }

    fn find_authorized(
        &self,
        caller_app_id: &str,
        key_name: &str,
        device_locked: bool,
    ) -> Result<&KeyEntry, KeyStoreError> {
        let mut saw_other_owner = false;
        for entry in self.entries.iter() {
            if !entry.in_use || !entry.matches_name(key_name) {
                continue;
            }
            if !entry.matches_owner(caller_app_id) {
                saw_other_owner = true;
                continue;
            }
            if entry.revoked {
                return Err(KeyStoreError::KeyRevoked);
            }
            if entry.auth_required && device_locked {
                return Err(KeyStoreError::DeviceLocked);
            }
            return Ok(entry);
        }
        if saw_other_owner {
            Err(KeyStoreError::PermissionDenied)
        } else {
            Err(KeyStoreError::NotFound)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encrypt(
        &self,
        caller_app_id: &str,
        device_locked: bool,
        key_name: &str,
        nonce: &[u8; 12],
        aad: &[u8],
        plaintext: &[u8],
        ciphertext_out: &mut [u8],
        tag_out: &mut [u8; 16],
    ) -> Result<(), KeyStoreError> {
        let entry = self.find_authorized(caller_app_id, key_name, device_locked)?;
        if crate::aead_encrypt(&entry.key_material, nonce, aad, plaintext, ciphertext_out, tag_out) {
            Ok(())
        } else {
            Err(KeyStoreError::CryptoFailed)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decrypt(
        &self,
        caller_app_id: &str,
        device_locked: bool,
        key_name: &str,
        nonce: &[u8; 12],
        aad: &[u8],
        ciphertext: &[u8],
        tag: &[u8; 16],
        plaintext_out: &mut [u8],
    ) -> Result<(), KeyStoreError> {
        let entry = self.find_authorized(caller_app_id, key_name, device_locked)?;
        if crate::aead_decrypt(&entry.key_material, nonce, aad, ciphertext, tag, plaintext_out) {
            Ok(())
        } else {
            Err(KeyStoreError::CryptoFailed)
        }
    }

    pub fn sign_hmac(
        &self,
        caller_app_id: &str,
        device_locked: bool,
        key_name: &str,
        message: &[u8],
    ) -> Result<[u8; 32], KeyStoreError> {
        let entry = self.find_authorized(caller_app_id, key_name, device_locked)?;
        Ok(crate::hmac_sha256(&entry.key_material, message))
    }

    pub fn rotate_key(
        &mut self,
        caller_app_id: &str,
        key_name: &str,
        new_raw_key: &[u8; 32],
    ) -> Result<u32, KeyStoreError> {
        for entry in self.entries.iter_mut() {
            if entry.in_use && entry.matches_owner(caller_app_id) && entry.matches_name(key_name) {
                if entry.revoked {
                    return Err(KeyStoreError::KeyRevoked);
                }
                crate::secure_zero(&mut entry.key_material);
                entry.key_material.copy_from_slice(new_raw_key);
                entry.version = entry.version.saturating_add(1);
                return Ok(entry.version);
            }
        }
        Err(KeyStoreError::NotFound)
    }

    pub fn revoke_and_delete(
        &mut self,
        caller_app_id: &str,
        key_name: &str,
    ) -> Result<(), KeyStoreError> {
        for entry in self.entries.iter_mut() {
            if entry.in_use && entry.matches_owner(caller_app_id) && entry.matches_name(key_name) {
                crate::secure_zero(&mut entry.key_material);
                entry.revoked = true;
                return Ok(());
            }
        }
        Err(KeyStoreError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystore_isolates_apps_enforces_lock_rotation_and_revocation() {
        let mut ks = AppKeyStore::new();
        let key1 = [0x11u8; 32];
        let key2 = [0x22u8; 32];
        let nonce = [0x33u8; 12];

        assert_eq!(ks.create_key("nc.notes", "db-key", &key1, true), Ok(1));

        // 1. Encrypt & decrypt by owner when unlocked succeeds.
        let pt = b"private note payload";
        let mut ct = [0u8; 20];
        let mut tag = [0u8; 16];
        let mut dec = [0u8; 20];
        ks.encrypt("nc.notes", false, "db-key", &nonce, b"aad", pt, &mut ct, &mut tag).unwrap();
        ks.decrypt("nc.notes", false, "db-key", &nonce, b"aad", &ct, &tag, &mut dec).unwrap();
        assert_eq!(&dec, pt);

        // 2. Different app is denied access.
        assert_eq!(
            ks.decrypt("nc.evil", false, "db-key", &nonce, b"aad", &ct, &tag, &mut dec),
            Err(KeyStoreError::PermissionDenied)
        );

        // 3. Locked device blocks auth_required keys.
        assert_eq!(
            ks.sign_hmac("nc.notes", true, "db-key", b"msg"),
            Err(KeyStoreError::DeviceLocked)
        );

        // 4. Key rotation increments version and invalidates old ciphertext under new key.
        assert_eq!(ks.rotate_key("nc.notes", "db-key", &key2), Ok(2));
        assert_eq!(
            ks.decrypt("nc.notes", false, "db-key", &nonce, b"aad", &ct, &tag, &mut dec),
            Err(KeyStoreError::CryptoFailed)
        );

        // 5. Revocation zeroizes key material and denies future use.
        ks.revoke_and_delete("nc.notes", "db-key").unwrap();
        assert_eq!(
            ks.sign_hmac("nc.notes", false, "db-key", b"msg"),
            Err(KeyStoreError::KeyRevoked)
        );
    }
}
