//! Canonical NetizenCore runtime permission registry and wire states.
//!
//! Runtime permissions are strictly separate from static package capabilities (`nc-caps`):
//! - Static capabilities govern API-class eligibility (clamped by trust tier).
//! - Runtime permissions record explicit user approval for sensitive resources per app ID.
//! - Declaring a permission in a package manifest is only a request, never a grant.
//! - Unknown permission names are never silently interpreted as grants.

/// Camera hardware capture and preview access (`camera`).
pub const PERM_CAMERA: u32 = 1 << 0;
/// Scoped read access to user photos/images in shared media (`media.photos.read`).
pub const PERM_MEDIA_PHOTOS_READ: u32 = 1 << 1;
/// Read access to the system Contacts service (`contacts.read`).
pub const PERM_CONTACTS_READ: u32 = 1 << 2;
/// Create/edit/delete access to the system Contacts service (`contacts.write`).
pub const PERM_CONTACTS_WRITE: u32 = 1 << 3;
/// Read access to protected SMS message history (`sms.read`).
pub const PERM_SMS_READ: u32 = 1 << 4;
/// Permission to transmit SMS messages through the telephony service (`sms.send`).
pub const PERM_SMS_SEND: u32 = 1 << 5;
/// Permission to initiate outgoing phone calls (`phone.call`).
pub const PERM_PHONE_CALL: u32 = 1 << 6;
/// Read access to telephony call history (`call_log.read`).
pub const PERM_CALL_LOG_READ: u32 = 1 << 7;
/// Reserved audio recording permission (`microphone.record`; unsupported until audio capture exists).
pub const PERM_MICROPHONE_RECORD: u32 = 1 << 8;

pub const ALL_KNOWN_PERMS: u32 = PERM_CAMERA
    | PERM_MEDIA_PHOTOS_READ
    | PERM_CONTACTS_READ
    | PERM_CONTACTS_WRITE
    | PERM_SMS_READ
    | PERM_SMS_SEND
    | PERM_PHONE_CALL
    | PERM_CALL_LOG_READ
    | PERM_MICROPHONE_RECORD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PermState {
    /// Not yet decided by the user; first access must prompt via the OS-owned dialog.
    Ask = 0,
    /// Explicitly granted by the user.
    Granted = 1,
    /// Explicitly denied by the user.
    Denied = 2,
    /// Platform/driver does not provide this resource (distinct from user denial).
    Unsupported = 3,
    /// The application manifest did not declare this runtime permission.
    NotDeclared = 4,
}

