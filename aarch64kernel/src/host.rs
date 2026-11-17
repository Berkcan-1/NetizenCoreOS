use crate::storage::SavedSettings;
use crate::{blk, input, storage, task, vm};
use alloc::string::String;
use core::fmt::Write;
use nc_abi::event::{
    pack_mobile_state, EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE,
    RET_ANIMATING, RET_CLOSE, RET_REDRAW,
};
use nc_abi::sys as sys_wire;
use nc_device_select::PROFILE;
use nc_gfx::{present, Canvas, Rect};
use nc_hal::FramebufferInfo;

const SURFACE: u64 = 1;
const MAX_STREAM: usize = nc_abi::draw::MAX_STREAM_BYTES;
const BRAND_ACCENT: u32 = 0x000A_59F7;
const EINVAL: i64 = -22;
const EFAULT: i64 = -14;
const EACCES: i64 = -13;
const ENOENT: i64 = -2;
const ENOMEM: i64 = -12;

const ANIM_TICKS: u64 = 2; // 20 ms = 50 FPS

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

macro_rules! log {
    ($($argument:tt)*) => {{
        let _ = writeln!(Serial, $($argument)*);
    }};
}

struct Compositor {
    canvas: Canvas<'static>,
    framebuffer: FramebufferInfo,
}

struct HostState {
    home_slot: usize,
    fg_slot: usize,
    active: bool,
    animating: bool,
    child_dirty: bool,
    seq: u64,
    last_update: u64,
    top_inset: i32,
    bottom_inset: i32,
    accent: u32,
    wallpaper: u8,
    tz_hours: i8,
    lang: u32,
    wifi: bool,
    bt: bool,
    brightness: u8,
    net_mmio_base: usize,
    net_mac: [u8; 6],
}

static mut COMPOSITOR: Option<Compositor> = None;
static mut HOST: HostState = HostState {
    home_slot: 0,
    fg_slot: 0,
    active: false,
    animating: false,
    child_dirty: false,
    seq: 0,
    last_update: 0,
    top_inset: 32,
    bottom_inset: 48,
    accent: BRAND_ACCENT,
    wallpaper: 0,
    tz_hours: 3,
    lang: 0,
    wifi: true,
    bt: false,
    brightness: 100,
    net_mmio_base: 0,
    net_mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
};
static mut STREAM: [u8; MAX_STREAM] = [0; MAX_STREAM];
static mut CHILD_STREAMS: [[u8; MAX_STREAM]; 2] = [[0; MAX_STREAM]; 2];
static mut CHILD_STREAM_LEN: [usize; 2] = [0; 2];

fn state() -> &'static mut HostState {
    unsafe { &mut *core::ptr::addr_of_mut!(HOST) }
}

fn compositor() -> Option<&'static mut Compositor> {
    unsafe { (*core::ptr::addr_of_mut!(COMPOSITOR)).as_mut() }
}

fn probe_virtio_net() {
    let host = state();
    let mut base = 0usize;
    for entry in PROFILE.drivers.iter() {
        if entry.compatible == "virtio,mmio" {
            base = entry.regs.base as usize;
        }
    }
    if base == 0 {
        return;
    }
    for slot in 0..32usize {
        let addr = base + slot * 0x200;
        let magic = unsafe { nc_arch::mmio_read32(addr) };
        let dev_id = unsafe { nc_arch::mmio_read32(addr + 0x008) };
        if magic == 0x7472_6976 && dev_id == 1 {
            host.net_mmio_base = addr;
            for i in 0..6 {
                host.net_mac[i] = unsafe { nc_arch::mmio_read8(addr + 0x100 + i) };
            }
            set_virtio_net_power(host.wifi);
            log!(
                "[ag] {:#x}: virtio-net MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                addr,
                host.net_mac[0],
                host.net_mac[1],
                host.net_mac[2],
                host.net_mac[3],
                host.net_mac[4],
                host.net_mac[5]
            );
            break;
        }
    }
}

fn set_virtio_net_power(enabled: bool) {
    let base = state().net_mmio_base;
    if base == 0 {
        return;
    }
    unsafe {
        if enabled {
            nc_arch::mmio_write32(base + 0x070, 1 | 2 | 4);
        } else {
            nc_arch::mmio_write32(base + 0x070, 0);
        }
    }
}

