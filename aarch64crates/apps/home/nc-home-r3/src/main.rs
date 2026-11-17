//! NetizenHome (Ring 3 / EL0): Compositor, Pencere Yöneticisi, Durum Çubuğu, Kontrol Merkezi,
//! Son Uygulamalar (yukarı kaydırarak kapatma), Widget Sistemi, 4'lü Klasörler ve Uzun Basmayla Serbest Izgara.
#![allow(dead_code)]
#![no_std]
#![no_main]

mod clock;
mod icons;
mod theme;

use clock::{format_time, from_unix_tz, MONTHS};
use nc_abi::event::{
    unpack_brightness, unpack_bt, unpack_fg_app, unpack_installed_mask, unpack_running_mask,
    unpack_tz_hours, unpack_wallpaper, unpack_wifi, EventBlock, OP_DRAW, OP_INIT, OP_RESUME,
    OP_UPDATE, RET_ANIMATING, RET_NONE, RET_REDRAW,
};
use nc_abi::sys as sys_wire;
use nc_app_sdk::ui::{
    draw_app_tile, draw_bt_glyph, draw_card, draw_folder_tile, draw_pill_btn, draw_slider,
    draw_sun_glyph, draw_wifi_glyph, ease_out_cubic, mix_color, step_towards, Dp, Glyph, AMBER,
    BLUE, CARD, CORAL, CORAL_SOFT, DIVIDER, GLOW_GREEN, GLOW_VIOLET, GLOW_WARM, GREEN, INK, LABEL,
    MUTED, ORANGE, SHADOW, SUBTLE, SURFACE_BG, TRACK, WALL_BOTTOM, WALL_MID, WALL_TOP, WHITE,
};
use nc_app_sdk::{dispatch, pointer, sys, Gfx, Line, RingApp};

const GRID_COLS: usize = 4;
const GRID_ROWS: usize = 3;
const GRID_CELLS: usize = GRID_COLS * GRID_ROWS;
const WALL_BANDS: i32 = 24;

const STATUS_H_DP: i32 = 28;
const NAV_H_DP: i32 = 44;
const ANIM_STEP: i32 = 128; // 2 karede akıcı ve anlık kübik geçiş
const LONG_PRESS_MS: u32 = 340;

#[derive(Clone, Copy)]
struct AppDef {
    id: &'static str,
    label_tr: &'static str,
    label_en: &'static str,
    glyph: Glyph,
    color: u32,
    bit: u8,
    surface_id: u32,
}

