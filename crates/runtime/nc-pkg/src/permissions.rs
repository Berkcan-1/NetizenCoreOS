//! Kernel-authoritative runtime permission store and strict XML persistence.
//!
//! Stored at `/data/appdata/nc.security/permissions.xml` (inaccessible to Ring 3 VFS).
//!
//! Security guarantees:
//! - Default-closed: missing or corrupted storage yields all-`Ask` (never implicit grants).
//! - Declaring a permission in a manifest is only a request, never a grant.
//! - System-tier classification NEVER implicitly grants runtime permissions.
//! - Schema version, app IDs, permission names, decision values, lengths, and record counts
//!   are strictly validated when parsing; any structural or semantic violation fails closed.
//! - Uninstalling a user application immediately removes its persisted permission records.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use nc_abi::perm::{
    is_single_known_perm, perm_from_name, perm_name, PermState, PERM_MICROPHONE_RECORD,
    PERM_REGISTRY,
};
use nc_abi::sys as wire;

use crate::manifest::valid_app_id;

pub const PERM_SCHEMA_VERSION: u32 = 1;
pub const MAX_PERMISSION_APPS: usize = 64;
pub const MAX_XML_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPermissionRecord {
    pub app_id: String,
    pub granted_mask: u32,
    pub denied_mask: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermParseError {
    TooLarge,
    BadHeader,
    UnsupportedSchema,
    MalformedTag,
    InvalidAppId,
    UnknownPermission,
    InvalidDecision,
    TooManyRecords,
    Truncated,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionStore {
    records: Vec<AppPermissionRecord>,
}

impl PermissionStore {
    pub const fn new() -> Self {
        Self { records: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn records(&self) -> &[AppPermissionRecord] {
        &self.records
    }

    pub fn masks_for(&self, app_id: &str) -> (u32, u32) {
        match self.records.iter().find(|r| r.app_id == app_id) {
            Some(rec) => (rec.granted_mask, rec.denied_mask),
            None => (0, 0),
        }
    }

    /// Evaluate the exact permission state for `(app_id, perm_bit)`.
    ///
    /// Note: `PERM_MICROPHONE_RECORD` is reserved as unsupported until a real audio
    /// capture backend exists; other permissions track user decisions (`Ask`/`Granted`/`Denied`)
    /// while hardware availability is enforced separately at service call time (`ERR_UNSUPPORTED`).
    pub fn check_state(&self, app_id: &str, declared_mask: u32, perm_bit: u32) -> PermState {
        if !valid_app_id(app_id) || !is_single_known_perm(perm_bit) {
            return PermState::NotDeclared;
        }
        if perm_bit == PERM_MICROPHONE_RECORD {
            return PermState::Unsupported;
        }
        if (declared_mask & perm_bit) == 0 {
            return PermState::NotDeclared;
        }
        let (granted, denied) = self.masks_for(app_id);
        if (denied & perm_bit) != 0 {
            PermState::Denied
        } else if (granted & perm_bit) != 0 {
            PermState::Granted
        } else {
            PermState::Ask
        }
    }

    /// Full enforcement gate at a sensitive kernel service boundary.
    ///
    /// Checks in strict order:
    /// 1. Authenticated non-empty valid `app_id` and valid single `perm_bit`
    /// 2. App manifest declared `perm_bit`
    /// 3. User granted `perm_bit` (`Granted`, not `Ask` or `Denied`)
    /// 4. Platform/driver reports the resource/service is available (`hw_supported`)
    pub fn enforce_service(
        &self,
        app_id: &str,
        declared_mask: u32,
        perm_bit: u32,
        hw_supported: bool,
    ) -> Result<(), i32> {
        if !valid_app_id(app_id) || !is_single_known_perm(perm_bit) {
            return Err(wire::ERR_BAD_REQUEST);
        }
        if perm_bit == PERM_MICROPHONE_RECORD {
            return Err(wire::ERR_UNSUPPORTED);
        }
        if (declared_mask & perm_bit) == 0 {
            return Err(wire::ERR_PERM_NOT_DECLARED);
        }
        match self.check_state(app_id, declared_mask, perm_bit) {
            PermState::Granted => {
                if hw_supported {
                    Ok(())
                } else {
                    Err(wire::ERR_UNSUPPORTED)
                }
            }
            PermState::Denied => Err(wire::ERR_PERM_DENIED),
            PermState::Ask => Err(wire::ERR_PERM_REQUIRED),
            PermState::Unsupported => Err(wire::ERR_UNSUPPORTED),
            PermState::NotDeclared => Err(wire::ERR_PERM_NOT_DECLARED),
        }
    }

    /// Update a persisted user decision (`Ask`, `Granted`, or `Denied`) for `(app_id, perm_bit)`.
    /// Setting `Ask` clears both granted and denied bits so the app will prompt again on next access.
    pub fn set_decision(&mut self, app_id: &str, perm_bit: u32, state: PermState) -> bool {
        if !valid_app_id(app_id) || !is_single_known_perm(perm_bit) || perm_bit == PERM_MICROPHONE_RECORD {
            return false;
        }
        if !matches!(state, PermState::Ask | PermState::Granted | PermState::Denied) {
            return false;
        }
        if let Some(rec) = self.records.iter_mut().find(|r| r.app_id == app_id) {
            rec.granted_mask &= !perm_bit;
            rec.denied_mask &= !perm_bit;
            match state {
                PermState::Granted => rec.granted_mask |= perm_bit,
                PermState::Denied => rec.denied_mask |= perm_bit,
                _ => {}
            }
            if rec.granted_mask == 0 && rec.denied_mask == 0 {
                self.records.retain(|r| r.app_id != app_id);
            }
            return true;
        }
        if state == PermState::Ask {
            return true;
        }
        if self.records.len() >= MAX_PERMISSION_APPS {
            return false;
        }
        let mut rec = AppPermissionRecord {
            app_id: String::from(app_id),
            granted_mask: 0,
            denied_mask: 0,
        };
        match state {
            PermState::Granted => rec.granted_mask = perm_bit,
            PermState::Denied => rec.denied_mask = perm_bit,
            _ => {}
        }
        self.records.push(rec);
        true
    }

    /// Purge all runtime permission decisions for an uninstalled user application.
    pub fn remove_app(&mut self, app_id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.app_id != app_id);
        self.records.len() != before
    }

    /// Deterministic versioned XML serialization.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        let _ = writeln!(out, "<permissions schema=\"{}\">", PERM_SCHEMA_VERSION);
        let mut sorted: Vec<&AppPermissionRecord> = self.records.iter().collect();
        sorted.sort_by(|a, b| a.app_id.as_str().cmp(b.app_id.as_str()));
        for rec in sorted {
            if rec.granted_mask == 0 && rec.denied_mask == 0 {
                continue;
            }
            let _ = writeln!(out, "  <app id=\"{}\">", rec.app_id);
            for info in &PERM_REGISTRY {
                if (rec.granted_mask & info.bit) != 0 {
                    let _ = writeln!(out, "    <grant name=\"{}\" state=\"granted\"/>", info.id);
                } else if (rec.denied_mask & info.bit) != 0 {
                    let _ = writeln!(out, "    <grant name=\"{}\" state=\"denied\"/>", info.id);
                }
            }
            let _ = writeln!(out, "  </app>");
        }
        let _ = writeln!(out, "</permissions>");
        out
    }

    /// Strict, bounded, fail-closed XML parser.
    /// Any corruption, truncation, unknown schema, invalid app ID, unknown permission name,
    /// or unexpected state rejects the entire file (caller must keep store empty/ask-all).
    pub fn from_xml(text: &str) -> Result<Self, PermParseError> {
        if text.len() > MAX_XML_BYTES {
            return Err(PermParseError::TooLarge);
        }
        let mut store = PermissionStore::new();
        let mut saw_root_open = false;
        let mut saw_root_close = false;
        let mut current_app: Option<AppPermissionRecord> = None;

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with("<?xml") {
                continue;
            }
            if saw_root_close {
                return Err(PermParseError::MalformedTag);
            }
            if !saw_root_open {
                let schema = parse_single_attr(line, "<permissions ", "schema", ">")
                    .ok_or(PermParseError::BadHeader)?;
                let ver = schema.parse::<u32>().map_err(|_| PermParseError::UnsupportedSchema)?;
                if ver != PERM_SCHEMA_VERSION {
                    return Err(PermParseError::UnsupportedSchema);
                }
                saw_root_open = true;
                continue;
            }
            if line == "</permissions>" {
                if current_app.is_some() {
                    return Err(PermParseError::Truncated);
                }
                saw_root_close = true;
                continue;
            }
            if line.starts_with("<app ") {
                if current_app.is_some() {
                    return Err(PermParseError::MalformedTag);
                }
                let id = parse_single_attr(line, "<app ", "id", ">")
                    .ok_or(PermParseError::MalformedTag)?;
                if !valid_app_id(id) {
                    return Err(PermParseError::InvalidAppId);
                }
                if store.records.len() >= MAX_PERMISSION_APPS
                    || store.records.iter().any(|r| r.app_id == id)
                {
                    return Err(PermParseError::TooManyRecords);
                }
                current_app = Some(AppPermissionRecord {
                    app_id: String::from(id),
                    granted_mask: 0,
                    denied_mask: 0,
                });
                continue;
            }
            if line == "</app>" {
                let rec = current_app.take().ok_or(PermParseError::MalformedTag)?;
                if rec.granted_mask != 0 || rec.denied_mask != 0 {
                    store.records.push(rec);
                }
                continue;
            }
            if line.starts_with("<grant ") {
                let rec = current_app.as_mut().ok_or(PermParseError::MalformedTag)?;
                let (name, state) = parse_two_attrs(line, "<grant ", "name", "state", "/>")
                    .ok_or(PermParseError::MalformedTag)?;
                let bit = perm_from_name(name).ok_or(PermParseError::UnknownPermission)?;
                if bit == PERM_MICROPHONE_RECORD {
                    return Err(PermParseError::InvalidDecision);
                }
                match state {
                    "granted" => {
                        rec.denied_mask &= !bit;
                        rec.granted_mask |= bit;
                    }
                    "denied" => {
                        rec.granted_mask &= !bit;
                        rec.denied_mask |= bit;
                    }
                    _ => return Err(PermParseError::InvalidDecision),
                }
                continue;
            }
            return Err(PermParseError::MalformedTag);
        }

        if !saw_root_open || !saw_root_close || current_app.is_some() {
            return Err(PermParseError::Truncated);
        }
        Ok(store)
    }

    pub fn format_perm_list(mask: u32) -> String {
        let mut out = String::new();
        for info in &PERM_REGISTRY {
            if (mask & info.bit) != 0 {
                if !out.is_empty() {
                    out.push(',');
                }
                out.push_str(perm_name(info.bit).unwrap_or(info.id));
            }
        }
        out
    }
}

fn parse_single_attr<'a>(line: &'a str, prefix: &str, key: &str, suffix: &str) -> Option<&'a str> {
    let body = line.strip_prefix(prefix)?.strip_suffix(suffix)?.trim();
    let rest = body.strip_prefix(key)?.strip_prefix("=\"")?;
    let val = rest.strip_suffix('"')?;
    if val.contains('"') || val.contains('<') || val.contains('>') {
        return None;
    }
    Some(val)
}

