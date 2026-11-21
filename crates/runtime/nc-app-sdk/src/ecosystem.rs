//! NetizenCore Public Ring 3 Ecosystem SDK:
//! `settings`, `apps`, `pkg`, `security`, `storage` modulleri.
//!
//! Tum fonksiyonlar `#![no_std]` uyumludur ve dinamik bellek ayirmaz.

use crate::sys;
use nc_abi::sys as wire;
use nc_abi::vfs as vfs_wire;

fn sys_call_raw<'a>(
    op: u8,
    arg0: u32,
    arg1: u32,
    arg2: u32,
    payload: &[u8],
    resp_buf: &'a mut [u8],
) -> Result<&'a [u8], i32> {
    let mut req = [0u8; 544];
    let Some(n_req) = wire::encode_request(&mut req, op, arg0, arg1, arg2, payload) else {
        return Err(wire::ERR_BAD_REQUEST);
    };
    let n = sys::sys_call(&req[..n_req], resp_buf);
    if n < 0 {
        return Err(wire::ERR_FAILED);
    }
    let n = (n as usize).min(resp_buf.len());
    match wire::parse_response(&resp_buf[..n]) {
        Ok((0, body)) => Ok(body),
        Ok((err, _)) => Err(err),
        Err(_) => Err(wire::ERR_FAILED),
    }
}

fn copy_str(dst: &mut [u8], s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len().min(dst.len());
    dst[..n].copy_from_slice(&b[..n]);
    n
}

fn parse_u32(s: &str) -> u32 {
    s.trim().parse::<u32>().unwrap_or(0)
}

fn parse_u64(s: &str) -> u64 {
    s.trim().parse::<u64>().unwrap_or(0)
}

fn parse_hex_u32(s: &str) -> u32 {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap_or(0)
}

pub mod settings {
    use super::*;

    /// Sistem donanim, depolama ve ayar bilgilerini `key=value` metni olarak okur.
    pub fn device_info<'a>(resp_buf: &'a mut [u8]) -> Result<&'a str, i32> {
        let body = sys_call_raw(wire::OP_DEVICE_INFO, 0, 0, 0, &[], resp_buf)?;
        core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)
    }

    /// Mevcut uygulamanin kendi kimlik ve yetki bilgisini okur.
    pub fn self_info<'a>(resp_buf: &'a mut [u8]) -> Result<&'a str, i32> {
        let body = sys_call_raw(wire::OP_SELF_INFO, 0, 0, 0, &[], resp_buf)?;
        core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)
    }

    /// Sayisal bir sistem ayarini (`nc_abi::sys::MOBILE_SET_*`) gunceller ve kalici yazar.
    pub fn set_u32(sub_op: u32, val: u32) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_MOBILE_SET, sub_op, val, 0, &[], &mut resp).map(|_| ())
    }

    /// Metin degerli bir sistem ayarini (or. cihaz adi, duvar kagidi resmi) gunceller.
    pub fn set_str(sub_op: u32, text: &str) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_MOBILE_SET, sub_op, 0, 0, text.as_bytes(), &mut resp).map(|_| ())
    }

    /// `/data` depolama tamponlarini diske esitler.
    pub fn sync_data() -> Result<(), i32> {
        set_u32(wire::MOBILE_SYNC_DATA, 0)
    }
}

/// Sistem geneli moduler Ring 3 sanal klavye (IME) istemci ve saglayici arayuzu.
pub mod ime {
    use super::*;

    /// Herhangi bir uygulama metin girisi icin aktif sistem klavyesini acar.
    pub fn show() -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_IME_CMD, wire::IME_SHOW, 0, 0, &[], &mut resp).map(|_| ())
    }

    /// Aktif sistem klavyesini gizler.
    pub fn hide() -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_IME_CMD, wire::IME_HIDE, 0, 0, &[], &mut resp).map(|_| ())
    }

    /// Yalnizca aktif Ring 3 IME uygulamasi tarafindan cagirilir: tusu hedef uygulamaya iletir.
    pub fn commit_key(key_code: u32) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_IME_CMD, wire::IME_COMMIT_KEY, key_code, 0, &[], &mut resp).map(|_| ())
    }

    /// Aktif moduler IME paketini degistirir (or. `"nc.keyboard"` veya baska bir IME paketi).
    pub fn set_active(ime_app_id: &str) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_IME_CMD, wire::IME_SET_ACTIVE, 0, 0, ime_app_id.as_bytes(), &mut resp)
            .map(|_| ())
    }
}

pub mod apps {
    use super::*;

    pub const STATE_STOPPED: u8 = 0;
    pub const STATE_PAUSED: u8 = 1;
    pub const STATE_FG: u8 = 2;

    #[derive(Clone, Copy)]
    pub struct AppEntry {
        pub id: [u8; 36],
        pub id_len: u8,
        pub name: [u8; 36],
        pub name_len: u8,
        pub version: [u8; 16],
        pub version_len: u8,
        pub icon_id: u32,
        pub is_system: bool,
        pub from_userdata: bool,
        pub state: u8,
        pub surface_id: u32,
        pub pages_kib: u32,
        pub requested_caps: u32,
        pub granted_caps: u32,
        pub verified: bool,
        pub size_bytes: u32,
    }

