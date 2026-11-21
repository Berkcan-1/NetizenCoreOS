use crate::storage::SavedSettings;
use crate::{blk, input, lock, perm, storage, task, vm};
use alloc::string::String;
use core::fmt::Write;
use nc_abi::event::{
    pack_mobile_state_v2, EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE,
    RET_ANIMATING, RET_CLOSE, RET_REDRAW,
};
use nc_abi::sys as sys_wire;
use nc_device_select::PROFILE;
use nc_gfx::{present, Canvas, Rect};
use nc_hal::FramebufferInfo;

const SURFACE: u64 = 1;
const MAX_STREAM: usize = nc_abi::draw::MAX_STREAM_BYTES;
const MAX_CHILD_SURFACES: usize = 12;
const BRAND_ACCENT: u32 = 0x000A_59F7;
const EINVAL: i64 = -22;
const EFAULT: i64 = -14;
const EACCES: i64 = -13;
const ENOENT: i64 = -2;
const ENOMEM: i64 = -12;

const ANIM_TICKS: u64 = 1; // 10 ms tick cadence (up to 60+ FPS during active transitions)

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
    shadow: Option<&'static mut [u32]>,
    shadow_valid: bool,
    last_sig: u64,
    last_sig_valid: bool,
}

fn stream_signature(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed ^ 0xCBF2_9CE4_8422_2325;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let v = u64::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]]);
        h = (h ^ v).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(23);
    }
    for &b in chunks.remainder() {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    h ^ (bytes.len() as u64)
}

struct HostState {
    home_slot: usize,
    fg_slot: usize,
    active: bool,
    animating: bool,
    child_anim: bool,
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
    dnd: bool,
    airplane: bool,
    brightness: u8,
    hw_backlight_active: bool,
    dark_theme: bool,
    screen_timeout_s: u32,
    ui_scale: u8,
    dev_options: bool,
    anim_scale: u8,
    show_touch: bool,
    verbose_log: bool,
    device_name: [u8; 32],
    device_name_len: usize,
    net_mmio_base: usize,
    net_mac: [u8; 6],
    last_touch_x: i32,
    last_touch_y: i32,
    last_touch_down: bool,
    pending_intent_target: [u8; 32],
    pending_intent_target_len: usize,
    pending_intent_data: [u8; 256],
    pending_intent_data_len: usize,
    ime_visible: bool,
    ime_slot: Option<usize>,
    ime_target_slot: usize,
    ime_id: [u8; 32],
    ime_id_len: usize,
}

static mut COMPOSITOR: Option<Compositor> = None;
static mut HOST: HostState = HostState {
    home_slot: 0,
    fg_slot: 0,
    active: false,
    animating: false,
    child_anim: false,
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
    dnd: false,
    airplane: false,
    brightness: 100,
    hw_backlight_active: false,
    dark_theme: false,
    screen_timeout_s: 60,
    ui_scale: 100,
    dev_options: false,
    anim_scale: 100,
    show_touch: false,
    verbose_log: false,
    device_name: [0; 32],
    device_name_len: 0,
    net_mmio_base: 0,
    net_mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
    last_touch_x: 0,
    last_touch_y: 0,
    last_touch_down: false,
    pending_intent_target: [0; 32],
    pending_intent_target_len: 0,
    pending_intent_data: [0; 256],
    pending_intent_data_len: 0,
    ime_visible: false,
    ime_slot: None,
    ime_target_slot: 0,
    ime_id: *b"nc.keyboard\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    ime_id_len: 11,
};
static mut STREAM: [u8; MAX_STREAM] = [0; MAX_STREAM];
static mut CHILD_STREAMS: [[u8; MAX_STREAM]; MAX_CHILD_SURFACES] = [[0; MAX_STREAM]; MAX_CHILD_SURFACES];
static mut CHILD_STREAM_LEN: [usize; MAX_CHILD_SURFACES] = [0; MAX_CHILD_SURFACES];
static mut CHILD_SURFACE_SLOT: [usize; MAX_CHILD_SURFACES] = [usize::MAX; MAX_CHILD_SURFACES];
static mut IME_STREAM: [u8; MAX_STREAM] = [0; MAX_STREAM];
static mut IME_STREAM_LEN: usize = 0;

fn state() -> &'static mut HostState {
    unsafe { &mut *core::ptr::addr_of_mut!(HOST) }
}

fn compositor() -> Option<&'static mut Compositor> {
    unsafe { (*core::ptr::addr_of_mut!(COMPOSITOR)).as_mut() }
}

fn copy_bytes(dst: &mut [u8], src: &str) -> usize {
    let b = src.as_bytes();
    let n = b.len().min(dst.len());
    dst[..n].copy_from_slice(&b[..n]);
    n
}

fn host_device_name(host: &HostState) -> &str {
    if host.device_name_len > 0 {
        core::str::from_utf8(&host.device_name[..host.device_name_len]).unwrap_or(PROFILE.display_name)
    } else {
        PROFILE.display_name
    }
}

fn find_surface_for_slot(slot: usize) -> Option<usize> {
    let map = unsafe { &*core::ptr::addr_of!(CHILD_SURFACE_SLOT) };
    map.iter().position(|&s| s == slot)
}

fn alloc_surface_for_slot(slot: usize) -> Option<usize> {
    if let Some(idx) = find_surface_for_slot(slot) {
        return Some(idx);
    }
    let map = unsafe { &mut *core::ptr::addr_of_mut!(CHILD_SURFACE_SLOT) };
    let lens = unsafe { &mut *core::ptr::addr_of_mut!(CHILD_STREAM_LEN) };
    for (idx, entry) in map.iter_mut().enumerate() {
        if *entry == usize::MAX || !task::is_alive(*entry) {
            *entry = slot;
            lens[idx] = 0;
            return Some(idx);
        }
    }
    None
}

fn release_surface_for_slot(slot: usize) {
    let map = unsafe { &mut *core::ptr::addr_of_mut!(CHILD_SURFACE_SLOT) };
    let lens = unsafe { &mut *core::ptr::addr_of_mut!(CHILD_STREAM_LEN) };
    for (idx, entry) in map.iter_mut().enumerate() {
        if *entry == slot {
            *entry = usize::MAX;
            lens[idx] = 0;
        }
    }
}

