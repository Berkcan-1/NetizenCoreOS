//! Minimal, stable, versioned Rust FFI adapter for the NetizenCore SPARK Ada Security Core.
//!
//! This crate contains NO independent security policy, NO authorization logic,
//! and NO cryptographic implementation. Every security decision is delegated
//! directly to the formally verified SPARK Ada core (`libnc_security.a`).

#![no_std]

pub const ABI_VERSION: u32 = 1;

// Authorization decision codes (mirrors NC_Security_Types.Auth_Decision)
pub const AUTH_ALLOW: u32 = 0;
pub const AUTH_DENY_UNKNOWN_APP: u32 = 1;
pub const AUTH_DENY_REVOKED_APP: u32 = 2;
pub const AUTH_DENY_ROLLBACK_VERSION: u32 = 3;
pub const AUTH_DENY_CAPABILITY_NOT_GRANTED: u32 = 4;
pub const AUTH_DENY_POLICY_LOCKED: u32 = 5;
pub const AUTH_DENY_INVALID_INPUT: u32 = 6;
pub const AUTH_DENY_IMPERSONATION: u32 = 7;
pub const AUTH_DENY_TABLE_FULL: u32 = 8;
pub const AUTH_DENY_SYSTEM_ONLY: u32 = 9;
pub const AUTH_DENY_UNTRUSTED_PUBLISHER: u32 = 10;

// Filesystem decision codes (mirrors NC_Security_Types.Fs_Decision)
pub const FS_ALLOW: u32 = 0;
pub const FS_DENY_BAD_PATH: u32 = 1;
pub const FS_DENY_READ_ONLY: u32 = 2;
pub const FS_DENY_PERMISSION: u32 = 3;

// Lock disk status (mirrors NC_Security_Lock.Lock_Disk_Status)
pub const LOCK_DISABLED: u32 = 0;
pub const LOCK_ENABLED: u32 = 1;
pub const LOCK_DESTROYED: u32 = 2;

// Lock auth result (mirrors NC_Security_Lock.Lock_Auth_Result)
pub const LOCK_AUTH_UNLOCKED: u32 = 0;
pub const LOCK_AUTH_INVALID_SECRET: u32 = 1;
pub const LOCK_AUTH_LOCKED_OUT: u32 = 2;
pub const LOCK_AUTH_NOT_ENABLED: u32 = 3;
pub const LOCK_AUTH_DESTROYED: u32 = 4;

// FDE status (mirrors NC_Security_Encryption.Encryption_Status)
pub const FDE_UNCONFIGURED: u32 = 0;
pub const FDE_LOCKED: u32 = 1;
pub const FDE_UNLOCKED: u32 = 2;
pub const FDE_CORRUPTED: u32 = 3;

