#![no_std]
extern crate alloc;

pub mod crypto;
pub mod manifest;
pub mod dysonsphere;
pub use dysonsphere::{
    authorize_active, authorize_app, clear_active_caller, init_dysonsphere, set_active_caller,
    AuthDecision, Capability, DysonSphereEngine,
};

use alloc::boxed::Box;
use alloc::vec::Vec;
use crypto::{compute_sha256, decode_hex};
use ed25519_dalek::{Signature, VerifyingKey};
pub use manifest::{PackageAbi, PackageManifest};

#[cfg(test)]
mod tests;

pub const NTZC_MAGIC: [u8; 4] = *b"NTZC";
pub const NTZC_MAGIC_V2: [u8; 4] = *b"NTZ2";

/// NTZ2 sabit başlık uzunluğu (bayt).
pub const NTZ2_HEADER_LEN: usize = 64;
/// Ed25519 imza uzunluğu (bayt).
pub const SIGNATURE_LEN: usize = 64;

/// Ring 3 imajlarının kabul politikası (yükleyiciyle AYNI: `nc-abi` bellek yerleşimi).
pub const RING3_POLICY: nc_elf::Policy = nc_elf::Policy {
    min_vaddr: nc_abi::USER_IMAGE_MIN,
    max_vaddr: nc_abi::USER_IMAGE_MAX,
    max_pages: nc_abi::ELF_MAX_PAGES,
};

/// Paketten çalışan uygulama üreten kanca. Çekirdek (Ring 3 çalışma zamanı) kaydeder; kayıtlı değilse
/// ya da paket `runnable()` değilse `None` döner ve uygulama ÇALIŞTIRILMAZ.
pub type PackageLoader = fn(&NtzcPackage) -> Option<Box<dyn nc_app::App>>;

static mut PACKAGE_LOADER: Option<PackageLoader> = None;

pub fn set_package_loader(loader: PackageLoader) {
    unsafe {
        *core::ptr::addr_of_mut!(PACKAGE_LOADER) = Some(loader);
    }
}

pub fn load_packaged_app(pkg: &NtzcPackage) -> Option<Box<dyn nc_app::App>> {
    if !pkg.runnable() {
        return None;
    }
    let loader = unsafe { *core::ptr::addr_of!(PACKAGE_LOADER) }?;
    loader(pkg)
}

pub const PERM_SYSTEM_SETTINGS: u32 = 1 << 0;
pub const PERM_POWER_CONTROL: u32 = 1 << 1;
pub const PERM_STORAGE_FULL: u32 = 1 << 2;

/// Güvenilir yayıncıların Ed25519 AÇIK anahtarları (64 hex karakter, küçük harf).
///
/// ÖNEMLİ (F-02): Bu liste bilerek BOŞ gönderilir. Eski sürümde burada, tohumu 32 adet
/// sıfır bayt olan bir anahtarın açık anahtarı vardı; bunun özel anahtarını herkes
/// bilir, dolayısıyla "güvenilir" sayılması gerçek bir koruma sağlamazdı. Kendi
/// yayıncı anahtarını `sdk/gen-signing-key.py` ile üret, ÖZEL anahtarı (seed) depoya
/// koyma, yalnızca açık anahtarın hex değerini buraya ekle.
pub static TRUSTED_PUBLISHERS_HEX: &[&str] = &[
    // "0123...abcdef (64 hex karakter)",
];

/// İptal edilmiş (artık güvenilmeyen) yayıncı anahtarları. Bir anahtar hem burada hem
/// `TRUSTED_PUBLISHERS_HEX` içinde ise REDDEDİLİR (iptal kazanır).
pub static REVOKED_PUBLISHERS_HEX: &[&str] = &[];

/// Paket reddedilme nedenleri (seri günlük / arayüz mesajı için).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageError {
    Malformed,
    UnsupportedVersion,
    BadManifest,
    MissingSignature,
    UntrustedPublisher,
    RevokedPublisher,
    BadSignature,
    PayloadHashMismatch,
}

