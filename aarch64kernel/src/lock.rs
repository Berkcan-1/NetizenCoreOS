//! Cekirdek zorunlu ekran kilidi ve kimlik bilgisi guvenligi (ARMv8-A).
//!
//! Kimlik bilgileri `settings.cfg` dosyasindan tamamen ayri olarak
//! `/data/appdata/nc.security/lock.cred` ve `/data/appdata/nc.security/lock.mark`
//! dosyalarinda 512 baytlik SHA-256 butunluk korumali sektorler halinde saklanir.
//!
//! Guvenlik notu:
//! - Duz metin PIN/parola ASLA diske yazilmaz.
//! - PBKDF2-HMAC-SHA256 (`nc_crypto::lock::DEFAULT_ITERATIONS`) + 16 bayt rastgele tuz
//!   + sabit zamanli karsilastirma kullanilir.
//! - Donanim TRNG (`RNDR`) QEMU virt profilinde mevcut olmadigi icin tuz uretimi
//!   `CNTVCT_EL0`, RTC zamani, cekirdek tikleri ve giris olaylari havuzunun SHA-256
//!   karistirilmasiyla yapilir (uretim donaniminda donanim TRNG ile guclendirilmelidir).
//! - Sabit varsayilan parola veya uygulama seviyesi atlatma (bypass) YOKTUR.
//! - Unutulan kimlik bilgisi kurtarma: yalnizca cevrimdisi `/data` temizligi (`NC_WIPE_DATA=1`
//!   veya kurtarma ortaminda `/data/appdata/nc.security/lock.*` silinmesi).

use crate::{storage, task};
use core::arch::asm;
use core::fmt::Write;
use core::ptr::addr_of_mut;
use nc_crypto::lock::{
    evaluate, secret_is_valid, LockRecord, LockState, Marker, DEFAULT_ITERATIONS, KIND_PASSWORD,
    KIND_PIN,
};
use nc_crypto::sha256;
use nc_fs::FileSystem;

const SEC_DIR: &str = "/appdata/nc.security";
const CRED_PATH: &str = "/appdata/nc.security/lock.cred";
const MARK_PATH: &str = "/appdata/nc.security/lock.mark";

struct Serial;
impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}
macro_rules! log {
    ($($arg:tt)*) => {{ let _ = writeln!(Serial, $($arg)*); }};
}

pub struct LockManager {
    pub locked: bool,
    pub enabled: bool,
    pub destroyed: bool,
    pub record: Option<LockRecord>,
    pub fails: u32,
    pub lockout_until_tick: u64,
    pub last_activity_tick: u64,
    pub timeout_secs: u32,
    pub lock_on_boot: bool,
    pub show_info: bool,
    entropy_pool: [u8; 32],
    entropy_ctr: u64,
}

impl LockManager {
    const fn new() -> Self {
        Self {
            locked: false,
            enabled: false,
            destroyed: false,
            record: None,
            fails: 0,
            lockout_until_tick: 0,
            last_activity_tick: 0,
            timeout_secs: 60,
            lock_on_boot: true,
            show_info: true,
            entropy_pool: [0xA5; 32],
            entropy_ctr: 0,
        }
    }
}

static mut LOCK_MGR: LockManager = LockManager::new();

fn mgr() -> &'static mut LockManager {
    unsafe { &mut *addr_of_mut!(LOCK_MGR) }
}

#[inline]
fn read_cntvct() -> u64 {
    let v: u64;
    unsafe {
        asm!("mrs {}, cntvct_el0", out(reg) v, options(nomem, nostack, preserves_flags));
    }
    v
}

/// Giris olaylarindan ve zamanlayicidan entropi havuzunu besler.
pub fn mix_entropy(a: u64, b: u64) {
    let m = mgr();
    m.entropy_ctr = m.entropy_ctr.wrapping_add(1);
    let mut buf = [0u8; 64];
    buf[0..32].copy_from_slice(&m.entropy_pool);
    buf[32..40].copy_from_slice(&read_cntvct().to_le_bytes());
    buf[40..48].copy_from_slice(&a.to_le_bytes());
    buf[48..56].copy_from_slice(&b.to_le_bytes());
    buf[56..64].copy_from_slice(&m.entropy_ctr.to_le_bytes());
    m.entropy_pool = sha256(&buf);
}

fn generate_salt() -> [u8; 16] {
    let rtc = nc_arch::rtc::unix_time_now().unwrap_or(0);
    mix_entropy(rtc, task::ticks());
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&mgr().entropy_pool[0..16]);
    salt
}

