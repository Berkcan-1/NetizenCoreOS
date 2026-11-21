//! Kernel-enforced runtime permission manager, OS-owned permission dialog,
//! and protected mobile services (Contacts, Scoped Media, Telephony/SMS, Camera).
//!
//! Security invariants:
//! - Caller identity is ALWAYS read from `task::current_app_id()`; Ring 3 payloads can NEVER
//!   impersonate another application.
//! - Permission decisions persist to `/data/appdata/nc.security/permissions.xml`, which is
//!   blocked from all Ring 3 VFS access by SPARK (`nc_security_filesystem.adb`).
//! - Contacts persist to `/data/appdata/nc.contacts.db/contacts.xml`, also blocked from all
//!   Ring 3 VFS access by SPARK so no app can bypass `contacts.read` / `contacts.write`.
//! - System-tier apps NEVER receive automatic runtime permission grants.
//! - Hardware availability is reported truthfully from `nc_device_select::PROFILE.hw_ops`.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;
use nc_abi::perm::{
    is_single_known_perm, perm_info, perm_prompt_question, PermRequestOutcome, PermState,
    PERM_CALL_LOG_READ, PERM_CAMERA, PERM_CONTACTS_READ, PERM_CONTACTS_WRITE,
    PERM_MEDIA_PHOTOS_READ, PERM_MICROPHONE_RECORD, PERM_PHONE_CALL, PERM_REGISTRY, PERM_SMS_READ,
    PERM_SMS_SEND,
};
use nc_abi::sys as wire;
use nc_device_select::PROFILE;
use nc_fs::{FileSystem, FileType};
use nc_gfx::{Canvas, Rect};
use nc_media::{decode_jpeg_wallpaper, decode_png_wallpaper, inspect, FitMode, MediaKind};
use nc_pkg::contacts::ContactsStore;
use nc_pkg::permissions::PermissionStore;

use crate::{input, storage, task};

const PERM_XML_PATH: &str = "/appdata/nc.security/permissions.xml";
const PERM_TMP_PATH: &str = "/appdata/nc.security/permissions.tmp";
const PERM_BAK_PATH: &str = "/appdata/nc.security/permissions.bak";

const CONTACTS_XML_PATH: &str = "/appdata/nc.contacts.db/contacts.xml";
const CONTACTS_TMP_PATH: &str = "/appdata/nc.contacts.db/contacts.tmp";
const CONTACTS_BAK_PATH: &str = "/appdata/nc.contacts.db/contacts.bak";

const MAX_MEDIA_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MEDIA_DECODE_PIXELS: u64 = 3_000_000;
const MAX_MEDIA_LIST: usize = 48;

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

const PROMPT_ENTER_MS: u32 = 210;
const PROMPT_EXIT_MS: u32 = 150;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptPhase {
    Entering,
    Visible,
    Exiting,
}

#[derive(Clone)]
struct PendingPrompt {
    caller_slot: usize,
    app_id: String,
    app_name: String,
    icon_id: u32,
    perm_bit: u32,
    phase: PromptPhase,
    phase_start_ms: u32,
    anim_p: i32, // 0..=256
    pressed_btn: u8, // 0=none, 1=deny, 2=allow
    decided_slot: Option<usize>,
}

fn now_ms() -> u32 {
    (task::ticks() * 1000 / task::TICK_HZ) as u32
}

fn ease_out_cubic_256(t: i32) -> i32 {
    let t = t.clamp(0, 256);
    let inv = 256 - t;
    256 - (inv * inv * inv) / (256 * 256)
}

fn ease_in_cubic_256(t: i32) -> i32 {
    let t = t.clamp(0, 256);
    (t * t * t) / (256 * 256)
}

struct MediaPreview {
    owner_slot: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pixels: Vec<u32>,
    gen: u32,
}

struct PermRuntime {
    store: PermissionStore,
    contacts: ContactsStore,
    prompt: Option<PendingPrompt>,
    prompt_gen: u32,
    preview: Option<MediaPreview>,
    preview_gen: u32,
    open_settings_requested: bool,
}

static mut RUNTIME: PermRuntime = PermRuntime {
    store: PermissionStore::new(),
    contacts: ContactsStore::new(),
    prompt: None,
    prompt_gen: 0,
    preview: None,
    preview_gen: 0,
    open_settings_requested: false,
};

fn rt() -> &'static mut PermRuntime {
    unsafe { &mut *addr_of_mut!(RUNTIME) }
}

pub fn load_from_storage() {
    let r = rt();
    r.store = PermissionStore::new();
    r.contacts = ContactsStore::new();

    let Some(fs) = storage::data_fs() else {
        log!("[izin] /data yok; tum izinler varsayilan Ask (kapali) durumda");
        return;
    };

    // Fail-closed load of permissions.xml (with backup recovery if primary is corrupt)
    let mut loaded_perms = false;
    for path in [PERM_XML_PATH, PERM_BAK_PATH] {
        if let Ok(bytes) = fs.read_file(path) {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                match PermissionStore::from_xml(text) {
                    Ok(store) => {
                        r.store = store;
                        loaded_perms = true;
                        if path == PERM_BAK_PATH {
                            log!("[izin] permissions.xml bozuk; yedekten kurtarildi");
                        } else {
                            log!("[izin] permissions.xml yuklendi ({} uygulama kaydi)", r.store.records().len());
                        }
                        break;
                    }
                    Err(e) => {
                        log!("[izin] {} reddedildi (fail-closed): {:?}", path, e);
                    }
                }
            }
        }
    }
    if !loaded_perms {
        log!("[izin] kalici izin kaydi yok; tum izinler Ask (onay gerekir)");
    }

    // Fail-closed load of system-managed contacts.xml
    for path in [CONTACTS_XML_PATH, CONTACTS_BAK_PATH] {
        if let Ok(bytes) = fs.read_file(path) {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                if let Ok(contacts) = ContactsStore::from_xml(text) {
                    r.contacts = contacts;
                    log!("[kisiler] veritabani yuklendi ({} kayit)", r.contacts.records().len());
                    break;
                }
            }
        }
    }
}

