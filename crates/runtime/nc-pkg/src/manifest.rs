extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use nc_app::Icon;

/// Uygulamanın çalışma modeli. YALNIZCA `Ring3V1` çalıştırılabilir: yerel (Ring 0) kod yolu kaldırıldı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageAbi {
    /// `abi=` anahtarı yok veya bilinmiyor: paket çalıştırılamaz (yalnızca bilgi gösterilir).
    Legacy,
    /// Yük, `nc-abi` olay-tabanlı sözleşmesine uygun statik bir ELF64'tür; Ring 3'te çalışır.
    Ring3V1,
}

#[derive(Clone, Debug)]
pub struct PackageManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub version_code: u32,
    pub icon: Icon,
    pub icon_name: String,
    pub abi: PackageAbi,
    pub target_arch: String,
    pub width_units: i32,
    pub height_units: i32,
    pub publisher_key_id: String,
    pub payload_hash: String,
    pub capabilities: Vec<String>,
    /// Declared runtime permission requests (e.g. `camera`, `contacts.read`).
    /// Declaration is strictly a request, NEVER an implicit grant.
    pub permissions: Vec<String>,
    pub signature: String,
    pub raw_signed_manifest: String,
}

/// Uygulama kimliği: 1..=64 karakter, yalnızca ASCII harf/rakam ve `.`, `_`, `-`.
pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

impl PackageManifest {
    pub fn parse(text: &str) -> Option<Self> {
        let mut id = String::new();
        let mut name = String::new();
        let mut version = String::from("1.0.0");
        let mut version_code = 1u32;
        let mut icon = Icon::Generic;
        let mut icon_name = String::from("generic");
        let mut abi = PackageAbi::Legacy;
        let mut target_arch = String::new();
        let mut width_units = 32;
        let mut height_units = 20;
        let mut publisher_key_id = String::new();
        let mut payload_hash = String::new();
        let mut capabilities = Vec::new();
        let mut permissions = Vec::new();
        let mut signature = String::new();
        let mut raw_signed_manifest = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with("sig=") {
                signature = String::from(&trimmed[4..]);
                continue;
            }

            raw_signed_manifest.push_str(trimmed);
            raw_signed_manifest.push('\n');

            if let Some((key, val)) = trimmed.split_once('=') {
                let k = key.trim();
                let v = val.trim().trim_matches('"');
                match k {
                    "id" => id = String::from(v),
                    "name" => name = String::from(v),
                    "version" => version = String::from(v),
                    "version_code" => {
                        if let Ok(vc) = v.parse::<u32>() {
                            version_code = vc;
                        }
                    }
                    "icon" => {
                        icon_name = if v.is_empty() { String::from("generic") } else { String::from(v) };
                        icon = match v {
                            "sliders" | "settings" => Icon::Sliders,
                            "monitor" | "system" => Icon::Monitor,
                            "stopwatch" => Icon::Stopwatch,
                            "folder" | "files" => Icon::Folder,
                            "globe" | "browser" => Icon::Globe,
                            _ => Icon::Generic,
                        };
                    }
                    "abi" => {
                        abi = if v == "ring3-v1" { PackageAbi::Ring3V1 } else { PackageAbi::Legacy };
                    }
                    "target_arch" => target_arch = String::from(v),
                    "width_units" => {
                        if let Ok(w) = v.parse::<i32>() {
                            width_units = w;
                        }
                    }
                    "height_units" => {
                        if let Ok(h) = v.parse::<i32>() {
                            height_units = h;
                        }
                    }
                    "publisher_key_id" => publisher_key_id = String::from(v),
                    "payload_hash" => payload_hash = String::from(v),
                    "capabilities" => {
                        for cap in v.split(',') {
                            let c = cap.trim();
                            if !c.is_empty() {
                                capabilities.push(String::from(c));
                            }
                        }
                    }
                    "permissions" => {
                        for perm in v.split(',') {
                            let p = perm.trim();
                            if !p.is_empty() && permissions.len() < 32 {
                                permissions.push(String::from(p));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Kimlik dosya yollarında (`/data/app/<kısa-kimlik>.ntzc`) kullanıldığı için sıkı doğrula:
        // yol ayırıcı, boşluk vb. içeren kimlikler reddedilir (yol atlatma / path traversal önlemi).
        if name.is_empty() || !valid_app_id(&id) {
            return None;
        }

        Some(PackageManifest {
            id,
            name,
            version,
            version_code,
            icon,
            icon_name,
            abi,
            target_arch,
            width_units,
            height_units,
            publisher_key_id,
            payload_hash,
            capabilities,
            permissions,
            signature,
            raw_signed_manifest,
        })
    }

    /// Bitmask of strictly known runtime permissions declared in the manifest.
    /// Unknown permission names are ignored here (never silently granted) and
    /// reported separately via `unknown_permissions()`.
    pub fn declared_permission_mask(&self) -> u32 {
        let mut mask = 0u32;
        for name in &self.permissions {
            if let Some(bit) = nc_abi::perm::perm_from_name(name) {
                mask |= bit;
            }
        }
        mask
    }

    /// Declared permission identifiers not present in the canonical OS registry.
    pub fn unknown_permissions(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for name in &self.permissions {
            if nc_abi::perm::perm_from_name(name).is_none() {
                out.push(name.as_str());
            }
        }
        out
    }
}