pub fn apply_saved_settings(cfg: SavedSettings) {
    let host = state();
    host.accent = cfg.accent & 0x00FF_FFFF;
    host.wallpaper = cfg.wallpaper.min(3);
    host.tz_hours = cfg.tz_hours.clamp(-12, 14);
    host.lang = cfg.lang.min(1);
    host.wifi = cfg.wifi;
    host.bt = cfg.bt;
    host.brightness = cfg.brightness.clamp(20, 100);
    set_virtio_net_power(host.wifi);
}

fn current_saved_settings(host: &HostState) -> SavedSettings {
    SavedSettings {
        accent: host.accent,
        wallpaper: host.wallpaper,
        tz_hours: host.tz_hours,
        lang: host.lang,
        wifi: host.wifi,
        bt: host.bt,
        brightness: host.brightness,
    }
}

fn running_apps_mask() -> u8 {
    let mut mask = 0u8;
    if task::find_app("nc.files").is_some() {
        mask |= 1 << 0;
    }
    if task::find_app("nc.settings").is_some() {
        mask |= 1 << 1;
    }
    mask
}

fn installed_apps_mask() -> u8 {
    let mut mask = 0u8;
    if storage::find_package("nc.files").is_some() {
        mask |= 1 << 0;
    }
    if storage::find_package("nc.settings").is_some() {
        mask |= 1 << 1;
    }
    mask
}

fn fg_app_code(host: &HostState) -> u8 {
    if host.fg_slot == host.home_slot {
        return 0;
    }
    if Some(host.fg_slot) == task::find_app("nc.files") {
        return 1;
    }
    if Some(host.fg_slot) == task::find_app("nc.settings") {
        return 2;
    }
    0
}

fn surface_idx_for_slot(slot: usize) -> Option<usize> {
    if Some(slot) == task::find_app("nc.files") {
        Some(0)
    } else if Some(slot) == task::find_app("nc.settings") {
        Some(1)
    } else {
        None
    }
}

fn make_event_for(slot: usize, op: u32, input: input::Snapshot) -> Option<EventBlock> {
    let compositor = compositor()?;
    let width = compositor.canvas.width() as i32;
    let full_height = compositor.canvas.height() as i32;
    let host = state();
    let height = if slot == host.home_slot {
        full_height
    } else {
        (full_height - host.top_inset - host.bottom_inset).max(160)
    };
    host.seq = host.seq.wrapping_add(1);
    let flags = pack_mobile_state(
        host.wallpaper,
        running_apps_mask(),
        fg_app_code(host),
        host.tz_hours,
        installed_apps_mask(),
        host.brightness,
        host.wifi,
        host.bt,
    );
    Some(EventBlock {
        op,
        flags,
        surface: SURFACE,
        area_x: 0,
        area_y: 0,
        area_w: width,
        area_h: height,
        unit: (width / 45).max(8),
        uptime_ms: (task::ticks() * 1000 / task::TICK_HZ) as u32,
        accent: host.accent,
        pointer_flags: input.pointer_flags,
        pointer_x: input.x,
        pointer_y: input.y,
        key: input.key,
        lang: host.lang,
        heap_base: 0,
        heap_size: 0,
        seq: host.seq,
    })
}

fn send_to(slot: usize, op: u32, input: input::Snapshot) -> bool {
    let block = match make_event_for(slot, op, input) {
        Some(block) => block,
        None => return false,
    };
    task::deliver(slot, &block)
}

fn empty_input() -> input::Snapshot {
    input::Snapshot { pointer_flags: 0, x: 0, y: 0, key: 0 }
}

pub fn attach(canvas: Canvas<'static>, framebuffer: FramebufferInfo, slot: usize) {
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    crate::input::set_screen(w, h);
    probe_virtio_net();
    unsafe {
        *core::ptr::addr_of_mut!(COMPOSITOR) = Some(Compositor { canvas, framebuffer });
    }
    let dp_w = w.min(h * 9 / 15).max(180).min(w);
    let host = state();
    host.top_inset = (28 * dp_w / 360).max(20);
    host.bottom_inset = (44 * dp_w / 360).max(32);
    host.home_slot = slot;
    host.fg_slot = slot;
    host.active = true;
    host.last_update = task::ticks();
    send_to(slot, OP_INIT, empty_input());
}