/// Probe ARMv8.5-A RNDR hardware TRNG availability via ID_AA64ISAR0_EL1[63:60].
pub fn hardware_trng_available() -> bool {
    let isar0: u64;
    unsafe {
        asm!("mrs {}, id_aa64isar0_el1", out(reg) isar0, options(nomem, nostack, preserves_flags));
    }
    ((isar0 >> 60) & 0xF) != 0
}

fn lockout_seconds_for(fails: u32) -> u64 {
    nc_crypto::lock::lockout_seconds_for(fails) as u64
}

pub fn load_from_storage(lock_on_boot: bool, timeout_secs: u32, show_info: bool) {
    let m = mgr();
    m.lock_on_boot = lock_on_boot;
    m.timeout_secs = timeout_secs;
    m.show_info = show_info;
    m.last_activity_tick = task::ticks();
    mix_entropy(nc_arch::rtc::unix_time_now().unwrap_or(0), task::ticks());

    let Some(fs) = storage::data_fs() else {
        return;
    };
    let mut cred_sector = [0u8; 512];
    let mut mark_sector = [0u8; 512];

    if let Ok(bytes) = fs.read_file(CRED_PATH) {
        if bytes.len() == 512 {
            cred_sector.copy_from_slice(&bytes);
        } else {
            cred_sector[0] = 0xFF; // Bozuk boyut -> Destroyed olarak isaretle
        }
    }
    if let Ok(bytes) = fs.read_file(MARK_PATH) {
        if bytes.len() == 512 {
            mark_sector.copy_from_slice(&bytes);
        } else {
            mark_sector[0] = 0xFF;
        }
    }

    // Delegate authoritative state machine loading to SPARK Ada Security Core.
    let _spark_status = unsafe {
        nc_spark_sec::nc_sec_lock_load_state(
            cred_sector.as_ptr(),
            mark_sector.as_ptr(),
            if lock_on_boot { 1 } else { 0 },
        )
    };
    if !hardware_trng_available() {
        log!("[SPARK/kilit] NOT: donanim RNDR TRNG bu hedefte yok; FDE anahtar uretimi donanim TRNG gerektirir");
    }

    match evaluate(&cred_sector, &mark_sector) {
        LockState::Disabled => {
            m.enabled = false;
            m.locked = false;
            m.destroyed = false;
            m.record = None;
            m.fails = 0;
        }
        LockState::Enabled { record, fails } => {
            m.enabled = true;
            m.destroyed = false;
            m.record = Some(record);
            m.fails = fails;
            m.locked = unsafe { nc_spark_sec::nc_sec_lock_is_locked() != 0 };
            log!(
                "[SPARK/kilit] ekran kilidi yuklendi (tur={}, basarisiz={}, acilis_kilidi={})",
                if record.kind == KIND_PIN { "PIN" } else { "Parola" },
                fails,
                m.locked
            );
        }
        LockState::Destroyed => {
            // FAIL-CLOSED (SPARK proven invariant): corrupted credential state NEVER unlocks!
            m.enabled = true;
            m.locked = true;
            m.destroyed = true;
            m.record = None;
            log!("[SPARK/kilit] UYARI: kilit kimlik dosyalari bozulmus -> FAIL-CLOSED (kilitli)");
        }
    }
}

fn persist_state(record: Option<LockRecord>, fails: u32) -> bool {
    let Some(fs) = storage::data_fs() else {
        return false;
    };
    if !storage::data_writable() {
        return false;
    }
    let _ = fs.create_dir("/appdata");
    let _ = fs.create_dir(SEC_DIR);
    match record {
        Some(rec) => {
            let cred_sector = rec.to_sector();
            let digest = LockRecord::sector_digest(&cred_sector);
            let marker = Marker { enabled: true, fails, credential_digest: digest };
            let mark_sector = marker.to_sector();
            if fs.write_file(CRED_PATH, &cred_sector).is_err() {
                return false;
            }
            if fs.write_file(MARK_PATH, &mark_sector).is_err() {
                return false;
            }
        }
        None => {
            let blank = [0u8; 512];
            let _ = fs.write_file(CRED_PATH, &blank);
            let _ = fs.write_file(MARK_PATH, &blank);
        }
    }
    storage::sync_data()
}

pub fn is_locked() -> bool {
    mgr().locked
}

pub fn is_enabled() -> bool {
    mgr().enabled
}

pub fn kind() -> u8 {
    mgr().record.map(|r| r.kind).unwrap_or(0)
}

pub fn fails() -> u32 {
    mgr().fails
}