    impl AppEntry {
        pub const EMPTY: AppEntry = AppEntry {
            id: [0; 36],
            id_len: 0,
            name: [0; 36],
            name_len: 0,
            version: [0; 16],
            version_len: 0,
            icon_id: nc_abi::draw::ICON_GENERIC,
            is_system: false,
            from_userdata: false,
            state: STATE_STOPPED,
            surface_id: 0,
            pages_kib: 0,
            requested_caps: 0,
            granted_caps: 0,
            verified: false,
            size_bytes: 0,
        };

        pub fn id_str(&self) -> &str {
            core::str::from_utf8(&self.id[..self.id_len as usize]).unwrap_or("")
        }

        pub fn name_str(&self) -> &str {
            core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
        }

        pub fn version_str(&self) -> &str {
            core::str::from_utf8(&self.version[..self.version_len as usize]).unwrap_or("")
        }
    }

    /// Cekirdekteki dinamik uygulama kayit defterini sorgular.
    /// Donus: `(registry_gen, fg_surface_id, entry_count)`.
    pub fn query_registry(resp_buf: &mut [u8], out: &mut [AppEntry]) -> Result<(u32, u8, usize), i32> {
        let body = sys_call_raw(wire::OP_APP_REGISTRY, 0, 0, 0, &[], resp_buf)?;
        let text = core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)?;
        let mut gen = 0u32;
        let mut fg_surf = 0u8;
        let mut count = 0usize;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("gen=") {
                gen = parse_u32(v);
            } else if let Some(v) = line.strip_prefix("fg_surface=") {
                fg_surf = parse_u32(v) as u8;
            } else if let Some(v) = line.strip_prefix("pkg=") {
                if count >= out.len() {
                    continue;
                }
                let mut parts = v.split('|');
                let id = parts.next().unwrap_or("");
                let name = parts.next().unwrap_or("");
                let version = parts.next().unwrap_or("1.0.0");
                let icon_id = parse_u32(parts.next().unwrap_or("6"));
                let tier = parts.next().unwrap_or("user");
                let src = parts.next().unwrap_or("userdata");
                let st = parts.next().unwrap_or("stopped");
                let surf = parse_u32(parts.next().unwrap_or("0"));
                let kib = parse_u32(parts.next().unwrap_or("0"));
                let req_c = parse_hex_u32(parts.next().unwrap_or("0"));
                let grant_c = parse_hex_u32(parts.next().unwrap_or("0"));
                let ver = parse_u32(parts.next().unwrap_or("0")) != 0;
                let sz = parse_u32(parts.next().unwrap_or("0"));

                let mut e = AppEntry::EMPTY;
                e.id_len = copy_str(&mut e.id, id) as u8;
                e.name_len = copy_str(&mut e.name, name) as u8;
                e.version_len = copy_str(&mut e.version, version) as u8;
                e.icon_id = if icon_id == 0 { nc_abi::draw::ICON_GENERIC } else { icon_id };
                e.is_system = tier == "system";
                e.from_userdata = src == "userdata";
                e.state = match st {
                    "fg" => STATE_FG,
                    "paused" => STATE_PAUSED,
                    _ => STATE_STOPPED,
                };
                e.surface_id = surf;
                e.pages_kib = kib;
                e.requested_caps = req_c;
                e.granted_caps = grant_c;
                e.verified = ver;
                e.size_bytes = sz;
                out[count] = e;
                count += 1;
            }
        }
        Ok((gen, fg_surf, count))
    }

    pub fn launch(app_id: &str) -> i64 {
        sys::app_launch(app_id)
    }

    pub fn stop(app_id: &str) -> i64 {
        sys::app_stop(app_id)
    }

    pub fn uninstall(app_id: &str, remove_data: bool) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_PKG_UNINSTALL, remove_data as u32, 0, 0, app_id.as_bytes(), &mut resp).map(|_| ())
    }

    pub fn clear_data(app_id: &str) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_APP_CLEAR_DATA, 0, 0, 0, app_id.as_bytes(), &mut resp).map(|_| ())
    }

    /// Hedef uygulamaya cekirdek aracili niyet (intent) gonderir (or. `.ntzc` dosya yolu).
    pub fn send_intent(target_app_id: &str, data: &str) -> Result<(), i32> {
        let mut buf = [0u8; 290];
        let t = target_app_id.as_bytes();
        let d = data.as_bytes();
        if t.len() + 1 + d.len() > buf.len() {
            return Err(wire::ERR_BAD_REQUEST);
        }
        buf[..t.len()].copy_from_slice(t);
        buf[t.len()] = 0;
        buf[t.len() + 1..t.len() + 1 + d.len()].copy_from_slice(d);
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_PENDING_INTENT, 1, 0, 0, &buf[..t.len() + 1 + d.len()], &mut resp).map(|_| ())
    }

    /// Mevcut uygulamaya gonderilmis bekleyen niyeti okur ve temizler.
    pub fn take_intent<'a>(resp_buf: &'a mut [u8]) -> Option<&'a str> {
        let body = sys_call_raw(wire::OP_PENDING_INTENT, 0, 0, 0, &[], resp_buf).ok()?;
        if body.is_empty() {
            return None;
        }
        core::str::from_utf8(body).ok()
    }
}

pub mod pkg {
    use super::*;