fn save_permissions() -> bool {
    let Some(fs) = storage::data_fs() else { return false };
    if !storage::data_writable() {
        return false;
    }
    let _ = fs.create_dir("/appdata");
    let _ = fs.create_dir("/appdata/nc.security");
    let xml = rt().store.to_xml();
    if fs.write_file(PERM_TMP_PATH, xml.as_bytes()).is_err() {
        return false;
    }
    if let Ok(prev) = fs.read_file(PERM_XML_PATH) {
        if !prev.is_empty() {
            let _ = fs.write_file(PERM_BAK_PATH, &prev);
        }
    }
    if fs.write_file(PERM_XML_PATH, xml.as_bytes()).is_err() {
        return false;
    }
    let _ = fs.remove(PERM_TMP_PATH);
    storage::sync_data()
}

fn save_contacts() -> bool {
    let Some(fs) = storage::data_fs() else { return false };
    if !storage::data_writable() {
        return false;
    }
    let _ = fs.create_dir("/appdata");
    let _ = fs.create_dir("/appdata/nc.contacts.db");
    let xml = rt().contacts.to_xml();
    if fs.write_file(CONTACTS_TMP_PATH, xml.as_bytes()).is_err() {
        return false;
    }
    if let Ok(prev) = fs.read_file(CONTACTS_XML_PATH) {
        if !prev.is_empty() {
            let _ = fs.write_file(CONTACTS_BAK_PATH, &prev);
        }
    }
    if fs.write_file(CONTACTS_XML_PATH, xml.as_bytes()).is_err() {
        return false;
    }
    let _ = fs.remove(CONTACTS_TMP_PATH);
    storage::sync_data()
}

pub fn on_user_app_uninstalled(app_id: &str) {
    let r = rt();
    if r.store.remove_app(app_id) {
        let _ = save_permissions();
        log!("[izin] kaldirilan uygulama izinleri temizlendi: {}", app_id);
    }
    if let Some(p) = &r.prompt {
        if p.app_id == app_id {
            r.prompt = None;
            r.prompt_gen = r.prompt_gen.wrapping_add(1);
        }
    }
}

pub fn on_task_terminated(slot: usize) {
    let r = rt();
    if let Some(p) = &r.prompt {
        if p.caller_slot == slot {
            r.prompt = None;
            r.prompt_gen = r.prompt_gen.wrapping_add(1);
        }
    }
    if let Some(prev) = &r.preview {
        if prev.owner_slot == slot {
            r.preview = None;
            r.preview_gen = r.preview_gen.wrapping_add(1);
        }
    }
}

pub fn clear_preview_for_slot(slot: usize) {
    let r = rt();
    if let Some(prev) = &r.preview {
        if prev.owner_slot == slot {
            r.preview = None;
            r.preview_gen = r.preview_gen.wrapping_add(1);
        }
    }
}

pub fn take_open_settings_request() -> bool {
    let r = rt();
    let val = r.open_settings_requested;
    r.open_settings_requested = false;
    val
}

pub fn prompt_active() -> bool {
    rt().prompt.is_some()
}

pub fn prompt_animating() -> bool {
    match &rt().prompt {
        Some(p) => p.phase != PromptPhase::Visible,
        None => false,
    }
}

pub enum PromptTickResult {
    Idle,
    Redraw,
    FinishedExit { caller_slot: usize },
}

/// Advance the kernel-owned permission dialog animation state using monotonic ticks.
/// Stops requesting frames immediately once settled (`Visible` or removed).
pub fn tick_prompt(reduced_motion: bool) -> PromptTickResult {
    let r = rt();
    let Some(p) = r.prompt.as_mut() else {
        return PromptTickResult::Idle;
    };
    let now = now_ms();
    match p.phase {
        PromptPhase::Visible => PromptTickResult::Idle,
        PromptPhase::Entering => {
            if reduced_motion {
                p.anim_p = 256;
                p.phase = PromptPhase::Visible;
                return PromptTickResult::Redraw;
            }
            let elapsed = now.wrapping_sub(p.phase_start_ms);
            if elapsed >= PROMPT_ENTER_MS {
                p.anim_p = 256;
                p.phase = PromptPhase::Visible;
            } else {
                let raw = (elapsed as i64 * 256 / PROMPT_ENTER_MS as i64) as i32;
                p.anim_p = ease_out_cubic_256(raw).clamp(1, 256);
            }
            PromptTickResult::Redraw
        }
        PromptPhase::Exiting => {
            if reduced_motion {
                let slot = p.decided_slot.unwrap_or(p.caller_slot);
                r.prompt = None;
                r.prompt_gen = r.prompt_gen.wrapping_add(1);
                return PromptTickResult::FinishedExit { caller_slot: slot };
            }
            let elapsed = now.wrapping_sub(p.phase_start_ms);
            if elapsed >= PROMPT_EXIT_MS {
                let slot = p.decided_slot.unwrap_or(p.caller_slot);
                r.prompt = None;
                r.prompt_gen = r.prompt_gen.wrapping_add(1);
                return PromptTickResult::FinishedExit { caller_slot: slot };
            }
            let raw = (elapsed as i64 * 256 / PROMPT_EXIT_MS as i64) as i32;
            p.anim_p = (256 - ease_in_cubic_256(raw)).clamp(0, 256);
            PromptTickResult::Redraw
        }
    }
}

pub fn overlay_signature_bits() -> u64 {
    let r = rt();
    let p_bits = match &r.prompt {
        Some(p) => {
            let phase_tag = match p.phase {
                PromptPhase::Entering => 1u64,
                PromptPhase::Visible => 2u64,
                PromptPhase::Exiting => 3u64,
            };
            ((r.prompt_gen as u64) << 32)
                | (p.perm_bit as u64)
                | ((p.anim_p as u64 & 0x1FF) << 16)
                | ((p.pressed_btn as u64 & 0x3) << 25)
                | (phase_tag << 27)
                | 0x8000_0000_0000_0000
        }
        None => (r.prompt_gen as u64) << 32,
    };
    let m_bits = match &r.preview {
        Some(m) => ((m.gen as u64) << 16) ^ ((m.w as u64) << 8) ^ (m.h as u64) ^ 0x4000_0000,
        None => (r.preview_gen as u64) << 16,
    };
    p_bits ^ m_bits
}