pub fn lockout_remaining_secs() -> u32 {
    let m = mgr();
    let now = task::ticks();
    if m.lockout_until_tick > now {
        ((m.lockout_until_tick - now + task::TICK_HZ - 1) / task::TICK_HZ) as u32
    } else {
        0
    }
}

pub fn config_timeout_secs() -> u32 {
    mgr().timeout_secs
}

pub fn config_lock_on_boot() -> bool {
    mgr().lock_on_boot
}

pub fn config_show_info() -> bool {
    mgr().show_info
}

pub fn set_policy(timeout_secs: u32, lock_on_boot: bool, show_info: bool) {
    let m = mgr();
    m.timeout_secs = timeout_secs;
    m.lock_on_boot = lock_on_boot;
    m.show_info = show_info;
}

pub fn note_activity(now_tick: u64) {
    let m = mgr();
    m.last_activity_tick = now_tick;
}

fn sync_spark_lock(record: Option<LockRecord>, fails: u32, locked: bool) {
    match record {
        Some(rec) => {
            let cred_sector = rec.to_sector();
            let digest = LockRecord::sector_digest(&cred_sector);
            let marker = Marker { enabled: true, fails, credential_digest: digest };
            let mark_sector = marker.to_sector();
            unsafe {
                let _ = nc_spark_sec::nc_sec_lock_load_state(
                    cred_sector.as_ptr(),
                    mark_sector.as_ptr(),
                    if locked { 1 } else { 0 },
                );
            }
        }
        None => {
            let blank = [0u8; 512];
            unsafe {
                let _ = nc_spark_sec::nc_sec_lock_load_state(blank.as_ptr(), blank.as_ptr(), 0);
            }
        }
    }
}

/// Cekirdek zamanlayici tikinde otomatik kilit zaman asimini denetler.
/// Kilit yeni devreye girdiyse `true` doner.
pub fn tick_idle(now_tick: u64) -> bool {
    let m = mgr();
    if !m.enabled || m.locked || m.timeout_secs == 0 {
        return false;
    }
    let elapsed_ticks = now_tick.saturating_sub(m.last_activity_tick);
    if elapsed_ticks >= (m.timeout_secs as u64) * task::TICK_HZ {
        if unsafe { nc_spark_sec::nc_sec_lock_status() } != nc_spark_sec::LOCK_ENABLED {
            sync_spark_lock(m.record, m.fails, true);
        } else {
            unsafe {
                let _ = nc_spark_sec::nc_sec_lock_now();
            }
        }
        m.locked = true;
        log!("[SPARK/kilit] bosta kalma suresi ({}s) doldu -> sistem kilitlendi", m.timeout_secs);
        return true;
    }
    false
}

/// Sistemi aninda kilitler (yalnizca kilit etkinse).
pub fn lock_now() -> bool {
    let m = mgr();
    if !m.enabled || m.record.is_none() {
        return false;
    }
    if unsafe { nc_spark_sec::nc_sec_lock_status() } != nc_spark_sec::LOCK_ENABLED {
        sync_spark_lock(m.record, m.fails, true);
    } else {
        unsafe {
            let _ = nc_spark_sec::nc_sec_lock_now();
        }
    }
    m.locked = true;
    log!("[SPARK/kilit] kullanici istegiyle sistem kilitlendi");
    true
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    Unlocked,
    InvalidSecret,
    LockedOut(u32),
    NotEnabled,
}