pub fn tick(ticks: u64) {
    let host = state();
    if !host.active {
        return;
    }
    let home = host.home_slot;
    if host.fg_slot != home && !task::is_alive(host.fg_slot) {
        host.fg_slot = home;
        let _ = send_to(home, OP_RESUME, empty_input());
        return;
    }

    let interval = if host.animating || crate::input::is_down() {
        ANIM_TICKS
    } else {
        task::TICK_HZ
    };
    let due = ticks.wrapping_sub(host.last_update) >= interval;
    let has_input = crate::input::pending();
    if !due && !has_input {
        return;
    }

    let snap = crate::input::snapshot();
    // Klavye tuşu varsa ve ön planda alt uygulama açıksa, tuşu alt uygulamaya ilet;
    // işaretçi ve zamanlayıcı olayları her zaman Ring 3 compositor'a (nc.home) gider.
    if snap.key != 0 && host.fg_slot != home && snap.pointer_flags == 0 {
        if send_to(host.fg_slot, OP_UPDATE, snap) {
            crate::input::consume();
            host.last_update = ticks;
        }
        return;
    }

    if send_to(home, OP_UPDATE, snap) {
        crate::input::consume();
        host.last_update = ticks;
    }
}

pub fn on_app_done(slot: usize, flags: u32) {
    let host = state();
    if !host.active {
        return;
    }
    let home = host.home_slot;
    if slot == home {
        host.animating = (flags & RET_ANIMATING) != 0;
        if flags & RET_REDRAW != 0 || host.child_dirty {
            host.child_dirty = false;
            let _ = send_to(home, OP_DRAW, empty_input());
        } else if host.animating {
            let snap = crate::input::snapshot();
            if send_to(home, OP_UPDATE, snap) {
                crate::input::consume();
                host.last_update = task::ticks();
            }
        }
        return;
    }

    // Alt uygulama (nc.files / nc.settings) olayını tamamladı
    if flags & RET_CLOSE != 0 {
        log!("[yasam] uygulama RET_CLOSE dondu (gorev {}) -> kapatiliyor", slot);
        if let Some(idx) = surface_idx_for_slot(slot) {
            unsafe {
                (*core::ptr::addr_of_mut!(CHILD_STREAM_LEN))[idx] = 0;
            }
        }
        task::kill_slot(slot);
        if host.fg_slot == slot {
            host.fg_slot = home;
        }
        let _ = send_to(home, OP_RESUME, empty_input());
        return;
    }

    if flags & RET_REDRAW != 0 {
        let _ = send_to(slot, OP_DRAW, empty_input());
    }
}

pub fn on_task_terminated(slot: usize) {
    let host = state();
    if !host.active {
        return;
    }
    if let Some(idx) = surface_idx_for_slot(slot) {
        unsafe {
            (*core::ptr::addr_of_mut!(CHILD_STREAM_LEN))[idx] = 0;
        }
    }
    if slot == host.fg_slot && slot != host.home_slot {
        host.fg_slot = host.home_slot;
        let _ = send_to(host.home_slot, OP_RESUME, empty_input());
    }
}