fn surface_id_for_app(app_id: &str) -> u32 {
    if let Some(slot) = task::find_app(app_id) {
        if let Some(idx) = find_surface_for_slot(slot) {
            return (idx + 1) as u32;
        }
    }
    0
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
            if crate::net::init_virtio_net(addr) {
                host.net_mac = crate::net::status().mac;
            } else {
                for i in 0..6 {
                    host.net_mac[i] = unsafe { nc_arch::mmio_read8(addr + 0x100 + i) };
                }
                set_virtio_net_power(host.wifi);
            }
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

/// Real virtio-net hardware state:
/// 0 = unavailable, 1 = disabled, 2 = enabled/disconnected, 4 = connected (link up).
fn real_wifi_state(host: &HostState) -> (bool, u8, u8) {
    let base = host.net_mmio_base;
    if base == 0 {
        return (false, 0, 0);
    }
    if !host.wifi || host.airplane {
        return (true, 1, 0);
    }
    let dev_status = unsafe { nc_arch::mmio_read32(base + 0x070) };
    if (dev_status & 4) == 0 {
        return (true, 2, 0);
    }
    // virtio-net config space: offset 0x100..0x105 = MAC, 0x106..0x107 = status (bit 0 = VIRTIO_NET_S_LINK_UP).
    let link_lo = unsafe { nc_arch::mmio_read8(base + 0x106) };
    let link_up = (link_lo & 1) != 0;
    if link_up {
        // Wired/virtio link has no real Wi-Fi RSSI; report 0 so UI does not fabricate signal dBm.
        (true, 4, 0)
    } else {
        (true, 2, 0)
    }
}

fn query_battery() -> nc_device::BatteryStatus {
    match PROFILE.hw_ops.battery {
        Some(f) => f(),
        None => nc_device::BatteryStatus::UNAVAILABLE,
    }
}

fn apply_hw_brightness(host: &mut HostState) {
    host.hw_backlight_active = match PROFILE.hw_ops.set_backlight {
        Some(f) => f(host.brightness),
        None => false,
    };
}

pub fn apply_saved_settings(cfg: &SavedSettings) {
    let host = state();
    host.accent = cfg.accent & 0x00FF_FFFF;
    host.wallpaper = cfg.wallpaper.min(5);
    if host.wallpaper == 5 && !crate::wallpaper::has_image() {
        host.wallpaper = 0;
    }
    host.tz_hours = cfg.tz_hours.clamp(-12, 14);
    host.lang = cfg.lang.min(1);
    host.wifi = cfg.wifi;
    host.bt = cfg.bt;
    host.dnd = cfg.dnd;
    host.airplane = cfg.airplane;
    host.brightness = cfg.brightness.clamp(20, 100);
    host.dark_theme = cfg.dark_theme;
    host.screen_timeout_s = cfg.screen_timeout_s.min(3600);
    host.ui_scale = cfg.ui_scale.clamp(85, 125);
    host.dev_options = cfg.dev_options;
    host.anim_scale = cfg.anim_scale.min(200);
    host.show_touch = cfg.show_touch;
    host.verbose_log = cfg.verbose_log;
    host.device_name_len = copy_bytes(&mut host.device_name, &cfg.device_name);
    set_virtio_net_power(host.wifi && !host.airplane);
    apply_hw_brightness(host);
}

fn current_saved_settings(host: &HostState) -> SavedSettings {
    SavedSettings {
        schema_version: 2,
        accent: host.accent,
        wallpaper: host.wallpaper,
        tz_hours: host.tz_hours,
        lang: host.lang,
        wifi: host.wifi,
        bt: host.bt,
        brightness: host.brightness,
        dark_theme: host.dark_theme,
        screen_timeout_s: host.screen_timeout_s,
        ui_scale: host.ui_scale,
        dev_options: host.dev_options,
        anim_scale: host.anim_scale,
        show_touch: host.show_touch,
        verbose_log: host.verbose_log,
        device_name: String::from(host_device_name(host)),
        lock_timeout_s: lock::config_timeout_secs(),
        lock_on_boot: lock::config_lock_on_boot(),
        lock_show_info: lock::config_show_info(),
        dnd: host.dnd,
        airplane: host.airplane,
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
    if task::find_app("nc.package-installer").is_some() {
        mask |= 1 << 2;
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
    if storage::find_package("nc.package-installer").is_some() {
        mask |= 1 << 2;
    }
    mask
}

fn fg_app_code(host: &HostState) -> u8 {
    if host.fg_slot == host.home_slot || lock::is_locked() {
        return 0;
    }
    if let Some(idx) = find_surface_for_slot(host.fg_slot) {
        ((idx + 1) as u8).min(15)
    } else {
        0
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
    let flags = pack_mobile_state_v2(
        host.wallpaper,
        running_apps_mask(),
        fg_app_code(host),
        host.tz_hours,
        installed_apps_mask(),
        host.brightness,
        host.wifi,
        host.bt,
        lock::is_locked(),
        host.dark_theme,
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

fn enforce_lock_foreground(host: &mut HostState) {
    let home = host.home_slot;
    host.ime_visible = false;
    unsafe { *core::ptr::addr_of_mut!(IME_STREAM_LEN) = 0; }
    if host.fg_slot != home {
        perm::clear_preview_for_slot(host.fg_slot);
        task::set_paused(host.fg_slot, true);
        let _ = send_to(host.fg_slot, OP_PAUSE, empty_input());
        host.fg_slot = home;
    }
    let _ = send_to(home, OP_RESUME, empty_input());
}

pub fn attach(canvas: Canvas<'static>, framebuffer: FramebufferInfo, slot: usize, shadow: Option<&'static mut [u32]>) {
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    crate::input::set_screen(w, h);
    probe_virtio_net();
    unsafe {
        *core::ptr::addr_of_mut!(COMPOSITOR) = Some(Compositor {
            canvas,
            framebuffer,
            shadow,
            shadow_valid: false,
            last_sig: 0,
            last_sig_valid: false,
        });
    }
    let dp_w = w.min(h * 9 / 15).max(180).min(w);
    let host = state();
    if host.device_name_len == 0 {
        host.device_name_len = copy_bytes(&mut host.device_name, PROFILE.display_name);
    }
    host.top_inset = (28 * dp_w / 360).max(20);
    host.bottom_inset = (44 * dp_w / 360).max(32);
    host.home_slot = slot;
    host.fg_slot = slot;
    host.active = true;
    host.last_update = task::ticks();
    lock::note_activity(host.last_update);
    send_to(slot, OP_INIT, empty_input());
}

pub fn tick(ticks: u64) {
    let host = state();
    if !host.active {
        return;
    }
    let home = host.home_slot;

    // Bosta kalma otomatik kilit denetimi
    if lock::tick_idle(ticks) {
        enforce_lock_foreground(host);
        return;
    }

    if host.fg_slot != home && !task::is_alive(host.fg_slot) {
        host.fg_slot = home;
        let _ = send_to(home, OP_RESUME, empty_input());
        return;
    }

    let perm_anim = !lock::is_locked() && perm::prompt_animating();
    let interval = if host.animating || host.child_anim || perm_anim || crate::input::is_down() {
        ANIM_TICKS
    } else {
        task::TICK_HZ
    };
    let due = ticks.wrapping_sub(host.last_update) >= interval;
    let has_input = crate::input::pending();
    if !due && !has_input {
        return;
    }

    if has_input {
        lock::note_activity(ticks);
    }

    // Advance kernel-owned permission dialog entrance/exit transition when active.
    if perm_anim && due {
        let reduced = host.anim_scale == 0;
        match perm::tick_prompt(reduced) {
            perm::PromptTickResult::Redraw => {
                host.last_update = ticks;
                host.child_dirty = !send_to(home, OP_DRAW, empty_input());
                if !has_input {
                    return;
                }
            }
            perm::PromptTickResult::FinishedExit { caller_slot } => {
                host.last_update = ticks;
                if task::is_alive(caller_slot) {
                    let _ = send_to(caller_slot, OP_UPDATE, empty_input());
                }
                host.child_dirty = !send_to(home, OP_DRAW, empty_input());
                if !has_input {
                    return;
                }
            }
            perm::PromptTickResult::Idle => {}
        }
    }

    if host.child_anim && host.fg_slot != home && !has_input && !lock::is_locked() {
        host.child_anim = false;
        if send_to(host.fg_slot, OP_UPDATE, empty_input()) {
            host.last_update = ticks;
        }
        return;
    }

    let snap = crate::input::snapshot();
    if has_input {
        lock::mix_entropy(((snap.x as u64) << 32) | (snap.y as u64), snap.key as u64);
        host.last_touch_x = snap.x;
        host.last_touch_y = snap.y;
        host.last_touch_down = (snap.pointer_flags & nc_abi::event::PTR_DOWN) != 0;
    }

    // OS-owned runtime permission dialog intercepts ALL touch/key input until decided & dismissed.
    if perm::prompt_active() && !lock::is_locked() && has_input {
        if let Some(comp) = compositor() {
            let sw = comp.canvas.width() as i32;
            let sh = comp.canvas.height() as i32;
            let reduced = host.anim_scale == 0;
            match perm::handle_prompt_input(snap, sw, sh, host.lang, reduced) {
                perm::PromptInputResult::Decided { caller_slot } => {
                    crate::input::consume();
                    host.last_update = ticks;
                    if task::is_alive(caller_slot) {
                        let _ = send_to(caller_slot, OP_UPDATE, empty_input());
                    }
                    host.child_dirty = !send_to(home, OP_DRAW, empty_input());
                    return;
                }
                perm::PromptInputResult::Redraw => {
                    crate::input::consume();
                    host.last_update = ticks;
                    host.child_dirty = !send_to(home, OP_DRAW, empty_input());
                    return;
                }
                perm::PromptInputResult::Consumed => {
                    crate::input::consume();
                    host.last_update = ticks;
                    return;
                }
                perm::PromptInputResult::NotActive => {}
            }
        }
    }

    // Moduler Ring 3 sanal klavye (IME) gorunurse, klavye tepsisi icindeki dokunmalari IME gorevine yonlendir!
    if host.ime_visible && !lock::is_locked() && has_input && snap.pointer_flags != 0 {
        if let (Some(ime_slot), Some(comp)) = (host.ime_slot, compositor()) {
            let sw = comp.canvas.width() as i32;
            let sh = comp.canvas.height() as i32;
            let dp_w = sw.min(sh * 9 / 15).max(180).min(sw);
            let ime_h = (216i64 * dp_w as i64 / 360) as i32;
            if snap.y >= (sh - ime_h).max(0) && task::is_alive(ime_slot) {
                if send_to(ime_slot, OP_UPDATE, snap) {
                    crate::input::consume();
                    host.last_update = ticks;
                }
                return;
            }
        }
    }

    // Kilitliyken klavye/dokunma olaylari ASLA alt uygulamalara gitmez, yalnizca NetizenHome kilit ekranina gider!
    if snap.key != 0 && host.fg_slot != home && snap.pointer_flags == 0 && !lock::is_locked() {
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

    if flags & RET_CLOSE != 0 {
        log!("[yasam] uygulama RET_CLOSE dondu (gorev {}) -> kapatiliyor", slot);
        release_surface_for_slot(slot);
        task::kill_slot(slot);
        if host.fg_slot == slot {
            host.fg_slot = home;
        }
        let _ = send_to(home, OP_RESUME, empty_input());
        return;
    }

    if flags & RET_ANIMATING != 0 && slot == host.fg_slot {
        host.child_anim = true;
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
    perm::on_task_terminated(slot);
    release_surface_for_slot(slot);
    if host.ime_slot == Some(slot) {
        host.ime_slot = None;
        host.ime_visible = false;
        unsafe { *core::ptr::addr_of_mut!(IME_STREAM_LEN) = 0; }
    }
    if host.ime_visible && host.ime_target_slot == slot {
        host.ime_visible = false;
        unsafe { *core::ptr::addr_of_mut!(IME_STREAM_LEN) = 0; }
    }
    if slot == host.fg_slot && slot != host.home_slot {
        host.fg_slot = host.home_slot;
        let _ = send_to(host.home_slot, OP_RESUME, empty_input());
    }
}

pub fn app_launch(pointer: u64, length: u64, action: u64) -> i64 {
    let host = state();
    let home = host.home_slot;

    // Ring 3 pencere yoneticisi eylemleri (action 2..=5): yalnizca sistem kabugu (home_slot)
    if (2..=5).contains(&action) && task::current_slot() != home {
        return EACCES;
    }
    match action {
        2 => {
            let old = host.fg_slot;
            if old != home {
                log!("[yasam] uygulama duraklatildi (gorev {}, OP_PAUSE) -> NetizenHome", old);
                perm::clear_preview_for_slot(old);
                task::set_paused(old, true);
                let _ = send_to(old, OP_PAUSE, empty_input());
                host.fg_slot = home;
            }
            return 0;
        }
        3 => {
            if lock::is_locked() {
                return EACCES;
            }
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
            if lock::is_locked() {
                return EACCES;
            }
            let fg = host.fg_slot;
            if fg != home {
                let x = ((pointer >> 16) & 0xFFFF) as i32;
                let y = (pointer & 0xFFFF) as i32;
                let pflags = length as u32;
                let child_snap = input::Snapshot { pointer_flags: pflags, x, y, key: 0 };
                let _ = send_to(fg, OP_UPDATE, child_snap);
                return 0;
            }
            return ENOENT;
        }
        5 => {
            host.top_inset = ((pointer >> 16) & 0xFFFF) as i32;
            host.bottom_inset = (pointer & 0xFFFF) as i32;
            return 0;
        }
        _ => {}
    }

    // Ekran kilitliyken hicbir uygulama baslatilamaz veya devam ettirilemez!
    if lock::is_locked() && action == 0 {
        log!("[kilit] kilitli durumdayken uygulama baslatma reddedildi");
        return EACCES;
    }

    if length == 0 || length > 64 {
        return EINVAL;
    }
    let mut id_buf = [0u8; 64];
    let copied = task::with_current_space(|space| space.copy_from_user(pointer, &mut id_buf[..length as usize])).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    let Ok(app_id) = core::str::from_utf8(&id_buf[..length as usize]) else {
        return EINVAL;
    };
    let mut caller_buf = [0u8; 32];
    let caller_len = task::current_app_id(&mut caller_buf);
    let caller_id = core::str::from_utf8(&caller_buf[..caller_len]).unwrap_or("");
    if nc_spark_sec::policy_authorize_launch(caller_id, task::current_caps() as u64, app_id)
        != nc_spark_sec::AUTH_ALLOW
    {
        return EACCES;
    }

    if action == 1 {
        if app_id == "nc.home" {
            return EACCES;
        }
        if let Some(slot) = task::find_app(app_id) {
            if slot == home || slot == task::current_slot() {
                return EACCES;
            }
            release_surface_for_slot(slot);
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
        let _ = alloc_surface_for_slot(existing_slot);
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
            let surf_idx = alloc_surface_for_slot(slot).unwrap_or(0);
            log!(
                "[yasam] '{}' baslatildi (EL0, gorev {}, yuzey {}, katman={:?}, yetki={:#x}, OP_INIT)",
                pkg.id,
                slot,
                surf_idx + 1,
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
    let caller_slot = task::current_slot();
    let host = state();
    let mut status: i32 = 0;
    let mut payload = String::new();

    match req.op {
        sys_wire::OP_SYSTEM_STATUS => {
            let bat = query_battery();
            let (wifi_hw, wifi_state, wifi_rssi) = real_wifi_state(host);
            let bt_hw = PROFILE.hw_ops.has_hw_bt;
            let bt_state: u8 = if !bt_hw { 0 } else if host.bt && !host.airplane { 2 } else { 1 };
            let cell_hw = PROFILE.hw_ops.has_hw_cellular;
            let cell_state: u8 = if !cell_hw { 0 } else { 1 };
            let free_frames = vm::free_frames();
            let usb = storage::usb_info();
            let _ = writeln!(payload, "bat_avail={}", bat.available as u8);
            let _ = writeln!(payload, "bat_pct={}", bat.percent);
            let _ = writeln!(payload, "bat_chg={}", bat.charging as u8);
            let _ = writeln!(payload, "bat_full={}", bat.full as u8);
            let _ = writeln!(payload, "bat_mv={}", bat.voltage_mv);
            let _ = writeln!(payload, "bat_temp={}", bat.temp_deci_c);
            let ns = crate::net::status();
            let _ = writeln!(payload, "wifi_hw={}", wifi_hw as u8);
            let _ = writeln!(payload, "wifi_radio={}", ns.wifi_radio_present as u8);
            let _ = writeln!(payload, "wifi_en={}", (host.wifi && !host.airplane) as u8);
            let _ = writeln!(payload, "wifi_state={}", wifi_state);
            let _ = writeln!(payload, "wifi_rssi={}", wifi_rssi);
            let _ = writeln!(payload, "wifi_scan_count={}", ns.wifi_scan_count);
            let _ = writeln!(
                payload,
                "net_mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                host.net_mac[0], host.net_mac[1], host.net_mac[2],
                host.net_mac[3], host.net_mac[4], host.net_mac[5]
            );
            let _ = writeln!(payload, "net_dhcp={}", ns.dhcp_bound as u8);
            let _ = writeln!(payload, "net_ip={}.{}.{}.{}", ns.ip[0], ns.ip[1], ns.ip[2], ns.ip[3]);
            let _ = writeln!(payload, "net_gw={}.{}.{}.{}", ns.gateway[0], ns.gateway[1], ns.gateway[2], ns.gateway[3]);
            let _ = writeln!(payload, "net_dns={}.{}.{}.{}", ns.dns[0], ns.dns[1], ns.dns[2], ns.dns[3]);
            let _ = writeln!(payload, "bt_hw={}", bt_hw as u8);
            let _ = writeln!(payload, "bt_en={}", (host.bt && !host.airplane && bt_hw) as u8);
            let _ = writeln!(payload, "bt_state={}", bt_state);
            let _ = writeln!(payload, "bt_conn=0");
            let _ = writeln!(payload, "cell_hw={}", cell_hw as u8);
            let _ = writeln!(payload, "cell_sim=0");
            let _ = writeln!(payload, "cell_state={}", cell_state);
            let _ = writeln!(payload, "cell_bars=0");
            let _ = writeln!(payload, "cell_data=0");
            let _ = writeln!(payload, "audio_hw={}", PROFILE.hw_ops.has_hw_audio as u8);
            let _ = writeln!(payload, "vol_pct=0");
            let _ = writeln!(payload, "bright_pct={}", host.brightness);
            let _ = writeln!(payload, "bright_hw={}", PROFILE.hw_ops.has_hw_backlight as u8);
            let _ = writeln!(payload, "dark_theme={}", host.dark_theme as u8);
            let _ = writeln!(payload, "anim_scale={}", host.anim_scale);
            let _ = writeln!(payload, "dnd={}", host.dnd as u8);
            let _ = writeln!(payload, "airplane={}", host.airplane as u8);
            let _ = writeln!(payload, "ram_mib={}", crate::boot::total_ram_mib());
            let _ = writeln!(payload, "free_mib={}", (free_frames * 4) / 1024);
            let _ = writeln!(payload, "data_mib={}", storage::data_sectors() * 512 / (1024 * 1024));
            if let Some(usage) = storage::data_usage() {
                let _ = writeln!(payload, "data_used_kib={}", usage.total() / 1024);
            } else {
                let _ = writeln!(payload, "data_used_kib=0");
            }
            let _ = writeln!(payload, "uptime_s={}", task::ticks() / task::TICK_HZ);
            let _ = writeln!(payload, "live_tasks={}", task::live_tasks_count());
            let _ = writeln!(payload, "usb_mounted={}", usb.mounted as u8);
            let _ = writeln!(payload, "device_model={}", PROFILE.display_name);
        }
        sys_wire::OP_WIDGET_REGISTRY => {
            // Versioned widget provider discovery v1 (system + installed packages).
            let _ = writeln!(payload, "api_version=1");
            let _ = writeln!(payload, "gen={}", storage::registry_generation());
        }
        sys_wire::OP_SELF_INFO => {
            let mut id_buf = [0u8; 32];
            let id_len = task::current_app_id(&mut id_buf);
            let id_str = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
            let _ = writeln!(payload, "id={}", id_str);
            let _ = writeln!(payload, "caps=0x{:08x}", caps);
            if let Some(pkg) = storage::find_package(id_str) {
                let _ = writeln!(payload, "name={}", pkg.name);
                let _ = writeln!(payload, "version={}", pkg.version);
                let _ = writeln!(payload, "tier={}", if pkg.tier == nc_pkg::Tier::System { "system" } else { "user" });
                let _ = writeln!(payload, "icon_id={}", pkg.icon_id);
            }
        }
        sys_wire::OP_APP_REGISTRY => {
            if caps & (nc_caps::SETTINGS_READ | nc_caps::PROCESS_LAUNCH) == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                let _ = writeln!(payload, "gen={}", storage::registry_generation());
                let _ = writeln!(payload, "fg_surface={}", fg_app_code(host));
                for pkg in storage::packages() {
                    let state_code = match task::find_app(&pkg.id) {
                        Some(slot) if slot == host.fg_slot => "fg",
                        Some(_) => "paused",
                        None => "stopped",
                    };
                    let surf_id = surface_id_for_app(&pkg.id);
                    let pages = task::find_app(&pkg.id)
                        .and_then(task::slot_info)
                        .map(|info| info.pages)
                        .unwrap_or(0);
                    let tier_str = match pkg.tier {
                        nc_pkg::Tier::System => "system",
                        nc_pkg::Tier::User => "user",
                    };
                    let src_str = match pkg.source {
                        nc_pkg::InstallSource::SystemImage => "system_image",
                        nc_pkg::InstallSource::UserData => "userdata",
                    };
                    let _ = writeln!(
                        payload,
                        "pkg={}|{}|{}|{}|{}|{}|{}|{}|{}|{:08x}|{:08x}|{}|{}",
                        pkg.id,
                        pkg.name,
                        pkg.version,
                        pkg.icon_id,
                        tier_str,
                        src_str,
                        state_code,
                        surf_id,
                        pages * 4,
                        pkg.requested_caps,
                        pkg.caps,
                        pkg.is_verified as u8,
                        pkg.pkg_bytes,
                    );
                }
            }
        }
        sys_wire::OP_PKG_INSPECT => {
            let Ok(path) = core::str::from_utf8(req.payload) else {
                status = sys_wire::ERR_BAD_REQUEST;
                payload.clear();
                return send_sys_reply(resp_ptr, resp_cap, status, payload.as_bytes());
            };
            match storage::inspect_package_from_path(caps, path) {
                Ok(info) => payload = info,
                Err(code) => status = code,
            }
        }
        sys_wire::OP_PKG_UNINSTALL => {
            let Ok(app_id) = core::str::from_utf8(req.payload) else {
                status = sys_wire::ERR_BAD_REQUEST;
                return send_sys_reply(resp_ptr, resp_cap, status, payload.as_bytes());
            };
            let remove_data = req.arg0 != 0;
            status = storage::uninstall_package(caps, app_id, remove_data);
            if status == 0 {
                let _ = send_to(host.home_slot, OP_RESUME, empty_input());
            }
        }
        sys_wire::OP_APP_CLEAR_DATA => {
            let Ok(app_id) = core::str::from_utf8(req.payload) else {
                status = sys_wire::ERR_BAD_REQUEST;
                return send_sys_reply(resp_ptr, resp_cap, status, payload.as_bytes());
            };
            status = storage::clear_app_data(caps, app_id);
        }
        sys_wire::OP_LOCK_STATUS => {
            let _ = writeln!(payload, "locked={}", lock::is_locked() as u8);
            let _ = writeln!(payload, "enabled={}", lock::is_enabled() as u8);
            let _ = writeln!(payload, "kind={}", lock::kind());
            let _ = writeln!(payload, "fails={}", lock::fails());
            let _ = writeln!(payload, "lockout_s={}", lock::lockout_remaining_secs());
            let _ = writeln!(payload, "timeout_s={}", lock::config_timeout_secs());
            let _ = writeln!(payload, "lock_on_boot={}", lock::config_lock_on_boot() as u8);
            let _ = writeln!(payload, "show_info={}", lock::config_show_info() as u8);
        }
        sys_wire::OP_LOCK_AUTH => {
            // Sadece home shell (kilit ekrani) veya SECURITY_MANAGE yetkili uygulamalar dogrulama cagirabilir
            if caller_slot != host.home_slot && (caps & nc_caps::SECURITY_MANAGE) == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                match lock::authenticate(req.payload) {
                    lock::AuthResult::Unlocked => {
                        let _ = writeln!(payload, "result=ok");
                    }
                    lock::AuthResult::InvalidSecret => {
                        status = sys_wire::ERR_DENIED;
                        let _ = writeln!(payload, "result=invalid");
                        let _ = writeln!(payload, "fails={}", lock::fails());
                    }
                    lock::AuthResult::LockedOut(rem) => {
                        status = sys_wire::ERR_DENIED;
                        let _ = writeln!(payload, "result=lockout");
                        let _ = writeln!(payload, "lockout_s={}", rem);
                        let _ = writeln!(payload, "fails={}", lock::fails());
                    }
                    lock::AuthResult::NotEnabled => {
                        let _ = writeln!(payload, "result=not_enabled");
                    }
                }
            }
        }
        sys_wire::OP_LOCK_CONFIG => {
            match req.arg0 {
                sys_wire::LOCK_CFG_LOCK_NOW => {
                    if caller_slot != host.home_slot && (caps & nc_caps::SECURITY_MANAGE) == 0 {
                        status = sys_wire::ERR_DENIED;
                    } else if lock::lock_now() {
                        enforce_lock_foreground(host);
                    } else {
                        status = sys_wire::ERR_FAILED;
                    }
                }
                sys_wire::LOCK_CFG_SET => {
                    if caps & nc_caps::SECURITY_MANAGE == 0 {
                        status = sys_wire::ERR_DENIED;
                    } else {
                        let kind = req.arg1 as u8;
                        let mut parts = req.payload.splitn(2, |&b| b == 0);
                        let current = parts.next().unwrap_or(&[]);
                        let new_sec = parts.next().unwrap_or(&[]);
                        match lock::configure_set(kind, current, new_sec) {
                            Ok(()) => {
                                let _ = writeln!(payload, "ok=1");
                            }
                            Err(msg) => {
                                status = sys_wire::ERR_FAILED;
                                let _ = writeln!(payload, "error={}", msg);
                            }
                        }
                    }
                }
                sys_wire::LOCK_CFG_DISABLE => {
                    if caps & nc_caps::SECURITY_MANAGE == 0 {
                        status = sys_wire::ERR_DENIED;
                    } else {
                        match lock::configure_disable(req.payload) {
                            Ok(()) => {
                                let _ = writeln!(payload, "ok=1");
                            }
                            Err(msg) => {
                                status = sys_wire::ERR_FAILED;
                                let _ = writeln!(payload, "error={}", msg);
                            }
                        }
                    }
                }
                _ => status = sys_wire::ERR_BAD_REQUEST,
            }
        }
        sys_wire::OP_PENDING_INTENT => {
            if req.arg0 == 1 {
                // Yazma: `target_app_id\0path`
                if caps & nc_caps::PROCESS_LAUNCH == 0 {
                    status = sys_wire::ERR_DENIED;
                } else {
                    let mut parts = req.payload.splitn(2, |&b| b == 0);
                    let target = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
                    let data = parts.next().and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
                    if target.is_empty() || data.is_empty() {
                        status = sys_wire::ERR_BAD_REQUEST;
                    } else {
                        host.pending_intent_target_len = copy_bytes(&mut host.pending_intent_target, target);
                        host.pending_intent_data_len = copy_bytes(&mut host.pending_intent_data, data);
                    }
                }
            } else {
                // Okuma ve temizleme: yalnizca hedef uygulama okuyabilir
                let mut id_buf = [0u8; 32];
                let id_len = task::current_app_id(&mut id_buf);
                let caller_id = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
                let target = core::str::from_utf8(&host.pending_intent_target[..host.pending_intent_target_len]).unwrap_or("");
                if !target.is_empty() && caller_id == target {
                    let data = core::str::from_utf8(&host.pending_intent_data[..host.pending_intent_data_len]).unwrap_or("");
                    payload.push_str(data);
                    host.pending_intent_target_len = 0;
                    host.pending_intent_data_len = 0;
                }
            }
        }
        sys_wire::OP_USB_INFO => {
            let info = storage::usb_info();
            let _ = writeln!(payload, "mounted={}", info.mounted as u8);
            let _ = writeln!(payload, "mount_path=/storage/usb0");
            let _ = writeln!(payload, "disk_index={}", if info.mounted { info.disk_index as i32 } else { -1 });
            let _ = writeln!(payload, "total_kib={}", info.total_kib);
            let _ = writeln!(payload, "read_only={}", info.read_only as u8);
            let _ = writeln!(payload, "fs_type={}", info.fs_type);
            let _ = writeln!(payload, "label={}", info.label);
        }
        sys_wire::OP_USB_EJECT => {
            if caps & (nc_caps::FS_USER_READ | nc_caps::FS_ALL_READ) == 0 {
                status = sys_wire::ERR_DENIED;
            } else if !storage::usb_eject() {
                status = sys_wire::ERR_FAILED;
            }
        }
        sys_wire::OP_DEVICE_INFO => {
            if caps & nc_caps::SETTINGS_READ == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                let (scr_w, scr_h) = match compositor() {
                    Some(c) => (c.canvas.width(), c.canvas.height()),
                    None => (0, 0),
                };
                let free_frames = vm::free_frames();
                let _ = writeln!(payload, "device_name={}", host_device_name(host));
                let _ = writeln!(payload, "device_model={}", PROFILE.display_name);
                let _ = writeln!(payload, "device_id={}", PROFILE.id);
                let _ = writeln!(payload, "soc={}", PROFILE.soc);
                let _ = writeln!(payload, "arch=armv8a (aarch64)");
                let _ = writeln!(payload, "os_version=0.4.0");
                let _ = writeln!(payload, "build_number=NC-2026.11-ARM64");
                let _ = writeln!(payload, "kernel_version=NetizenCore-AArch64 0.4.0");
                let _ = writeln!(payload, "sdk_version=ring3-v1 (ABI v{})", nc_abi::ABI_VERSION);
                let _ = writeln!(payload, "screen={}x{}", scr_w, scr_h);
                let _ = writeln!(payload, "ram_mib={}", crate::boot::total_ram_mib());
                let _ = writeln!(payload, "free_frames={}", free_frames);
                let _ = writeln!(payload, "free_mib={}", (free_frames * 4) / 1024);
                let _ = writeln!(payload, "live_tasks={}", task::live_tasks_count());
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
                let usb = storage::usb_info();
                let _ = writeln!(payload, "usb_mounted={}", usb.mounted as u8);
                let _ = writeln!(payload, "usb_kib={}", usb.total_kib);
                let _ = writeln!(payload, "usb_fs={}", usb.fs_type);
                let _ = writeln!(payload, "usb_label={}", usb.label);
                let _ = writeln!(payload, "usb_ro={}", usb.read_only as u8);
                if let Some(usage) = storage::data_usage() {
                    let _ = writeln!(payload, "data_used_kib={}", usage.total() / 1024);
                    let _ = writeln!(payload, "data_apps_kib={}", usage.apps / 1024);
                    let _ = writeln!(payload, "data_media_kib={}", usage.media / 1024);
                    let _ = writeln!(payload, "data_appdata_kib={}", usage.appdata / 1024);
                    let _ = writeln!(payload, "data_other_kib={}", usage.other / 1024);
                    let _ = writeln!(payload, "data_files={}", usage.files);
                    let _ = writeln!(payload, "data_usage_complete={}", usage.complete as u8);
                }
                if let Some(usage) = storage::system_usage() {
                    let _ = writeln!(payload, "sys_used_kib={}", usage.total() / 1024);
                    let _ = writeln!(payload, "sys_files={}", usage.files);
                }
                let sys_pkgs = storage::packages().iter().filter(|p| p.tier == nc_pkg::Tier::System).count();
                let user_pkgs = storage::packages().iter().filter(|p| p.tier == nc_pkg::Tier::User).count();
                let _ = writeln!(payload, "pkg_count={}", sys_pkgs);
                let _ = writeln!(payload, "user_app_count={}", user_pkgs);
                let _ = writeln!(payload, "registry_gen={}", storage::registry_generation());
                let _ = writeln!(payload, "media_count={}", storage::cached_media_count());
                let _ = writeln!(payload, "accent=0x{:06x}", host.accent);
                let _ = writeln!(payload, "wallpaper={}", host.wallpaper);
                if let Some(name) = crate::wallpaper::current_name() {
                    let _ = writeln!(payload, "wp_current={}", name);
                }
                for name in crate::wallpaper::candidates() {
                    let _ = writeln!(payload, "wp_image={}", name);
                }
                let _ = writeln!(payload, "tz_hours={}", host.tz_hours);
                let _ = writeln!(payload, "lang={}", host.lang);
                let ns = crate::net::status();
                let _ = writeln!(payload, "wifi={}", host.wifi as u8);
                let _ = writeln!(payload, "wifi_hw={}", (host.net_mmio_base != 0) as u8);
                let _ = writeln!(payload, "wifi_radio={}", ns.wifi_radio_present as u8);
                let _ = writeln!(payload, "wifi_scan_count={}", ns.wifi_scan_count);
                let _ = writeln!(payload, "net_dhcp={}", ns.dhcp_bound as u8);
                let _ = writeln!(payload, "net_ip={}.{}.{}.{}", ns.ip[0], ns.ip[1], ns.ip[2], ns.ip[3]);
                let _ = writeln!(payload, "net_gw={}.{}.{}.{}", ns.gateway[0], ns.gateway[1], ns.gateway[2], ns.gateway[3]);
                let _ = writeln!(payload, "net_dns={}.{}.{}.{}", ns.dns[0], ns.dns[1], ns.dns[2], ns.dns[3]);
                let _ = writeln!(payload, "bt={}", host.bt as u8);
                let _ = writeln!(payload, "bt_hw=0");
                let _ = writeln!(payload, "audio_hw=0");
                let _ = writeln!(payload, "vibrator_hw=0");
                let _ = writeln!(payload, "gps_hw=0");
                let _ = writeln!(payload, "brightness={}", host.brightness);
                let _ = writeln!(payload, "dark_theme={}", host.dark_theme as u8);
                let _ = writeln!(payload, "screen_timeout_s={}", host.screen_timeout_s);
                let _ = writeln!(payload, "ui_scale={}", host.ui_scale);
                let _ = writeln!(payload, "dev_options={}", host.dev_options as u8);
                let _ = writeln!(payload, "anim_scale={}", host.anim_scale);
                let _ = writeln!(payload, "show_touch={}", host.show_touch as u8);
                let _ = writeln!(payload, "verbose_log={}", host.verbose_log as u8);
                let _ = writeln!(payload, "lock_enabled={}", lock::is_enabled() as u8);
                let _ = writeln!(payload, "lock_kind={}", lock::kind());
                let _ = writeln!(payload, "lock_timeout_s={}", lock::config_timeout_secs());
                let _ = writeln!(payload, "lock_on_boot={}", lock::config_lock_on_boot() as u8);
                let _ = writeln!(payload, "lock_show_info={}", lock::config_show_info() as u8);
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
                | sys_wire::MOBILE_SET_WALLPAPER_IMAGE
                | sys_wire::MOBILE_SET_BRIGHTNESS
                | sys_wire::MOBILE_SET_DARK_THEME
                | sys_wire::MOBILE_SET_SCREEN_TIMEOUT
                | sys_wire::MOBILE_SET_UI_SCALE
                | sys_wire::MOBILE_SET_DEV_OPTIONS
                | sys_wire::MOBILE_SET_ANIM_SCALE
                | sys_wire::MOBILE_SET_SHOW_TOUCH
                | sys_wire::MOBILE_SET_VERBOSE_LOG
                | sys_wire::MOBILE_SET_DEVICE_NAME => nc_caps::SETTINGS_WRITE_DISPLAY,
                sys_wire::MOBILE_SET_TZ | sys_wire::MOBILE_SET_LANG => nc_caps::SETTINGS_WRITE_LOCALE,
                sys_wire::MOBILE_SET_WIFI
                | sys_wire::MOBILE_SET_BT
                | sys_wire::MOBILE_SET_DND
                | sys_wire::MOBILE_SET_AIRPLANE
                | sys_wire::MOBILE_SYNC_DATA => {
                    nc_caps::SETTINGS_WRITE_NETWORK | nc_caps::SETTINGS_WRITE_DISPLAY
                }
                sys_wire::MOBILE_SET_LOCK_TIMEOUT
                | sys_wire::MOBILE_SET_LOCK_ON_BOOT
                | sys_wire::MOBILE_SET_LOCK_SHOW_INFO => nc_caps::SECURITY_MANAGE,
                _ => 0,
            };
            if need == 0 {
                status = sys_wire::ERR_BAD_REQUEST;
            } else if caps & need == 0 {
                status = sys_wire::ERR_DENIED;
            } else {
                match req.arg0 {
                    sys_wire::MOBILE_SET_ACCENT => host.accent = req.arg1 & 0x00FF_FFFF,
                    sys_wire::MOBILE_SET_WALLPAPER => {
                        crate::wallpaper::clear();
                        host.wallpaper = (req.arg1 as u8).min(4);
                    }
                    sys_wire::MOBILE_SET_WALLPAPER_IMAGE => {
                        let applied = match (core::str::from_utf8(req.payload), compositor()) {
                            (Ok(name), Some(compositor)) => crate::wallpaper::set(
                                name,
                                compositor.canvas.width(),
                                compositor.canvas.height(),
                            ),
                            _ => false,
                        };
                        if applied {
                            host.wallpaper = 5;
                        } else {
                            status = sys_wire::ERR_FAILED;
                        }
                    }
                    sys_wire::MOBILE_SET_TZ => host.tz_hours = (req.arg1 as i32).clamp(-12, 14) as i8,
                    sys_wire::MOBILE_SET_LANG => host.lang = req.arg1.min(1),
                    sys_wire::MOBILE_SYNC_DATA => {
                        if !storage::sync_data() {
                            status = sys_wire::ERR_FAILED;
                        }
                    }
                    sys_wire::MOBILE_SET_WIFI => {
                        host.wifi = req.arg1 != 0;
                        if host.wifi {
                            host.airplane = false;
                        }
                        set_virtio_net_power(host.wifi && !host.airplane);
                    }
                    sys_wire::MOBILE_SET_BT => {
                        if PROFILE.hw_ops.has_hw_bt {
                            host.bt = req.arg1 != 0;
                        } else {
                            host.bt = false;
                            status = sys_wire::ERR_FAILED;
                        }
                    }
                    sys_wire::MOBILE_SET_DND => host.dnd = req.arg1 != 0,
                    sys_wire::MOBILE_SET_AIRPLANE => {
                        host.airplane = req.arg1 != 0;
                        set_virtio_net_power(host.wifi && !host.airplane);
                    }
                    sys_wire::MOBILE_SET_BRIGHTNESS => {
                        host.brightness = (req.arg1 as u8).clamp(20, 100);
                        apply_hw_brightness(host);
                    }
                    sys_wire::MOBILE_SET_DARK_THEME => host.dark_theme = req.arg1 != 0,
                    sys_wire::MOBILE_SET_SCREEN_TIMEOUT => host.screen_timeout_s = req.arg1.min(3600),
                    sys_wire::MOBILE_SET_UI_SCALE => host.ui_scale = (req.arg1 as u8).clamp(85, 125),
                    sys_wire::MOBILE_SET_DEV_OPTIONS => host.dev_options = req.arg1 != 0,
                    sys_wire::MOBILE_SET_ANIM_SCALE => host.anim_scale = (req.arg1 as u8).min(200),
                    sys_wire::MOBILE_SET_SHOW_TOUCH => host.show_touch = req.arg1 != 0,
                    sys_wire::MOBILE_SET_VERBOSE_LOG => host.verbose_log = req.arg1 != 0,
                    sys_wire::MOBILE_SET_DEVICE_NAME => {
                        if let Ok(name) = core::str::from_utf8(req.payload) {
                            let trimmed = name.trim();
                            if !trimmed.is_empty() && trimmed.len() <= 32 {
                                host.device_name_len = copy_bytes(&mut host.device_name, trimmed);
                            } else {
                                status = sys_wire::ERR_BAD_REQUEST;
                            }
                        } else {
                            status = sys_wire::ERR_BAD_REQUEST;
                        }
                    }
                    sys_wire::MOBILE_SET_LOCK_TIMEOUT => {
                        lock::set_policy(req.arg1.min(3600), lock::config_lock_on_boot(), lock::config_show_info());
                    }
                    sys_wire::MOBILE_SET_LOCK_ON_BOOT => {
                        lock::set_policy(lock::config_timeout_secs(), req.arg1 != 0, lock::config_show_info());
                    }
                    sys_wire::MOBILE_SET_LOCK_SHOW_INFO => {
                        lock::set_policy(lock::config_timeout_secs(), lock::config_lock_on_boot(), req.arg1 != 0);
                    }
                    _ => status = sys_wire::ERR_BAD_REQUEST,
                }
                if status == 0 && req.arg0 != sys_wire::MOBILE_SYNC_DATA {
                    let saved = storage::save_settings_cfg(&current_saved_settings(host));
                    if host.verbose_log {
                        log!("[ayarlar] sub_op={} guncellendi (kalici={})", req.arg0, saved);
                    }
                    if task::current_slot() == host.home_slot && host.fg_slot != host.home_slot {
                        let _ = send_to(host.fg_slot, OP_UPDATE, empty_input());
                    }
                }
            }
        }
        sys_wire::OP_WL_POLL | sys_wire::OP_WL_CMD => {
            if req.op == sys_wire::OP_WL_CMD && req.arg0 == 1 {
                let _ = crate::net::wifi_scan();
            }
            let ns = crate::net::status();
            let _ = writeln!(payload, "wifi_radio={}", ns.wifi_radio_present as u8);
            let _ = writeln!(payload, "wifi_state={}", ns.wifi_state as u8);
            let scans = crate::net::cached_wifi_scans();
            let _ = writeln!(payload, "scan_count={}", scans.len());
            for (idx, entry) in scans.iter().enumerate() {
                let _ = writeln!(
                    payload,
                    "net_{}={}|{}|{}|{}",
                    idx,
                    entry.ssid_str(),
                    entry.rssi_dbm,
                    entry.security.as_str(),
                    entry.channel
                );
            }
        }
        sys_wire::OP_IME_CMD => {
            match req.arg0 {
                sys_wire::IME_SHOW => {
                    if lock::is_locked() {
                        status = sys_wire::ERR_DENIED;
                    } else {
                        host.ime_target_slot = caller_slot;
                        let ime_id = core::str::from_utf8(&host.ime_id[..host.ime_id_len])
                            .unwrap_or("nc.keyboard");
                        let slot_opt = if let Some(s) = host.ime_slot.filter(|&s| task::is_alive(s)) {
                            Some(s)
                        } else if let Some(existing) = task::find_app(ime_id) {
                            host.ime_slot = Some(existing);
                            Some(existing)
                        } else if let Some(pkg) = storage::find_package(ime_id) {
                            match task::spawn_app(
                                &pkg.id,
                                &pkg.name,
                                &pkg.payload,
                                pkg.tier,
                                pkg.caps,
                            ) {
                                Ok(new_slot) => {
                                    host.ime_slot = Some(new_slot);
                                    log!("[ime] moduler klavye baslatildi: '{}' (gorev {})", ime_id, new_slot);
                                    let _ = send_to(new_slot, OP_INIT, empty_input());
                                    Some(new_slot)
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        };
                        if let Some(ime_slot) = slot_opt {
                            host.ime_visible = true;
                            let _ = send_to(ime_slot, OP_RESUME, empty_input());
                            let _ = send_to(ime_slot, OP_DRAW, empty_input());
                            host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
                        } else {
                            status = sys_wire::ERR_FAILED;
                        }
                    }
                }
                sys_wire::IME_HIDE => {
                    host.ime_visible = false;
                    unsafe { *core::ptr::addr_of_mut!(IME_STREAM_LEN) = 0; }
                    if let Some(ime_slot) = host.ime_slot.filter(|&s| task::is_alive(s)) {
                        let _ = send_to(ime_slot, OP_PAUSE, empty_input());
                    }
                    host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
                }
                sys_wire::IME_COMMIT_KEY => {
                    if host.ime_slot != Some(caller_slot) || !host.ime_visible || lock::is_locked() {
                        status = sys_wire::ERR_DENIED;
                    } else {
                        let key_snap = input::Snapshot {
                            pointer_flags: 0,
                            x: 0,
                            y: 0,
                            key: req.arg1,
                        };
                        let target = host.ime_target_slot;
                        if task::is_alive(target) {
                            let _ = send_to(target, OP_UPDATE, key_snap);
                        }
                    }
                }
                sys_wire::IME_STATUS => {
                    let ime_id = core::str::from_utf8(&host.ime_id[..host.ime_id_len])
                        .unwrap_or("nc.keyboard");
                    let _ = writeln!(payload, "visible={}", host.ime_visible as u8);
                    let _ = writeln!(payload, "ime_id={}", ime_id);
                    let _ = writeln!(payload, "height_dp=216");
                }
                sys_wire::IME_SET_ACTIVE => {
                    if caps & (nc_caps::SETTINGS_WRITE_KEYBOARD | nc_caps::SECURITY_MANAGE) == 0 {
                        status = sys_wire::ERR_DENIED;
                    } else if let Ok(new_id) = core::str::from_utf8(req.payload) {
                        let trimmed = new_id.trim();
                        if !trimmed.is_empty() && trimmed.len() <= 32 && storage::find_package(trimmed).is_some() {
                            if let Some(old) = host.ime_slot.take() {
                                task::kill_slot(old);
                            }
                            host.ime_visible = false;
                            unsafe { *core::ptr::addr_of_mut!(IME_STREAM_LEN) = 0; }
                            host.ime_id_len = copy_bytes(&mut host.ime_id, trimmed);
                        } else {
                            status = sys_wire::ERR_BAD_REQUEST;
                        }
                    } else {
                        status = sys_wire::ERR_BAD_REQUEST;
                    }
                }
                _ => status = sys_wire::ERR_BAD_REQUEST,
            }
        }
        sys_wire::OP_PERM_CALL => {
            let (st, bytes, redraw) = perm::handle_perm_call(&req);
            if perm::take_open_settings_request() && !lock::is_locked() {
                host.pending_intent_target_len = copy_bytes(&mut host.pending_intent_target, "nc.settings");
                host.pending_intent_data_len = copy_bytes(&mut host.pending_intent_data, "permissions");
                let home = host.home_slot;
                if let Some(existing_slot) = task::find_app("nc.settings") {
                    let _ = alloc_surface_for_slot(existing_slot);
                    if host.fg_slot != existing_slot && host.fg_slot != home {
                        perm::clear_preview_for_slot(host.fg_slot);
                        task::set_paused(host.fg_slot, true);
                    }
                    host.fg_slot = existing_slot;
                    task::set_paused(existing_slot, false);
                    let _ = send_to(existing_slot, OP_RESUME, empty_input());
                    let _ = send_to(home, OP_RESUME, empty_input());
                } else if let Some(pkg) = storage::find_package("nc.settings") {
                    if let Ok(slot) = task::spawn_app(&pkg.id, &pkg.name, &pkg.payload, pkg.tier, pkg.caps) {
                        let _ = alloc_surface_for_slot(slot);
                        if host.fg_slot != home && host.fg_slot != slot {
                            perm::clear_preview_for_slot(host.fg_slot);
                            task::set_paused(host.fg_slot, true);
                        }
                        host.fg_slot = slot;
                        task::set_paused(slot, false);
                        let _ = send_to(slot, OP_INIT, empty_input());
                        let _ = send_to(home, OP_RESUME, empty_input());
                    }
                }
            }
            if redraw {
                host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
            }
            return send_sys_reply(resp_ptr, resp_cap, st, &bytes);
        }
        sys_wire::OP_CONTACTS_CALL => {
            let (st, bytes) = perm::handle_contacts_call(&req);
            return send_sys_reply(resp_ptr, resp_cap, st, &bytes);
        }
        sys_wire::OP_MEDIA_CALL => {
            let (st, bytes, redraw) = perm::handle_media_call(&req);
            if redraw {
                host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
            }
            return send_sys_reply(resp_ptr, resp_cap, st, &bytes);
        }
        sys_wire::OP_TELEPHONY_CALL => {
            let (st, bytes) = perm::handle_telephony_call(&req);
            return send_sys_reply(resp_ptr, resp_cap, st, &bytes);
        }
        sys_wire::OP_CAMERA_CALL => {
            let (st, bytes) = perm::handle_camera_call(&req);
            return send_sys_reply(resp_ptr, resp_cap, st, &bytes);
        }
        _ => status = sys_wire::ERR_BAD_REQUEST,
    }

    send_sys_reply(resp_ptr, resp_cap, status, payload.as_bytes())
}

fn send_sys_reply(resp_ptr: u64, resp_cap: u64, status: i32, bytes: &[u8]) -> i64 {
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

    if host.ime_slot == Some(caller) {
        let ime_buf = unsafe {
            core::slice::from_raw_parts_mut(
                core::ptr::addr_of_mut!(IME_STREAM) as *mut u8,
                length as usize,
            )
        };
        let copied = task::with_current_space(|space| space.copy_from_user(pointer, ime_buf)).unwrap_or(false);
        if !copied {
            return EFAULT;
        }
        if nc_abi::draw::validate(ime_buf).is_err() {
            return EINVAL;
        }
        unsafe {
            *core::ptr::addr_of_mut!(IME_STREAM_LEN) = length as usize;
        }
        host.child_dirty = !send_to(host.home_slot, OP_DRAW, empty_input());
        return length as i64;
    }

    if caller != host.home_slot {
        let Some(idx) = alloc_surface_for_slot(caller) else {
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
    let locked = lock::is_locked();

    let touch_sig = if host.show_touch && host.last_touch_down {
        ((host.last_touch_x as u64) << 16) ^ (host.last_touch_y as u64) ^ 0x1_0000_0000
    } else {
        0
    };
    let mut sig = stream_signature(
        buffer,
        ((crate::wallpaper::generation() as u64) << 24)
            | ((locked as u64) << 17)
            | ((host.brightness as u64) << 8)
            | (host.wallpaper as u64)
            ^ touch_sig
            ^ perm::overlay_signature_bits(),
    );
    if !locked {
        unsafe {
            for idx in 0..MAX_CHILD_SURFACES {
                let len = (*core::ptr::addr_of!(CHILD_STREAM_LEN))[idx];
                if len > 0 {
                    let child = core::slice::from_raw_parts(core::ptr::addr_of!(CHILD_STREAMS[idx]) as *const u8, len);
                    sig = stream_signature(child, sig);
                }
            }
            if host.ime_visible {
                let ime_len = *core::ptr::addr_of!(IME_STREAM_LEN);
                if ime_len > 0 {
                    let ime_bytes = core::slice::from_raw_parts(core::ptr::addr_of!(IME_STREAM) as *const u8, ime_len);
                    sig = stream_signature(ime_bytes, sig ^ 0x494D_4501);
                }
            }
        }
    }
    if compositor.last_sig_valid && compositor.last_sig == sig {
        return length as i64;
    }
    compositor.last_sig = sig;
    compositor.last_sig_valid = true;

    let wp_drawn = if host.wallpaper == 5 {
        if crate::wallpaper::blit(&mut compositor.canvas) {
            true
        } else {
            host.wallpaper = 0;
            crate::wallpaper::clear();
            crate::wallpaper::blit_preset(0, &mut compositor.canvas)
        }
    } else {
        crate::wallpaper::blit_preset(host.wallpaper, &mut compositor.canvas)
    };
    if !wp_drawn {
        compositor.canvas.fill_rect(0, 0, width, screen_h, 0x0010_141C);
    }
    let area = Rect::new(0, 0, width, screen_h);
    let result = nc_ui::remote::replay_with_surfaces_and_icons(
        &mut compositor.canvas,
        area,
        buffer,
        |surf_id| {
            // Ekran kilitliyken hicbir alt uygulama icerigi ekrana cizilmez!
            if locked || surf_id == 0 || surf_id as usize > MAX_CHILD_SURFACES {
                return None;
            }
            let idx = (surf_id - 1) as usize;
            unsafe {
                let len = (*core::ptr::addr_of!(CHILD_STREAM_LEN))[idx];
                if len > 0 {
                    Some(core::slice::from_raw_parts(
                        core::ptr::addr_of!(CHILD_STREAMS[idx]) as *const u8,
                        len,
                    ))
                } else {
                    None
                }
            }
        },
        |_id, _side| {
            #[cfg(feature = "ui-mobile")]
            {
                nc_mobile_assets::icon(_id, _side).map(|icon| nc_ui::remote::IconBitmap {
                    size: icon.size,
                    pixels: icon.pixels,
                })
            }
            #[cfg(not(feature = "ui-mobile"))]
            {
                None
            }
        },
    );
    if result.is_err() {
        nc_arch::serial::write_str("[host] gecersiz cizim akisi reddedildi\n");
        return EINVAL;
    }

    // Kernel-decoded scoped media preview (Gallery), clipped to foreground app viewport
    if !locked && host.fg_slot != host.home_slot {
        perm::blit_media_preview(&mut compositor.canvas, host.fg_slot, host.top_inset, host.bottom_inset);
    }

    // Moduler Ring 3 sanal klavye (IME) katmanini en uste ciz (ekran kilitli degilken)
    if host.ime_visible && !locked {
        let ime_len = unsafe { *core::ptr::addr_of!(IME_STREAM_LEN) };
        if ime_len > 0 {
            let ime_bytes = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(IME_STREAM) as *const u8,
                    ime_len,
                )
            };
            let _ = nc_ui::remote::replay(&mut compositor.canvas, area, ime_bytes);
        }
    }

    // OS-owned runtime permission request dialog (non-forgeable, always topmost above apps & IME)
    if !locked && perm::prompt_active() {
        perm::draw_prompt(
            &mut compositor.canvas,
            host.dark_theme,
            host.accent,
            host.lang,
            host.anim_scale == 0,
        );
    }

    // Gelistirici Secenekleri: dokunmalari goster
    if host.show_touch && host.last_touch_down {
        compositor.canvas.fill_rect_alpha(
            (host.last_touch_x - 8).max(0),
            (host.last_touch_y - 8).max(0),
            16,
            16,
            host.accent,
            160,
        );
    }

    if !host.hw_backlight_active && host.brightness < 100 {
        let dim_alpha = ((100 - host.brightness as u32) * 190 / 80).min(200);
        compositor.canvas.fill_rect_alpha(0, 0, width, screen_h, 0x0000_0000, dim_alpha);
    }

    unsafe {
        match compositor.shadow.as_deref_mut() {
            Some(shadow) if compositor.shadow_valid => {
                nc_gfx::present_diff(&compositor.canvas, shadow, &compositor.framebuffer)
            }
            Some(shadow) => {
                present(&compositor.canvas, &compositor.framebuffer);
                let count = compositor.canvas.pixels().len().min(shadow.len());
                shadow[..count].copy_from_slice(&compositor.canvas.pixels()[..count]);
                compositor.shadow_valid = true;
            }
            None => present(&compositor.canvas, &compositor.framebuffer),
        }
    }
    length as i64
}

pub fn text_width(pointer: u64, length: u64, cap_height: u64) -> i64 {
    if length > nc_abi::draw::MAX_TEXT_BYTES as u64 {
        return EINVAL;
    }
    let mut bytes = [0u8; nc_abi::draw::MAX_TEXT_BYTES];
    let copied = task::with_current_space(|space| space.copy_from_user(pointer, &mut bytes[..length as usize])).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    let Ok(text) = core::str::from_utf8(&bytes[..length as usize]) else {
        return EINVAL;
    };
    let cap = (cap_height as i32).clamp(1, nc_abi::draw::MAX_CAP_HEIGHT);
    nc_font::text_width(text, &nc_ui::text_style(cap, 2)) as i64
}

pub fn recovery() -> ! {
    nc_arch::serial::write_str("[host] NetizenHome sonlandi; sistem kurtarma beklemesine gecti\n");
    if let Some(compositor) = compositor() {
        let w = compositor.canvas.width() as i32;
        let h = compositor.canvas.height() as i32;
        compositor.canvas.fill_rect(0, 0, w, h, 0x000F_172A);
        let style = nc_ui::text_style(16, 2);
        nc_ui::draw_text_color(&mut compositor.canvas, "NetizenHome durdu", 32, h / 2 - 16, &style, 0x00FF_FFFF);
        unsafe {
            present(&compositor.canvas, &compositor.framebuffer);
        }
    }
    nc_arch::halt_forever();
}