    /// Verilen yoldaki `.ntzc` paketini cekirdekte denetler ve yetkili meta verisini dondurur.
    pub fn inspect<'a>(path: &str, resp_buf: &'a mut [u8]) -> Result<&'a str, i32> {
        let body = sys_call_raw(wire::OP_PKG_INSPECT, 0, 0, 0, path.as_bytes(), resp_buf)?;
        core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)
    }

    /// Yetkili uygulama tarafindan paketi kurar (`PACKAGE_INSTALL` gerekir).
    pub fn install(path: &str) -> i64 {
        sys::pkg_install(path)
    }

    pub const fn target_arch() -> &'static str {
        #[cfg(target_arch = "aarch64")]
        { "aarch64-unknown-none" }
        #[cfg(target_arch = "x86_64")]
        { "x86_64-unknown-none" }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        { "host" }
    }

    pub const fn abi_version() -> u32 {
        nc_abi::ABI_VERSION
    }
}

pub mod security {
    use super::*;

    #[derive(Clone, Copy, Default)]
    pub struct LockStatus {
        pub locked: bool,
        pub enabled: bool,
        pub kind: u8,
        pub fails: u32,
        pub lockout_s: u32,
        pub timeout_s: u32,
        pub lock_on_boot: bool,
        pub show_info: bool,
    }

    pub fn lock_status() -> Option<LockStatus> {
        let mut resp = [0u8; 256];
        let body = sys_call_raw(wire::OP_LOCK_STATUS, 0, 0, 0, &[], &mut resp).ok()?;
        let text = core::str::from_utf8(body).ok()?;
        let mut st = LockStatus::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "locked" => st.locked = parse_u32(v) != 0,
                "enabled" => st.enabled = parse_u32(v) != 0,
                "kind" => st.kind = parse_u32(v) as u8,
                "fails" => st.fails = parse_u32(v),
                "lockout_s" => st.lockout_s = parse_u32(v),
                "timeout_s" => st.timeout_s = parse_u32(v),
                "lock_on_boot" => st.lock_on_boot = parse_u32(v) != 0,
                "show_info" => st.show_info = parse_u32(v) != 0,
                _ => {}
            }
        }
        Some(st)
    }

    /// Ekran kilidini dogrular. Basarisizsa `(fails, lockout_s)` dondurur.
    pub fn authenticate(secret: &[u8]) -> Result<(), (u32, u32)> {
        let mut resp = [0u8; 128];
        let mut req = [0u8; 96];
        let Some(n_req) = wire::encode_request(&mut req, wire::OP_LOCK_AUTH, 0, 0, 0, secret) else {
            return Err((0, 0));
        };
        let n = sys::sys_call(&req[..n_req], &mut resp);
        if n < 0 {
            return Err((0, 0));
        }
        let n = (n as usize).min(resp.len());
        match wire::parse_response(&resp[..n]) {
            Ok((0, _)) => Ok(()),
            Ok((_, body)) => {
                let mut fails = 0u32;
                let mut lockout = 0u32;
                if let Ok(text) = core::str::from_utf8(body) {
                    for line in text.lines() {
                        if let Some(v) = line.strip_prefix("fails=") {
                            fails = parse_u32(v);
                        } else if let Some(v) = line.strip_prefix("lockout_s=") {
                            lockout = parse_u32(v);
                        }
                    }
                }
                Err((fails, lockout))
            }
            Err(_) => Err((0, 0)),
        }
    }

    pub fn lock_now() -> Result<(), i32> {
        let mut resp = [0u8; 64];
        sys_call_raw(wire::OP_LOCK_CONFIG, wire::LOCK_CFG_LOCK_NOW, 0, 0, &[], &mut resp).map(|_| ())
    }

    pub fn configure_set(kind: u8, current_secret: &[u8], new_secret: &[u8]) -> Result<(), i32> {
        let mut payload = [0u8; 128];
        if current_secret.len() + 1 + new_secret.len() > payload.len() {
            return Err(wire::ERR_BAD_REQUEST);
        }
        payload[..current_secret.len()].copy_from_slice(current_secret);
        payload[current_secret.len()] = 0;
        payload[current_secret.len() + 1..current_secret.len() + 1 + new_secret.len()].copy_from_slice(new_secret);
        let len = current_secret.len() + 1 + new_secret.len();
        let mut resp = [0u8; 128];
        sys_call_raw(wire::OP_LOCK_CONFIG, wire::LOCK_CFG_SET, kind as u32, 0, &payload[..len], &mut resp).map(|_| ())
    }

    pub fn configure_disable(current_secret: &[u8]) -> Result<(), i32> {
        let mut resp = [0u8; 128];
        sys_call_raw(wire::OP_LOCK_CONFIG, wire::LOCK_CFG_DISABLE, 0, 0, current_secret, &mut resp).map(|_| ())
    }
}

pub mod storage {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct UsbStatus {
        pub mounted: bool,
        pub disk_index: i32,
        pub total_kib: u64,
        pub read_only: bool,
        pub fs_type: [u8; 16],
        pub fs_type_len: u8,
        pub label: [u8; 32],
        pub label_len: u8,
    }

    impl UsbStatus {
        pub const EMPTY: UsbStatus = UsbStatus {
            mounted: false,
            disk_index: -1,
            total_kib: 0,
            read_only: true,
            fs_type: [0; 16],
            fs_type_len: 0,
            label: [0; 32],
            label_len: 0,
        };

        pub fn fs_type_str(&self) -> &str {
            core::str::from_utf8(&self.fs_type[..self.fs_type_len as usize]).unwrap_or("none")
        }

        pub fn label_str(&self) -> &str {
            core::str::from_utf8(&self.label[..self.label_len as usize]).unwrap_or("")
        }
    }

