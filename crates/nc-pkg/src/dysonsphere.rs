// crates/nc-pkg/src/dysonsphere.rs
#![allow(static_mut_refs)]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// DysonSphere dar kapsamlı yetki tanımlayıcıları (Bölüm 6.2)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Capability {
    SettingsRead          = 1 << 0,
    SettingsWriteDisplay  = 1 << 1,
    SettingsWriteLocale   = 1 << 2,
    SettingsWriteKeyboard = 1 << 3,
    SettingsWriteNetwork  = 1 << 4,
    SettingsWriteDrivers  = 1 << 5,
    FsUserRead            = 1 << 6,
    FsUserWrite           = 1 << 7,
    FsUserDelete          = 1 << 8,
    FsGrantRead           = 1 << 9,
    FsGrantWrite          = 1 << 10,
    PackageInstall        = 1 << 11,
    NetworkClient         = 1 << 12,
    SystemAuditRead       = 1 << 13,
}

impl Capability {
    #[inline]
    pub const fn bit(self) -> u32 {
        self as u32
    }

    /// Metin tabanlı manifest yetkisini dar kapsamlı enum'a çevirir.
    /// Bilinmeyen yetkiler kesinlikle reddedilir (None döner).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "settings.read" => Some(Self::SettingsRead),
            "settings.write.display" => Some(Self::SettingsWriteDisplay),
            "settings.write.locale" => Some(Self::SettingsWriteLocale),
            "settings.write.keyboard" => Some(Self::SettingsWriteKeyboard),
            "settings.write.network" => Some(Self::SettingsWriteNetwork),
            "settings.write.drivers" => Some(Self::SettingsWriteDrivers),
            "filesystem.user.read" => Some(Self::FsUserRead),
            "filesystem.user.write" => Some(Self::FsUserWrite),
            "filesystem.user.delete" => Some(Self::FsUserDelete),
            "filesystem.grant.read" => Some(Self::FsGrantRead),
            "filesystem.grant.write" => Some(Self::FsGrantWrite),
            "package.install" => Some(Self::PackageInstall),
            "network.client" => Some(Self::NetworkClient),
            "system.audit.read" => Some(Self::SystemAuditRead),
            _ => None,
        }
    }
}

/// Yetkilendirme Karar Sonucu
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthDecision {
    Allow,
    DenyUnknownApp,
    DenyRevokedApp,
    DenyRollbackVersion,
    DenyCapabilityNotGranted,
    DenyPolicyLocked,
}

/// Kayıtlı Uygulama Kimlik Kaydı (Bölüm 6.1)
#[derive(Clone, Debug)]
pub struct AppIdentityRecord {
    pub app_id: String,
    pub publisher_key_id: String,
    pub version_code: u32,
    pub min_allowed_version: u32,
    pub payload_hash: [u8; 32],
    pub granted_caps: u32,
    pub revoked: bool,
    pub is_builtin: bool,
}

/// Sınırlı Denetim Kaydı (Audit Event - Bölüm 2 Madde 13 & Bölüm 11)
#[derive(Clone, Copy, Debug)]
pub struct AuditEvent {
    pub seq: u32,
    pub app_id_tag: [u8; 24],
    pub requested_cap: u32,
    pub decision: AuthDecision,
}

const AUDIT_RING_CAPACITY: usize = 64;
const MAX_REGISTERED_APPS: usize = 32;

pub struct DysonSphereEngine {
    pub initialized: bool,
    pub policy_version: u32,
    pub apps: Vec<AppIdentityRecord>,
    pub active_app_id: String,
    audit_ring: [AuditEvent; AUDIT_RING_CAPACITY],
    audit_write_idx: usize,
    audit_seq: u32,
}

static ENGINE_LOCK: AtomicBool = AtomicBool::new(false);
static mut GLOBAL_ENGINE: Option<DysonSphereEngine> = None;
static DENIED_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct EngineGuard;
impl EngineGuard {
    fn acquire() -> Option<Self> {
        let mut spins = 0u32;
        while ENGINE_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spins += 1;
            if spins > 100_000 {
                return None; // Kilitlenme olursa fail-closed (güvenli ret)
            }
            core::hint::spin_loop();
        }
        Some(EngineGuard)
    }
}
impl Drop for EngineGuard {
    fn drop(&mut self) {
        ENGINE_LOCK.store(false, Ordering::Release);
    }
}