impl PackageError {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageError::Malformed => "malformed package",
            PackageError::UnsupportedVersion => "unsupported package version",
            PackageError::BadManifest => "invalid manifest",
            PackageError::MissingSignature => "missing signature",
            PackageError::UntrustedPublisher => "untrusted publisher key",
            PackageError::RevokedPublisher => "revoked publisher key",
            PackageError::BadSignature => "signature verification failed",
            PackageError::PayloadHashMismatch => "payload hash mismatch",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NtzcPackage {
    pub uid: u32,
    pub permissions: u32,
    pub manifest: PackageManifest,
    pub payload: Vec<u8>,
    pub is_verified: bool,
}

impl NtzcPackage {
    /// Paketi ayrıştırır. NTZ2 için Ed25519 imzası DOĞRULANIR; başarısızsa `None`.
    /// NTZC v1 (imzasız) paketler `is_verified == false` ile döner ve ASLA yerel kod
    /// olarak çalıştırılmaz / kurulmaz (bkz. `FilesApp`, `nc-apps`).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        Self::parse_checked(bytes).ok()
    }

    /// `parse` ile aynı, ancak ret nedenini döndürür (günlük için).
    pub fn parse_checked(bytes: &[u8]) -> Result<Self, PackageError> {
        Self::parse_with_keys(bytes, TRUSTED_PUBLISHERS_HEX, REVOKED_PUBLISHERS_HEX)
    }

    /// Anahtar listeleri parametreli sürüm (testler için). Üretim kodu `parse`/`parse_checked` kullanır.
    pub fn parse_with_keys(
        bytes: &[u8],
        trusted: &[&str],
        revoked: &[&str],
    ) -> Result<Self, PackageError> {
        if bytes.len() < 28 {
            return Err(PackageError::Malformed);
        }
        if bytes[0..4] == NTZC_MAGIC_V2 {
            return Self::parse_v2(bytes, trusted, revoked);
        }
        if bytes[0..4] == NTZC_MAGIC {
            return Self::parse_v1(bytes);
        }
        Err(PackageError::Malformed)
    }

    fn parse_v2(bytes: &[u8], trusted: &[&str], revoked: &[&str]) -> Result<Self, PackageError> {
        use PackageError::*;

        if bytes.len() < NTZ2_HEADER_LEN {
            return Err(Malformed);
        }
        let rd = |o: usize| -> usize {
            u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as usize
        };
        if rd(8) != 2 {
            return Err(UnsupportedVersion);
        }
        let (m_off, m_len, p_off, p_len) = (rd(16), rd(20), rd(24), rd(28));
        let m_end = m_off.checked_add(m_len).ok_or(Malformed)?;
        let p_end = p_off.checked_add(p_len).ok_or(Malformed)?;
        if m_off < NTZ2_HEADER_LEN
            || p_off < NTZ2_HEADER_LEN
            || m_end > bytes.len()
            || p_end > bytes.len()
        {
            return Err(Malformed);
        }
        // Manifest ve payload bölgeleri örtüşmemeli.
        if !(m_end <= p_off || p_end <= m_off) {
            return Err(Malformed);
        }

        let header_hash = &bytes[32..64];
        let payload_bytes = &bytes[p_off..p_end];
        let manifest_text = core::str::from_utf8(&bytes[m_off..m_end]).map_err(|_| Malformed)?;

        // 1) İmzalı kısmı ve imzayı ayır. İmza, `sig=` satırından ÖNCEKİ ham baytlar üzerindedir
        //    (normalizasyon yok). Manifest YALNIZCA bu imzalı kısımdan ayrıştırılır; böylece imza
        //    satırından sonra eklenmiş hiçbir içerik yetki/kimlik belirleyemez.
        let (signed_text, signature_bytes) = split_signed_manifest(manifest_text)?;
        let manifest = PackageManifest::parse(signed_text).ok_or(BadManifest)?;

        // 2) Yayıncı anahtarı: iptal edilmemiş VE güvenilir listede olmalı.
        let key_hex = manifest
            .publisher_key_id
            .strip_prefix("ed25519:")
            .ok_or(BadManifest)?;
        let pubkey = decode_key(key_hex).ok_or(BadManifest)?;
        if revoked.iter().any(|h| decode_key(h) == Some(pubkey)) {
            return Err(RevokedPublisher);
        }
        if !trusted.iter().any(|h| decode_key(h) == Some(pubkey)) {
            return Err(UntrustedPublisher);
        }

        // 3) Gerçek Ed25519 doğrulaması (verify_strict: zayıf/küçük mertebeli anahtarları ve
        //    şekil değiştirebilir imzaları reddeder).
        let verifying_key = VerifyingKey::from_bytes(&pubkey).map_err(|_| BadManifest)?;
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify_strict(signed_text.as_bytes(), &signature)
            .map_err(|_| BadSignature)?;

        // 4) Payload hash: bağlayıcı olan İMZALI manifestteki değerdir; başlıktaki kopya yalnızca
        //    tutarlılık kontrolüdür. İkisi de payload'ın gerçek SHA-256'sına eşit olmalı.
        let mut manifest_hash = [0u8; 32];
        decode_hex(&manifest.payload_hash, &mut manifest_hash).map_err(|_| BadManifest)?;
        let actual_hash = compute_sha256(payload_bytes);
        if actual_hash != manifest_hash || header_hash != &actual_hash[..] {
            return Err(PayloadHashMismatch);
        }

        let mut payload = Vec::with_capacity(p_len);
        payload.extend_from_slice(payload_bytes);

        let _ = dysonsphere::init_dysonsphere();
        Ok(NtzcPackage {
            uid: 1001,
            permissions: 0,
            manifest,
            payload,
            is_verified: true,
        })
    }