pub fn hw_available_for(perm_bit: u32) -> bool {
    match perm_bit {
        PERM_CAMERA => PROFILE.hw_ops.has_hw_camera,
        PERM_MEDIA_PHOTOS_READ => storage::data_fs().is_some(),
        PERM_CONTACTS_READ | PERM_CONTACTS_WRITE => true,
        PERM_SMS_READ | PERM_SMS_SEND | PERM_PHONE_CALL | PERM_CALL_LOG_READ => {
            PROFILE.hw_ops.has_hw_cellular
        }
        PERM_MICROPHONE_RECORD => false,
        _ => false,
    }
}

pub fn unsupported_mask_for_device() -> u32 {
    let mut mask = 0u32;
    for info in &PERM_REGISTRY {
        if !hw_available_for(info.bit) {
            mask |= info.bit;
        }
    }
    mask
}

fn caller_context() -> (usize, String, u32, u32) {
    let slot = task::current_slot();
    let mut id_buf = [0u8; 32];
    let len = task::current_app_id(&mut id_buf);
    let id = String::from(core::str::from_utf8(&id_buf[..len]).unwrap_or(""));
    let caps = task::current_caps();
    let declared = storage::find_package(&id).map(|p| p.declared_perms).unwrap_or(0);
    (slot, id, caps, declared)
}

fn enforce_caller_perm(perm_bit: u32) -> Result<(usize, String, u32), i32> {
    let (slot, id, caps, declared) = caller_context();
    if id.is_empty() {
        return Err(wire::ERR_DENIED);
    }
    let hw = hw_available_for(perm_bit);
    rt().store.enforce_service(&id, declared, perm_bit, hw)?;
    Ok((slot, id, caps))
}

/// Handle `OP_PERM_CALL`. Returns `(status, body_bytes, need_overlay_redraw)`.
pub fn handle_perm_call(req: &wire::Request<'_>) -> (i32, Vec<u8>, bool) {
    let (slot, caller_id, caps, declared) = caller_context();
    let r = rt();

    match req.arg0 {
        wire::PERM_SUB_CHECK => {
            let bit = req.arg1;
            if !is_single_known_perm(bit) {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            }
            let st = r.store.check_state(&caller_id, declared, bit);
            let hw = hw_available_for(bit) as u8;
            let pending = r
                .prompt
                .as_ref()
                .map(|p| p.app_id == caller_id && p.perm_bit == bit)
                .unwrap_or(false) as u8;
            (0, alloc::vec![st as u8, hw, pending], false)
        }
        wire::PERM_SUB_REQUEST => {
            let bit = req.arg1;
            if !is_single_known_perm(bit) {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            }
            let st = r.store.check_state(&caller_id, declared, bit);
            match st {
                PermState::NotDeclared => {
                    (0, alloc::vec![PermRequestOutcome::NotDeclared as u8], false)
                }
                PermState::Unsupported => {
                    (0, alloc::vec![PermRequestOutcome::Unsupported as u8], false)
                }
                PermState::Granted => (0, alloc::vec![PermRequestOutcome::Granted as u8], false),
                PermState::Denied => (0, alloc::vec![PermRequestOutcome::Denied as u8], false),
                PermState::Ask => {
                    // Deduplicate: do not create repeated dialogs on polling/redraw.
                    if let Some(cur) = &r.prompt {
                        if cur.app_id == caller_id && cur.perm_bit == bit {
                            return (0, alloc::vec![PermRequestOutcome::Pending as u8], false);
                        }
                        // Another prompt is already open
                        return (0, alloc::vec![PermRequestOutcome::Pending as u8], false);
                    }
                    let (app_name, icon_id) = match storage::find_package(&caller_id) {
                        Some(p) => (p.name.clone(), p.icon_id),
                        None => (caller_id.clone(), nc_abi::draw::ICON_GENERIC),
                    };
                    r.prompt = Some(PendingPrompt {
                        caller_slot: slot,
                        app_id: caller_id,
                        app_name,
                        icon_id,
                        perm_bit: bit,
                        phase: PromptPhase::Entering,
                        phase_start_ms: now_ms(),
                        anim_p: 8,
                        pressed_btn: 0,
                        decided_slot: None,
                    });
                    r.prompt_gen = r.prompt_gen.wrapping_add(1);
                    (0, alloc::vec![PermRequestOutcome::Pending as u8], true)
                }
            }
        }
        wire::PERM_SUB_LIST_ALL => {
            if caps & nc_caps::SECURITY_MANAGE == 0 {
                return (wire::ERR_DENIED, Vec::new(), false);
            }
            let unsup = unsupported_mask_for_device();
            let mut out = String::new();
            for pkg in storage::packages() {
                let (granted, denied) = r.store.masks_for(&pkg.id);
                let _ = writeln!(
                    out,
                    "app_perm={}|{:08x}|{:08x}|{:08x}|{:08x}",
                    pkg.id, pkg.declared_perms, granted, denied, unsup
                );
            }
            (0, out.into_bytes(), false)
        }
        wire::PERM_SUB_SET => {
            if caps & nc_caps::SECURITY_MANAGE == 0 {
                return (wire::ERR_DENIED, Vec::new(), false);
            }
            let Ok(target_id) = core::str::from_utf8(req.payload) else {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            };
            let target_id = target_id.trim();
            let Some(pkg) = storage::find_package(target_id) else {
                return (wire::ERR_NOT_FOUND, Vec::new(), false);
            };
            let bit = req.arg1;
            if !is_single_known_perm(bit) || (pkg.declared_perms & bit) == 0 {
                return (wire::ERR_PERM_NOT_DECLARED, Vec::new(), false);
            }
            let new_state = PermState::from_u8(req.arg2 as u8);
            if !r.store.set_decision(target_id, bit, new_state) {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            }
            let _ = save_permissions();
            // If a media permission was revoked, immediately clear any active preview owned by that app
            if bit == PERM_MEDIA_PHOTOS_READ && new_state != PermState::Granted {
                if let Some(target_slot) = task::find_app(target_id) {
                    clear_preview_for_slot(target_slot);
                }
            }
            (0, Vec::new(), true)
        }
        wire::PERM_SUB_OPEN_SETTINGS => {
            r.open_settings_requested = true;
            (0, Vec::new(), false)
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new(), false),
    }
}