impl DysonSphereEngine {
    pub fn new() -> Self {
        let empty_event = AuditEvent {
            seq: 0,
            app_id_tag: [0u8; 24],
            requested_cap: 0,
            decision: AuthDecision::DenyPolicyLocked,
        };
        let mut engine = Self {
            initialized: true,
            policy_version: 1,
            apps: Vec::with_capacity(MAX_REGISTERED_APPS),
            active_app_id: String::from("nc.kernel"),
            audit_ring: [empty_event; AUDIT_RING_CAPACITY],
            audit_write_idx: 0,
            audit_seq: 0,
        };
        engine.seed_builtin_policies();
        engine
    }

    /// Bölüm 6.3: Gömülü Sistem Uygulamaları İçin En Az Ayrıcalık (Least Privilege) Politikası
    fn seed_builtin_policies(&mut self) {
        // 1. Ayarlar (nc.settings)
        self.register_builtin(
            "nc.settings",
            Capability::SettingsRead.bit()
                | Capability::SettingsWriteDisplay.bit()
                | Capability::SettingsWriteLocale.bit()
                | Capability::SettingsWriteKeyboard.bit()
                | Capability::SettingsWriteNetwork.bit()
                | Capability::SettingsWriteDrivers.bit(),
        );

        // 2. Dosyalar (nc.files)
        self.register_builtin(
            "nc.files",
            Capability::FsUserRead.bit()
                | Capability::FsUserWrite.bit()
                | Capability::FsUserDelete.bit()
                | Capability::PackageInstall.bit(),
        );

        // 3. Tarayıcı (nc.browser)
        self.register_builtin(
            "nc.browser",
            Capability::NetworkClient.bit() | Capability::FsUserRead.bit(),
        );

        // 4. Sistem İzleyici (nc.system)
        self.register_builtin(
            "nc.system",
            Capability::SettingsRead.bit() | Capability::SystemAuditRead.bit(),
        );

        // 5. Kronometre (nc.stopwatch) - Hiçbir hassas yetkisi yok
        self.register_builtin("nc.stopwatch", 0);
    }

    fn register_builtin(&mut self, app_id: &str, granted_caps: u32) {
        self.apps.push(AppIdentityRecord {
            app_id: String::from(app_id),
            publisher_key_id: String::from("system:builtin"),
            version_code: 1,
            min_allowed_version: 1,
            payload_hash: [0u8; 32],
            granted_caps,
            revoked: false,
            is_builtin: true,
        });
    }

    /// Harici bir .ntzc paketi doğrulandığında kayıt defterine ekler.
    /// İstenen yetkiler (requested_caps), merkezi tavan politikası ile sınırlandırılır!
    pub fn register_verified_package(
        &mut self,
        app_id: &str,
        publisher_key_id: &str,
        version_code: u32,
        payload_hash: [u8; 32],
        requested_cap_names: &[String],
        is_verified_v2: bool,
    ) -> Result<(), AuthDecision> {
        // Gömülü sistem kimliklerinin taklit edilmesini (impersonation) engelle!
        if matches!(
            app_id,
            "nc.settings" | "nc.files" | "nc.system" | "nc.stopwatch" | "nc.browser" | "nc.kernel"
        ) {
            self.record_audit(app_id, 0, AuthDecision::DenyCapabilityNotGranted);
            return Err(AuthDecision::DenyCapabilityNotGranted);
        }

        // Üçüncü parti uygulamaların alabileceği maksimum izin tavanı (Asla ayar/sürücü değiştiremez)
        let third_party_ceiling = if is_verified_v2 {
            Capability::SettingsRead.bit()
                | Capability::FsUserRead.bit()
                | Capability::FsUserWrite.bit()
                | Capability::NetworkClient.bit()
        } else {
            // İmzasız v1 paketler: 0 yetki (Tam Sandbox)
            0
        };

        let mut requested_mask = 0u32;
        for name in requested_cap_names {
            if let Some(cap) = Capability::from_name(name) {
                requested_mask |= cap.bit();
            }
        }

        // İstek != Onay: Sadece tavan politikasının izin verdiği kesişimi onayla
        let granted_caps = requested_mask & third_party_ceiling;

        // Mevcut kayıt varsa sürüm düşürme (rollback) kontrolü yap
        if let Some(existing) = self.apps.iter_mut().find(|a| a.app_id == app_id) {
            if version_code < existing.min_allowed_version {
                self.record_audit(app_id, granted_caps, AuthDecision::DenyRollbackVersion);
                return Err(AuthDecision::DenyRollbackVersion);
            }
            existing.version_code = version_code;
            existing.min_allowed_version = version_code;
            existing.payload_hash = payload_hash;
            existing.granted_caps = granted_caps;
            return Ok(());
        }

        if self.apps.len() >= MAX_REGISTERED_APPS {
            return Err(AuthDecision::DenyPolicyLocked);
        }

        self.apps.push(AppIdentityRecord {
            app_id: String::from(app_id),
            publisher_key_id: String::from(publisher_key_id),
            version_code,
            min_allowed_version: version_code,
            payload_hash,
            granted_caps,
            revoked: false,
            is_builtin: false,
        });

        Ok(())
    }

