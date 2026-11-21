//! Thin Rust adapter over the authoritative SPARK Ada DysonSphere Engine (`NC_Security_DysonSphere`).
//!
//! All authorization decisions, package registration, rollback prevention,
//! capability clamping, revocation, and bounded audit logging execute exclusively
//! inside the formally verified SPARK Ada security core (`libnc_security.a`).
//!
//! Architectural security fix:
//! - The legacy global mutable `active_app_id` string (`set_active_caller` / `authorize_active`)
//!   has been ELIMINATED as an authorization source. Every authorization request must pass
//!   the authenticated caller identity obtained from the kernel's active task record.

extern crate alloc;
use alloc::string::String;

/// DysonSphere narrow capability identifiers (mirrors `NC_Security_Capabilities`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Capability {
    SettingsRead          = nc_caps::SETTINGS_READ,
    SettingsWriteDisplay  = nc_caps::SETTINGS_WRITE_DISPLAY,
    SettingsWriteLocale   = nc_caps::SETTINGS_WRITE_LOCALE,
    SettingsWriteKeyboard = nc_caps::SETTINGS_WRITE_KEYBOARD,
    SettingsWriteNetwork  = nc_caps::SETTINGS_WRITE_NETWORK,
    SettingsWriteDrivers  = nc_caps::SETTINGS_WRITE_DRIVERS,
    FsUserRead            = nc_caps::FS_USER_READ,
    FsUserWrite           = nc_caps::FS_USER_WRITE,
    FsUserDelete          = nc_caps::FS_USER_DELETE,
    FsGrantRead           = nc_caps::FS_GRANT_READ,
    FsGrantWrite          = nc_caps::FS_GRANT_WRITE,
    PackageInstall        = nc_caps::PACKAGE_INSTALL,
    NetworkClient         = nc_caps::NETWORK_CLIENT,
    SystemAuditRead       = nc_caps::SYSTEM_AUDIT_READ,
    IpcEndpoint           = nc_caps::IPC_ENDPOINT,
    AudioPlay             = nc_caps::AUDIO_PLAY,
    NotifyPost            = nc_caps::NOTIFY_POST,
    FsAllRead             = nc_caps::FS_ALL_READ,
    FsAllWrite            = nc_caps::FS_ALL_WRITE,
    ProcessLaunch         = nc_caps::PROCESS_LAUNCH,
    PackageRemove         = nc_caps::PACKAGE_REMOVE,
    SecurityManage        = nc_caps::SECURITY_MANAGE,
}

impl Capability {
    #[inline]
    pub const fn bit(self) -> u32 {
        self as u32
    }

    /// Parse manifest capability string; unknown capabilities are strictly rejected.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "settings.read" => Some(Self::SettingsRead),
            "settings.write.display" => Some(Self::SettingsWriteDisplay),
            "settings.write.locale" => Some(Self::SettingsWriteLocale),
            "settings.write.keyboard" => Some(Self::SettingsWriteKeyboard),
            "settings.write.network" => Some(Self::SettingsWriteNetwork),
            "settings.write.drivers" => Some(Self::SettingsWriteDrivers),
            "filesystem.user.read" | "fs.user.read" => Some(Self::FsUserRead),
            "filesystem.user.write" | "fs.user.write" => Some(Self::FsUserWrite),
            "filesystem.user.delete" | "fs.user.delete" => Some(Self::FsUserDelete),
            "filesystem.grant.read" | "fs.grant.read" => Some(Self::FsGrantRead),
            "filesystem.grant.write" | "fs.grant.write" => Some(Self::FsGrantWrite),
            "package.install" => Some(Self::PackageInstall),
            "network.client" => Some(Self::NetworkClient),
            "system.audit.read" => Some(Self::SystemAuditRead),
            "ipc.endpoint" => Some(Self::IpcEndpoint),
            "audio.play" => Some(Self::AudioPlay),
            "notification.post" => Some(Self::NotifyPost),
            "filesystem.all.read" | "fs.all.read" => Some(Self::FsAllRead),
            "filesystem.all.write" | "fs.all.write" => Some(Self::FsAllWrite),
            "process.launch" => Some(Self::ProcessLaunch),
            "package.remove" => Some(Self::PackageRemove),
            "security.manage" => Some(Self::SecurityManage),
            _ => None,
        }
    }
}