/// Handle `OP_CONTACTS_CALL` (strictly enforces `contacts.read` / `contacts.write`).
pub fn handle_contacts_call(req: &wire::Request<'_>) -> (i32, Vec<u8>) {
    let r = rt();
    match req.arg0 {
        wire::CONTACTS_SUB_LIST => {
            if let Err(code) = enforce_caller_perm(PERM_CONTACTS_READ) {
                return (code, Vec::new());
            }
            let mut out = String::new();
            for c in r.contacts.records() {
                let _ = writeln!(out, "contact={}|{}|{}|{}|{}", c.id, c.name, c.phone, c.email, c.note);
            }
            (0, out.into_bytes())
        }
        wire::CONTACTS_SUB_LOOKUP => {
            if let Err(code) = enforce_caller_perm(PERM_CONTACTS_READ) {
                return (code, Vec::new());
            }
            let query = core::str::from_utf8(req.payload).unwrap_or("");
            let mut out = String::new();
            for c in r.contacts.lookup(query) {
                let _ = writeln!(out, "contact={}|{}|{}|{}|{}", c.id, c.name, c.phone, c.email, c.note);
            }
            (0, out.into_bytes())
        }
        wire::CONTACTS_SUB_UPSERT => {
            if let Err(code) = enforce_caller_perm(PERM_CONTACTS_WRITE) {
                return (code, Vec::new());
            }
            let mut parts = req.payload.splitn(4, |&b| b == 0);
            let name = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
            let phone = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
            let email = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
            let note = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
            match r.contacts.upsert(req.arg1, name, phone, email, note) {
                Ok(id) => {
                    let _ = save_contacts();
                    let mut out = String::new();
                    let _ = writeln!(out, "id={}", id);
                    (0, out.into_bytes())
                }
                Err(_) => (wire::ERR_BAD_REQUEST, Vec::new()),
            }
        }
        wire::CONTACTS_SUB_DELETE => {
            if let Err(code) = enforce_caller_perm(PERM_CONTACTS_WRITE) {
                return (code, Vec::new());
            }
            if r.contacts.delete(req.arg1) {
                let _ = save_contacts();
                (0, Vec::new())
            } else {
                (wire::ERR_NOT_FOUND, Vec::new())
            }
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

fn valid_media_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 44
        && !name.contains(['/', '\\', '\0'])
        && !name.starts_with('.')
        && name.chars().all(|c| !c.is_control())
}

fn is_supported_image_name(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            ext.eq_ignore_ascii_case("png")
                || ext.eq_ignore_ascii_case("jpg")
                || ext.eq_ignore_ascii_case("jpeg")
        }
        None => false,
    }
}

fn decode_media_image(name: &str, max_w: u32, max_h: u32) -> Option<(u32, u32, Vec<u32>)> {
    if !valid_media_filename(name) || !is_supported_image_name(name) {
        return None;
    }
    let fs = storage::data_fs()?;
    let mut path = String::from("/media/");
    path.push_str(name);
    let bytes = fs.read_file(&path).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_FILE_BYTES {
        return None;
    }
    let info = inspect(&bytes).ok()?;
    let (sw, sh) = (info.width? as u64, info.height? as u64);
    if sw == 0 || sh == 0 || sw.saturating_mul(sh) > MAX_MEDIA_DECODE_PIXELS {
        return None;
    }
    let raster = match info.kind {
        MediaKind::Png => decode_png_wallpaper(&bytes, max_w, max_h, FitMode::Fit),
        MediaKind::Jpeg => decode_jpeg_wallpaper(&bytes, max_w, max_h, FitMode::Fit),
        _ => return None,
    }
    .ok()?;
    let mut pixels = raster.pixels_argb;
    for px in pixels.iter_mut() {
        *px &= 0x00FF_FFFF;
    }
    Some((raster.width, raster.height, pixels))
}

/// Handle `OP_MEDIA_CALL` (scoped photo access gated strictly by `media.photos.read`).
pub fn handle_media_call(req: &wire::Request<'_>) -> (i32, Vec<u8>, bool) {
    match req.arg0 {
        wire::MEDIA_SUB_PREVIEW_CLEAR => {
            let slot = task::current_slot();
            clear_preview_for_slot(slot);
            (0, Vec::new(), true)
        }
        wire::MEDIA_SUB_LIST => {
            if let Err(code) = enforce_caller_perm(PERM_MEDIA_PHOTOS_READ) {
                return (code, Vec::new(), false);
            }
            let Some(fs) = storage::data_fs() else {
                return (wire::ERR_UNSUPPORTED, Vec::new(), false);
            };
            let Ok(entries) = fs.list_dir("/media") else {
                return (0, Vec::new(), false);
            };
            let mut names: Vec<(String, u64)> = Vec::new();
            for entry in entries.iter() {
                if !matches!(entry.file_type, FileType::RegularFile) {
                    continue;
                }
                if !valid_media_filename(&entry.name) || !is_supported_image_name(&entry.name) {
                    continue;
                }
                if entry.size == 0 || entry.size > MAX_MEDIA_FILE_BYTES as u64 {
                    continue;
                }
                names.push((entry.name.clone(), entry.size));
            }
            names.sort_by(|a, b| a.0.cmp(&b.0));
            names.truncate(MAX_MEDIA_LIST);

            let mut out = String::new();
            for (name, size) in names {
                let mut header = [0u8; 64];
                let mut path = String::from("/media/");
                path.push_str(&name);
                let (w, h, fmt) = match fs.read_file_range(&path, 0, &mut header) {
                    Ok(n) if n > 0 => match inspect(&header[..n]) {
                        Ok(info) => (
                            info.width.unwrap_or(0),
                            info.height.unwrap_or(0),
                            match info.kind {
                                MediaKind::Png => "PNG",
                                MediaKind::Jpeg => "JPEG",
                                _ => "IMG",
                            },
                        ),
                        Err(_) => (0, 0, "IMG"),
                    },
                    _ => (0, 0, "IMG"),
                };
                let _ = writeln!(out, "photo={}|{}|{}|{}|{}", name, size, w, h, fmt);
            }
            (0, out.into_bytes(), false)
        }
        wire::MEDIA_SUB_THUMB => {
            if let Err(code) = enforce_caller_perm(PERM_MEDIA_PHOTOS_READ) {
                return (code, Vec::new(), false);
            }
            let Ok(name) = core::str::from_utf8(req.payload) else {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            };
            let Some((tw, th, pixels)) = decode_media_image(name.trim(), 10, 10) else {
                return (wire::ERR_NOT_FOUND, Vec::new(), false);
            };
            let mut out = Vec::with_capacity(2 + (tw * th * 3) as usize);
            out.push(tw as u8);
            out.push(th as u8);
            for &px in &pixels {
                out.push(((px >> 16) & 0xFF) as u8);
                out.push(((px >> 8) & 0xFF) as u8);
                out.push((px & 0xFF) as u8);
            }
            (0, out, false)
        }
        wire::MEDIA_SUB_PREVIEW_SET => {
            let (slot, _, _) = match enforce_caller_perm(PERM_MEDIA_PHOTOS_READ) {
                Ok(v) => v,
                Err(code) => return (code, Vec::new(), false),
            };
            let Ok(name) = core::str::from_utf8(req.payload) else {
                return (wire::ERR_BAD_REQUEST, Vec::new(), false);
            };
            let x = ((req.arg1 >> 16) & 0xFFFF) as i32;
            let y = (req.arg1 & 0xFFFF) as i32;
            let w = (((req.arg2 >> 16) & 0xFFFF) as i32).clamp(32, 1024);
            let h = ((req.arg2 & 0xFFFF) as i32).clamp(32, 1024);
            let Some((dw, dh, pixels)) = decode_media_image(name.trim(), w as u32, h as u32) else {
                return (wire::ERR_NOT_FOUND, Vec::new(), false);
            };
            let r = rt();
            r.preview_gen = r.preview_gen.wrapping_add(1);
            r.preview = Some(MediaPreview {
                owner_slot: slot,
                x,
                y,
                w: dw as i32,
                h: dh as i32,
                pixels,
                gen: r.preview_gen,
            });
            (0, Vec::new(), true)
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new(), false),
    }
}