pub fn app_launch(pointer: u64, length: u64, action: u64) -> i64 {
    let host = state();
    let home = host.home_slot;

    // Ring 3 pencere yöneticisi eylemleri (action 2..5)
    match action {
        2 => {
            // Ön plandaki alt uygulamayı duraklat (OP_PAUSE) ve ana ekrana dön
            let old = host.fg_slot;
            if old != home {
                log!("[yasam] uygulama duraklatildi (gorev {}, OP_PAUSE) -> NetizenHome", old);
                task::set_paused(old, true);
                let _ = send_to(old, OP_PAUSE, empty_input());
                host.fg_slot = home;
            }
            return 0;
        }
        3 => {
            // Ön plandaki alt uygulamaya Geri (key::ESCAPE) gönder
            let fg = host.fg_slot;
            if fg != home {
                let back_snap = input::Snapshot {
                    pointer_flags: 0,
                    x: 0,
                    y: 0,
                    key: nc_abi::key::ESCAPE,
                };
                let _ = send_to(fg, OP_UPDATE, back_snap);
                return 1;
            }
            return 0;
        }
        4 => {
            // Dokunma olayını ön plandaki alt uygulamaya ilet
            let fg = host.fg_slot;
            if fg != home {
                let x = ((pointer >> 16) & 0xFFFF) as i32;
                let y = (pointer & 0xFFFF) as i32;
                let pflags = length as u32;
                let child_snap = input::Snapshot {
                    pointer_flags: pflags,
                    x,
                    y,
                    key: 0,
                };
                let _ = send_to(fg, OP_UPDATE, child_snap);
                return 0;
            }
            return ENOENT;
        }
        5 => {
            // Görünüm alanı kenar boşluklarını güncelle (top_inset, bottom_inset)
            host.top_inset = ((pointer >> 16) & 0xFFFF) as i32;
            host.bottom_inset = (pointer & 0xFFFF) as i32;
            return 0;
        }
        _ => {}
    }

    if length == 0 || length > 32 {
        return EINVAL;
    }
    if task::current_caps() & nc_caps::PROCESS_LAUNCH == 0 {
        return EACCES;
    }
    let mut id_buf = [0u8; 32];
    let copied = task::with_current_space(|space| space.copy_from_user(pointer, &mut id_buf[..length as usize])).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    let Ok(app_id) = core::str::from_utf8(&id_buf[..length as usize]) else {
        return EINVAL;
    };

    if action == 1 {
        if app_id == "nc.home" {
            return EACCES;
        }
        if let Some(slot) = task::find_app(app_id) {
            if slot == home || slot == task::current_slot() {
                return EACCES;
            }
            if let Some(idx) = surface_idx_for_slot(slot) {
                unsafe {
                    (*core::ptr::addr_of_mut!(CHILD_STREAM_LEN))[idx] = 0;
                }
            }
            if task::kill_slot(slot) {
                if host.fg_slot == slot {
                    host.fg_slot = home;
                }
                log!("[yasam] '{}' durduruldu (gorev {}, bellek serbest birakildi)", app_id, slot);
                return 0;
            }
        }
        return ENOENT;
    }

    if app_id == "nc.home" {
        if host.fg_slot != home {
            task::set_paused(host.fg_slot, true);
            let _ = send_to(host.fg_slot, OP_PAUSE, empty_input());
            host.fg_slot = home;
        }
        let _ = send_to(home, OP_RESUME, empty_input());
        return 0;
    }

    if let Some(existing_slot) = task::find_app(app_id) {
        log!("[yasam] '{}' arka plandan devam ettiriliyor (gorev {}, OP_RESUME)", app_id, existing_slot);
        if host.fg_slot != existing_slot && host.fg_slot != home {
            task::set_paused(host.fg_slot, true);
        }
        host.fg_slot = existing_slot;
        task::set_paused(existing_slot, false);
        let _ = send_to(existing_slot, OP_RESUME, empty_input());
        let _ = send_to(home, OP_RESUME, empty_input());
        return existing_slot as i64;
    }

    let Some(pkg) = storage::find_package(app_id) else {
        log!("[yasam] '{}' paketi bulunamadi", app_id);
        return ENOENT;
    };

    let spawned = match task::spawn_app(&pkg.id, &pkg.name, &pkg.payload, pkg.tier, pkg.caps) {
        Ok(slot) => Ok(slot),
        Err(_) => {
            if task::evict_oldest_paused(home).is_some() {
                task::spawn_app(&pkg.id, &pkg.name, &pkg.payload, pkg.tier, pkg.caps)
            } else {
                Err("gorev tablosu dolu veya bellek yok")
            }
        }
    };

    match spawned {
        Ok(slot) => {
            log!(
                "[yasam] '{}' baslatildi (EL0, gorev {}, katman={:?}, yetki={:#x}, OP_INIT)",
                pkg.id,
                slot,
                pkg.tier,
                pkg.caps
            );
            if host.fg_slot != home && host.fg_slot != slot {
                task::set_paused(host.fg_slot, true);
            }
            host.fg_slot = slot;
            task::set_paused(slot, false);
            let _ = send_to(slot, OP_INIT, empty_input());
            let _ = send_to(home, OP_RESUME, empty_input());
            slot as i64
        }
        Err(reason) => {
            log!("[yasam] '{}' baslatilamadi: {}", app_id, reason);
            ENOMEM
        }
    }
}