fn parse_two_attrs<'a>(
    line: &'a str,
    prefix: &str,
    k1: &str,
    k2: &str,
    suffix: &str,
) -> Option<(&'a str, &'a str)> {
    let body = line.strip_prefix(prefix)?.strip_suffix(suffix)?.trim();
    let rest = body.strip_prefix(k1)?.strip_prefix("=\"")?;
    let q1 = rest.find('"')?;
    let v1 = &rest[..q1];
    let after1 = rest[q1 + 1..].trim_start();
    let rest2 = after1.strip_prefix(k2)?.strip_prefix("=\"")?;
    let v2 = rest2.strip_suffix('"')?;
    if v1.contains('<') || v1.contains('>') || v2.contains('"') || v2.contains('<') || v2.contains('>') {
        return None;
    }
    Some((v1, v2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_abi::perm::*;

    #[test]
    fn default_ask_and_system_apps_never_implicitly_granted() {
        let store = PermissionStore::new();
        let declared = PERM_CAMERA | PERM_CONTACTS_READ;
        assert_eq!(store.check_state("nc.camera", declared, PERM_CAMERA), PermState::Ask);
        assert_eq!(store.check_state("nc.camera", declared, PERM_SMS_SEND), PermState::NotDeclared);
        assert_eq!(
            store.check_state("nc.camera", declared | PERM_MICROPHONE_RECORD, PERM_MICROPHONE_RECORD),
            PermState::Unsupported
        );
        assert_eq!(
            store.enforce_service("nc.camera", declared, PERM_CAMERA, true),
            Err(wire::ERR_PERM_REQUIRED)
        );
        assert_eq!(
            store.enforce_service("nc.camera", declared, PERM_SMS_SEND, true),
            Err(wire::ERR_PERM_NOT_DECLARED)
        );
    }

    #[test]
    fn grant_deny_revoke_and_hardware_unavailable_are_distinct() {
        let mut store = PermissionStore::new();
        let declared = PERM_CAMERA | PERM_CONTACTS_READ;

        assert!(store.set_decision("nc.camera", PERM_CAMERA, PermState::Granted));
        assert_eq!(store.check_state("nc.camera", declared, PERM_CAMERA), PermState::Granted);
        // Hardware supported -> allowed
        assert_eq!(store.enforce_service("nc.camera", declared, PERM_CAMERA, true), Ok(()));
        // Hardware unavailable (e.g. QEMU) -> ERR_UNSUPPORTED (distinct from denial)
        assert_eq!(
            store.enforce_service("nc.camera", declared, PERM_CAMERA, false),
            Err(wire::ERR_UNSUPPORTED)
        );

        // User denies -> ERR_PERM_DENIED immediately
        assert!(store.set_decision("nc.camera", PERM_CAMERA, PermState::Denied));
        assert_eq!(store.check_state("nc.camera", declared, PERM_CAMERA), PermState::Denied);
        assert_eq!(
            store.enforce_service("nc.camera", declared, PERM_CAMERA, true),
            Err(wire::ERR_PERM_DENIED)
        );

        // Reset to Ask -> ERR_PERM_REQUIRED
        assert!(store.set_decision("nc.camera", PERM_CAMERA, PermState::Ask));
        assert_eq!(store.check_state("nc.camera", declared, PERM_CAMERA), PermState::Ask);
        assert_eq!(
            store.enforce_service("nc.camera", declared, PERM_CAMERA, true),
            Err(wire::ERR_PERM_REQUIRED)
        );
    }

    #[test]
    fn xml_persistence_roundtrip_and_uninstall_cleanup() {
        let mut store = PermissionStore::new();
        store.set_decision("nc.gallery", PERM_MEDIA_PHOTOS_READ, PermState::Granted);
        store.set_decision("com.example.chat", PERM_CONTACTS_READ, PermState::Granted);
        store.set_decision("com.example.chat", PERM_SMS_SEND, PermState::Denied);

        let xml = store.to_xml();
        let loaded = PermissionStore::from_xml(&xml).expect("valid xml must parse");
        assert_eq!(
            loaded.check_state("nc.gallery", PERM_MEDIA_PHOTOS_READ, PERM_MEDIA_PHOTOS_READ),
            PermState::Granted
        );
        assert_eq!(
            loaded.check_state("com.example.chat", PERM_CONTACTS_READ | PERM_SMS_SEND, PERM_CONTACTS_READ),
            PermState::Granted
        );
        assert_eq!(
            loaded.check_state("com.example.chat", PERM_CONTACTS_READ | PERM_SMS_SEND, PERM_SMS_SEND),
            PermState::Denied
        );

        let mut cleaned = loaded;
        assert!(cleaned.remove_app("com.example.chat"));
        assert_eq!(
            cleaned.check_state("com.example.chat", PERM_CONTACTS_READ, PERM_CONTACTS_READ),
            PermState::Ask
        );
        assert_eq!(
            cleaned.check_state("nc.gallery", PERM_MEDIA_PHOTOS_READ, PERM_MEDIA_PHOTOS_READ),
            PermState::Granted
        );
    }

    #[test]
    fn corrupt_or_truncated_xml_fails_closed() {
        let mut store = PermissionStore::new();
        store.set_decision("nc.contacts", PERM_CONTACTS_READ, PermState::Granted);
        let good = store.to_xml();

        // Truncated
        assert!(PermissionStore::from_xml(&good[..good.len() - 10]).is_err());
        // Unknown permission name
        let bad_perm = good.replace("contacts.read", "fs.all.write");
        assert_eq!(PermissionStore::from_xml(&bad_perm), Err(PermParseError::UnknownPermission));
        // Bad schema version
        let bad_schema = good.replace("schema=\"1\"", "schema=\"99\"");
        assert_eq!(PermissionStore::from_xml(&bad_schema), Err(PermParseError::UnsupportedSchema));
        // Invalid decision value
        let bad_dec = good.replace("state=\"granted\"", "state=\"always\"");
        assert_eq!(PermissionStore::from_xml(&bad_dec), Err(PermParseError::InvalidDecision));
        // Path traversal in app id
        let bad_id = good.replace("id=\"nc.contacts\"", "id=\"../evil\"");
        assert_eq!(PermissionStore::from_xml(&bad_id), Err(PermParseError::InvalidAppId));
    }

    #[test]
    fn manifest_permissions_and_trusted_builtins() {
        let text = "id=com.example.cam\nname=Demo Cam\npermissions=camera, media.photos.read, unknown.perm\n";
        let m = crate::PackageManifest::parse(text).expect("manifest parses");
        assert_eq!(m.declared_permission_mask(), PERM_CAMERA | PERM_MEDIA_PHOTOS_READ);
        assert_eq!(m.unknown_permissions(), alloc::vec!["unknown.perm"]);
        for id in ["nc.gallery", "nc.messages", "nc.phone", "nc.contacts", "nc.camera", "nc.keyboard"] {
            assert!(crate::NtzcPackage::is_trusted_builtin(id), "expected builtin: {id}");
        }
        assert!(!crate::NtzcPackage::is_trusted_builtin("com.example.cam"));
    }
}
