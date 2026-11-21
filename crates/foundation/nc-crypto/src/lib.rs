//! Thin Rust adapter over the authoritative SPARK Ada Cryptographic Core (`NC_Security_Crypto`).
//!
//! No independent Rust SHA-256, HMAC, PBKDF2, AEAD, or constant-time comparison
//! implementation remains here. All cryptographic primitives execute in SPARK Ada.
#![no_std]

pub mod keystore;
pub mod lock;

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        nc_spark_sec::nc_sec_crypto_sha256(data.as_ptr(), data.len(), out.as_mut_ptr());
    }
    out
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        nc_spark_sec::nc_sec_crypto_hmac_sha256(
            key.as_ptr(),
            key.len(),
            message.as_ptr(),
            message.len(),
            out.as_mut_ptr(),
        );
    }
    out
}

pub fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    if out.is_empty() || iterations == 0 {
        return;
    }
    let _ = unsafe {
        nc_spark_sec::nc_sec_crypto_pbkdf2_sha256(
            password.as_ptr(),
            password.len(),
            salt.as_ptr(),
            salt.len(),
            iterations,
            out.as_mut_ptr(),
            out.len(),
        )
    };
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    unsafe { nc_spark_sec::nc_sec_crypto_const_time_eq(a.as_ptr(), b.as_ptr(), a.len()) != 0 }
}

pub fn secure_zero(buf: &mut [u8]) {
    unsafe {
        nc_spark_sec::nc_sec_crypto_secure_zero(buf.as_mut_ptr(), buf.len());
    }
}

/// Authenticated Encryption with Associated Data (ChaCha20-Poly1305, RFC 8439) in SPARK Ada.
pub fn aead_encrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    plaintext: &[u8],
    ciphertext: &mut [u8],
    tag: &mut [u8; 16],
) -> bool {
    if ciphertext.len() != plaintext.len() {
        return false;
    }
    unsafe {
        nc_spark_sec::nc_sec_crypto_aead_encrypt(
            key.as_ptr(),
            nonce.as_ptr(),
            aad.as_ptr(),
            aad.len(),
            plaintext.as_ptr(),
            plaintext.len(),
            ciphertext.as_mut_ptr(),
            tag.as_mut_ptr(),
        ) == 0
    }
}

/// Authenticated Decryption (ChaCha20-Poly1305, RFC 8439) in SPARK Ada.
/// Fail-closed: zeroizes `plaintext` on authentication failure.
pub fn aead_decrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8; 16],
    plaintext: &mut [u8],
) -> bool {
    if plaintext.len() != ciphertext.len() {
        return false;
    }
    unsafe {
        nc_spark_sec::nc_sec_crypto_aead_decrypt(
            key.as_ptr(),
            nonce.as_ptr(),
            aad.as_ptr(),
            aad.len(),
            ciphertext.as_ptr(),
            ciphertext.len(),
            tag.as_ptr(),
            plaintext.as_mut_ptr(),
        ) == 0
    }
}

#[cfg(test)]
mod tests;