impl PermState {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Granted,
            2 => Self::Denied,
            3 => Self::Unsupported,
            4 => Self::NotDeclared,
            _ => Self::Ask,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PermRequestOutcome {
    /// OS-owned permission dialog is now displayed; poll `check()` on subsequent events.
    Pending = 0,
    /// Already granted.
    Granted = 1,
    /// Denied by user (must be changed in Settings).
    Denied = 2,
    /// Resource/API unsupported on this platform.
    Unsupported = 3,
    /// Permission not declared in the app's installed manifest.
    NotDeclared = 4,
}

impl PermRequestOutcome {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Granted,
            2 => Self::Denied,
            3 => Self::Unsupported,
            4 => Self::NotDeclared,
            _ => Self::Pending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermInfo {
    pub bit: u32,
    pub id: &'static str,
    pub title_tr: &'static str,
    pub title_en: &'static str,
    pub desc_tr: &'static str,
    pub desc_en: &'static str,
}

pub const PERM_REGISTRY: [PermInfo; 9] = [
    PermInfo {
        bit: PERM_CAMERA,
        id: "camera",
        title_tr: "Kamera",
        title_en: "Camera",
        desc_tr: "Kamera önizlemesi açma ve fotoğraf çekme",
        desc_en: "Open camera preview and capture photos",
    },
    PermInfo {
        bit: PERM_MEDIA_PHOTOS_READ,
        id: "media.photos.read",
        title_tr: "Fotoğraflar ve Medya",
        title_en: "Photos & Media",
        desc_tr: "Cihazdaki fotoğrafları görüntüleme",
        desc_en: "View photos stored on this device",
    },
    PermInfo {
        bit: PERM_CONTACTS_READ,
        id: "contacts.read",
        title_tr: "Kişileri Okuma",
        title_en: "Read Contacts",
        desc_tr: "Kayıtlı kişilerinizi görüntüleme ve arama",
        desc_en: "View and search your saved contacts",
    },
    PermInfo {
        bit: PERM_CONTACTS_WRITE,
        id: "contacts.write",
        title_tr: "Kişileri Düzenleme",
        title_en: "Modify Contacts",
        desc_tr: "Kişi ekleme, düzenleme ve silme",
        desc_en: "Add, edit, and delete contacts",
    },
    PermInfo {
        bit: PERM_SMS_READ,
        id: "sms.read",
        title_tr: "SMS Mesajlarını Okuma",
        title_en: "Read SMS Messages",
        desc_tr: "SMS konuşmalarını ve mesajları görüntüleme",
        desc_en: "View SMS conversations and messages",
    },
    PermInfo {
        bit: PERM_SMS_SEND,
        id: "sms.send",
        title_tr: "SMS Gönderme",
        title_en: "Send SMS Messages",
        desc_tr: "Kısa mesaj (SMS) gönderme",
        desc_en: "Send SMS text messages",
    },
    PermInfo {
        bit: PERM_PHONE_CALL,
        id: "phone.call",
        title_tr: "Telefon Aramaları",
        title_en: "Phone Calls",
        desc_tr: "Sesli telefon araması başlatma",
        desc_en: "Place voice phone calls",
    },
    PermInfo {
        bit: PERM_CALL_LOG_READ,
        id: "call_log.read",
        title_tr: "Arama Geçmişi",
        title_en: "Call History",
        desc_tr: "Geçmiş arama kayıtlarını okuma",
        desc_en: "Read call log history",
    },
    PermInfo {
        bit: PERM_MICROPHONE_RECORD,
        id: "microphone.record",
        title_tr: "Mikrofon",
        title_en: "Microphone",
        desc_tr: "Mikrofon ile ses kaydı (bu cihazda desteklenmiyor)",
        desc_en: "Record audio via microphone (unsupported on this device)",
    },
];

/// Localized prompt question heading for a runtime permission.
pub fn perm_prompt_question(bit: u32, en: bool) -> &'static str {
    match (bit, en) {
        (PERM_CAMERA, true) => "Allow Camera access?",
        (PERM_CAMERA, false) => "Kamera erişimine izin verilsin mi?",
        (PERM_MEDIA_PHOTOS_READ, true) => "Allow Photos & Media access?",
        (PERM_MEDIA_PHOTOS_READ, false) => "Fotoğraflar ve Medya erişimine izin verilsin mi?",
        (PERM_CONTACTS_READ, true) => "Allow reading your Contacts?",
        (PERM_CONTACTS_READ, false) => "Kişilerinizi okumaya izin verilsin mi?",
        (PERM_CONTACTS_WRITE, true) => "Allow modifying your Contacts?",
        (PERM_CONTACTS_WRITE, false) => "Kişilerinizi düzenlemeye izin verilsin mi?",
        (PERM_SMS_READ, true) => "Allow reading SMS messages?",
        (PERM_SMS_READ, false) => "SMS mesajlarını okumaya izin verilsin mi?",
        (PERM_SMS_SEND, true) => "Allow sending SMS messages?",
        (PERM_SMS_SEND, false) => "SMS göndermeye izin verilsin mi?",
        (PERM_PHONE_CALL, true) => "Allow placing phone calls?",
        (PERM_PHONE_CALL, false) => "Telefon araması yapmaya izin verilsin mi?",
        (PERM_CALL_LOG_READ, true) => "Allow reading Call History?",
        (PERM_CALL_LOG_READ, false) => "Arama geçmişini okumaya izin verilsin mi?",
        (PERM_MICROPHONE_RECORD, true) => "Allow Microphone access?",
        (PERM_MICROPHONE_RECORD, false) => "Mikrofon erişimine izin verilsin mi?",
        (_, true) => "Allow this permission?",
        (_, false) => "Bu izne onay verilsin mi?",
    }
}

/// Strict lookup from canonical permission identifier to bitmask.
/// Unknown names return `None` and must never be interpreted as grants.
pub fn perm_from_name(name: &str) -> Option<u32> {
    let trimmed = name.trim();
    for info in &PERM_REGISTRY {
        if info.id == trimmed {
            return Some(info.bit);
        }
    }
    None
}

pub fn perm_info(bit: u32) -> Option<&'static PermInfo> {
    if bit.count_ones() != 1 {
        return None;
    }
    PERM_REGISTRY.iter().find(|info| info.bit == bit)
}

pub fn perm_name(bit: u32) -> Option<&'static str> {
    perm_info(bit).map(|i| i.id)
}

pub const fn is_single_known_perm(bit: u32) -> bool {
    bit.count_ones() == 1 && (bit & ALL_KNOWN_PERMS) == bit
}

// Compile-time check: every registered permission has a unique single bit.
const _: () = {
    let mut i = 0;
    let mut seen = 0u32;
    while i < PERM_REGISTRY.len() {
        let b = PERM_REGISTRY[i].bit;
        assert!(b.count_ones() == 1, "permission must be a single bit");
        assert!(seen & b == 0, "duplicate permission bit");
        seen |= b;
        i += 1;
    }
    assert!(seen == ALL_KNOWN_PERMS, "ALL_KNOWN_PERMS mismatch");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_registry_round_trips_and_rejects_unknowns() {
        for info in &PERM_REGISTRY {
            assert_eq!(perm_from_name(info.id), Some(info.bit));
            assert_eq!(perm_name(info.bit), Some(info.id));
            assert!(is_single_known_perm(info.bit));
            assert!(!info.title_tr.is_empty() && !info.title_en.is_empty());
            assert!(!info.desc_tr.is_empty() && !info.desc_en.is_empty());
            assert!(!perm_prompt_question(info.bit, false).is_empty());
            assert!(!perm_prompt_question(info.bit, true).is_empty());
        }
        assert!(perm_info(PERM_CONTACTS_READ).unwrap().title_tr.contains('ş'));
        assert!(perm_info(PERM_CAMERA).unwrap().desc_tr.contains('ğ'));
        assert!(perm_prompt_question(PERM_CAMERA, false).contains('ş'));
        assert_eq!(perm_from_name(""), None);
        assert_eq!(perm_from_name("all"), None);
        assert_eq!(perm_from_name("fs.all.read"), None);
        assert_eq!(perm_from_name("camera.admin"), None);
        assert!(!is_single_known_perm(0));
        assert!(!is_single_known_perm(PERM_CAMERA | PERM_CONTACTS_READ));
        assert!(!is_single_known_perm(1 << 30));
    }
}