    pub fn usb_info() -> Option<UsbStatus> {
        let mut resp = [0u8; 256];
        let body = sys_call_raw(wire::OP_USB_INFO, 0, 0, 0, &[], &mut resp).ok()?;
        let text = core::str::from_utf8(body).ok()?;
        let mut st = UsbStatus::EMPTY;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "mounted" => st.mounted = parse_u32(v) != 0,
                "disk_index" => st.disk_index = v.trim().parse::<i32>().unwrap_or(-1),
                "total_kib" => st.total_kib = parse_u64(v),
                "read_only" => st.read_only = parse_u32(v) != 0,
                "fs_type" => st.fs_type_len = copy_str(&mut st.fs_type, v) as u8,
                "label" => st.label_len = copy_str(&mut st.label, v) as u8,
                _ => {}
            }
        }
        Some(st)
    }

    pub fn usb_rescan() -> bool {
        let mut req = [0u8; 64];
        let mut resp = [0u8; 32];
        let Some(n_req) = vfs_wire::encode_request(&mut req, vfs_wire::OP_USB_RESCAN, 0, 0, "", &[]) else {
            return false;
        };
        let n = sys::vfs_call(&req[..n_req], &mut resp);
        if n < 0 {
            return false;
        }
        matches!(vfs_wire::parse_response(&resp[..(n as usize).min(resp.len())]), Ok((0, [1, ..])))
    }

    pub fn usb_eject() -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_USB_EJECT, 0, 0, 0, &[], &mut resp).map(|_| ())
    }

    pub fn mkdir(path: &str) -> bool {
        let mut req = [0u8; 320];
        let mut resp = [0u8; 32];
        let Some(n_req) = vfs_wire::encode_request(&mut req, vfs_wire::OP_MKDIR, 0, 0, path, &[]) else {
            return false;
        };
        let n = sys::vfs_call(&req[..n_req], &mut resp);
        if n < 0 {
            return false;
        }
        matches!(vfs_wire::parse_response(&resp[..(n as usize).min(resp.len())]), Ok((0 | vfs_wire::ERR_ALREADY_EXISTS, _)))
    }

    /// Reads up to `out.len()` bytes from `path` via VFS. Returns byte count on success.
    pub fn read_file(path: &str, out: &mut [u8]) -> Option<usize> {
        let mut req = [0u8; 320];
        let Some(n_req) = vfs_wire::encode_request(&mut req, vfs_wire::OP_READ_RANGE, 0, out.len() as u64, path, &[]) else {
            return None;
        };
        let mut resp = [0u8; 4096 + vfs_wire::RESP_HEADER];
        let cap = (out.len() + vfs_wire::RESP_HEADER).min(resp.len());
        let n = sys::vfs_call(&req[..n_req], &mut resp[..cap]);
        if n < vfs_wire::RESP_HEADER as i64 {
            return None;
        }
        match vfs_wire::parse_response(&resp[..(n as usize).min(cap)]) {
            Ok((0, body)) => {
                let len = body.len().min(out.len());
                out[..len].copy_from_slice(&body[..len]);
                Some(len)
            }
            _ => None,
        }
    }

    /// Atomically writes `data` (up to 4096 bytes) to `path` via VFS.
    pub fn write_file(path: &str, data: &[u8]) -> bool {
        if data.len() > 4096 {
            return false;
        }
        let mut req = [0u8; vfs_wire::REQ_HEADER + vfs_wire::MAX_PATH + 4096];
        let Some(n_req) = vfs_wire::encode_request(&mut req, vfs_wire::OP_WRITE, 0, data.len() as u64, path, data) else {
            return false;
        };
        let mut resp = [0u8; 32];
        let n = sys::vfs_call(&req[..n_req], &mut resp);
        if n < vfs_wire::RESP_HEADER as i64 {
            return false;
        }
        matches!(vfs_wire::parse_response(&resp[..(n as usize).min(resp.len())]), Ok((0, _)))
    }
}

/// Real hardware & system status queried via `OP_SYSTEM_STATUS`.
pub mod status {
    use super::*;

    pub const CONN_UNAVAILABLE: u8 = 0;
    pub const CONN_DISABLED: u8 = 1;
    pub const CONN_DISCONNECTED: u8 = 2;
    pub const CONN_CONNECTING: u8 = 3;
    pub const CONN_CONNECTED: u8 = 4;
    pub const CONN_LIMITED: u8 = 5;

    #[derive(Clone, Copy)]
    pub struct SystemStatus {
        pub bat_available: bool,
        pub bat_percent: u8,
        pub bat_charging: bool,
        pub bat_full: bool,
        pub bat_voltage_mv: u16,
        pub bat_temp_deci_c: i16,
        pub wifi_hw: bool,
        pub wifi_enabled: bool,
        pub wifi_state: u8,
        pub wifi_rssi: u8,
        pub net_mac: [u8; 18],
        pub net_mac_len: u8,
        pub bt_hw: bool,
        pub bt_enabled: bool,
        pub bt_state: u8,
        pub bt_connected: u8,
        pub cell_hw: bool,
        pub cell_sim: bool,
        pub cell_state: u8,
        pub cell_bars: u8,
        pub cell_data: bool,
        pub audio_hw: bool,
        pub vol_pct: u8,
        pub bright_pct: u8,
        pub bright_hw: bool,
        pub dark_theme: bool,
        pub anim_scale: u8,
        pub dnd: bool,
        pub airplane: bool,
        pub ram_total_mib: u32,
        pub ram_free_mib: u32,
        pub data_total_mib: u32,
        pub data_used_kib: u32,
        pub uptime_s: u32,
        pub live_tasks: u32,
        pub usb_mounted: bool,
        pub device_model: [u8; 32],
        pub device_model_len: u8,
    }