/// Ekran kilidini acmak icin kimlik dogrulamasi yapar (SPARK Ada yetkili durum makinesi).
pub fn authenticate(secret: &[u8]) -> AuthResult {
    let m = mgr();
    if m.destroyed {
        // FAIL-CLOSED: corrupted credential state NEVER unlocks!
        m.locked = true;
        return AuthResult::InvalidSecret;
    }
    if !m.enabled {
        m.locked = false;
        return AuthResult::NotEnabled;
    }
    let rem = lockout_remaining_secs();
    if rem > 0 {
        return AuthResult::LockedOut(rem);
    }
    let Some(record) = m.record else {
        m.locked = true;
        return AuthResult::InvalidSecret;
    };
    // Ensure SPARK Global_Lock is synchronized with the active LockRecord
    if unsafe { nc_spark_sec::nc_sec_lock_status() } != nc_spark_sec::LOCK_ENABLED {
        sync_spark_lock(Some(record), m.fails, m.locked);
    }
    let mut delay_secs = 0u32;
    let res = unsafe {
        nc_spark_sec::nc_sec_lock_authenticate(
            secret.as_ptr(),
            secret.len(),
            0,
            &mut delay_secs,
        )
    };
    match res {
        nc_spark_sec::LOCK_AUTH_UNLOCKED => {
            m.locked = false;
            m.fails = 0;
            m.lockout_until_tick = 0;
            m.last_activity_tick = task::ticks();
            let _ = persist_state(Some(record), 0);
            log!("[SPARK/kilit] kimlik dogrulandi -> kilit acildi");
            AuthResult::Unlocked
        }
        nc_spark_sec::LOCK_AUTH_NOT_ENABLED => {
            m.locked = false;
            AuthResult::NotEnabled
        }
        nc_spark_sec::LOCK_AUTH_DESTROYED => {
            m.destroyed = true;
            m.locked = true;
            AuthResult::InvalidSecret
        }
        _ => {
            m.fails = m.fails.saturating_add(1);
            let delay = if delay_secs > 0 { delay_secs as u64 } else { lockout_seconds_for(m.fails) };
            if delay > 0 {
                m.lockout_until_tick = task::ticks().saturating_add(delay * task::TICK_HZ);
            }
            let _ = persist_state(Some(record), m.fails);
            log!("[SPARK/kilit] gecersiz kimlik (deneme={}, bekleme={}s)", m.fails, delay);
            if delay > 0 {
                AuthResult::LockedOut(delay as u32)
            } else {
                AuthResult::InvalidSecret
            }
        }
    }
}

/// Yeni PIN/Parola ayarlar veya mevcut olani degistirir.
/// Kilit zaten etkinse `current_secret` dogrulanmadan degisiklik yapilmaz!
pub fn configure_set(kind: u8, current_secret: &[u8], new_secret: &[u8]) -> Result<(), &'static str> {
    if kind != KIND_PIN && kind != KIND_PASSWORD {
        return Err("gecersiz kilit turu");
    }
    if !secret_is_valid(kind, new_secret) {
        return Err("gecersiz PIN veya parola bicimi");
    }
    let m = mgr();
    if m.enabled {
        if lockout_remaining_secs() > 0 {
            return Err("cok fazla hatali deneme");
        }
        let Some(rec) = m.record else {
            return Err("kilit kaydi yok");
        };
        if !rec.verify(current_secret) {
            m.fails = m.fails.saturating_add(1);
            let delay = lockout_seconds_for(m.fails);
            if delay > 0 {
                m.lockout_until_tick = task::ticks().saturating_add(delay * task::TICK_HZ);
            }
            let _ = persist_state(Some(rec), m.fails);
            return Err("mevcut kimlik bilgisi yanlis");
        }
    }
    let salt = generate_salt();
    let rec = LockRecord::derive(kind, new_secret, salt, DEFAULT_ITERATIONS);
    if !persist_state(Some(rec), 0) {
        return Err("kimlik bilgisi diske yazilamadi");
    }
    // Synchronize SPARK Ada authoritative lock state machine immediately!
    sync_spark_lock(Some(rec), 0, m.locked);
    m.record = Some(rec);
    m.enabled = true;
    m.destroyed = false;
    m.fails = 0;
    m.lockout_until_tick = 0;
    m.last_activity_tick = task::ticks();
    log!("[SPARK/kilit] ekran kilidi yapilandirildi (tur={})", if kind == KIND_PIN { "PIN" } else { "Parola" });
    Ok(())
}

/// Ekran kilidini kapatir. Mevcut kimlik dogrulamasi ZORUNLUDUR.
pub fn configure_disable(current_secret: &[u8]) -> Result<(), &'static str> {
    let m = mgr();
    if !m.enabled {
        return Ok(());
    }
    if lockout_remaining_secs() > 0 {
        return Err("cok fazla hatali deneme");
    }
    let Some(rec) = m.record else {
        return Err("kilit kaydi yok");
    };
    if !rec.verify(current_secret) {
        m.fails = m.fails.saturating_add(1);
        let delay = lockout_seconds_for(m.fails);
        if delay > 0 {
            m.lockout_until_tick = task::ticks().saturating_add(delay * task::TICK_HZ);
        }
        let _ = persist_state(Some(rec), m.fails);
        return Err("mevcut kimlik bilgisi yanlis");
    }
    if !persist_state(None, 0) {
        return Err("kimlik bilgisi temizlenemedi");
    }
    // Synchronize SPARK Ada authoritative lock state machine to Disabled!
    sync_spark_lock(None, 0, false);
    m.enabled = false;
    m.locked = false;
    m.destroyed = false;
    m.record = None;
    m.fails = 0;
    m.lockout_until_tick = 0;
    log!("[SPARK/kilit] ekran kilidi devre disi birakildi");
    Ok(())
}