pub fn sys_call(req_ptr: u64, req_len: u64, resp_ptr: u64, resp_cap: u64) -> i64 {
    if req_len < sys_wire::REQ_HEADER as u64
        || req_len as usize > sys_wire::REQ_HEADER + sys_wire::MAX_REQUEST
        || resp_cap < sys_wire::RESP_HEADER as u64
    {
        return EINVAL;
    }
    let mut req_buf = alloc::vec![0u8; req_len as usize];
    let ok = task::with_current_space(|space| space.copy_from_user(req_ptr, &mut req_buf)).unwrap_or(false);
    if !ok {
        return EFAULT;
    }
    let Ok(req) = sys_wire::parse_request(&req_buf) else {
        return EINVAL;
    };

    let caps = task::current_caps();
    let host = state();
    let mut status: i32 = 0;
    let mut payload = String::new();

    match req.op {
        sys_wire::OP_DEVICE_INFO => {
            if caps & nc_caps::SETTINGS_READ == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                let (scr_w, scr_h) = match compositor() {
                    Some(c) => (c.canvas.width(), c.canvas.height()),
                    None => (0, 0),
                };
                let free_frames = vm::free_frames();
                let _ = writeln!(payload, "device_name={}", PROFILE.display_name);
                let _ = writeln!(payload, "device_id={}", PROFILE.id);
                let _ = writeln!(payload, "soc={}", PROFILE.soc);
                let _ = writeln!(payload, "arch=armv8a (aarch64)");
                let _ = writeln!(payload, "screen={}x{}", scr_w, scr_h);
                let _ = writeln!(payload, "ram_mib={}", crate::boot::total_ram_mib());
                let _ = writeln!(payload, "free_frames={}", free_frames);
                let _ = writeln!(payload, "free_mib={}", (free_frames * 4) / 1024);
                let _ = writeln!(payload, "gic={}", if nc_arch::gic::ready() { 1 } else { 0 });
                let _ = writeln!(payload, "tick_hz={}", task::TICK_HZ);
                let _ = writeln!(payload, "uptime_s={}", task::ticks() / task::TICK_HZ);
                let _ = writeln!(payload, "rtc_unix={}", nc_arch::rtc::unix_time_now().unwrap_or(0));
                let _ = writeln!(payload, "disks={}", blk::count());
                let _ = writeln!(payload, "sys_mounted={}", storage::system_fs().is_some() as u8);
                let _ = writeln!(payload, "sys_mib={}", storage::system_sectors() * 512 / (1024 * 1024));
                let _ = writeln!(payload, "data_mounted={}", storage::data_fs().is_some() as u8);
                let _ = writeln!(payload, "data_writable={}", storage::data_writable() as u8);
                let _ = writeln!(payload, "data_mib={}", storage::data_sectors() * 512 / (1024 * 1024));
                let sys_pkgs = storage::packages().iter().filter(|p| p.tier == nc_pkg::Tier::System).count();
                let user_pkgs = storage::packages().iter().filter(|p| p.tier == nc_pkg::Tier::User).count();
                let _ = writeln!(payload, "pkg_count={}", sys_pkgs);
                let _ = writeln!(payload, "user_app_count={}", user_pkgs);
                let _ = writeln!(payload, "media_count={}", storage::cached_media_count());
                let _ = writeln!(payload, "accent=0x{:06x}", host.accent);
                let _ = writeln!(payload, "wallpaper={}", host.wallpaper);
                let _ = writeln!(payload, "tz_hours={}", host.tz_hours);
                let _ = writeln!(payload, "lang={}", host.lang);
                let _ = writeln!(payload, "wifi={}", host.wifi as u8);
                let _ = writeln!(payload, "bt={}", host.bt as u8);
                let _ = writeln!(payload, "brightness={}", host.brightness);
                let _ = writeln!(
                    payload,
                    "net_mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    host.net_mac[0],
                    host.net_mac[1],
                    host.net_mac[2],
                    host.net_mac[3],
                    host.net_mac[4],
                    host.net_mac[5]
                );

                for pkg in storage::packages() {
                    let state_code = match task::find_app(&pkg.id) {
                        Some(slot) if slot == host.fg_slot => "fg",
                        Some(_) => "paused",
                        None => "stopped",
                    };
                    let pages = task::find_app(&pkg.id)
                        .and_then(task::slot_info)
                        .map(|info| info.pages)
                        .unwrap_or(0);
                    let tier_str = match pkg.tier {
                        nc_pkg::Tier::System => "system",
                        nc_pkg::Tier::User => "user",
                    };
                    let _ = writeln!(
                        payload,
                        "app={}|{}|{}|{}|{}|{}",
                        pkg.id,
                        pkg.name,
                        pkg.version,
                        tier_str,
                        state_code,
                        pages * 4
                    );
                }
            }
        }
        sys_wire::OP_MOBILE_SET => {
            let need = match req.arg0 {
                sys_wire::MOBILE_SET_ACCENT
                | sys_wire::MOBILE_SET_WALLPAPER
                | sys_wire::MOBILE_SET_BRIGHTNESS => nc_caps::SETTINGS_WRITE_DISPLAY,
                sys_wire::MOBILE_SET_TZ | sys_wire::MOBILE_SET_LANG => nc_caps::SETTINGS_WRITE_LOCALE,
                sys_wire::MOBILE_SET_WIFI | sys_wire::MOBILE_SET_BT | sys_wire::MOBILE_SYNC_DATA => {
                    nc_caps::SETTINGS_READ | nc_caps::SETTINGS_WRITE_DISPLAY
                }
                _ => 0,
            };
            if need == 0 {
                status = sys_wire::ERR_BAD_REQUEST;
            } else if caps & need == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                match req.arg0 {
                    sys_wire::MOBILE_SET_ACCENT => {
                        host.accent = req.arg1 & 0x00FF_FFFF;
                    }
                    sys_wire::MOBILE_SET_WALLPAPER => {
                        host.wallpaper = (req.arg1 as u8).min(3);
                    }
                    sys_wire::MOBILE_SET_TZ => {
                        host.tz_hours = (req.arg1 as i32).clamp(-12, 14) as i8;
                    }
                    sys_wire::MOBILE_SET_LANG => {
                        host.lang = req.arg1.min(1);
                    }
                    sys_wire::MOBILE_SYNC_DATA => {
                        if !storage::sync_data() {
                            status = sys_wire::ERR_FAILED;
                        }
                    }
                    sys_wire::MOBILE_SET_WIFI => {
                        host.wifi = req.arg1 != 0;
                        set_virtio_net_power(host.wifi);
                    }
                    sys_wire::MOBILE_SET_BT => {
                        host.bt = req.arg1 != 0;
                    }
                    sys_wire::MOBILE_SET_BRIGHTNESS => {
                        host.brightness = (req.arg1 as u8).clamp(20, 100);
                    }
                    _ => status = sys_wire::ERR_BAD_REQUEST,
                }
                if status == 0 && req.arg0 != sys_wire::MOBILE_SYNC_DATA {
                    let saved = storage::save_settings_cfg(current_saved_settings(host));
                    log!(
                        "[ayarlar] guncellendi (accent={:#08x}, tema={}, saat=UTC{:+}, dil={}, wifi={}, bt={}, parlaklik={}%, kalici={})",
                        host.accent,
                        host.wallpaper,
                        host.tz_hours,
                        host.lang,
                        host.wifi,
                        host.bt,
                        host.brightness,
                        saved
                    );
                    if task::current_slot() == host.home_slot && host.fg_slot != host.home_slot {
                        let _ = send_to(host.fg_slot, OP_UPDATE, empty_input());
                    }
                }
            }
        }
        _ => status = sys_wire::ERR_BAD_REQUEST,
    }

    let bytes = payload.as_bytes();
    let total = sys_wire::RESP_HEADER + bytes.len();
    if total > resp_cap as usize {
        return nc_abi::EFBIG;
    }
    let mut out = alloc::vec![0u8; total];
    if sys_wire::encode_response_header(&mut out, status, bytes.len()).is_none() {
        return EINVAL;
    }
    out[sys_wire::RESP_HEADER..total].copy_from_slice(bytes);
    let copied = task::with_current_space(|space| space.copy_to_user(resp_ptr, &out)).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    total as i64
}