    impl SystemStatus {
        pub const EMPTY: SystemStatus = SystemStatus {
            bat_available: false,
            bat_percent: 0,
            bat_charging: false,
            bat_full: false,
            bat_voltage_mv: 0,
            bat_temp_deci_c: i16::MIN,
            wifi_hw: false,
            wifi_enabled: false,
            wifi_state: CONN_UNAVAILABLE,
            wifi_rssi: 0,
            net_mac: [0; 18],
            net_mac_len: 0,
            bt_hw: false,
            bt_enabled: false,
            bt_state: CONN_UNAVAILABLE,
            bt_connected: 0,
            cell_hw: false,
            cell_sim: false,
            cell_state: CONN_UNAVAILABLE,
            cell_bars: 0,
            cell_data: false,
            audio_hw: false,
            vol_pct: 0,
            bright_pct: 100,
            bright_hw: false,
            dark_theme: false,
            anim_scale: 100,
            dnd: false,
            airplane: false,
            ram_total_mib: 0,
            ram_free_mib: 0,
            data_total_mib: 0,
            data_used_kib: 0,
            uptime_s: 0,
            live_tasks: 0,
            usb_mounted: false,
            device_model: [0; 32],
            device_model_len: 0,
        };

        pub fn mac_str(&self) -> &str {
            core::str::from_utf8(&self.net_mac[..self.net_mac_len as usize]).unwrap_or("")
        }

        pub fn model_str(&self) -> &str {
            core::str::from_utf8(&self.device_model[..self.device_model_len as usize]).unwrap_or("NetizenCore")
        }

        pub fn reduced_motion(&self) -> bool {
            self.anim_scale == 0
        }
    }

    pub fn query() -> Option<SystemStatus> {
        let mut resp = [0u8; 1024];
        let body = sys_call_raw(wire::OP_SYSTEM_STATUS, 0, 0, 0, &[], &mut resp).ok()?;
        let text = core::str::from_utf8(body).ok()?;
        let mut st = SystemStatus::EMPTY;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "bat_avail" => st.bat_available = parse_u32(v) != 0,
                "bat_pct" => st.bat_percent = parse_u32(v).min(100) as u8,
                "bat_chg" => st.bat_charging = parse_u32(v) != 0,
                "bat_full" => st.bat_full = parse_u32(v) != 0,
                "bat_mv" => st.bat_voltage_mv = parse_u32(v).min(65535) as u16,
                "bat_temp" => st.bat_temp_deci_c = v.trim().parse::<i16>().unwrap_or(i16::MIN),
                "wifi_hw" => st.wifi_hw = parse_u32(v) != 0,
                "wifi_en" => st.wifi_enabled = parse_u32(v) != 0,
                "wifi_state" => st.wifi_state = parse_u32(v) as u8,
                "wifi_rssi" => st.wifi_rssi = parse_u32(v) as u8,
                "net_mac" => st.net_mac_len = copy_str(&mut st.net_mac, v) as u8,
                "bt_hw" => st.bt_hw = parse_u32(v) != 0,
                "bt_en" => st.bt_enabled = parse_u32(v) != 0,
                "bt_state" => st.bt_state = parse_u32(v) as u8,
                "bt_conn" => st.bt_connected = parse_u32(v) as u8,
                "cell_hw" => st.cell_hw = parse_u32(v) != 0,
                "cell_sim" => st.cell_sim = parse_u32(v) != 0,
                "cell_state" => st.cell_state = parse_u32(v) as u8,
                "cell_bars" => st.cell_bars = parse_u32(v) as u8,
                "cell_data" => st.cell_data = parse_u32(v) != 0,
                "audio_hw" => st.audio_hw = parse_u32(v) != 0,
                "vol_pct" => st.vol_pct = parse_u32(v).min(100) as u8,
                "bright_pct" => st.bright_pct = parse_u32(v).clamp(20, 100) as u8,
                "bright_hw" => st.bright_hw = parse_u32(v) != 0,
                "dark_theme" => st.dark_theme = parse_u32(v) != 0,
                "anim_scale" => st.anim_scale = parse_u32(v).min(200) as u8,
                "dnd" => st.dnd = parse_u32(v) != 0,
                "airplane" => st.airplane = parse_u32(v) != 0,
                "ram_mib" => st.ram_total_mib = parse_u32(v),
                "free_mib" => st.ram_free_mib = parse_u32(v),
                "data_mib" => st.data_total_mib = parse_u32(v),
                "data_used_kib" => st.data_used_kib = parse_u32(v),
                "uptime_s" => st.uptime_s = parse_u32(v),
                "live_tasks" => st.live_tasks = parse_u32(v),
                "usb_mounted" => st.usb_mounted = parse_u32(v) != 0,
                "device_model" => st.device_model_len = copy_str(&mut st.device_model, v) as u8,
                _ => {}
            }
        }
        Some(st)
    }
}

