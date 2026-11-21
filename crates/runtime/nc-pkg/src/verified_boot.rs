//! Verified Boot Chain Manifest & Signature Verifier (Feature 11).
//!
//! Binds kernel/system images to Ed25519 digital signatures, SPARK Ada SHA-256
//! hashes, monotonic rollback indices, key revocation lists, and explicit
//! production vs development trust modes.
//!
//! Limitation note: hardware-backed monotonic rollback counters (RPMB/TEE/fuse)
//! are target-specific; on QEMU virt without RPMB, `min_rollback_index` is
//! supplied by the caller/platform trust store and documented as software-tracked.

use crate::crypto::compute_sha256;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

pub const VB_MAGIC: [u8; 4] = *b"NCVB";
pub const VB_VERSION: u16 = 1;
pub const VB_MANIFEST_SIZE: usize = 192;
pub const VB_SIGNED_BODY_LEN: usize = 128;

pub const VB_FLAG_PRODUCTION: u16 = 0;
pub const VB_FLAG_DEVELOPMENT: u16 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Production,
    Development,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootVerifyError {
    MalformedManifest,
    UnsupportedVersion,
    DevImageInProduction,
    KeyRevoked,
    UntrustedKey,
    InvalidSignature,
    RollbackDetected,
    SizeMismatch,
    HashMismatch,
    ArchMismatch,
}

#[derive(Clone, Copy)]
pub struct TrustedKey {
    pub key_id: [u8; 32],
    pub public_key: [u8; 32],
}

pub struct TrustPolicy<'a> {
    pub mode: BootMode,
    pub expected_arch: u32,
    pub min_rollback_index: u32,
    pub trusted_keys: &'a [TrustedKey],
    pub revoked_key_ids: &'a [[u8; 32]],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootManifest {
    pub version: u16,
    pub flags: u16,
    pub arch_id: u32,
    pub version_code: u32,
    pub rollback_index: u32,
    pub image_size: u64,
    pub image_id: [u8; 32],
    pub image_sha256: [u8; 32],
    pub key_id: [u8; 32],
    pub signature: [u8; 64],
}

impl BootManifest {
    pub fn encode(&self) -> [u8; VB_MANIFEST_SIZE] {
        let mut out = [0u8; VB_MANIFEST_SIZE];
        out[0..4].copy_from_slice(&VB_MAGIC);
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..12].copy_from_slice(&self.arch_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.version_code.to_le_bytes());
        out[16..20].copy_from_slice(&self.rollback_index.to_le_bytes());
        out[20..24].copy_from_slice(&0u32.to_le_bytes());
        out[24..32].copy_from_slice(&self.image_size.to_le_bytes());
        out[32..64].copy_from_slice(&self.image_id);
        out[64..96].copy_from_slice(&self.image_sha256);
        out[96..128].copy_from_slice(&self.key_id);
        out[128..192].copy_from_slice(&self.signature);
        out
    }

    pub fn parse(raw: &[u8]) -> Result<Self, BootVerifyError> {
        if raw.len() != VB_MANIFEST_SIZE {
            return Err(BootVerifyError::MalformedManifest);
        }
        if raw[0..4] != VB_MAGIC {
            return Err(BootVerifyError::MalformedManifest);
        }
        let version = u16::from_le_bytes([raw[4], raw[5]]);
        if version != VB_VERSION {
            return Err(BootVerifyError::UnsupportedVersion);
        }
        let mut image_id = [0u8; 32];
        let mut image_sha256 = [0u8; 32];
        let mut key_id = [0u8; 32];
        let mut signature = [0u8; 64];
        image_id.copy_from_slice(&raw[32..64]);
        image_sha256.copy_from_slice(&raw[64..96]);
        key_id.copy_from_slice(&raw[96..128]);
        signature.copy_from_slice(&raw[128..192]);
        Ok(Self {
            version,
            flags: u16::from_le_bytes([raw[6], raw[7]]),
            arch_id: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            version_code: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
            rollback_index: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
            image_size: u64::from_le_bytes([
                raw[24], raw[25], raw[26], raw[27], raw[28], raw[29], raw[30], raw[31],
            ]),
            image_id,
            image_sha256,
            key_id,
            signature,
        })
    }
}