const APPS: [AppDef; 2] = [
    AppDef {
        id: "nc.files",
        label_tr: "Dosyalar",
        label_en: "Files",
        glyph: Glyph::Files,
        color: ORANGE,
        bit: 1 << 0,
        surface_id: 1,
    },
    AppDef {
        id: "nc.settings",
        label_tr: "Ayarlar",
        label_en: "Settings",
        glyph: Glyph::Settings,
        color: BLUE,
        bit: 1 << 1,
        surface_id: 2,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Empty,
    App(u8),
    Folder([Option<u8>; 4]),
}

struct HomeShell {
    initialized: bool,
    stamp: i64,
    last_flags: u32,

    // Serbest 4x3 ızgara ve 4'lü klasör durumu
    grid: [Cell; GRID_CELLS],
    edit_mode: bool,
    selected_cell: Option<usize>,
    open_folder_cell: Option<usize>,

    // Animasyon durumları (0..=256) ve pencere boyutlandırma modu
    app_anim: i32,
    app_target: i32,
    active_surface: u32,
    windowed_mode: bool,

    nav_anim: i32,
    nav_target: i32,

    cc_anim: i32,
    cc_target: i32,

    recents_open: bool,
    recents_anim: i32,

    // Dokunma / Uzun basma / Yukarı kaydırma (Swipe-up) takibi
    was_down: bool,
    down_start_ms: u32,
    down_start_x: i32,
    down_start_y: i32,
    cur_x: i32,
    cur_y: i32,
    long_press_fired: bool,
    pressed_cell: i32,
    swipe_card_idx: i32,
    swipe_dy: i32,
}

fn wall_clock() -> i64 {
    sys::clock_get(1)
}

fn palette_for(preset: u8) -> (u32, u32, u32, u32, u32) {
    match preset {
        1 => (0x000F_172A, 0x001E_1B4B, 0x0031_1042, 0x0063_66F1, 0x00A8_55F7),
        2 => (0x007C_2D12, 0x009A_3412, 0x004C_1D95, 0x00FB_923C, 0x00F4_3F5E),
        3 => (0x0006_4E3B, 0x000F_766E, 0x001E_3A8A, 0x0034_D399, 0x0038_BDF8),
        _ => (WALL_TOP, WALL_MID, WALL_BOTTOM, GLOW_WARM, GLOW_GREEN),
    }
}

fn draw_wallpaper(gfx: &mut Gfx<'_>, ev: &EventBlock, dp: &Dp, with_glow: bool) {
    let (top, mid, bottom, glow_a, glow_b) = palette_for(unpack_wallpaper(ev.flags));
    let h = ev.area_h;
    for band in 0..WALL_BANDS {
        let y0 = h * band / WALL_BANDS;
        let y1 = h * (band + 1) / WALL_BANDS;
        let t = band * 1000 / (WALL_BANDS - 1);
        let color = if t < 500 {
            mix_color(top, mid, t, 500)
        } else {
            mix_color(mid, bottom, t - 500, 500)
        };
        gfx.fill_rect(0, y0, ev.area_w, y1 - y0 + 1, color);
    }
    if with_glow {
        gfx.dot(dp.x(75), h * 26 / 100, dp.d(64), glow_a, 120);
        gfx.dot(dp.x(220), h * 48 / 100, dp.d(72), glow_b, 130);
        gfx.dot(dp.x(290), h * 76 / 100, dp.d(76), GLOW_VIOLET, 140);
    }
}

impl HomeShell {
    const fn new() -> Self {
        let mut grid = [Cell::Empty; GRID_CELLS];
        grid[0] = Cell::App(0);
        grid[1] = Cell::App(1);
        Self {
            initialized: false,
            stamp: -1,
            last_flags: 0,
            grid,
            edit_mode: false,
            selected_cell: None,
            open_folder_cell: None,
            app_anim: 0,
            app_target: 0,
            active_surface: 0,
            windowed_mode: false,
            nav_anim: 0,
            nav_target: 0,
            cc_anim: 0,
            cc_target: 0,
            recents_open: false,
            recents_anim: 0,
            was_down: false,
            down_start_ms: 0,
            down_start_x: 0,
            down_start_y: 0,
            cur_x: 0,
            cur_y: 0,
            long_press_fired: false,
            pressed_cell: -1,
            swipe_card_idx: -1,
            swipe_dy: 0,
        }
    }

    fn sync_from_flags(&mut self, ev: &EventBlock) {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let _ = sys::wm_set_viewport(dp.d(STATUS_H_DP), dp.d(NAV_H_DP));

        let fg = unpack_fg_app(ev.flags);
        if fg != 0 {
            self.active_surface = fg as u32;
            self.app_target = 256;
            self.nav_target = 256;
            self.recents_open = false;
        } else {
            self.app_target = 0;
            self.nav_target = if self.recents_open { 256 } else { 0 };
        }
    }

    fn is_animating(&self) -> bool {
        let r_target = if self.recents_open { 256 } else { 0 };
        self.app_anim != self.app_target
            || self.nav_anim != self.nav_target
            || self.cc_anim != self.cc_target
            || self.recents_anim != r_target
    }

    fn step_animations(&mut self) -> bool {
        let (a, m1) = step_towards(self.app_anim, self.app_target, ANIM_STEP);
        self.app_anim = a;
        if self.app_anim == 0 && self.app_target == 0 {
            self.active_surface = 0;
        }

        let (n, m2) = step_towards(self.nav_anim, self.nav_target, ANIM_STEP);
        self.nav_anim = n;

        let (c, m3) = step_towards(self.cc_anim, self.cc_target, ANIM_STEP);
        self.cc_anim = c;

        let r_target = if self.recents_open { 256 } else { 0 };
        let (r, m4) = step_towards(self.recents_anim, r_target, ANIM_STEP);
        self.recents_anim = r;

        m1 || m2 || m3 || m4
    }

    fn app_surface_rect(&self, ev: &EventBlock, dp: &Dp) -> (i32, i32, i32, i32, i32) {
        let eased = ease_out_cubic(self.app_anim);
        let status_h = dp.d(STATUS_H_DP);
        let nav_h = (dp.d(NAV_H_DP) * ease_out_cubic(self.nav_anim)) / 256;
        let full_w = ev.area_w;
        let full_h = (ev.area_h - status_h - nav_h).max(160);

        let (target_w, target_h, target_r) = if self.windowed_mode {
            (full_w * 86 / 100, full_h * 82 / 100, dp.d(18))
        } else {
            (full_w, full_h, 0)
        };

        let w = target_w * (160 + 96 * eased / 256) / 256;
        let h = target_h * (160 + 96 * eased / 256) / 256;
        let x = (full_w - w) / 2;
        let base_y = status_h + (full_h - target_h) / 2;
        let y = base_y + (target_h - h) + (ev.area_h / 6) * (256 - eased) / 256;
        let radius = target_r.max(dp.d(22) * (256 - eased) / 256);
        (x, y, w.max(1), h.max(1), radius)
    }

    fn cell_rect(&self, cell_idx: usize, dp: &Dp) -> (i32, i32, i32) {
        let col = (cell_idx % GRID_COLS) as i32;
        let row = (cell_idx / GRID_COLS) as i32;
        let size = dp.d(56);
        let cell_w = 82;
        let x = dp.x(16) + dp.d(cell_w * col + cell_w / 2) - size / 2;
        let y = dp.d(172 + row * 88);
        (x, y, size)
    }

    fn hit_grid_cell(&self, x: i32, y: i32, dp: &Dp) -> Option<usize> {
        for idx in 0..GRID_CELLS {
            let (cx, cy, size) = self.cell_rect(idx, dp);
            let pad = dp.d(10);
            if x >= cx - pad && x < cx + size + pad && y >= cy - pad && y < cy + size + dp.d(22) {
                return Some(idx);
            }
        }
        None
    }

    fn dock_slot_rect(&self, idx: usize, dp: &Dp, area_h: i32) -> (i32, i32, i32) {
        let dock_y = area_h - dp.d(82);
        let size = dp.d(50);
        let x = if idx == 0 {
            dp.x(126) - size / 2
        } else {
            dp.x(234) - size / 2
        };
        (x, dock_y + dp.d(9), size)
    }

    fn move_or_group_cells(&mut self, from: usize, to: usize) {
        if from == to || from >= GRID_CELLS || to >= GRID_CELLS {
            return;
        }
        match (self.grid[from], self.grid[to]) {
            ( item, Cell::Empty ) => {
                self.grid[to] = item;
                self.grid[from] = Cell::Empty;
            }
            (Cell::App(a), Cell::App(b)) => {
                // İki uygulamayı 4'lü klasörde grupla
                self.grid[to] = Cell::Folder([Some(b), Some(a), None, None]);
                self.grid[from] = Cell::Empty;
            }
            (Cell::App(a), Cell::Folder(mut slots)) => {
                for slot in slots.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(a);
                        self.grid[from] = Cell::Empty;
                        break;
                    }
                }
                self.grid[to] = Cell::Folder(slots);
            }
            _ => {
                self.grid.swap(from, to);
            }
        }
    }

    fn ungroup_folder(&mut self, folder_cell: usize) {
        let Cell::Folder(slots) = self.grid[folder_cell] else { return };
        let mut first = true;
        for app_idx in slots.into_iter().flatten() {
            if first {
                self.grid[folder_cell] = Cell::App(app_idx);
                first = false;
            } else if let Some(empty) = self.grid.iter().position(|c| *c == Cell::Empty) {
                self.grid[empty] = Cell::App(app_idx);
            }
        }
        if first {
            self.grid[folder_cell] = Cell::Empty;
        }
        self.open_folder_cell = None;
    }

    fn launch_app_by_idx(&mut self, app_idx: usize) {
        if app_idx >= APPS.len() {
            return;
        }
        let app = &APPS[app_idx];
        self.open_folder_cell = None;
        self.edit_mode = false;
        self.selected_cell = None;
        self.recents_open = false;
        self.cc_target = 0;
        self.active_surface = app.surface_id;
        self.app_target = 256;
        self.nav_target = 256;
        sys::log("[home] aç: ");
        sys::log(app.id);
        sys::log("\n");
        let _ = sys::app_launch(app.id);
    }

    fn handle_control_center_click(&mut self, x: i32, y: i32, dp: &Dp, ev: &EventBlock) -> bool {
        // Kontrol Merkezi kartı (dp.x(16)..dp.x(344), dp.d(36)..dp.d(262))
        if y < dp.d(36) || y > dp.d(266) || x < dp.x(16) || x > dp.x(344) {
            self.cc_target = 0;
            return true;
        }
        // Wi-Fi kutusu (dp.x(28..174), dp.d(52..116))
        if y >= dp.d(50) && y < dp.d(118) && x >= dp.x(28) && x < dp.x(174) {
            let wifi = unpack_wifi(ev.flags);
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_WIFI, (!wifi) as u32);
            return true;
        }
        // Bluetooth kutusu (dp.x(186..332), dp.d(52..116))
        if y >= dp.d(50) && y < dp.d(118) && x >= dp.x(186) && x < dp.x(332) {
            let bt = unpack_bt(ev.flags);
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_BT, (!bt) as u32);
            return true;
        }
        // Parlaklık kaydırıcısı (dp.x(30..330), dp.d(146..196))
        if y >= dp.d(140) && y < dp.d(200) && x >= dp.x(30) && x < dp.x(330) {
            let rel = (x - dp.x(56)).clamp(0, dp.d(248));
            let pct = (20 + rel * 80 / dp.d(248).max(1)).clamp(20, 100) as u32;
            let rounded = ((pct + 5) / 10) * 10;
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_BRIGHTNESS, rounded);
            return true;
        }
        // Tüm Ayarlar düğmesi (dp.y(210..250))
        if y >= dp.d(208) && y < dp.d(254) && x >= dp.x(28) && x < dp.x(332) {
            self.cc_target = 0;
            self.launch_app_by_idx(1);
            return true;
        }
        true
    }

    fn handle_nav_bar_click(&mut self, x: i32, dp: &Dp, ev: &EventBlock) {
        let third = dp.width / 3;
        let rel_x = x - dp.origin_x;
        if rel_x < third {
            // Geri (<)
            if self.recents_open {
                self.recents_open = false;
                if unpack_fg_app(ev.flags) == 0 {
                    self.nav_target = 0;
                }
            } else if unpack_fg_app(ev.flags) != 0 {
                let _ = sys::wm_send_back();
            }
        } else if rel_x < third * 2 {
            // Ana Ekran (O)
            self.recents_open = false;
            self.cc_target = 0;
            self.open_folder_cell = None;
            self.edit_mode = false;
            self.selected_cell = None;
            if unpack_fg_app(ev.flags) != 0 {
                let _ = sys::wm_pause_to_home();
            }
            self.app_target = 0;
            self.nav_target = 0;
        } else {
            // Son Uygulamalar (□)
            if unpack_fg_app(ev.flags) != 0 {
                let _ = sys::wm_pause_to_home();
                self.app_target = 0;
            }
            self.recents_open = !self.recents_open;
            self.nav_target = if self.recents_open { 256 } else { 0 };
        }
    }

    fn recent_card_rect(&self, order_idx: usize, dp: &Dp) -> (i32, i32, i32, i32) {
        let y = dp.d(92 + (order_idx as i32) * 156);
        (dp.x(24), y, dp.d(312), dp.d(140))
    }

    fn handle_recents_pointer(&mut self, x: i32, y: i32, down: bool, clicked: bool, dp: &Dp, ev: &EventBlock) {
        let running = unpack_running_mask(ev.flags);
        let mut active_indices = [0usize; 2];
        let mut count = 0usize;
        for (i, app) in APPS.iter().enumerate() {
            if running & app.bit != 0 {
                active_indices[count] = i;
                count += 1;
            }
        }

        if clicked {
            // Tümünü Temizle hapı (dp.y(414..450))
            if count > 0 && y >= dp.d(410) && y < dp.d(454) && x >= dp.x(90) && x < dp.x(270) {
                for &idx in &active_indices[..count] {
                    let _ = sys::app_stop(APPS[idx].id);
                }
                self.recents_open = false;
                self.nav_target = 0;
                return;
            }
            self.swipe_card_idx = -1;
            self.swipe_dy = 0;
            for (ord, &app_idx) in active_indices[..count].iter().enumerate() {
                let (cx, cy, cw, ch) = self.recent_card_rect(ord, dp);
                // Sağ üst Kapat (Yukarı ok / X) hapı
                if x >= cx + cw - dp.d(72) && x < cx + cw && y >= cy && y < cy + dp.d(38) {
                    let _ = sys::app_stop(APPS[app_idx].id);
                    return;
                }
                if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                    self.swipe_card_idx = app_idx as i32;
                    break;
                }
            }
        } else if down && self.swipe_card_idx >= 0 {
            self.swipe_dy = (y - self.down_start_y).min(dp.d(20));
        } else if !down && self.swipe_card_idx >= 0 {
            let idx = self.swipe_card_idx as usize;
            let dy = self.swipe_dy;
            self.swipe_card_idx = -1;
            self.swipe_dy = 0;
            if dy < -dp.d(32) {
                // Yukarı kaydırıldı -> arka plandan kaldır ve RAM'i boşalt!
                let _ = sys::app_stop(APPS[idx].id);
            } else if dy.abs() < dp.d(12) {
                // Dokunuldu -> uygulamayı ön plana getir!
                self.launch_app_by_idx(idx);
            }
        }
    }

    fn handle_pointer(&mut self, ev: &EventBlock) -> bool {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let status_h = dp.d(STATUS_H_DP);
        let nav_h = dp.d(NAV_H_DP);

        let Some(p) = pointer(ev) else {
            if self.was_down && !self.long_press_fired && self.pressed_cell >= 0 {
                let cell = self.pressed_cell as usize;
                self.pressed_cell = -1;
                self.on_grid_cell_tap(cell, ev);
                self.was_down = false;
                return true;
            }
            self.was_down = false;
            self.pressed_cell = -1;
            return false;
        };

        self.cur_x = p.x;
        self.cur_y = p.y;

        if p.clicked {
            self.down_start_ms = ev.uptime_ms;
            self.down_start_x = p.x;
            self.down_start_y = p.y;
            self.long_press_fired = false;
            self.was_down = p.down;

            // 1) Üst Durum Çubuğu -> Pencere Boyutu Değiştir veya Kontrol Merkezi aç/kapat
            if p.y < status_h {
                if self.app_target > 0 && !self.recents_open && p.x >= dp.x(198) && p.x < dp.x(262) {
                    self.windowed_mode = !self.windowed_mode;
                    return true;
                }
                self.cc_target = if self.cc_target == 0 { 256 } else { 0 };
                return true;
            }

            // 2) Kontrol Merkezi açıksa
            if self.cc_target > 0 {
                return self.handle_control_center_click(p.x, p.y, &dp, ev);
            }

            // 3) Alt Gezinme Çubuğu görünürse ve oraya dokunulduysa
            if self.nav_target > 0 && p.y >= ev.area_h - nav_h {
                self.handle_nav_bar_click(p.x, &dp, ev);
                return true;
            }

            // 4) Son Uygulamalar ekranı açıksa
            if self.recents_open {
                self.handle_recents_pointer(p.x, p.y, p.down, true, &dp, ev);
                return true;
            }

            // 5) Ön planda alt uygulama açıksa -> dokunmayı alt uygulama pencere koordinatlarına ölçekleyerek ilet
            if unpack_fg_app(ev.flags) != 0 && self.app_target > 0 {
                let (wx, wy, ww, wh, _) = self.app_surface_rect(ev, &dp);
                if p.x >= wx && p.x < wx + ww && p.y >= wy && p.y < wy + wh {
                    let full_w = ev.area_w.max(1);
                    let full_h = (ev.area_h - status_h - nav_h).max(160);
                    let child_x = ((p.x - wx) as i64 * full_w as i64 / ww.max(1) as i64) as i32;
                    let child_y = ((p.y - wy) as i64 * full_h as i64 / wh.max(1) as i64) as i32;
                    let _ = sys::wm_forward_touch(child_x.max(0), child_y.max(0), ev.pointer_flags);
                    return false;
                }
            }

            // 6) Klasör penceresi açıksa
            if let Some(fcell) = self.open_folder_cell {
                if self.handle_folder_modal_click(fcell, p.x, p.y, &dp) {
                    return true;
                }
            }

            // 7) Ana ekran Widget'ları ve Düzenle hapı (y: 40..162 dp)
            if p.y >= dp.d(36) && p.y < dp.d(164) {
                // Sağ üst "Düzenle / Bitti" hapı (dp.x(266..344), dp.d(36..66))
                if p.x >= dp.x(260) && p.x < dp.x(348) && p.y < dp.d(68) {
                    self.edit_mode = !self.edit_mode;
                    self.selected_cell = None;
                    return true;
                }
                // Sol Saat/Bağlantı Widget'ı -> Kontrol Merkezi aç
                if p.x >= dp.x(16) && p.x < dp.x(176) && p.y >= dp.d(68) {
                    self.cc_target = 256;
                    return true;
                }
                // Sağ Depolama Widget'ı -> Dosyalar aç
                if p.x >= dp.x(184) && p.x < dp.x(344) && p.y >= dp.d(68) {
                    self.launch_app_by_idx(0);
                    return true;
                }
            }

            // 8) Alt Dock simgeleri
            let installed = unpack_installed_mask(ev.flags);
            for (idx, app) in APPS.iter().enumerate() {
                if installed & app.bit == 0 {
                    continue;
                }
                let (dx, dy, ds) = self.dock_slot_rect(idx, &dp, ev.area_h);
                if p.x >= dx && p.x < dx + ds && p.y >= dy && p.y < dy + ds {
                    self.launch_app_by_idx(idx);
                    return true;
                }
            }

            // 9) Ana Izgara hücreleri
            if let Some(cell) = self.hit_grid_cell(p.x, p.y, &dp) {
                if !p.down {
                    // Tek tıklama (bas-bırak aynı olayda geldiyse)
                    self.on_grid_cell_tap(cell, ev);
                } else {
                    self.pressed_cell = cell as i32;
                }
                return true;
            }
            return false;
        }

        // Basılı tutulurken veya bırakılırken (drag / swipe / long-press)
        if self.recents_open {
            self.handle_recents_pointer(p.x, p.y, p.down, false, &dp, ev);
            self.was_down = p.down;
            return true;
        }

        if p.down {
            self.was_down = true;
            if !self.long_press_fired
                && self.pressed_cell >= 0
                && ev.uptime_ms.wrapping_sub(self.down_start_ms) >= LONG_PRESS_MS
            {
                let cell = self.pressed_cell as usize;
                if self.grid[cell] != Cell::Empty {
                    self.long_press_fired = true;
                    self.edit_mode = true;
                    self.selected_cell = Some(cell);
                    return true;
                }
            }
        } else if self.was_down {
            self.was_down = false;
            if self.pressed_cell >= 0 {
                let start_cell = self.pressed_cell as usize;
                self.pressed_cell = -1;
                if let Some(drop_cell) = self.hit_grid_cell(p.x, p.y, &dp) {
                    if self.long_press_fired && drop_cell != start_cell {
                        self.move_or_group_cells(start_cell, drop_cell);
                        self.selected_cell = Some(drop_cell);
                        return true;
                    }
                }
                if !self.long_press_fired {
                    self.on_grid_cell_tap(start_cell, ev);
                    return true;
                }
            }
        }

        false
    }

    fn on_grid_cell_tap(&mut self, cell: usize, _ev: &EventBlock) {
        if self.edit_mode {
            match self.selected_cell {
                None => {
                    if self.grid[cell] != Cell::Empty {
                        self.selected_cell = Some(cell);
                    }
                }
                Some(src) if src == cell => {
                    self.selected_cell = None;
                }
                Some(src) => {
                    self.move_or_group_cells(src, cell);
                    self.selected_cell = None;
                }
            }
            return;
        }

        match self.grid[cell] {
            Cell::Empty => {}
            Cell::App(app_idx) => self.launch_app_by_idx(app_idx as usize),
            Cell::Folder(_) => {
                self.open_folder_cell = Some(cell);
            }
        }
    }

    fn handle_folder_modal_click(&mut self, fcell: usize, x: i32, y: i32, dp: &Dp) -> bool {
        let mx = dp.x(44);
        let my = dp.d(160);
        let mw = dp.d(272);
        let mh = dp.d(244);
        if x < mx || x >= mx + mw || y < my || y >= my + mh {
            self.open_folder_cell = None;
            return true;
        }
        // Grubu Çöz düğmesi (my + dp.d(194..230))
        if y >= my + dp.d(190) && y < my + dp.d(234) {
            self.ungroup_folder(fcell);
            return true;
        }
        let Cell::Folder(slots) = self.grid[fcell] else {
            self.open_folder_cell = None;
            return true;
        };
        let size = dp.d(56);
        for (i, slot) in slots.iter().enumerate() {
            let Some(app_idx) = *slot else { continue };
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            let ix = dp.x(96 + col * 112);
            let iy = my + dp.d(42 + row * 74);
            if x >= ix && x < ix + size && y >= iy && y < iy + size + dp.d(18) {
                self.launch_app_by_idx(app_idx as usize);
                return true;
            }
        }
        true
    }

    fn draw_status_bar(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let on_light = self.app_anim > 128 && !self.recents_open && !self.windowed_mode;
        let fg = if on_light { INK } else { LABEL };
        let tz_hours = unpack_tz_hours(ev.flags);
        if let Some(local) = from_unix_tz(wall_clock(), tz_hours) {
            let mut time = *b"--:--";
            format_time(&local, &mut time);
            if let Ok(text) = core::str::from_utf8(&time) {
                gfx.text(dp.x(20), dp.d(9), dp.d(8), fg, 3, text);
            }
        }

        // Ön planda uygulama açıkken pencere boyutlandırma hapı (Tam Ekran / Pencere)
        if self.app_anim > 64 && !self.recents_open {
            let en = ev.lang == 1;
            let r_fill = if self.windowed_mode { ev.accent } else if on_light { TRACK } else { WHITE };
            let r_alpha = if self.windowed_mode { 255 } else if on_light { 220 } else { 75 };
            let r_fg = if self.windowed_mode { WHITE } else if on_light { INK } else { WHITE };
            gfx.panel(dp.x(202), dp.d(5), dp.d(58), dp.d(18), dp.d(9), r_fill, r_alpha, 0, 0);
            let lbl = if self.windowed_mode {
                if en { "Full" } else { "Tam Ekran" }
            } else if en {
                "Window"
            } else {
                "Pencere"
            };
            gfx.text_centered(dp.x(231), dp.d(10), dp.d(6), r_fg, 1, lbl);
        }

        // Sağ üst durum hapı (Wi-Fi, Bluetooth, Parlaklık)
        let pill_fill = if on_light { TRACK } else { WHITE };
        let pill_alpha = if on_light { 220 } else { 55 };
        gfx.panel(dp.x(266), dp.d(5), dp.d(78), dp.d(18), dp.d(9), pill_fill, pill_alpha, 0, 0);

        let wifi_col = if unpack_wifi(ev.flags) {
            if on_light { ev.accent } else { WHITE }
        } else {
            SUBTLE
        };
        draw_wifi_glyph(gfx, dp.x(282), dp.d(14), dp.d(5), wifi_col);

        let bt_col = if unpack_bt(ev.flags) {
            if on_light { ev.accent } else { WHITE }
        } else {
            SUBTLE
        };
        draw_bt_glyph(gfx, dp.x(305), dp.d(14), dp.d(5), bt_col);

        draw_sun_glyph(gfx, dp.x(328), dp.d(14), dp.d(5), if on_light { AMBER } else { WHITE });
    }

    fn draw_widgets(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = ev.lang == 1;
        let tz_hours = unpack_tz_hours(ev.flags);

        // Düzenleme modu başlığı ve hapı
        let edit_lbl = if self.edit_mode {
            if en { "Done" } else { "Bitti" }
        } else if en {
            "Edit"
        } else {
            "Düzenle"
        };
        let edit_bg = if self.edit_mode { ev.accent } else { WHITE };
        let edit_a = if self.edit_mode { 255 } else { 75 };
        gfx.panel(dp.x(274), dp.d(38), dp.d(70), dp.d(22), dp.d(11), edit_bg, edit_a, WHITE, 90);
        gfx.text_centered(dp.x(309), dp.d(45), dp.d(7), WHITE, 2, edit_lbl);

        if self.edit_mode {
            gfx.text(
                dp.x(20),
                dp.d(45),
                dp.d(7),
                LABEL,
                1,
                if en {
                    "Tap an icon, then tap any cell or app to group"
                } else {
                    "Simge seçip boş hücreye taşıyın veya klasörleyin"
                },
            );
        }

        // Widget 1: Saat, Tarih ve Bağlantı Özeti (Sol cam kart)
        gfx.panel(dp.x(16), dp.d(68), dp.d(158), dp.d(86), dp.d(20), WHITE, 85, WHITE, 130);
        if let Some(local) = from_unix_tz(wall_clock(), tz_hours) {
            let mut time = *b"--:--";
            format_time(&local, &mut time);
            if let Ok(text) = core::str::from_utf8(&time) {
                gfx.text(dp.x(28), dp.d(80), dp.d(16), WHITE, 2, text);
            }
            let mut date_line = Line::new();
            let m_idx = (local.month.saturating_sub(1) as usize).min(11);
            date_line.number(local.day as u32, 1).text(" ").text(MONTHS[m_idx]);
            gfx.text(dp.x(28), dp.d(106), dp.d(7), LABEL, 1, date_line.as_str());
        }
        let mut net_line = Line::new();
        net_line
            .text("Wi-Fi: ")
            .text(if unpack_wifi(ev.flags) {
                if en { "On" } else { "Açık" }
            } else if en {
                "Off"
            } else {
                "Kapalı"
            })
            .text("  •  BT: ")
            .text(if unpack_bt(ev.flags) {
                if en { "On" } else { "Açık" }
            } else if en {
                "Off"
            } else {
                "Kapalı"
            });
        gfx.text(dp.x(28), dp.d(132), dp.d(7), LABEL, 1, net_line.as_str());

        // Widget 2: Depolama ve Hızlı Durum (Sağ cam kart)
        gfx.panel(dp.x(186), dp.d(68), dp.d(158), dp.d(86), dp.d(20), WHITE, 85, WHITE, 130);
        gfx.text(
            dp.x(198),
            dp.d(82),
            dp.d(8),
            WHITE,
            2,
            if en { "Storage & Tasks" } else { "Depolama & Görev" },
        );
        gfx.text(dp.x(198), dp.d(102), dp.d(13), WHITE, 2, "32 GB");
        let running = unpack_running_mask(ev.flags).count_ones();
        let mut run_line = Line::new();
        run_line
            .number(running, 1)
            .text(if en { " active in background" } else { " uygulama arka planda" });
        gfx.text(dp.x(198), dp.d(132), dp.d(7), LABEL, 1, run_line.as_str());
    }

    fn draw_home_grid(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = ev.lang == 1;
        let installed = unpack_installed_mask(ev.flags);
        let running = unpack_running_mask(ev.flags);

        for idx in 0..GRID_CELLS {
            let (cx, cy, size) = self.cell_rect(idx, dp);
            let is_sel = self.selected_cell == Some(idx);

            if self.edit_mode {
                let border_col = if is_sel { ev.accent } else { WHITE };
                let border_a = if is_sel { 255 } else { 75 };
                gfx.panel(
                    cx - dp.d(4),
                    cy - dp.d(4),
                    size + dp.d(8),
                    size + dp.d(8),
                    dp.d(18),
                    WHITE,
                    if is_sel { 65 } else { 20 },
                    border_col,
                    border_a,
                );
            }

            match self.grid[idx] {
                Cell::Empty => {}
                Cell::App(app_idx) => {
                    let Some(app) = APPS.get(app_idx as usize) else { continue };
                    if installed & app.bit == 0 {
                        continue;
                    }
                    let color = if app.id == "nc.settings" { ev.accent } else { app.color };
                    draw_app_tile(gfx, app.glyph, cx, cy, size, color, self.pressed_cell == idx as i32 || is_sel);
                    let lbl = if en { app.label_en } else { app.label_tr };
                    gfx.text_centered(cx + size / 2, cy + size + dp.d(7), dp.d(8), LABEL, 2, lbl);
                    if running & app.bit != 0 {
                        gfx.dot(cx + size / 2, cy + size + dp.d(19), dp.d(3).max(2), GREEN, 1024);
                    }
                }
                Cell::Folder(slots) => {
                    let mut visuals: [Option<(Glyph, u32)>; 4] = [None; 4];
                    for (s_i, slot) in slots.iter().enumerate() {
                        if let Some(a_idx) = *slot {
                            if let Some(app) = APPS.get(a_idx as usize) {
                                let col = if app.id == "nc.settings" { ev.accent } else { app.color };
                                visuals[s_i] = Some((app.glyph, col));
                            }
                        }
                    }
                    draw_folder_tile(gfx, cx, cy, size, &visuals, is_sel);
                    gfx.text_centered(
                        cx + size / 2,
                        cy + size + dp.d(7),
                        dp.d(8),
                        LABEL,
                        2,
                        if en { "Folder" } else { "Klasör" },
                    );
                }
            }
        }

        // Alt cam Dock
        let dock_y = ev.area_h - dp.d(82);
        gfx.panel(dp.x(26), dock_y, dp.d(308), dp.d(68), dp.d(26), WHITE, 88, WHITE, 125);
        for (idx, app) in APPS.iter().enumerate() {
            if installed & app.bit == 0 {
                continue;
            }
            let (dx, dy, ds) = self.dock_slot_rect(idx, dp, ev.area_h);
            let color = if app.id == "nc.settings" { ev.accent } else { app.color };
            draw_app_tile(gfx, app.glyph, dx, dy, ds, color, false);
            if running & app.bit != 0 {
                gfx.dot(dx + ds / 2, dy + ds + dp.d(5), dp.d(3).max(2), GREEN, 1024);
            }
        }
    }

    fn draw_folder_modal(&self, fcell: usize, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let Cell::Folder(slots) = self.grid[fcell] else { return };
        let en = ev.lang == 1;
        let mx = dp.x(44);
        let my = dp.d(160);
        let mw = dp.d(272);
        let mh = dp.d(244);
        draw_card(gfx, mx, my, mw, mh, dp.d(26));
        gfx.text_centered(dp.x(180), my + dp.d(16), dp.d(10), INK, 2, if en { "Folder (4 Slots)" } else { "Klasör (4 Yuva)" });

        let size = dp.d(54);
        for (i, slot) in slots.iter().enumerate() {
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            let ix = dp.x(96 + col * 112);
            let iy = my + dp.d(42 + row * 74);
            if let Some(app_idx) = *slot {
                if let Some(app) = APPS.get(app_idx as usize) {
                    let color = if app.id == "nc.settings" { ev.accent } else { app.color };
                    draw_app_tile(gfx, app.glyph, ix, iy, size, color, false);
                    gfx.text_centered(
                        ix + size / 2,
                        iy + size + dp.d(6),
                        dp.d(7),
                        INK,
                        1,
                        if en { app.label_en } else { app.label_tr },
                    );
                }
            } else {
                gfx.panel(ix, iy, size, size, dp.d(15), TRACK, 150, 0, 0);
            }
        }

        draw_pill_btn(
            gfx,
            dp.x(86),
            my + dp.d(196),
            dp.d(188),
            dp.d(32),
            TRACK,
            INK,
            dp.d(8),
            if en { "Ungroup Folder" } else { "Klasörü Çöz" },
        );
    }

    fn draw_control_center(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        if self.cc_anim <= 0 {
            return;
        }
        let en = ev.lang == 1;
        let eased = ease_out_cubic(self.cc_anim);
        let card_h = dp.d(228);
        let cy = dp.d(36) - (card_h * (256 - eased) / 256);

        draw_card(gfx, dp.x(16), cy, dp.d(328), card_h, dp.d(24));

        // Wi-Fi Düğmesi
        let wifi = unpack_wifi(ev.flags);
        let wf_bg = if wifi { ev.accent } else { TRACK };
        let wf_fg = if wifi { WHITE } else { INK };
        gfx.panel(dp.x(28), cy + dp.d(16), dp.d(146), dp.d(62), dp.d(18), wf_bg, 255, 0, 0);
        draw_wifi_glyph(gfx, dp.x(50), cy + dp.d(46), dp.d(8), wf_fg);
        gfx.text(dp.x(68), cy + dp.d(32), dp.d(9), wf_fg, 2, "Wi-Fi");
        gfx.text(
            dp.x(68),
            cy + dp.d(50),
            dp.d(7),
            if wifi { WHITE } else { MUTED },
            1,
            if wifi {
                if en { "Connected" } else { "Bağlı" }
            } else if en {
                "Off"
            } else {
                "Kapalı"
            },
        );

        // Bluetooth Düğmesi
        let bt = unpack_bt(ev.flags);
        let bt_bg = if bt { ev.accent } else { TRACK };
        let bt_fg = if bt { WHITE } else { INK };
        gfx.panel(dp.x(186), cy + dp.d(16), dp.d(146), dp.d(62), dp.d(18), bt_bg, 255, 0, 0);
        draw_bt_glyph(gfx, dp.x(208), cy + dp.d(46), dp.d(8), bt_fg);
        gfx.text(dp.x(226), cy + dp.d(32), dp.d(9), bt_fg, 2, "Bluetooth");
        gfx.text(
            dp.x(226),
            cy + dp.d(50),
            dp.d(7),
            if bt { WHITE } else { MUTED },
            1,
            if bt {
                if en { "On" } else { "Açık" }
            } else if en {
                "Off"
            } else {
                "Kapalı"
            },
        );

        // Parlaklık Kaydırıcısı
        let br = unpack_brightness(ev.flags);
        gfx.panel(dp.x(28), cy + dp.d(92), dp.d(304), dp.d(68), dp.d(18), SURFACE_BG, 255, 0, 0);
        draw_sun_glyph(gfx, dp.x(46), cy + dp.d(126), dp.d(8), AMBER);
        let mut br_lbl = Line::new();
        br_lbl
            .text(if en { "Brightness  " } else { "Parlaklık  %" })
            .number(br as u32, 1);
        if en {
            br_lbl.text("%");
        }
        gfx.text(dp.x(64), cy + dp.d(104), dp.d(8), INK, 2, br_lbl.as_str());
        draw_slider(gfx, dp.x(64), cy + dp.d(124), dp.d(252), dp.d(18), br, ev.accent);

        // Ayarlar bağlantısı
        draw_pill_btn(
            gfx,
            dp.x(28),
            cy + dp.d(174),
            dp.d(304),
            dp.d(36),
            TRACK,
            INK,
            dp.d(8),
            if en { "Open System Settings" } else { "Sistem Ayarlarını Aç" },
        );
    }

    fn draw_recents(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = ev.lang == 1;
        let running = unpack_running_mask(ev.flags);
        gfx.panel(0, 0, ev.area_w, ev.area_h, 0, SHADOW, 110, 0, 0);

        gfx.text_centered(
            dp.x(180),
            dp.d(48),
            dp.d(12),
            WHITE,
            2,
            if en { "Recent Applications" } else { "Son Uygulamalar" },
        );
        gfx.text_centered(
            dp.x(180),
            dp.d(68),
            dp.d(7),
            LABEL,
            1,
            if en {
                "Swipe card upward to close"
            } else {
                "Kapatmak için kartı yukarı kaydırın"
            },
        );

        let mut count = 0usize;
        for (idx, app) in APPS.iter().enumerate() {
            if running & app.bit == 0 {
                continue;
            }
            let (cx, base_cy, cw, ch) = self.recent_card_rect(count, dp);
            let dy = if self.swipe_card_idx == idx as i32 { self.swipe_dy } else { 0 };
            let cy = base_cy + dy;

            draw_card(gfx, cx, cy, cw, ch, dp.d(20));

            // Canlı uygulama yüzeyi minyatürü + üst başlık şeridi
            gfx.embed_surface(
                app.surface_id,
                cx + dp.d(8),
                cy + dp.d(40),
                cw - dp.d(16),
                ch - dp.d(48),
            );
            gfx.panel(cx, cy, cw, dp.d(40), dp.d(20), CARD, 255, DIVIDER, 255);

            let icon_s = dp.d(26);
            let col = if app.id == "nc.settings" { ev.accent } else { app.color };
            draw_app_tile(gfx, app.glyph, cx + dp.d(12), cy + dp.d(7), icon_s, col, false);
            gfx.text(
                cx + dp.d(46),
                cy + dp.d(15),
                dp.d(9),
                INK,
                2,
                if en { app.label_en } else { app.label_tr },
            );

            draw_pill_btn(
                gfx,
                cx + cw - dp.d(68),
                cy + dp.d(8),
                dp.d(56),
                dp.d(24),
                CORAL_SOFT,
                CORAL,
                dp.d(7),
                if en { "Close" } else { "Kapat" },
            );
            count += 1;
        }

        if count == 0 {
            gfx.panel(dp.x(48), dp.d(196), dp.d(264), dp.d(96), dp.d(22), WHITE, 95, WHITE, 140);
            gfx.text_centered(
                dp.x(180),
                dp.d(238),
                dp.d(9),
                WHITE,
                2,
                if en { "No background apps" } else { "Arka planda açık uygulama yok" },
            );
        } else {
            draw_pill_btn(
                gfx,
                dp.x(100),
                dp.d(416),
                dp.d(160),
                dp.d(34),
                WHITE,
                INK,
                dp.d(8),
                if en { "Clear All" } else { "Tümünü Temizle" },
            );
        }
    }

    fn draw_animated_app_surface(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        if self.app_anim <= 0 || self.active_surface == 0 {
            return;
        }
        let (x, y, w, h, radius) = self.app_surface_rect(ev, dp);
        gfx.panel(x, y, w, h, radius, SURFACE_BG, 255, 0, 0);
        gfx.embed_surface(self.active_surface, x, y, w, h);
    }

    fn draw_animated_nav_bar(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        if self.nav_anim <= 0 {
            return;
        }
        let eased = ease_out_cubic(self.nav_anim);
        let nav_h = dp.d(NAV_H_DP);
        let top = ev.area_h - (nav_h * eased / 256);
        let cy = top + nav_h / 2;

        gfx.fill_rect(0, top, ev.area_w, nav_h + 2, SURFACE_BG);
        gfx.fill_rect(0, top, ev.area_w, 1, DIVIDER);

        let third = dp.width / 3;
        let ox = dp.origin_x;
        let ink = MUTED;

        // 1) Geri düğmesi (yumuşak gezinme oku)
        let bx = ox + third / 2;
        let arm = dp.d(5).max(4);
        for step in 0..=arm {
            gfx.fill_rect(bx - arm / 2 + step, cy - step, 2, 2, ink);
            gfx.fill_rect(bx - arm / 2 + step, cy + step - 1, 2, 2, ink);
        }

        // 2) Ana Ekran düğmesi (vurgulu halkalı merkez nokta)
        let hx = ox + third + third / 2;
        let r = dp.d(9).max(7);
        gfx.dot(hx, cy, r, ev.accent, 1024);
        gfx.dot(hx, cy, (r - 3).max(4), SURFACE_BG, 1024);
        gfx.dot(hx, cy, (r / 2).max(3), ev.accent, 1024);

        // 3) Son Uygulamalar düğmesi (yuvarlatılmış kare)
        let rx = ox + third * 2 + third / 2;
        let side = dp.d(15).max(12);
        let col = if self.recents_open { ev.accent } else { ink };
        gfx.panel(rx - side / 2, cy - side / 2, side, side, dp.d(4).max(3), SURFACE_BG, 255, col, 255);
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let dp = Dp::new(ev.area_w, ev.area_h);

        // Tam ekran uygulama çizilirken arka planı gereksiz yere çizme (iş bütçesi tasarrufu)
        if self.app_anim < 256 || self.recents_open || self.windowed_mode {
            draw_wallpaper(gfx, ev, &dp, !self.is_animating());
            if !self.recents_open && self.app_anim < 256 {
                self.draw_widgets(ev, gfx, &dp);
                self.draw_home_grid(ev, gfx, &dp);
                if let Some(fcell) = self.open_folder_cell {
                    self.draw_folder_modal(fcell, ev, gfx, &dp);
                }
            }
        } else {
            gfx.fill_rect(0, 0, ev.area_w, dp.d(STATUS_H_DP), SURFACE_BG);
        }

        if !self.recents_open {
            self.draw_animated_app_surface(ev, gfx, &dp);
        } else {
            self.draw_recents(ev, gfx, &dp);
        }

        self.draw_status_bar(ev, gfx, &dp);
        self.draw_control_center(ev, gfx, &dp);
        self.draw_animated_nav_bar(ev, gfx, &dp);
    }
}

impl RingApp for HomeShell {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.initialized = true;
                self.sync_from_flags(ev);
                self.last_flags = ev.flags;
                let moving = self.step_animations();
                if moving {
                    RET_REDRAW | RET_ANIMATING
                } else {
                    RET_REDRAW
                }
            }
            OP_UPDATE => {
                let mut redraw = false;
                let stamp = wall_clock() / 60;
                if stamp != self.stamp || ev.flags != self.last_flags {
                    self.stamp = stamp;
                    self.last_flags = ev.flags;
                    self.sync_from_flags(ev);
                    redraw = true;
                }
                if self.handle_pointer(ev) {
                    redraw = true;
                }
                let moving = self.step_animations();
                if moving {
                    RET_REDRAW | RET_ANIMATING
                } else if redraw {
                    RET_REDRAW
                } else {
                    RET_NONE
                }
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                if self.is_animating() {
                    RET_ANIMATING
                } else {
                    RET_NONE
                }
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: HomeShell = HomeShell::new();
static mut COMMANDS: [u8; 16384] = [0; 16384];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