/// Kernel-enforced runtime permission SDK (`nc_abi::perm`).
/// Works identically for built-in system apps and third-party SDK apps.
pub mod perm {
    use super::*;
    pub use nc_abi::perm::{
        PermRequestOutcome, PermState, PERM_CALL_LOG_READ, PERM_CAMERA, PERM_CONTACTS_READ,
        PERM_CONTACTS_WRITE, PERM_MEDIA_PHOTOS_READ, PERM_MICROPHONE_RECORD, PERM_PHONE_CALL,
        PERM_REGISTRY, PERM_SMS_READ, PERM_SMS_SEND,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PermStatus {
        pub state: PermState,
        pub hw_available: bool,
        pub pending: bool,
    }

    #[derive(Clone, Copy)]
    pub struct AppPermSummary {
        pub app_id: [u8; 36],
        pub app_id_len: u8,
        pub declared_mask: u32,
        pub granted_mask: u32,
        pub denied_mask: u32,
        pub unsupported_mask: u32,
    }

    impl AppPermSummary {
        pub const EMPTY: AppPermSummary = AppPermSummary {
            app_id: [0; 36],
            app_id_len: 0,
            declared_mask: 0,
            granted_mask: 0,
            denied_mask: 0,
            unsupported_mask: 0,
        };

        pub fn id_str(&self) -> &str {
            core::str::from_utf8(&self.app_id[..self.app_id_len as usize]).unwrap_or("")
        }
    }

    /// Check the caller's own runtime permission state (kernel authenticates caller identity).
    pub fn check(perm_bit: u32) -> PermStatus {
        let mut resp = [0u8; 32];
        match sys_call_raw(wire::OP_PERM_CALL, wire::PERM_SUB_CHECK, perm_bit, 0, &[], &mut resp) {
            Ok(body) if body.len() >= 3 => PermStatus {
                state: PermState::from_u8(body[0]),
                hw_available: body[1] != 0,
                pending: body[2] != 0,
            },
            _ => PermStatus { state: PermState::Denied, hw_available: false, pending: false },
        }
    }

    /// Request a declared runtime permission via the non-blocking OS-owned dialog.
    /// Repeated calls while already pending will NOT spawn duplicate dialogs.
    pub fn request(perm_bit: u32) -> PermRequestOutcome {
        let mut resp = [0u8; 32];
        match sys_call_raw(wire::OP_PERM_CALL, wire::PERM_SUB_REQUEST, perm_bit, 0, &[], &mut resp) {
            Ok(body) if !body.is_empty() => PermRequestOutcome::from_u8(body[0]),
            Err(wire::ERR_PERM_NOT_DECLARED) => PermRequestOutcome::NotDeclared,
            Err(wire::ERR_UNSUPPORTED) => PermRequestOutcome::Unsupported,
            _ => PermRequestOutcome::Denied,
        }
    }

    /// Open Settings -> App Permissions so the user can change a denied permission.
    pub fn open_settings() -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_PERM_CALL, wire::PERM_SUB_OPEN_SETTINGS, 0, 0, &[], &mut resp).map(|_| ())
    }

    /// Privileged (`SECURITY_MANAGE`): list all installed apps and their runtime permission masks.
    pub fn list_all(resp_buf: &mut [u8], out: &mut [AppPermSummary]) -> Result<usize, i32> {
        let body = sys_call_raw(wire::OP_PERM_CALL, wire::PERM_SUB_LIST_ALL, 0, 0, &[], resp_buf)?;
        let text = core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)?;
        let mut count = 0usize;
        for line in text.lines() {
            let Some(v) = line.strip_prefix("app_perm=") else { continue };
            if count >= out.len() {
                break;
            }
            let mut parts = v.split('|');
            let id = parts.next().unwrap_or("");
            let declared = parse_hex_u32(parts.next().unwrap_or("0"));
            let granted = parse_hex_u32(parts.next().unwrap_or("0"));
            let denied = parse_hex_u32(parts.next().unwrap_or("0"));
            let unsup = parse_hex_u32(parts.next().unwrap_or("0"));
            let mut item = AppPermSummary::EMPTY;
            item.app_id_len = copy_str(&mut item.app_id, id) as u8;
            item.declared_mask = declared;
            item.granted_mask = granted;
            item.denied_mask = denied;
            item.unsupported_mask = unsup;
            out[count] = item;
            count += 1;
        }
        Ok(count)
    }

    /// Privileged (`SECURITY_MANAGE`): update an application's runtime permission state immediately.
    pub fn set_app_perm(app_id: &str, perm_bit: u32, state: PermState) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(
            wire::OP_PERM_CALL,
            wire::PERM_SUB_SET,
            perm_bit,
            state as u32,
            app_id.as_bytes(),
            &mut resp,
        )
        .map(|_| ())
    }
}

/// System-managed Contacts service SDK (enforces `contacts.read` / `contacts.write`).
pub mod contacts {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct ContactEntry {
        pub id: u32,
        pub name: [u8; 48],
        pub name_len: u8,
        pub phone: [u8; 28],
        pub phone_len: u8,
        pub email: [u8; 48],
        pub email_len: u8,
        pub note: [u8; 64],
        pub note_len: u8,
    }

    impl ContactEntry {
        pub const EMPTY: ContactEntry = ContactEntry {
            id: 0,
            name: [0; 48],
            name_len: 0,
            phone: [0; 28],
            phone_len: 0,
            email: [0; 48],
            email_len: 0,
            note: [0; 64],
            note_len: 0,
        };

        pub fn name_str(&self) -> &str {
            core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
        }
        pub fn phone_str(&self) -> &str {
            core::str::from_utf8(&self.phone[..self.phone_len as usize]).unwrap_or("")
        }
        pub fn email_str(&self) -> &str {
            core::str::from_utf8(&self.email[..self.email_len as usize]).unwrap_or("")
        }
        pub fn note_str(&self) -> &str {
            core::str::from_utf8(&self.note[..self.note_len as usize]).unwrap_or("")
        }
    }