pub fn gui_draw(surface: u64, pointer: u64, length: u64) -> i64 {
    if surface != SURFACE || length == 0 || length as usize > MAX_STREAM {
        return EINVAL;
    }
    let host = state();
    let caller = task::current_slot();

    // Alt uygulama çizim akışı gönderdiyse: doğrula, tampona kaydet ve Ring 3 compositor'u (nc.home) uyandır
    if caller != host.home_slot {
        let Some(idx) = surface_idx_for_slot(caller) else {
            return EINVAL;
        };
        let child_buf = unsafe {
            core::slice::from_raw_parts_mut(
                core::ptr::addr_of_mut!(CHILD_STREAMS[idx]) as *mut u8,
                length as usize,
            )
        };
        let copied = task::with_current_space(|space| space.copy_from_user(pointer, child_buf)).unwrap_or(false);
        if !copied {
            return EFAULT;
        }
        if nc_abi::draw::validate(child_buf).is_err() {
            return EINVAL;
        }
        unsafe {
            (*core::ptr::addr_of_mut!(CHILD_STREAM_LEN))[idx] = length as usize;
        }
        host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
        return length as i64;
    }
    host.child_dirty = false;

    let compositor = match compositor() {
        Some(compositor) => compositor,
        None => return EINVAL,
    };
    let buffer = unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STREAM) as *mut u8, length as usize) };
    let copied = task::with_current_space(|space| space.copy_from_user(pointer, buffer)).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    let width = compositor.canvas.width() as i32;
    let screen_h = compositor.canvas.height() as i32;
    let area = Rect::new(0, 0, width, screen_h);
    let result = nc_ui::remote::replay_with_surfaces(&mut compositor.canvas, area, buffer, |surf_id| unsafe {
        match surf_id {
            1 => {
                let len = (*core::ptr::addr_of!(CHILD_STREAM_LEN))[0];
                if len > 0 {
                    Some(core::slice::from_raw_parts(
                        core::ptr::addr_of!(CHILD_STREAMS[0]) as *const u8,
                        len,
                    ))
                } else {
                    None
                }
            }
            2 => {
                let len = (*core::ptr::addr_of!(CHILD_STREAM_LEN))[1];
                if len > 0 {
                    Some(core::slice::from_raw_parts(
                        core::ptr::addr_of!(CHILD_STREAMS[1]) as *const u8,
                        len,
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    });
    if result.is_err() {
        nc_arch::serial::write_str("[host] gecersiz cizim akisi reddedildi\n");
        return EINVAL;
    }

    // Parlaklık düzeyini ekrana uygula (%20..%100)
    if host.brightness < 100 {
        let dim_alpha = ((100 - host.brightness as u32) * 190 / 80).min(200);
        let full = Rect::new(0, 0, width, screen_h);
        nc_ui::draw_panel(&mut compositor.canvas, full, 0, 0x0000_0000, dim_alpha, 0, 0);
    }

    unsafe {
        present(&compositor.canvas, &compositor.framebuffer);
    }
    length as i64
}

pub fn text_width(pointer: u64, length: u64, cap_height: u64) -> i64 {
    if length == 0 || length > 256 {
        return EINVAL;
    }
    let mut buffer = [0u8; 256];
    let copied = task::with_current_space(|space| space.copy_from_user(pointer, &mut buffer[..length as usize])).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    match core::str::from_utf8(&buffer[..length as usize]) {
        Ok(text) => nc_ui::remote::measure_text(text, cap_height.min(128) as i32) as i64,
        Err(_) => EINVAL,
    }
}

pub fn recovery() -> ! {
    state().active = false;
    nc_arch::serial::write_str("[host] NetizenHome durdu; kurtarma ekrani\n");
    if let Some(compositor) = compositor() {
        let width = compositor.canvas.width() as i32;
        let height = compositor.canvas.height() as i32;
        compositor.canvas.fill_rect(0, 0, width, height, nc_gfx::rgb(241, 243, 245));
        let style = nc_ui::text_style((width / 22).max(14), 4);
        nc_ui::draw_text_color(&mut compositor.canvas, "NetizenHome durdu", width / 10, height / 3, &style, nc_gfx::rgb(24, 36, 49));
        unsafe {
            present(&compositor.canvas, &compositor.framebuffer);
        }
    }
    loop {
        crate::home::pause();
    }
}