extern "C" {
    pub fn nc_sec_abi_version() -> u32;
    pub fn nc_sec_init(expected_abi_version: u32) -> u32;
    pub fn nc_sec_is_initialized() -> u32;

    pub fn nc_sec_caps_user_allowed() -> u64;
    pub fn nc_sec_caps_system_only() -> u64;
    pub fn nc_sec_caps_clamp_user(declared: u64) -> u64;
    pub fn nc_sec_caps_dropped_for_user(declared: u64) -> u64;
    pub fn nc_sec_caps_is_valid_mask(mask: u64) -> u32;

    pub fn nc_sec_ds_authorize(
        caller_id_ptr: *const u8,
        caller_id_len: usize,
        requested_cap: u64,
    ) -> u32;

    pub fn nc_sec_ds_register_package(
        app_id_ptr: *const u8,
        app_id_len: usize,
        publisher_ptr: *const u8,
        publisher_len: usize,
        version_code: u32,
        payload_hash: *const u8,
        requested_caps: u64,
        approved_caps: u64,
        source: u32,
        is_verified_v2: u32,
        platform_signed: u32,
    ) -> u32;

    pub fn nc_sec_ds_revoke_app(app_id_ptr: *const u8, app_id_len: usize) -> u32;
    pub fn nc_sec_ds_query_granted_caps(app_id_ptr: *const u8, app_id_len: usize) -> u64;
    pub fn nc_sec_ds_denied_count() -> u64;

    pub fn nc_sec_policy_authorize_launch(
        caller_id_ptr: *const u8,
        caller_id_len: usize,
        caller_caps: u64,
        target_id_ptr: *const u8,
        target_id_len: usize,
    ) -> u32;

    pub fn nc_sec_policy_authorize_ipc(
        sender_id_ptr: *const u8,
        sender_id_len: usize,
        sender_caps: u64,
        target_id_ptr: *const u8,
        target_id_len: usize,
        privileged: u32,
    ) -> u32;

    pub fn nc_sec_policy_resolve_tier(
        source: u32,
        is_verified: u32,
        platform_signed: u32,
    ) -> u32;

    pub fn nc_sec_fs_check(
        app_id_ptr: *const u8,
        app_id_len: usize,
        caps: u64,
        path_ptr: *const u8,
        path_len: usize,
        op: u32,
    ) -> u32;

    pub fn nc_sec_crypto_sha256(data_ptr: *const u8, data_len: usize, out_hash: *mut u8);

    pub fn nc_sec_crypto_hmac_sha256(
        key_ptr: *const u8,
        key_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
        out_mac: *mut u8,
    );

    pub fn nc_sec_crypto_pbkdf2_sha256(
        pass_ptr: *const u8,
        pass_len: usize,
        salt_ptr: *const u8,
        salt_len: usize,
        iterations: u32,
        out_ptr: *mut u8,
        out_len: usize,
    ) -> u32;

    pub fn nc_sec_crypto_const_time_eq(a_ptr: *const u8, b_ptr: *const u8, len: usize) -> u32;
    pub fn nc_sec_crypto_secure_zero(buf_ptr: *mut u8, buf_len: usize);

    pub fn nc_sec_crypto_aead_encrypt(
        key_ptr: *const u8,
        nonce_ptr: *const u8,
        aad_ptr: *const u8,
        aad_len: usize,
        pt_ptr: *const u8,
        pt_len: usize,
        ct_out_ptr: *mut u8,
        tag_out: *mut u8,
    ) -> u32;

    pub fn nc_sec_crypto_aead_decrypt(
        key_ptr: *const u8,
        nonce_ptr: *const u8,
        aad_ptr: *const u8,
        aad_len: usize,
        ct_ptr: *const u8,
        ct_len: usize,
        tag_ptr: *const u8,
        pt_out_ptr: *mut u8,
    ) -> u32;

    pub fn nc_sec_lock_secret_valid(kind: u8, secret_ptr: *const u8, secret_len: usize) -> u32;
    pub fn nc_sec_lock_lockout_secs(fails: u32) -> u32;

    pub fn nc_sec_lock_derive_sectors(
        kind: u8,
        secret_ptr: *const u8,
        secret_len: usize,
        salt_ptr: *const u8,
        iterations: u32,
        fails: u32,
        cred_sector_out: *mut u8,
        mark_sector_out: *mut u8,
    ) -> u32;

    pub fn nc_sec_lock_evaluate_sectors(
        cred_sector_ptr: *const u8,
        mark_sector_ptr: *const u8,
        out_kind: *mut u8,
        out_iterations: *mut u32,
        out_fails: *mut u32,
        out_salt: *mut u8,
        out_hash: *mut u8,
    ) -> u32;

    pub fn nc_sec_lock_verify_sector(
        cred_sector_ptr: *const u8,
        secret_ptr: *const u8,
        secret_len: usize,
    ) -> u32;

    pub fn nc_sec_lock_load_state(
        cred_sector_ptr: *const u8,
        mark_sector_ptr: *const u8,
        lock_on_boot: u32,
    ) -> u32;

    pub fn nc_sec_lock_is_locked() -> u32;
    pub fn nc_sec_lock_status() -> u32;
    pub fn nc_sec_lock_now() -> u32;

    pub fn nc_sec_lock_authenticate(
        secret_ptr: *const u8,
        secret_len: usize,
        lockout_active: u32,
        out_delay_secs: *mut u32,
    ) -> u32;

    pub fn nc_sec_fde_load_metadata(meta_sector_ptr: *const u8) -> u32;
    pub fn nc_sec_fde_provision(
        pass_ptr: *const u8,
        pass_len: usize,
        salt_ptr: *const u8,
        nonce_ptr: *const u8,
        raw_dek_ptr: *const u8,
        iterations: u32,
        meta_sector_out: *mut u8,
    ) -> u32;
    pub fn nc_sec_fde_unlock(pass_ptr: *const u8, pass_len: usize) -> u32;
    pub fn nc_sec_fde_lock();
    pub fn nc_sec_fde_status() -> u32;
    pub fn nc_sec_fde_encrypt_block(
        block_id: u64,
        nonce_ptr: *const u8,
        pt_ptr: *const u8,
        pt_len: usize,
        ct_out_ptr: *mut u8,
        tag_out: *mut u8,
    ) -> u32;
    pub fn nc_sec_fde_decrypt_block(
        block_id: u64,
        nonce_ptr: *const u8,
        ct_ptr: *const u8,
        ct_len: usize,
        tag_ptr: *const u8,
        pt_out_ptr: *mut u8,
    ) -> u32;
}

/// Fail-closed trap invoked if an impossible Ada runtime check fires.
#[no_mangle]
pub extern "C" fn nc_sec_fatal_trap(code: u32, line: i32) -> ! {
    panic!("SPARK security runtime check violation (code={}, line={})", code, line);
}

#[inline]
pub fn init() -> bool {
    unsafe { nc_sec_init(ABI_VERSION) == 0 }
}

#[inline]
pub fn is_initialized() -> bool {
    unsafe { nc_sec_is_initialized() != 0 }
}

#[inline]
pub fn abi_version() -> u32 {
    unsafe { nc_sec_abi_version() }
}

#[inline]
pub fn ds_authorize(caller_id: &str, cap: u64) -> u32 {
    let bytes = caller_id.as_bytes();
    unsafe { nc_sec_ds_authorize(bytes.as_ptr(), bytes.len(), cap) }
}