/// Handle `OP_TELEPHONY_CALL` (truthful hardware status + runtime permission enforcement).
pub fn handle_telephony_call(req: &wire::Request<'_>) -> (i32, Vec<u8>) {
    let cell = PROFILE.hw_ops.has_hw_cellular;
    match req.arg0 {
        wire::TEL_SUB_STATUS => {
            let mut out = String::new();
            let _ = writeln!(out, "voice_avail={}", cell as u8);
            let _ = writeln!(out, "sms_avail={}", cell as u8);
            let _ = writeln!(out, "call_log_avail={}", cell as u8);
            (0, out.into_bytes())
        }
        wire::TEL_SUB_DIAL => {
            if let Err(code) = enforce_caller_perm(PERM_PHONE_CALL) {
                return (code, Vec::new());
            }
            // No modem/RIL driver currently implements voice calls
            (wire::ERR_UNSUPPORTED, Vec::new())
        }
        wire::TEL_SUB_CALL_LOG => {
            if let Err(code) = enforce_caller_perm(PERM_CALL_LOG_READ) {
                return (code, Vec::new());
            }
            (wire::ERR_UNSUPPORTED, Vec::new())
        }
        wire::TEL_SUB_SMS_LIST => {
            if let Err(code) = enforce_caller_perm(PERM_SMS_READ) {
                return (code, Vec::new());
            }
            (wire::ERR_UNSUPPORTED, Vec::new())
        }
        wire::TEL_SUB_SMS_SEND => {
            if let Err(code) = enforce_caller_perm(PERM_SMS_SEND) {
                return (code, Vec::new());
            }
            (wire::ERR_UNSUPPORTED, Vec::new())
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

/// Handle `OP_CAMERA_CALL` (enforces `camera` permission + truthful hardware probing).
pub fn handle_camera_call(req: &wire::Request<'_>) -> (i32, Vec<u8>) {
    let cam = PROFILE.hw_ops.has_hw_camera;
    match req.arg0 {
        wire::CAM_SUB_STATUS => {
            let mut out = String::new();
            let _ = writeln!(out, "cam_avail={}", cam as u8);
            let _ = writeln!(out, "mic_avail=0");
            (0, out.into_bytes())
        }
        wire::CAM_SUB_OPEN | wire::CAM_SUB_CAPTURE => {
            if let Err(code) = enforce_caller_perm(PERM_CAMERA) {
                return (code, Vec::new());
            }
            (wire::ERR_UNSUPPORTED, Vec::new())
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

/// Composite kernel-decoded Gallery media preview if owned by the active foreground task.
pub fn blit_media_preview(canvas: &mut Canvas<'_>, fg_slot: usize, top_inset: i32, bottom_inset: i32) {
    let Some(prev) = &rt().preview else { return };
    if prev.owner_slot != fg_slot || prev.w <= 0 || prev.h <= 0 {
        return;
    }
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    let max_y = (ch - bottom_inset).max(top_inset);
    let dst_x = prev.x.clamp(0, cw);
    let dst_y = (top_inset + prev.y).clamp(top_inset, max_y);
    let copy_w = prev.w.min(cw - dst_x).max(0) as usize;
    let copy_h = prev.h.min(max_y - dst_y).max(0) as usize;
    let stride = canvas.width();
    let dst_pixels = canvas.pixels_mut();
    for row in 0..copy_h {
        let s_off = row * prev.w as usize;
        let d_off = (dst_y as usize + row) * stride + dst_x as usize;
        if s_off + copy_w <= prev.pixels.len() && d_off + copy_w <= dst_pixels.len() {
            dst_pixels[d_off..d_off + copy_w].copy_from_slice(&prev.pixels[s_off..s_off + copy_w]);
        }
    }
}

fn wrap_kernel_lines(text: &str, style: &nc_font::Style, max_w: i32, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let max_w = max_w.max(32);
    let mut rest = text.trim();
    while !rest.is_empty() && lines.len() < max_lines {
        if nc_font::text_width(rest, style) <= max_w || lines.len() + 1 == max_lines {
            lines.push(String::from(rest));
            break;
        }
        let mut best_space: Option<usize> = None;
        let mut last_fit = 0usize;
        for (idx, ch) in rest.char_indices() {
            let end = idx + ch.len_utf8();
            if nc_font::text_width(&rest[..end], style) > max_w {
                break;
            }
            last_fit = end;
            if ch == ' ' {
                best_space = Some(idx);
            }
        }
        let split = match best_space {
            Some(sp) if sp > 0 => sp,
            _ => last_fit.max(rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1)),
        };
        let chunk = rest[..split].trim_end();
        if !chunk.is_empty() {
            lines.push(String::from(chunk));
        }
        rest = rest[split..].trim_start();
    }
    lines
}

fn draw_text_alpha(
    canvas: &mut Canvas<'_>,
    text: &str,
    x: i32,
    y: i32,
    style: &nc_font::Style,
    color: u32,
    alpha: u32,
) {
    let a = alpha.min(255);
    if a == 0 {
        return;
    }
    if a >= 255 {
        nc_ui::draw_text_color(canvas, text, x, y, style, color);
        return;
    }
    let mut plot = |px: i32, py: i32, coverage: u8| {
        let cov = ((coverage as u32 * a) / 255) as u8;
        if cov > 0 {
            canvas.blend_pixel(px, py, color, cov);
        }
    };
    nc_font::draw_text(text, x, y, style, &mut plot);
}

struct DialogLayout {
    dp_w: i32,
    pad: i32,
    card: Rect,
    icon_rect: Rect,
    deny_btn: Rect,
    allow_btn: Rect,
    badge_style: nc_font::Style,
    app_style: nc_font::Style,
    title_style: nc_font::Style,
    body_style: nc_font::Style,
    note_style: nc_font::Style,
    btn_style: nc_font::Style,
    app_lines: Vec<String>,
    title_lines: Vec<String>,
    desc_lines: Vec<String>,
    note_lines: Vec<String>,
    line_gap: i32,
    section_gap: i32,
}

fn compute_dialog_layout(
    screen_w: i32,
    screen_h: i32,
    prompt: &PendingPrompt,
    en: bool,
) -> DialogLayout {
    // Density scale from the shorter screen dimension so portrait & landscape both feel balanced
    let min_side = screen_w.min(screen_h).max(200);
    let dp_w = min_side.clamp(240, 440);
    let d = |v: i32| (v as i64 * dp_w as i64 / 360) as i32;

    let safe_h_margin = d(16).max(12);
    let safe_v_margin = d(18).max(12);
    let max_card_h = (screen_h - safe_v_margin * 2).max(160);
    let card_w = d(324)
        .min(screen_w - safe_h_margin * 2)
        .max(216.min(screen_w - 8));

    let badge_cap = d(8).clamp(8, 10);
    let app_cap = d(10).clamp(9, 13);
    let title_cap = d(13).clamp(11, 16);
    let body_cap = d(9).clamp(8, 12);
    let note_cap = d(8).clamp(8, 10);
    let btn_cap = d(10).clamp(9, 13);

    let badge_style = nc_ui::text_style(badge_cap, 12);
    let app_style = nc_ui::text_style(app_cap, 8);
    let title_style = nc_ui::text_style(title_cap, 8);
    let body_style = nc_ui::text_style(body_cap, 8);
    let note_style = nc_ui::text_style(note_cap, 8);
    let btn_style = nc_ui::text_style(btn_cap, 10);

    let info = perm_info(prompt.perm_bit);
    let question = perm_prompt_question(prompt.perm_bit, en);
    let desc = info.map(|i| if en { i.desc_en } else { i.desc_tr }).unwrap_or("");
    let note = if en {
        "You can change this anytime in Settings > App Permissions."
    } else {
        "Bunu istediğiniz zaman Ayarlar > Uygulama İzinleri'nden değiştirebilirsiniz."
    };

    let deny_lbl = if en { "Deny" } else { "Reddet" };
    let allow_lbl = if en { "Allow" } else { "İzin Ver" };

    // First pass: standard spacing; if taller than max_card_h, tighten cleanly to compact layout
    for attempt in 0..2 {
        let compact = attempt == 1;
        let pad = if compact { d(14).max(10) } else { d(20).max(14) };
        let inner_w = (card_w - pad * 2).max(140);
        let icon_s = if compact { d(28).max(22) } else { d(34).max(26) };
        let icon_gap = d(10).max(8);
        let app_text_w = (inner_w - icon_s - icon_gap).max(80);
        let line_gap = if compact { d(4).max(3) } else { d(6).max(4) };
        let section_gap = if compact { d(9).max(6) } else { d(14).max(10) };

        let app_lines = wrap_kernel_lines(&prompt.app_name, &app_style, app_text_w, 2);
        let title_lines = wrap_kernel_lines(question, &title_style, inner_w, 3);
        let desc_lines = wrap_kernel_lines(desc, &body_style, inner_w, 3);
        let note_lines = if compact {
            Vec::new()
        } else {
            wrap_kernel_lines(note, &note_style, inner_w, 2)
        };

        let btn_h = if compact { d(38).max(34) } else { d(44).max(38) };
        let btn_gap = d(10).max(8);
        let side_btn_w = (inner_w - btn_gap) / 2;
        let dw = nc_font::text_width(deny_lbl, &btn_style);
        let aw = nc_font::text_width(allow_lbl, &btn_style);
        let btn_pad_x = d(14).max(10);
        let stacked = side_btn_w < dw.max(aw) + btn_pad_x * 2 || card_w < d(256);
        let buttons_h = if stacked { btn_h * 2 + btn_gap } else { btn_h };

        let app_block_h = icon_s.max(app_lines.len().max(1) as i32 * (app_cap + line_gap) - line_gap);
        let title_h = title_lines.len().max(1) as i32 * (title_cap + line_gap) - line_gap;
        let desc_h = if desc_lines.is_empty() {
            0
        } else {
            desc_lines.len() as i32 * (body_cap + line_gap) - line_gap
        };
        let note_h = if note_lines.is_empty() {
            0
        } else {
            note_lines.len() as i32 * (note_cap + line_gap) - line_gap
        };

        let mut total_h = pad
            + badge_cap
            + section_gap
            + app_block_h
            + section_gap
            + title_h
            + if desc_h > 0 { d(8).max(5) + desc_h } else { 0 }
            + if note_h > 0 { section_gap + note_h } else { 0 }
            + section_gap
            + buttons_h
            + pad;

        if !compact && total_h > max_card_h {
            continue;
        }
        total_h = total_h.min(max_card_h);

        let card_x = (screen_w - card_w) / 2;
        let card_y = (screen_h - total_h) / 2;
        let card = Rect::new(card_x, card_y, card_w, total_h);

        let icon_y = card_y + pad + badge_cap + section_gap;
        let icon_rect = Rect::new(card_x + pad, icon_y, icon_s, icon_s);

        let (deny_btn, allow_btn) = if stacked {
            // Stacked full-width buttons: Allow on top, Deny below (comfortable thumb reach)
            let allow_y = card_y + total_h - pad - btn_h * 2 - btn_gap;
            let deny_y = card_y + total_h - pad - btn_h;
            (
                Rect::new(card_x + pad, deny_y, inner_w, btn_h),
                Rect::new(card_x + pad, allow_y, inner_w, btn_h),
            )
        } else {
            let by = card_y + total_h - pad - btn_h;
            (
                Rect::new(card_x + pad, by, side_btn_w, btn_h),
                Rect::new(card_x + pad + side_btn_w + btn_gap, by, side_btn_w, btn_h),
            )
        };

        return DialogLayout {
            dp_w,
            pad,
            card,
            icon_rect,
            deny_btn,
            allow_btn,
            badge_style,
            app_style,
            title_style,
            body_style,
            note_style,
            btn_style,
            app_lines,
            title_lines,
            desc_lines,
            note_lines,
            line_gap,
            section_gap,
        };
    }

    unreachable!()
}

fn rect_contains(r: Rect, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

pub enum PromptInputResult {
    NotActive,
    Consumed,
    Redraw,
    Decided { caller_slot: usize },
}

/// Handle touch/key events exclusively while the OS-owned permission dialog is visible.
pub fn handle_prompt_input(
    snap: input::Snapshot,
    screen_w: i32,
    screen_h: i32,
    lang: u32,
    reduced_motion: bool,
) -> PromptInputResult {
    let r = rt();
    let Some(prompt_snap) = r.prompt.clone() else {
        return PromptInputResult::NotActive;
    };

    // Never accept a second decision or leak input while animating out!
    if prompt_snap.phase == PromptPhase::Exiting {
        return PromptInputResult::Consumed;
    }

    let en = lang == 1;
    let lay = compute_dialog_layout(screen_w, screen_h, &prompt_snap, en);
    let interactive_touch = prompt_snap.phase == PromptPhase::Visible || prompt_snap.anim_p >= 140;

    // Visual pressed highlight feedback on touch down/move
    let mut redraw = false;
    if interactive_touch {
        let new_pressed = if (snap.pointer_flags & nc_abi::event::PTR_DOWN) != 0 {
            if rect_contains(lay.allow_btn, snap.x, snap.y) {
                2
            } else if rect_contains(lay.deny_btn, snap.x, snap.y) {
                1
            } else {
                0
            }
        } else {
            0
        };
        if let Some(p) = r.prompt.as_mut() {
            if p.pressed_btn != new_pressed {
                p.pressed_btn = new_pressed;
                redraw = true;
            }
        }
    }

    let mut decision: Option<PermState> = None;
    if snap.key == nc_abi::key::ESCAPE {
        decision = Some(PermState::Denied);
    } else if snap.key == nc_abi::key::ENTER {
        decision = Some(PermState::Granted);
    } else if interactive_touch && (snap.pointer_flags & nc_abi::event::PTR_CLICKED) != 0 {
        if rect_contains(lay.allow_btn, snap.x, snap.y) {
            decision = Some(PermState::Granted);
        } else if rect_contains(lay.deny_btn, snap.x, snap.y) {
            decision = Some(PermState::Denied);
        }
    }

    if let Some(new_state) = decision {
        // Apply decision in trusted store first; fail closed if persistence fails on writable /data
        let mut applied = r.store.set_decision(&prompt_snap.app_id, prompt_snap.perm_bit, new_state);
        if applied && storage::data_fs().is_some() && storage::data_writable() {
            if !save_permissions() {
                log!(
                    "[izin] UYARI: permissions.xml kaydedilemedi (fail-closed): app={} perm={:#x}",
                    prompt_snap.app_id,
                    prompt_snap.perm_bit
                );
                if new_state == PermState::Granted {
                    r.store.set_decision(&prompt_snap.app_id, prompt_snap.perm_bit, PermState::Ask);
                    applied = false;
                }
            }
        }
        let _ = applied;
        log!(
            "[izin] kullanici karari: app={} perm={:#x} -> {:?}",
            prompt_snap.app_id,
            prompt_snap.perm_bit,
            new_state
        );

        if reduced_motion {
            r.prompt = None;
            r.prompt_gen = r.prompt_gen.wrapping_add(1);
            return PromptInputResult::Decided { caller_slot: prompt_snap.caller_slot };
        }
        if let Some(p) = r.prompt.as_mut() {
            p.phase = PromptPhase::Exiting;
            p.phase_start_ms = now_ms();
            p.decided_slot = Some(prompt_snap.caller_slot);
            p.pressed_btn = 0;
        }
        return PromptInputResult::Redraw;
    }

    if redraw {
        PromptInputResult::Redraw
    } else {
        PromptInputResult::Consumed
    }
}

/// Draw the native OS-owned, non-forgeable permission prompt dialog directly on the kernel canvas.
pub fn draw_prompt(canvas: &mut Canvas<'_>, dark: bool, accent: u32, lang: u32, reduced_motion: bool) {
    let Some(prompt) = rt().prompt.clone() else { return };
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    let en = lang == 1;
    let lay = compute_dialog_layout(w, h, &prompt, en);
    let d = |v: i32| (v as i64 * lay.dp_w as i64 / 360) as i32;

    let p = if reduced_motion { 256 } else { prompt.anim_p.clamp(0, 256) };
    if p <= 0 {
        return;
    }

    // Restrained full-screen dim scrim
    let scrim_alpha = (164 * p / 256).clamp(1, 200) as u32;
    canvas.fill_rect_alpha(0, 0, w, h, 0x0005_070C, scrim_alpha);

    // Subtle scale (96% -> 100%) + gentle vertical rise (10dp -> 0)
    let scale_pct = if reduced_motion { 100 } else { 96 + (4 * p / 256) };
    let rise_y = if reduced_motion { 0 } else { (256 - p) * d(10) / 256 };
    let anim_w = (lay.card.width * scale_pct / 100).max(160);
    let anim_h = (lay.card.height * scale_pct / 100).max(120);
    let anim_x = lay.card.x + (lay.card.width - anim_w) / 2;
    let anim_y = lay.card.y + (lay.card.height - anim_h) / 2 + rise_y;
    let anim_card = Rect::new(anim_x, anim_y, anim_w, anim_h);

    let dx = anim_x - lay.card.x;
    let dy = anim_y - lay.card.y;

    let (card_bg, rim, rim_a, ink, muted, subtle, sec_btn_bg, sec_btn_press) = if dark {
        (0x0015_1A23, 0x00FF_FFFF, 34u32, 0x00F5_F7FA, 0x009B_A6B8, 0x006F_7B8F, 0x0023_2936, 0x002D_3545)
    } else {
        (0x00FF_FFFF, 0x000F_172A, 28u32, 0x000F_172A, 0x0052_6072, 0x007E_8B9B, 0x00EE_F1F6, 0x00E0_E6EF)
    };

    let card_r = d(22).clamp(14, 26);
    let card_alpha = (252 * p / 256).clamp(24, 255) as u32;
    let content_alpha = (255 * p / 256).clamp(0, 255) as u32;

    // Restrained ambient drop shadow (2 fast panels, zero heavy per-pixel blur)
    let sh1 = Rect::new(anim_card.x + 2, anim_card.y + d(6).max(3), anim_card.width - 4, anim_card.height);
    nc_ui::draw_panel(canvas, sh1, card_r, 0x0002_0408, (28 * p / 256) as u32, 0, 0);
    nc_ui::draw_panel(canvas, anim_card, card_r, card_bg, card_alpha, rim, (rim_a * p as u32) / 256);

    let tx = lay.card.x + lay.pad + dx;
    let mut ty = lay.card.y + lay.pad + dy;

    // 1. Security provenance badge
    let badge_lbl = if en { "NETIZENCORE SECURITY" } else { "NETIZENCORE GÜVENLİK" };
    draw_text_alpha(canvas, badge_lbl, tx, ty, &lay.badge_style, accent, content_alpha);
    ty += lay.badge_style.cap_height + lay.section_gap;

    // 2. Trusted app icon + verified display name
    let icon_r = Rect::new(lay.icon_rect.x + dx, ty, lay.icon_rect.width, lay.icon_rect.height);
    let mut drew_icon = false;
    #[cfg(feature = "ui-mobile")]
    if content_alpha >= 180 {
        let side = icon_r.width.max(1) as u32;
        if let Some(icon) = nc_mobile_assets::icon(prompt.icon_id, side) {
            canvas.draw_argb_premul(icon.pixels, icon.size, icon.size, icon_r);
            drew_icon = true;
        }
    }
    if !drew_icon {
        let ir = icon_r.width * 28 / 100;
        nc_ui::draw_panel(canvas, icon_r, ir, accent, (44 * p / 256) as u32, accent, (120 * p / 256) as u32);
        let init_ch = prompt.app_name.chars().next().unwrap_or('A');
        let mut ib = [0u8; 4];
        let is = init_ch.encode_utf8(&mut ib);
        let iw = nc_font::text_width(is, &lay.app_style);
        draw_text_alpha(
            canvas,
            is,
            icon_r.x + (icon_r.width - iw) / 2,
            icon_r.y + (icon_r.height - lay.app_style.cap_height) / 2,
            &lay.app_style,
            accent,
            content_alpha,
        );
    }

    let app_tx = icon_r.x + icon_r.width + d(10).max(8);
    let app_total_h = lay.app_lines.len().max(1) as i32 * (lay.app_style.cap_height + lay.line_gap) - lay.line_gap;
    let mut app_ty = icon_r.y + ((icon_r.height - app_total_h) / 2).max(0);
    for line in &lay.app_lines {
        draw_text_alpha(canvas, line, app_tx, app_ty, &lay.app_style, ink, content_alpha);
        app_ty += lay.app_style.cap_height + lay.line_gap;
    }
    ty += icon_r.height.max(app_total_h) + lay.section_gap;

    // 3. Clear resource question title
    for line in &lay.title_lines {
        draw_text_alpha(canvas, line, tx, ty, &lay.title_style, ink, content_alpha);
        ty += lay.title_style.cap_height + lay.line_gap;
    }
    ty += d(4).max(2);

    // 4. Concise canonical permission description
    for line in &lay.desc_lines {
        draw_text_alpha(canvas, line, tx, ty, &lay.body_style, muted, content_alpha);
        ty += lay.body_style.cap_height + lay.line_gap;
    }

    // 5. Low-emphasis Settings helper note (when space permits)
    if !lay.note_lines.is_empty() {
        ty += lay.section_gap - lay.line_gap;
        for line in &lay.note_lines {
            draw_text_alpha(canvas, line, tx, ty, &lay.note_style, subtle, content_alpha);
            ty += lay.note_style.cap_height + lay.line_gap;
        }
    }

    // 6. Action buttons (neutral Deny + live OS-accent Allow)
    let deny_r = Rect::new(lay.deny_btn.x + dx, lay.deny_btn.y + dy, lay.deny_btn.width, lay.deny_btn.height);
    let allow_r = Rect::new(lay.allow_btn.x + dx, lay.allow_btn.y + dy, lay.allow_btn.width, lay.allow_btn.height);
    let btn_radius = deny_r.height / 2;

    let deny_fill = if prompt.pressed_btn == 1 { sec_btn_press } else { sec_btn_bg };
    nc_ui::draw_panel(canvas, deny_r, btn_radius, deny_fill, card_alpha, rim, (rim_a * p as u32) / 256);
    let deny_lbl = if en { "Deny" } else { "Reddet" };
    let dw = nc_font::text_width(deny_lbl, &lay.btn_style);
    draw_text_alpha(
        canvas,
        deny_lbl,
        deny_r.x + (deny_r.width - dw) / 2,
        deny_r.y + (deny_r.height - lay.btn_style.cap_height) / 2,
        &lay.btn_style,
        ink,
        content_alpha,
    );

    let allow_fill = if prompt.pressed_btn == 2 {
        nc_gfx::lerp_color(accent, 0x0000_0000, 46)
    } else {
        accent
    };
    nc_ui::draw_panel(canvas, allow_r, btn_radius, allow_fill, card_alpha, 0, 0);
    let allow_lbl = if en { "Allow" } else { "İzin Ver" };
    let aw = nc_font::text_width(allow_lbl, &lay.btn_style);
    draw_text_alpha(
        canvas,
        allow_lbl,
        allow_r.x + (allow_r.width - aw) / 2,
        allow_r.y + (allow_r.height - lay.btn_style.cap_height) / 2,
        &lay.btn_style,
        0x00FF_FFFF,
        content_alpha,
    );
}