/// Verify a signed boot manifest and its payload image against `policy`.
/// Never allows development-flagged images in Production mode; revoked keys
/// always win over trusted keys.
pub fn verify_boot_image(
    manifest_bytes: &[u8],
    image_bytes: &[u8],
    policy: &TrustPolicy<'_>,
) -> Result<BootManifest, BootVerifyError> {
    let manifest = BootManifest::parse(manifest_bytes)?;

    if manifest.arch_id != policy.expected_arch {
        return Err(BootVerifyError::ArchMismatch);
    }
    if policy.mode == BootMode::Production && (manifest.flags & VB_FLAG_DEVELOPMENT) != 0 {
        return Err(BootVerifyError::DevImageInProduction);
    }
    if manifest.rollback_index < policy.min_rollback_index {
        return Err(BootVerifyError::RollbackDetected);
    }

    // Revocation always overrides trust list.
    if policy.revoked_key_ids.iter().any(|k| k == &manifest.key_id) {
        return Err(BootVerifyError::KeyRevoked);
    }
    let anchor = policy
        .trusted_keys
        .iter()
        .find(|k| k.key_id == manifest.key_id)
        .ok_or(BootVerifyError::UntrustedKey)?;

    let vk = VerifyingKey::from_bytes(&anchor.public_key)
        .map_err(|_| BootVerifyError::InvalidSignature)?;
    let sig = Signature::from_bytes(&manifest.signature);
    vk.verify(&manifest_bytes[..VB_SIGNED_BODY_LEN], &sig)
        .map_err(|_| BootVerifyError::InvalidSignature)?;

    if image_bytes.len() as u64 != manifest.image_size {
        return Err(BootVerifyError::SizeMismatch);
    }
    let actual_hash = compute_sha256(image_bytes);
    if !nc_crypto::constant_time_eq(&actual_hash, &manifest.image_sha256) {
        return Err(BootVerifyError::HashMismatch);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_signed(
        sk: &SigningKey,
        key_id: [u8; 32],
        image: &[u8],
        rollback: u32,
        flags: u16,
    ) -> [u8; VB_MANIFEST_SIZE] {
        let mut m = BootManifest {
            version: VB_VERSION,
            flags,
            arch_id: nc_abi::ARCH_AARCH64,
            version_code: 42,
            rollback_index: rollback,
            image_size: image.len() as u64,
            image_id: *b"netizencore-kernel-aarch64\0\0\0\0\0\0",
            image_sha256: compute_sha256(image),
            key_id,
            signature: [0u8; 64],
        };
        let mut raw = m.encode();
        let sig = sk.sign(&raw[..VB_SIGNED_BODY_LEN]);
        m.signature = sig.to_bytes();
        raw = m.encode();
        raw
    }

    #[test]
    fn verified_boot_checks_signature_hash_revocation_rollback_and_mode() {
        let sk = SigningKey::from_bytes(&[0x42u8; 32]);
        let key_id = *b"nc-root-key-v1\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        let image = b"FAKE_KERNEL_IMAGE_BYTES_FOR_UNIT_TEST";
        let raw = make_signed(&sk, key_id, image, 5, VB_FLAG_PRODUCTION);

        let anchor = [TrustedKey {
            key_id,
            public_key: sk.verifying_key().to_bytes(),
        }];
        let prod_policy = TrustPolicy {
            mode: BootMode::Production,
            expected_arch: nc_abi::ARCH_AARCH64,
            min_rollback_index: 5,
            trusted_keys: &anchor,
            revoked_key_ids: &[],
        };

        // 1. Valid signed image passes.
        assert!(verify_boot_image(&raw, image, &prod_policy).is_ok());

        // 2. Corrupted image fails hash check.
        let mut bad_img = *image;
        bad_img[0] ^= 0x01;
        assert_eq!(verify_boot_image(&raw, &bad_img, &prod_policy), Err(BootVerifyError::HashMismatch));

        // 3. Corrupted signature fails.
        let mut bad_sig = raw;
        bad_sig[150] ^= 0x80;
        assert_eq!(verify_boot_image(&bad_sig, image, &prod_policy), Err(BootVerifyError::InvalidSignature));

        // 4. Rollback attempt rejected.
        let old_raw = make_signed(&sk, key_id, image, 4, VB_FLAG_PRODUCTION);
        assert_eq!(verify_boot_image(&old_raw, image, &prod_policy), Err(BootVerifyError::RollbackDetected));

        // 5. Revoked key wins over trusted key list.
        let revoked_policy = TrustPolicy {
            revoked_key_ids: &[key_id],
            ..prod_policy
        };
        assert_eq!(verify_boot_image(&raw, image, &revoked_policy), Err(BootVerifyError::KeyRevoked));

        // 6. Development image rejected in production mode.
        let dev_raw = make_signed(&sk, key_id, image, 5, VB_FLAG_DEVELOPMENT);
        assert_eq!(verify_boot_image(&dev_raw, image, &prod_policy), Err(BootVerifyError::DevImageInProduction));
    }

    #[test]
    fn verified_boot_parser_fuzz_smoke_never_panics() {
        let mut x = 0xCAFE_BABE_1234_5678u64;
        let mut buf = [0u8; VB_MANIFEST_SIZE];
        for iter in 0..1024usize {
            let len = iter % (VB_MANIFEST_SIZE + 1);
            for b in &mut buf[..len] {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            if iter % 4 == 0 && len >= 4 {
                buf[0..4].copy_from_slice(&VB_MAGIC);
            }
            let _ = BootManifest::parse(&buf[..len]);
        }
    }
}