    fn parse_contacts_body(body: &[u8], out: &mut [ContactEntry]) -> Result<usize, i32> {
        let text = core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)?;
        let mut count = 0usize;
        for line in text.lines() {
            let Some(v) = line.strip_prefix("contact=") else { continue };
            if count >= out.len() {
                break;
            }
            let mut parts = v.split('|');
            let id = parse_u32(parts.next().unwrap_or("0"));
            let name = parts.next().unwrap_or("");
            let phone = parts.next().unwrap_or("");
            let email = parts.next().unwrap_or("");
            let note = parts.next().unwrap_or("");
            let mut entry = ContactEntry::EMPTY;
            entry.id = id;
            entry.name_len = copy_str(&mut entry.name, name) as u8;
            entry.phone_len = copy_str(&mut entry.phone, phone) as u8;
            entry.email_len = copy_str(&mut entry.email, email) as u8;
            entry.note_len = copy_str(&mut entry.note, note) as u8;
            out[count] = entry;
            count += 1;
        }
        Ok(count)
    }

    pub fn list(resp_buf: &mut [u8], out: &mut [ContactEntry]) -> Result<usize, i32> {
        let body = sys_call_raw(wire::OP_CONTACTS_CALL, wire::CONTACTS_SUB_LIST, 0, 0, &[], resp_buf)?;
        parse_contacts_body(body, out)
    }

    pub fn lookup(query: &str, resp_buf: &mut [u8], out: &mut [ContactEntry]) -> Result<usize, i32> {
        let body = sys_call_raw(
            wire::OP_CONTACTS_CALL,
            wire::CONTACTS_SUB_LOOKUP,
            0,
            0,
            query.as_bytes(),
            resp_buf,
        )?;
        parse_contacts_body(body, out)
    }

    pub fn upsert(id: u32, name: &str, phone: &str, email: &str, note: &str) -> Result<u32, i32> {
        let mut payload = [0u8; 256];
        let mut pos = 0usize;
        for (idx, field) in [name, phone, email, note].iter().enumerate() {
            let b = field.as_bytes();
            let extra = if idx < 3 { 1 } else { 0 };
            if pos + b.len() + extra > payload.len() {
                return Err(wire::ERR_BAD_REQUEST);
            }
            payload[pos..pos + b.len()].copy_from_slice(b);
            pos += b.len();
            if idx < 3 {
                payload[pos] = 0;
                pos += 1;
            }
        }
        let mut resp = [0u8; 64];
        let body = sys_call_raw(
            wire::OP_CONTACTS_CALL,
            wire::CONTACTS_SUB_UPSERT,
            id,
            0,
            &payload[..pos],
            &mut resp,
        )?;
        let text = core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)?;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("id=") {
                return Ok(parse_u32(v));
            }
        }
        Ok(id)
    }

    pub fn delete(id: u32) -> Result<(), i32> {
        let mut resp = [0u8; 32];
        sys_call_raw(wire::OP_CONTACTS_CALL, wire::CONTACTS_SUB_DELETE, id, 0, &[], &mut resp).map(|_| ())
    }
}

/// Scoped Media service SDK (enforces `media.photos.read`; no broad filesystem caps needed).
pub mod media {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct MediaPhoto {
        pub name: [u8; 44],
        pub name_len: u8,
        pub size_bytes: u32,
        pub width: u32,
        pub height: u32,
        pub format: [u8; 8],
        pub format_len: u8,
    }

    impl MediaPhoto {
        pub const EMPTY: MediaPhoto = MediaPhoto {
            name: [0; 44],
            name_len: 0,
            size_bytes: 0,
            width: 0,
            height: 0,
            format: [0; 8],
            format_len: 0,
        };

        pub fn name_str(&self) -> &str {
            core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
        }
        pub fn format_str(&self) -> &str {
            core::str::from_utf8(&self.format[..self.format_len as usize]).unwrap_or("")
        }
    }

    pub fn list_photos(resp_buf: &mut [u8], out: &mut [MediaPhoto]) -> Result<usize, i32> {
        let body = sys_call_raw(wire::OP_MEDIA_CALL, wire::MEDIA_SUB_LIST, 0, 0, &[], resp_buf)?;
        let text = core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)?;
        let mut count = 0usize;
        for line in text.lines() {
            let Some(v) = line.strip_prefix("photo=") else { continue };
            if count >= out.len() {
                break;
            }
            let mut parts = v.split('|');
            let mut item = MediaPhoto::EMPTY;
            item.name_len = copy_str(&mut item.name, parts.next().unwrap_or("")) as u8;
            item.size_bytes = parse_u32(parts.next().unwrap_or("0"));
            item.width = parse_u32(parts.next().unwrap_or("0"));
            item.height = parse_u32(parts.next().unwrap_or("0"));
            item.format_len = copy_str(&mut item.format, parts.next().unwrap_or("IMG")) as u8;
            out[count] = item;
            count += 1;
        }
        Ok(count)
    }

    /// Fetch a small downsampled RGB thumbnail (`[w, h, rgb...]`) decoded by the kernel media service.
    pub fn thumbnail(name: &str, resp_buf: &mut [u8]) -> Result<(u8, u8, usize), i32> {
        let body = sys_call_raw(
            wire::OP_MEDIA_CALL,
            wire::MEDIA_SUB_THUMB,
            0,
            0,
            name.as_bytes(),
            resp_buf,
        )?;
        if body.len() < 2 {
            return Err(wire::ERR_FAILED);
        }
        let w = body[0];
        let h = body[1];
        let need = (w as usize) * (h as usize) * 3;
        if body.len() < 2 + need {
            return Err(wire::ERR_FAILED);
        }
        Ok((w, h, need))
    }

    /// Request the kernel compositor to display a decoded full preview of `name` inside `(x, y, w, h)`
    /// (window-relative coordinates) while this app is in the foreground.
    pub fn set_preview(name: &str, x: i32, y: i32, w: i32, h: i32) -> Result<(), i32> {
        let xy = (((x.max(0) as u32) & 0xFFFF) << 16) | ((y.max(0) as u32) & 0xFFFF);
        let wh = (((w.max(0) as u32) & 0xFFFF) << 16) | ((h.max(0) as u32) & 0xFFFF);
        let mut resp = [0u8; 32];
        sys_call_raw(
            wire::OP_MEDIA_CALL,
            wire::MEDIA_SUB_PREVIEW_SET,
            xy,
            wh,
            name.as_bytes(),
            &mut resp,
        )
        .map(|_| ())
    }

    pub fn clear_preview() {
        let mut resp = [0u8; 32];
        let _ = sys_call_raw(wire::OP_MEDIA_CALL, wire::MEDIA_SUB_PREVIEW_CLEAR, 0, 0, &[], &mut resp);
    }
}