    fn parse_v1(bytes: &[u8]) -> Result<Self, PackageError> {
        use PackageError::*;

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 1 {
            return Err(UnsupportedVersion);
        }
        let manifest_len = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
        let payload_offset = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
        let payload_len = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;

        let total_m = 28usize.checked_add(manifest_len).ok_or(Malformed)?;
        let total_p = payload_offset.checked_add(payload_len).ok_or(Malformed)?;
        if total_m > bytes.len() || total_p > bytes.len() {
            return Err(Malformed);
        }

        let manifest_text = core::str::from_utf8(&bytes[28..total_m]).map_err(|_| Malformed)?;
        let manifest = PackageManifest::parse(manifest_text).ok_or(BadManifest)?;

        let mut payload = Vec::with_capacity(payload_len);
        payload.extend_from_slice(&bytes[payload_offset..total_p]);

        // v1 imzasızdır: dosyadaki `uid`/`permissions` alanlarına GÜVENİLMEZ (aksi halde
        // `uid=1000` ile "Sistem Uygulaması" rozeti ve izin bitleri taklit edilebilirdi).
        Ok(NtzcPackage {
            uid: 1001,
            permissions: 0,
            manifest,
            payload,
            is_verified: false,
        })
    }

    /// Çalıştırılabilir mi? Yalnızca: imzası doğrulanmış + `abi=ring3-v1` + geçerli (W^X, aralık içi) ELF yükü.
    /// Başka hiçbir çalıştırma yolu yoktur.
    pub fn runnable(&self) -> bool {
        self.is_verified && self.manifest.abi == PackageAbi::Ring3V1 && nc_elf::parse(&self.payload, &RING3_POLICY).is_ok()
    }

    /// Manifestte bildirilen yetkilerin `nc-caps` bit karşılığı (bilinmeyen adlar yok sayılır).
    pub fn capability_bits(&self) -> u32 {
        let mut bits = 0u32;
        for name in &self.manifest.capabilities {
            if let Some(cap) = Capability::from_name(name) {
                bits |= cap as u32;
            }
        }
        bits
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

fn decode_key(hex: &str) -> Option<[u8; 32]> {
    let mut key = [0u8; 32];
    decode_hex(hex.trim(), &mut key).ok()?;
    Some(key)
}

/// Manifest metnini (imzalı kısım, imza) olarak böler.
///
/// Kurallar: tam olarak BİR adet satır başında `sig=<128 hex>` satırı olmalı ve bu satır
/// manifestin son anlamlı satırı olmalı. İmzalı mesaj, bu satırdan önceki ham baytların
/// tamamıdır.
fn split_signed_manifest(text: &str) -> Result<(&str, [u8; SIGNATURE_LEN]), PackageError> {
    let mut sig_start: Option<usize> = None;
    let mut sig_hex: &str = "";
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        if line.starts_with("sig=") {
            if sig_start.is_some() {
                return Err(PackageError::BadManifest);
            }
            sig_start = Some(pos);
            sig_hex = line[4..].trim();
        } else if line.trim_start().starts_with("sig=") {
            return Err(PackageError::BadManifest);
        } else if sig_start.is_some() && !line.trim().is_empty() {
            // İmza satırından sonra içerik olamaz.
            return Err(PackageError::BadManifest);
        }
        pos += line.len();
    }
    let start = sig_start.ok_or(PackageError::MissingSignature)?;
    let mut sig = [0u8; SIGNATURE_LEN];
    decode_hex(sig_hex, &mut sig).map_err(|_| PackageError::BadSignature)?;
    Ok((&text[..start], sig))
}