#[inline]
pub fn ds_register_package(
    app_id: &str,
    publisher: &str,
    version_code: u32,
    payload_hash: &[u8; 32],
    requested_caps: u64,
    approved_caps: u64,
    from_system_image: bool,
    is_verified_v2: bool,
    platform_signed: bool,
) -> u32 {
    let id = app_id.as_bytes();
    let pub_b = publisher.as_bytes();
    unsafe {
        nc_sec_ds_register_package(
            id.as_ptr(),
            id.len(),
            pub_b.as_ptr(),
            pub_b.len(),
            version_code,
            payload_hash.as_ptr(),
            requested_caps,
            approved_caps,
            if from_system_image { 1 } else { 0 },
            if is_verified_v2 { 1 } else { 0 },
            if platform_signed { 1 } else { 0 },
        )
    }
}

#[inline]
pub fn ds_revoke_app(app_id: &str) -> bool {
    let id = app_id.as_bytes();
    unsafe { nc_sec_ds_revoke_app(id.as_ptr(), id.len()) != 0 }
}

#[inline]
pub fn ds_query_granted_caps(app_id: &str) -> u64 {
    let id = app_id.as_bytes();
    unsafe { nc_sec_ds_query_granted_caps(id.as_ptr(), id.len()) }
}

#[inline]
pub fn ds_denied_count() -> u64 {
    unsafe { nc_sec_ds_denied_count() }
}

#[inline]
pub fn policy_authorize_launch(caller_id: &str, caller_caps: u64, target_id: &str) -> u32 {
    let c = caller_id.as_bytes();
    let t = target_id.as_bytes();
    unsafe { nc_sec_policy_authorize_launch(c.as_ptr(), c.len(), caller_caps, t.as_ptr(), t.len()) }
}

#[inline]
pub fn policy_authorize_ipc(
    sender_id: &str,
    sender_caps: u64,
    target_id: &str,
    privileged: bool,
) -> u32 {
    let s = sender_id.as_bytes();
    let t = target_id.as_bytes();
    unsafe {
        nc_sec_policy_authorize_ipc(
            s.as_ptr(),
            s.len(),
            sender_caps,
            t.as_ptr(),
            t.len(),
            if privileged { 1 } else { 0 },
        )
    }
}

#[inline]
pub fn fs_check(app_id: &str, caps: u64, path: &str, op: u32) -> u32 {
    let id = app_id.as_bytes();
    let p = path.as_bytes();
    unsafe { nc_sec_fs_check(id.as_ptr(), id.len(), caps, p.as_ptr(), p.len(), op) }
}

#[inline]
pub fn fde_status() -> u32 {
    unsafe { nc_sec_fde_status() }
}

#[inline]
pub fn fde_load_metadata(sector: &[u8; 512]) -> u32 {
    unsafe { nc_sec_fde_load_metadata(sector.as_ptr()) }
}

#[inline]
pub fn fde_provision(
    passphrase: &[u8],
    salt: &[u8; 16],
    nonce: &[u8; 12],
    raw_dek: &[u8; 32],
    iterations: u32,
    meta_out: &mut [u8; 512],
) -> bool {
    unsafe {
        nc_sec_fde_provision(
            passphrase.as_ptr(),
            passphrase.len(),
            salt.as_ptr(),
            nonce.as_ptr(),
            raw_dek.as_ptr(),
            iterations,
            meta_out.as_mut_ptr(),
        ) == 0
    }
}

#[inline]
pub fn fde_unlock(passphrase: &[u8]) -> bool {
    unsafe { nc_sec_fde_unlock(passphrase.as_ptr(), passphrase.len()) == 0 }
}

#[inline]
pub fn fde_lock() {
    unsafe { nc_sec_fde_lock() }
}

#[inline]
pub fn fde_encrypt_block(
    block_id: u64,
    nonce: &[u8; 12],
    plaintext: &[u8],
    ciphertext_out: &mut [u8],
    tag_out: &mut [u8; 16],
) -> bool {
    if plaintext.len() != ciphertext_out.len() || plaintext.is_empty() || plaintext.len() > 4096 {
        return false;
    }
    unsafe {
        nc_sec_fde_encrypt_block(
            block_id,
            nonce.as_ptr(),
            plaintext.as_ptr(),
            plaintext.len(),
            ciphertext_out.as_mut_ptr(),
            tag_out.as_mut_ptr(),
        ) == 0
    }
}

#[inline]
pub fn fde_decrypt_block(
    block_id: u64,
    nonce: &[u8; 12],
    ciphertext: &[u8],
    tag: &[u8; 16],
    plaintext_out: &mut [u8],
) -> bool {
    if ciphertext.len() != plaintext_out.len() || ciphertext.is_empty() || ciphertext.len() > 4096 {
        return false;
    }
    unsafe {
        nc_sec_fde_decrypt_block(
            block_id,
            nonce.as_ptr(),
            ciphertext.as_ptr(),
            ciphertext.len(),
            tag.as_ptr(),
            plaintext_out.as_mut_ptr(),
        ) == 0
    }
}