    /// Merkez Yetkilendirme Karar Fonksiyonu (Deterministik & Default-Deny)
    pub fn check_authorization(&mut self, app_id: &str, cap: Capability) -> AuthDecision {
        if !self.initialized {
            return AuthDecision::DenyPolicyLocked;
        }

        // Çekirdek içi doğrudan çağrı
        if app_id == "nc.kernel" {
            return AuthDecision::Allow;
        }

        let decision = match self.apps.iter().find(|a| a.app_id == app_id) {
            None => AuthDecision::DenyUnknownApp,
            Some(record) => {
                if record.revoked {
                    AuthDecision::DenyRevokedApp
                } else if record.version_code < record.min_allowed_version {
                    AuthDecision::DenyRollbackVersion
                } else if (record.granted_caps & cap.bit()) != 0 {
                    AuthDecision::Allow
                } else {
                    AuthDecision::DenyCapabilityNotGranted
                }
            }
        };

        self.record_audit(app_id, cap.bit(), decision);
        if decision != AuthDecision::Allow {
            DENIED_COUNTER.fetch_add(1, Ordering::Relaxed);
        }
        decision
    }

    /// Uygulamayı anında iptal et / devre dışı bırak (Revocation)
    pub fn revoke_app(&mut self, app_id: &str) -> bool {
        if let Some(rec) = self.apps.iter_mut().find(|a| a.app_id == app_id && !a.is_builtin) {
            rec.revoked = true;
            rec.granted_caps = 0;
            return true;
        }
        false
    }

    fn record_audit(&mut self, app_id: &str, cap_bits: u32, decision: AuthDecision) {
        let mut tag = [0u8; 24];
        let bytes = app_id.as_bytes();
        let copy_len = bytes.len().min(24);
        tag[..copy_len].copy_from_slice(&bytes[..copy_len]);

        self.audit_seq = self.audit_seq.wrapping_add(1);
        self.audit_ring[self.audit_write_idx] = AuditEvent {
            seq: self.audit_seq,
            app_id_tag: tag,
            requested_cap: cap_bits,
            decision,
        };
        self.audit_write_idx = (self.audit_write_idx + 1) % AUDIT_RING_CAPACITY;
    }
}

// --- GLOBAL DYSONSPHERE API (Çekirdek ve Servisler İçin) ---

pub fn init_dysonsphere() {
    if let Some(_g) = EngineGuard::acquire() {
        unsafe {
            if GLOBAL_ENGINE.is_none() {
                GLOBAL_ENGINE = Some(DysonSphereEngine::new());
            }
        }
    }
}

pub fn set_active_caller(app_id: &str) {
    if let Some(_g) = EngineGuard::acquire() {
        unsafe {
            if GLOBAL_ENGINE.is_none() {
                GLOBAL_ENGINE = Some(DysonSphereEngine::new());
            }
            if let Some(ref mut eng) = GLOBAL_ENGINE {
                eng.active_app_id.clear();
                eng.active_app_id.push_str(app_id);
            }
        }
    }
}

pub fn clear_active_caller() {
    set_active_caller("nc.kernel");
}

/// Aktif çalışan uygulamanın belirtilen yetkiye sahip olup olmadığını denetler (Fail-Closed)
pub fn authorize_active(cap: Capability) -> bool {
    let _g = match EngineGuard::acquire() {
        Some(g) => g,
        None => return false, // Kilit alınamazsa asla izin verme!
    };
    unsafe {
        if GLOBAL_ENGINE.is_none() {
            GLOBAL_ENGINE = Some(DysonSphereEngine::new());
        }
        if let Some(ref mut eng) = GLOBAL_ENGINE {
            let caller = eng.active_app_id.clone();
            eng.check_authorization(&caller, cap) == AuthDecision::Allow
        } else {
            false
        }
    }
}

pub fn authorize_app(app_id: &str, cap: Capability) -> AuthDecision {
    let _g = match EngineGuard::acquire() {
        Some(g) => g,
        None => return AuthDecision::DenyPolicyLocked,
    };
    unsafe {
        if GLOBAL_ENGINE.is_none() {
            GLOBAL_ENGINE = Some(DysonSphereEngine::new());
        }
        if let Some(ref mut eng) = GLOBAL_ENGINE {
            eng.check_authorization(app_id, cap)
        } else {
            AuthDecision::DenyPolicyLocked
        }
    }
}

pub fn total_denied_count() -> usize {
    DENIED_COUNTER.load(Ordering::Relaxed)
}