/// Deterministic authorization decision returned by SPARK DysonSphere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthDecision {
    Allow,
    DenyUnknownApp,
    DenyRevokedApp,
    DenyRollbackVersion,
    DenyCapabilityNotGranted,
    DenyPolicyLocked,
    DenyInvalidInput,
    DenyImpersonation,
    DenyTableFull,
    DenySystemOnly,
    DenyUntrustedPublisher,
}

fn map_decision(code: u32) -> AuthDecision {
    match code {
        nc_spark_sec::AUTH_ALLOW => AuthDecision::Allow,
        nc_spark_sec::AUTH_DENY_UNKNOWN_APP => AuthDecision::DenyUnknownApp,
        nc_spark_sec::AUTH_DENY_REVOKED_APP => AuthDecision::DenyRevokedApp,
        nc_spark_sec::AUTH_DENY_ROLLBACK_VERSION => AuthDecision::DenyRollbackVersion,
        nc_spark_sec::AUTH_DENY_CAPABILITY_NOT_GRANTED => AuthDecision::DenyCapabilityNotGranted,
        nc_spark_sec::AUTH_DENY_POLICY_LOCKED => AuthDecision::DenyPolicyLocked,
        nc_spark_sec::AUTH_DENY_INVALID_INPUT => AuthDecision::DenyInvalidInput,
        nc_spark_sec::AUTH_DENY_IMPERSONATION => AuthDecision::DenyImpersonation,
        nc_spark_sec::AUTH_DENY_TABLE_FULL => AuthDecision::DenyTableFull,
        nc_spark_sec::AUTH_DENY_SYSTEM_ONLY => AuthDecision::DenySystemOnly,
        nc_spark_sec::AUTH_DENY_UNTRUSTED_PUBLISHER => AuthDecision::DenyUntrustedPublisher,
        _ => AuthDecision::DenyPolicyLocked,
    }
}

/// Initialize the authoritative SPARK Ada DysonSphere engine.
pub fn init_dysonsphere() {
    if !nc_spark_sec::is_initialized() {
        let _ = nc_spark_sec::init();
    }
}

/// Authorize an explicit, kernel-authenticated caller identity for `cap` via SPARK DysonSphere.
pub fn authorize_app(app_id: &str, cap: Capability) -> AuthDecision {
    map_decision(nc_spark_sec::ds_authorize(app_id, cap.bit() as u64))
}

/// Register a verified package with SPARK DysonSphere (distinct requested vs approved caps).
#[allow(clippy::too_many_arguments)]
pub fn register_verified_package(
    app_id: &str,
    publisher_key_id: &str,
    version_code: u32,
    payload_hash: [u8; 32],
    requested_cap_names: &[String],
    approved_caps: u64,
    from_system_image: bool,
    is_verified_v2: bool,
    platform_signed: bool,
) -> Result<(), AuthDecision> {
    let mut requested_mask = 0u64;
    for name in requested_cap_names {
        if let Some(cap) = Capability::from_name(name) {
            requested_mask |= cap.bit() as u64;
        }
    }
    let code = nc_spark_sec::ds_register_package(
        app_id,
        publisher_key_id,
        version_code,
        &payload_hash,
        requested_mask,
        approved_caps,
        from_system_image,
        is_verified_v2,
        platform_signed,
    );
    let dec = map_decision(code);
    if dec == AuthDecision::Allow {
        Ok(())
    } else {
        Err(dec)
    }
}

/// Immediately revoke an application in SPARK DysonSphere.
pub fn revoke_app(app_id: &str) -> bool {
    nc_spark_sec::ds_revoke_app(app_id)
}

/// Query effective granted capabilities for a registered application from SPARK DysonSphere.
pub fn query_granted_caps(app_id: &str) -> u64 {
    nc_spark_sec::ds_query_granted_caps(app_id)
}

pub fn total_denied_count() -> usize {
    nc_spark_sec::ds_denied_count() as usize
}