/// Trusted Telephony & SMS service SDK (truthful hardware availability + runtime permissions).
pub mod telephony {
    use super::*;

    #[derive(Clone, Copy, Default)]
    pub struct TelephonyStatus {
        pub voice_available: bool,
        pub sms_available: bool,
        pub call_log_available: bool,
    }

    pub fn status() -> TelephonyStatus {
        let mut resp = [0u8; 256];
        let Ok(body) = sys_call_raw(wire::OP_TELEPHONY_CALL, wire::TEL_SUB_STATUS, 0, 0, &[], &mut resp) else {
            return TelephonyStatus::default();
        };
        let Ok(text) = core::str::from_utf8(body) else {
            return TelephonyStatus::default();
        };
        let mut st = TelephonyStatus::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "voice_avail" => st.voice_available = parse_u32(v) != 0,
                "sms_avail" => st.sms_available = parse_u32(v) != 0,
                "call_log_avail" => st.call_log_available = parse_u32(v) != 0,
                _ => {}
            }
        }
        st
    }

    pub fn dial(number: &str) -> Result<(), i32> {
        let mut resp = [0u8; 64];
        sys_call_raw(wire::OP_TELEPHONY_CALL, wire::TEL_SUB_DIAL, 0, 0, number.as_bytes(), &mut resp).map(|_| ())
    }

    pub fn call_log<'a>(resp_buf: &'a mut [u8]) -> Result<&'a str, i32> {
        let body = sys_call_raw(wire::OP_TELEPHONY_CALL, wire::TEL_SUB_CALL_LOG, 0, 0, &[], resp_buf)?;
        core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)
    }

    pub fn sms_list<'a>(resp_buf: &'a mut [u8]) -> Result<&'a str, i32> {
        let body = sys_call_raw(wire::OP_TELEPHONY_CALL, wire::TEL_SUB_SMS_LIST, 0, 0, &[], resp_buf)?;
        core::str::from_utf8(body).map_err(|_| wire::ERR_FAILED)
    }

    pub fn sms_send(recipient: &str, text: &str) -> Result<(), i32> {
        let mut payload = [0u8; 320];
        let r = recipient.as_bytes();
        let t = text.as_bytes();
        if r.len() + 1 + t.len() > payload.len() {
            return Err(wire::ERR_BAD_REQUEST);
        }
        payload[..r.len()].copy_from_slice(r);
        payload[r.len()] = 0;
        payload[r.len() + 1..r.len() + 1 + t.len()].copy_from_slice(t);
        let mut resp = [0u8; 64];
        sys_call_raw(
            wire::OP_TELEPHONY_CALL,
            wire::TEL_SUB_SMS_SEND,
            0,
            0,
            &payload[..r.len() + 1 + t.len()],
            &mut resp,
        )
        .map(|_| ())
    }
}

/// Trusted Camera service SDK (enforces `camera` permission + truthful hardware availability).
pub mod camera {
    use super::*;

    #[derive(Clone, Copy, Default)]
    pub struct CameraStatus {
        pub camera_available: bool,
        pub mic_available: bool,
    }

    pub fn status() -> CameraStatus {
        let mut resp = [0u8; 128];
        let Ok(body) = sys_call_raw(wire::OP_CAMERA_CALL, wire::CAM_SUB_STATUS, 0, 0, &[], &mut resp) else {
            return CameraStatus::default();
        };
        let Ok(text) = core::str::from_utf8(body) else {
            return CameraStatus::default();
        };
        let mut st = CameraStatus::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "cam_avail" => st.camera_available = parse_u32(v) != 0,
                "mic_avail" => st.mic_available = parse_u32(v) != 0,
                _ => {}
            }
        }
        st
    }

    pub fn open() -> Result<(), i32> {
        let mut resp = [0u8; 64];
        sys_call_raw(wire::OP_CAMERA_CALL, wire::CAM_SUB_OPEN, 0, 0, &[], &mut resp).map(|_| ())
    }

    pub fn capture() -> Result<(), i32> {
        let mut resp = [0u8; 64];
        sys_call_raw(wire::OP_CAMERA_CALL, wire::CAM_SUB_CAPTURE, 0, 0, &[], &mut resp).map(|_| ())
    }
}
