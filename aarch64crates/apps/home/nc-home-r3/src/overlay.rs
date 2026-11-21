use nc_abi::draw::ICON_SETTINGS;
use nc_abi::event::{
    unpack_brightness, unpack_dark_theme, unpack_tz_hours, unpack_wallpaper, EventBlock,
};
use nc_app_sdk::status::{CONN_CONNECTED, CONN_CONNECTING, CONN_DISCONNECTED, CONN_LIMITED, CONN_UNAVAILABLE};
use nc_app_sdk::widget::WidgetSize;
use nc_app_sdk::{apps, Gfx, Line};
use nc_mobile_ui::{
    draw_airplane_glyph, draw_battery_icon, draw_bt_glyph, draw_cellular_bars, draw_dnd_glyph,
    draw_plus_glyph, draw_sun_glyph, draw_theme_glyph, draw_volume_glyph, draw_wifi_status_icon,
    GlassTheme, Palette, CORAL, WHITE,
};

use crate::aero::*;
use crate::clock::*;
use crate::layout::{Geo, R};
use crate::model::*;
use crate::motion::lerp;
use crate::shell::{HomeShell, Kind};
use crate::view::{tr, wall_clock};
use crate::widgets::{provider_meta, PROVIDER_COUNT};

fn date_line(ev: &EventBlock, out: &mut Line) -> bool {
    let Some(l) = from_unix_tz(wall_clock(), unpack_tz_hours(ev.flags)) else { return false };
    let en = ev.lang == 1;
    let day = if en { DAYS_EN[l.weekday as usize % 7] } else { DAYS[l.weekday as usize % 7] };
    let mi = (l.month.saturating_sub(1) as usize).min(11);
    out.text(day).text(", ").number(l.day as u32, 1).text(" ").text(if en { MONTHS_EN[mi] } else { MONTHS[mi] });
    true
}

pub fn lock_keypad_rect(geo: &Geo, idx: usize) -> R {
    let d = &geo.dp;
    let col = (idx % 3) as i32;
    let row = (idx / 3) as i32;
    let bw = d.d(68);
    let bh = d.d(52);
    let gap = d.d(14);
    let total_w = bw * 3 + gap * 2;
    let x0 = (geo.w - total_w) / 2;
    let y0 = (geo.h - d.d(280)).max(geo.status_h + d.d(170));
    R::new(x0 + col * (bw + gap), y0 + row * (bh + d.d(12)), bw, bh)
}

impl HomeShell {
    fn cc_pressed(&self, r: R) -> bool {
        self.is_down && self.gest.kind == Kind::Cc && self.gest.base == 0 && r.contains(self.gest.sx, self.gest.sy)
    }

    pub fn draw_lock_screen(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let d = &geo.dp;
        let en = ev.lang == 1;
        g.panel(0, 0, geo.w, geo.h, 0, DEEP, 212, 0, 0);

        if self.lock_show_info {
            if let Some(l) = from_unix_tz(wall_clock(), unpack_tz_hours(ev.flags)) {
                let mut s = *b"--:--";
                format_time(&l, &mut s);
                if let Ok(text) = core::str::from_utf8(&s) {
                    g.text_centered(geo.w / 2, geo.status_h + d.d(26), d.d(36), WHITE, 3, text);
                }
            }
            let mut dl = Line::new();
            if date_line(ev, &mut dl) {
                g.text_centered(geo.w / 2, geo.status_h + d.d(74), d.d(10), SOFT_WHITE, 2, dl.as_str());
            }
        }

        let prompt_y = geo.status_h + d.d(104);
        if self.lock_lockout_s > 0 {
            let mut l = Line::new();
            l.text(if en { "Locked out: wait " } else { "Cok fazla deneme: " })
                .number(self.lock_lockout_s, 1)
                .text("s");
            g.text_centered(geo.w / 2, prompt_y, d.d(9), CORAL, 2, l.as_str());
        } else if self.lock_err {
            g.text_centered(
                geo.w / 2,
                prompt_y,
                d.d(9),
                CORAL,
                2,
                if en { "Incorrect PIN / Password" } else { "Hatali PIN / Parola" },
            );
        } else {
            g.text_centered(
                geo.w / 2,
                prompt_y,
                d.d(9),
                SOFT_WHITE,
                2,
                if self.lock_kind == 2 {
                    if en { "Enter Password (keyboard + Enter)" } else { "Parolayi girin (klavye + Enter)" }
                } else {
                    if en { "Enter 4-digit PIN" } else { "4 haneli PIN girin" }
                },
            );
        }

        let dot_y = geo.status_h + d.d(136);
        let slots = if self.lock_kind == 2 { self.lock_len.max(6).min(12) } else { 4 };
        let spacing = d.d(22);
        let start_x = geo.w / 2 - ((slots as i32 - 1) * spacing) / 2;
        for i in 0..slots {
            let filled = i < self.lock_len;
            let cx = start_x + i as i32 * spacing;
            if filled {
                g.dot(cx, dot_y, d.d(6).max(4), ev.accent, 1024);
            } else {
                g.dot(cx, dot_y, d.d(6).max(4), WHITE, 120);
            }
        }

        const LABELS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "0", "OK"];
        for idx in 0..12 {
            let r = lock_keypad_rect(geo, idx);
            let is_ok = idx == 11;
            let bg = if is_ok { ev.accent } else { WHITE };
            let alpha = if is_ok { 220 } else { 32 };
            g.panel(r.x, r.y, r.w, r.h, r.h / 2, bg, alpha, WHITE, 40);
            g.text_centered(r.cx(), r.cy() - d.d(7), d.d(13), WHITE, 2, LABELS[idx]);
        }
    }

    pub fn draw_launch(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let t = self.app_p;
        if t <= 0 {
            return;
        }
        let e = t.clamp(0, 256);
        let a = geo.app;
        let pal = Palette::for_dark(unpack_dark_theme(ev.flags));
        if t >= 232 && self.active_surface != 0 {
            g.panel(a.x, a.y, a.w, a.h, 0, pal.bg, 255, 0, 0);
            g.embed_surface(self.active_surface, a.x, a.y, a.w, a.h);
            return;
        }
        let (ox, oy, os) = if self.app_open { self.origin } else { self.rect_for_app(self.launch_app as usize, geo) };
        let x = lerp(ox, a.x, e);
        let y = lerp(oy, a.y, e);
        let w = lerp(os, a.w, e);
        let h = lerp(os, a.h, e);
        let r = lerp(os * 24 / 100, 0, e);
        let apps = self.apps_slice();
        let (col, iid) = match apps.get(self.launch_app as usize) {
            Some(app) => (app_color(app, ev.accent), app_icon_id(app)),
            None => (ev.accent, nc_abi::draw::ICON_GENERIC),
        };
        let color = nc_mobile_ui::mix_color(col, pal.bg, e, 256);
        g.panel(x, y, w, h, r, color, 255, 0, 0);
        if e < 130 {
            let size = w.min(h);
            let gs = size * (256 - e / 2) / 256;
            let gx = x + (w - gs) / 2;
            let gy = y + (h - gs) / 2;
            g.icon(valid_icon_id(iid), gx, gy, gs, gs);
        }
    }

    /// Persistent System Status Bar:
    /// Left: Real time + DND / Airplane icons.
    /// Right: Real Cellular (only if hw), Bluetooth (only if hw & enabled), Wi-Fi (real state), Battery (real or unavailable).
    pub fn draw_status(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let t = if self.recents_open || self.locked { 0 } else { self.app_p };
        let d = &geo.dp;
        let dark = unpack_dark_theme(ev.flags);
        let th = GlassTheme::for_dark(dark);
        if t > 0 {
            g.panel(0, 0, geo.w, geo.status_h, 0, th.status_bar_bg, (t * 255 / 180).min(255) as u32, 0, 0);
        }
        let over_light_app = t > 140 && !dark;
        let fg = if over_light_app { th.status_fg_on_bar } else { WHITE };
        let cut_bg = if over_light_app { th.status_bar_bg } else { DEEP };
        let cy = geo.status_h / 2;
        let cap = d.d(9).max(8);

        // Left: Time + status flags (DND, Airplane)
        let mut lx = d.x(14);
        if let Some(l) = from_unix_tz(wall_clock(), unpack_tz_hours(ev.flags)) {
            let mut s = *b"--:--";
            format_time(&l, &mut s);
            if let Ok(text) = core::str::from_utf8(&s) {
                g.text(lx, (geo.status_h - cap) / 2, cap, fg, 2, text);
                lx += d.d(42);
            }
        }
        let st = &self.sys_status;
        if st.dnd {
            draw_dnd_glyph(g, lx + d.d(5), cy, d.d(4).max(4), fg, cut_bg);
            lx += d.d(14);
        }
        if st.airplane {
            draw_airplane_glyph(g, lx + d.d(5), cy, d.d(4).max(4), fg);
        }

        // Right: Battery + Percentage (if available) + Wi-Fi + BT + Cellular
        let mut rx = d.x(346);
        let bat_w = d.d(22).max(18);
        let bat_h = d.d(10).max(9);
        rx -= bat_w;
        draw_battery_icon(
            g,
            rx,
            cy - bat_h / 2,
            bat_w,
            bat_h,
            fg,
            st.bat_percent,
            st.bat_charging,
            st.bat_available,
        );
        if st.bat_available {
            let mut bl = Line::new();
            bl.number(st.bat_percent as u32, 1).text("%");
            rx -= d.d(28);
            g.text(rx, (geo.status_h - d.d(8).max(8)) / 2, d.d(8).max(8), fg, 1, bl.as_str());
        }
        rx -= d.d(14);

        // Wi-Fi status icon (only when hardware exists or user has it enabled)
        if st.wifi_hw {
            draw_wifi_status_icon(g, rx, cy, d.d(5).max(4), fg, st.wifi_state, st.wifi_rssi);
            rx -= d.d(18);
        }

        // Bluetooth icon (only when hardware exists and is enabled)
        if st.bt_hw && st.bt_enabled {
            draw_bt_glyph(g, rx, cy, d.d(5).max(4), fg);
            if st.bt_connected > 0 {
                g.dot(rx - d.d(6), cy, 1, fg, 1024);
                g.dot(rx + d.d(6), cy, 1, fg, 1024);
            }
            rx -= d.d(16);
        }

        // Cellular signal bars (ONLY when physical modem hardware exists)
        if st.cell_hw {
            let cw = d.d(14).max(12);
            let ch = d.d(10).max(8);
            rx -= cw;
            draw_cellular_bars(g, rx, cy - ch / 2, cw, ch, fg, st.cell_bars);
        }
    }

    pub fn draw_nav(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        if self.nav_p <= 0 || self.locked {
            return;
        }
        let d = &geo.dp;
        let top = geo.h - geo.nav_h * self.nav_p / 256;
        let dark = self.recents_open || unpack_dark_theme(ev.flags);
        let th = GlassTheme::for_dark(dark);
        let on_wallpaper = !self.app_open && !self.recents_open && self.drawer_p < 180;
        if on_wallpaper {
            g.panel(0, top, geo.w, geo.nav_h + 2, 0, DEEP, 155, WHITE, 18);
        } else if self.recents_open {
            g.panel(0, top, geo.w, geo.nav_h + 2, 0, DEEP, 210, 0, 0);
        } else {
            g.fill_rect(0, top, geo.w, geo.nav_h + 2, th.nav_bg);
        }
        let ink = if on_wallpaper || self.recents_open { WHITE } else { th.nav_ink };
        let bg_fill = if on_wallpaper || self.recents_open { DEEP } else { th.nav_bg };
        let cy = top + geo.nav_h / 2;
        let third = d.width / 3;
        let bx = d.origin_x + third / 2;
        let arm = d.d(5).max(4);
        for s in 0..=arm {
            g.fill_rect(bx - arm / 2 + s, cy - s, 2, 2, ink);
            g.fill_rect(bx - arm / 2 + s, cy + s - 1, 2, 2, ink);
        }
        let hx = d.origin_x + third + third / 2;
        let r = d.d(8).max(7);
        g.dot(hx, cy, r, ink, 1024);
        g.dot(hx, cy, (r - 3).max(3), bg_fill, 1024);
        let rx = d.origin_x + third * 2 + third / 2;
        let side = d.d(14).max(12);
        let col = if self.recents_open { WHITE } else { ink };
        g.panel(rx - side / 2, cy - side / 2, side, side, d.d(4).max(3), bg_fill, 255, col, 255);
    }

    pub fn draw_cc_dim(&self, g: &mut Gfx<'_>, geo: &Geo) {
        let p = self
            .cc_p
            .max(self.folder_p)
            .max(self.search_p)
            .max(self.wpick_p)
            .max(self.cust_p)
            .min(256);
        if p <= 0 {
            return;
        }
        g.panel(0, 0, geo.w, geo.h, 0, DEEP, (p * 116 / 256) as u32, 0, 0);
    }

    /// Comprehensive Control Center with real hardware states and Light/Dark theme coherence.
    pub fn draw_cc(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.cc_p.min(256);
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let en = ev.lang == 1;
        let dark = unpack_dark_theme(ev.flags);
        let th = GlassTheme::for_dark(dark);
        let st = &self.sys_status;
        let c = geo.cc;
        let off = -(256 - p) * (c.bottom() + d.d(16)) / 256;

        themed_sheet(g, &th, c.x, c.y + off, c.w, c.h, d.d(24));

        // Header: Date + Real Battery status + Lock button
        let s0 = stag(p, 0, 16);
        let mut head = Line::new();
        if date_line(ev, &mut head) {
            let hy = c.y + off + d.d(15) + (256 - s0) * d.d(4) / 256;
            g.text(c.x + d.d(16), hy, d.d(8).max(8), fade_to(th.ink, th.sheet_bg, s0), 2, head.as_str());
        }
        // Compact battery status pill in CC header
        let bat_px = if self.lock_enabled { c.right() - d.d(152) } else { c.right() - d.d(84) };
        let bat_py = c.y + off + d.d(12);
        let mut bat_txt = Line::new();
        if st.bat_available {
            bat_txt.number(st.bat_percent as u32, 1).text("%");
        } else {
            bat_txt.text(if en { "No bat" } else { "Pil yok" });
        }
        g.panel(bat_px, bat_py, d.d(68), d.d(20), d.d(10), th.tile_off_bg, scale_alpha(180, s0), 0, 0);
        g.text_centered(bat_px + d.d(34), bat_py + d.d(6), d.d(7).max(8), fade_to(th.muted, th.sheet_bg, s0), 1, bat_txt.as_str());

        if self.lock_enabled {
            let lr = geo.cc_lock_btn();
            g.panel(lr.x, lr.y + off, lr.w, lr.h, d.d(12), th.tile_off_bg, scale_alpha(200, s0), 0, 0);
            g.text_centered(lr.cx(), lr.y + off + d.d(7), d.d(7).max(8), th.ink, 2, tr(ev, "Kilitle", "Lock"));
        }

        // 2x2 Connectivity & System Tiles:
        // 0: Wi-Fi, 1: Bluetooth, 2: Mobile Data, 3: Do Not Disturb
        for idx in 0..4usize {
            let r = geo.cc_tile(idx);
            let s = stag(p, 1 + idx as i32, 14);
            let dy = off + (256 - s) * d.d(8) / 256;
            let k = if self.cc_pressed(r) { d.d(2) } else { 0 };
            let (rx, ry, rw, rh) = (r.x + k, r.y + dy + k, r.w - k * 2, r.h - k * 2);

            let (avail, active, title, subtitle) = match idx {
                0 => {
                    let sub = match st.wifi_state {
                        CONN_CONNECTED => if en { "Connected" } else { "Bağlı" },
                        CONN_CONNECTING => if en { "Connecting" } else { "Bağlanıyor" },
                        CONN_DISCONNECTED => if en { "No link" } else { "Bağlı değil" },
                        CONN_LIMITED => if en { "Limited" } else { "Sınırlı" },
                        CONN_UNAVAILABLE => if en { "Unavailable" } else { "Donanım yok" },
                        _ => if en { "Off" } else { "Kapalı" },
                    };
                    (st.wifi_hw, st.wifi_hw && st.wifi_enabled && !st.airplane, "Wi-Fi", sub)
                }
                1 => {
                    let sub = if !st.bt_hw {
                        if en { "Unavailable" } else { "Donanım yok" }
                    } else if st.bt_enabled {
                        if st.bt_connected > 0 {
                            if en { "Connected" } else { "Bağlı" }
                        } else {
                            if en { "On" } else { "Açık" }
                        }
                    } else {
                        if en { "Off" } else { "Kapalı" }
                    };
                    (st.bt_hw, st.bt_hw && st.bt_enabled && !st.airplane, "Bluetooth", sub)
                }
                2 => {
                    let sub = if !st.cell_hw {
                        if en { "No modem" } else { "Modem yok" }
                    } else if !st.cell_sim {
                        if en { "No SIM" } else { "SIM yok" }
                    } else if st.cell_data {
                        if en { "Active" } else { "Açık" }
                    } else {
                        if en { "Off" } else { "Kapalı" }
                    };
                    (st.cell_hw, st.cell_hw && st.cell_data && !st.airplane, if en { "Mobile Data" } else { "Mobil Veri" }, sub)
                }
                _ => {
                    let sub = if st.dnd {
                        if en { "Active" } else { "Açık" }
                    } else {
                        if en { "Off" } else { "Kapalı" }
                    };
                    (true, st.dnd, if en { "Do Not Disturb" } else { "Rahatsız Etme" }, sub)
                }
            };

            let bg_col = if active { th.tile_on_bg } else { th.tile_off_bg };
            let bg_a = if !avail {
                scale_alpha(110, s)
            } else if active {
                scale_alpha(245, s)
            } else {
                scale_alpha(th.tile_off_alpha as i32, s)
            };
            g.panel(rx, ry, rw, rh, d.d(16), bg_col, bg_a, 0, 0);

            let bx = r.x + d.d(26);
            let by = r.cy() + dy;
            let br = d.d(14);
            let icon_bg = if active { th.tile_on_fg } else { th.sheet_bg };
            let icon_fg = if active { th.tile_on_bg } else if avail { th.ink } else { th.subtle };
            g.dot(bx, by, br, icon_bg, (if avail { 900 } else { 420 } * s / 256) as u32);

            match idx {
                0 => draw_wifi_status_icon(g, bx, by, d.d(6).max(5), icon_fg, st.wifi_state, st.wifi_rssi),
                1 => draw_bt_glyph(g, bx, by, d.d(6).max(5), icon_fg),
                2 => draw_cellular_bars(g, bx - d.d(6), by - d.d(5), d.d(12), d.d(10), icon_fg, st.cell_bars),
                _ => draw_dnd_glyph(g, bx, by, d.d(6).max(5), icon_fg, icon_bg),
            }

            let fg_title = if active { th.tile_on_fg } else if avail { th.ink } else { th.subtle };
            let fg_sub = if active {
                fade_to(th.tile_on_fg, th.tile_on_bg, 200)
            } else {
                th.muted
            };
            g.text(r.x + d.d(48), r.y + dy + d.d(13), d.d(8).max(8), fade_to(fg_title, th.sheet_bg, s), 2, title);
            g.text(r.x + d.d(48), r.y + dy + d.d(31), d.d(7).max(8), fade_to(fg_sub, th.sheet_bg, s), 1, subtitle);
        }

        // Brightness Slider (real HW PWM on X6826C, honestly labeled Software on QEMU ramfb)
        let s5 = stag(p, 5, 14);
        let b = geo.cc_bright_card();
        let by0 = b.y + off + (256 - s5) * d.d(8) / 256;
        g.panel(b.x, by0, b.w, b.h, d.d(16), th.slider_track, scale_alpha(th.slider_track_alpha as i32, s5), 0, 0);
        let pct = (if self.brightness_touch >= 0 { self.brightness_touch } else { unpack_brightness(ev.flags) as i32 }).clamp(20, 100);
        let fw = ((pct - 20) * b.w / 80).max(d.d(48)).min(b.w);
        g.panel(b.x, by0, fw, b.h, d.d(16), th.slider_fill, scale_alpha(242, s5), 0, 0);
        draw_sun_glyph(g, b.x + d.d(24), by0 + b.h / 2, d.d(7).max(6), th.slider_fill_ink);
        let mut bl = Line::new();
        bl.number(pct as u32, 1).text("%");
        if !st.bright_hw {
            bl.text(if en { " (Soft)" } else { " (Yazılım)" });
        }
        let over = fw > b.w - d.d(96);
        let vc = if over { th.slider_fill_ink } else { th.ink };
        g.text(b.x + b.w - d.d(92), by0 + b.h / 2 - d.d(5), d.d(8).max(8), fade_to(vc, th.sheet_bg, s5), 1, bl.as_str());

        // Audio Volume Row (honest unavailable state when audio_hw == 0)
        let s6 = stag(p, 6, 14);
        let au = geo.cc_audio_card();
        let ay0 = au.y + off + (256 - s6) * d.d(8) / 256;
        g.panel(au.x, ay0, au.w, au.h, d.d(14), th.tile_off_bg, scale_alpha(if st.audio_hw { 210 } else { 115 }, s6), 0, 0);
        draw_volume_glyph(g, au.x + d.d(22), ay0 + au.h / 2, d.d(6).max(5), if st.audio_hw { th.ink } else { th.subtle }, !st.audio_hw);
        let au_txt = if st.audio_hw {
            if en { "Volume" } else { "Ses" }
        } else {
            if en { "Audio output unavailable" } else { "Ses donanımı yok" }
        };
        g.text(au.x + d.d(42), ay0 + au.h / 2 - d.d(5), d.d(8).max(8), if st.audio_hw { th.ink } else { th.subtle }, 1, au_txt);

        // Quick Actions: Theme (Light/Dark) + Airplane Mode
        let s7 = stag(p, 7, 14);
        let tb = geo.cc_theme_btn();
        let ty0 = tb.y + off + (256 - s7) * d.d(8) / 256;
        g.panel(tb.x, ty0, tb.w, tb.h, d.d(14), th.tile_off_bg, scale_alpha(th.tile_off_alpha as i32, s7), 0, 0);
        draw_theme_glyph(g, tb.x + d.d(22), ty0 + tb.h / 2, d.d(7).max(6), th.ink, th.tile_off_bg);
        g.text(
            tb.x + d.d(40),
            ty0 + tb.h / 2 - d.d(5),
            d.d(8).max(8),
            th.ink,
            2,
            if dark { if en { "Dark Theme" } else { "Koyu Tema" } } else { if en { "Light Theme" } else { "Açık Tema" } },
        );

        let ab = geo.cc_airplane_btn();
        let ay1 = ab.y + off + (256 - s7) * d.d(8) / 256;
        g.panel(
            ab.x,
            ay1,
            ab.w,
            ab.h,
            d.d(14),
            if st.airplane { th.tile_on_bg } else { th.tile_off_bg },
            scale_alpha(if st.airplane { 245 } else { th.tile_off_alpha as i32 }, s7),
            0,
            0,
        );
        draw_airplane_glyph(g, ab.x + d.d(22), ay1 + ab.h / 2, d.d(6).max(5), if st.airplane { th.tile_on_fg } else { th.ink });
        g.text(
            ab.x + d.d(40),
            ay1 + ab.h / 2 - d.d(5),
            d.d(8).max(8),
            if st.airplane { th.tile_on_fg } else { th.ink },
            2,
            if en { "Airplane" } else { "Uçak Modu" },
        );

        // Footer: Inline Wi-Fi detail OR All Settings shortcut
        let sr = geo.cc_settings();
        let sy = sr.y + off + (256 - s7) * d.d(8) / 256;
        if self.cc_wifi_detail {
            g.panel(sr.x, sy, sr.w, sr.h, sr.h / 2, th.tile_off_bg, scale_alpha(200, s7), 0, 0);
            let mut info = Line::new();
            info.text("MAC: ").text(if st.mac_str().is_empty() { "--" } else { st.mac_str() });
            g.text_centered(sr.cx(), sy + sr.h / 2 - d.d(4), d.d(7).max(8), th.muted, 1, info.as_str());
        } else {
            g.panel(sr.x, sy, sr.w, sr.h, sr.h / 2, th.tile_off_bg, scale_alpha(165, s7), 0, 0);
            mini_tile_icon(g, ICON_SETTINGS, sr.x + d.d(6), sy + d.d(5), sr.h - d.d(10));
            g.text(sr.x + d.d(36), sy + sr.h / 2 - d.d(5), d.d(8).max(8), th.ink, 2, tr(ev, "Tüm Ayarlar", "All Settings"));
            chevron(g, sr.right() - d.d(16), sy + sr.h / 2, d.d(4), th.muted);
        }
    }

    /// Widget Picker Sheet (opened from Edit Mode -> "+ Widgets").
    pub fn draw_wpick(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.wpick_p.min(256);
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let en = ev.lang == 1;
        let th = GlassTheme::for_dark(unpack_dark_theme(ev.flags));
        let s = geo.wpick_sheet();
        let off = (256 - p) * (s.h + d.d(24)) / 256;
        themed_sheet(g, &th, s.x, s.y + off, s.w, s.h, d.d(24));

        // Handle & Title
        g.panel(s.cx() - d.d(18), s.y + off + d.d(8), d.d(36), d.d(4), d.d(2), th.subtle, 140, 0, 0);
        g.text(s.x + d.d(16), s.y + off + d.d(18), d.d(11), th.ink, 2, tr(ev, "Widget Ekle", "Add Widget"));

        // Size filter chips: Small / Medium / Large
        let sizes = [WidgetSize::Small, WidgetSize::Medium, WidgetSize::Large];
        for (i, sz) in sizes.iter().enumerate() {
            let cr = geo.wpick_size_chip(i);
            let sel = self.wpick_size == *sz;
            g.panel(
                cr.x,
                cr.y + off,
                cr.w,
                cr.h,
                cr.h / 2,
                if sel { th.tile_on_bg } else { th.tile_off_bg },
                235,
                0,
                0,
            );
            g.text_centered(
                cr.cx(),
                cr.cy() + off - d.d(5),
                d.d(8).max(8),
                if sel { th.tile_on_fg } else { th.ink },
                2,
                sz.label(en),
            );
        }

        // Registered widget providers
        for i in 0..PROVIDER_COUNT {
            let Some(meta) = provider_meta(i as u8) else { continue };
            let rr = geo.wpick_row(i);
            if rr.bottom() + off > s.bottom() + off - d.d(6) {
                break;
            }
            let supported = meta.supports(self.wpick_size);
            g.panel(
                rr.x,
                rr.y + off,
                rr.w,
                rr.h,
                d.d(14),
                th.card_bg,
                if supported { th.card_alpha } else { 110 },
                th.rim,
                th.rim_alpha,
            );
            g.text(rr.x + d.d(14), rr.y + off + d.d(12), d.d(9), if supported { th.ink } else { th.subtle }, 2, meta.display_name(en));
            g.text(rr.x + d.d(14), rr.y + off + d.d(31), d.d(7).max(8), th.muted, 1, meta.description(en));
            let px = rr.right() - d.d(24);
            let py = rr.cy() + off;
            g.dot(px, py, d.d(12), if supported { th.tile_on_bg } else { th.tile_off_bg }, 1024);
            draw_plus_glyph(g, px, py, d.d(4), if supported { th.tile_on_fg } else { th.subtle });
        }
    }

    /// Home Screen Customization Sheet (Appearance, Wallpaper, Layout, Icons, Motion).
    pub fn draw_customize(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.cust_p.min(256);
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let en = ev.lang == 1;
        let dark = unpack_dark_theme(ev.flags);
        let wp = unpack_wallpaper(ev.flags);
        let th = GlassTheme::for_dark(dark);
        let s = geo.cust_sheet();
        let off = (256 - p) * (s.h + d.d(24)) / 256;
        themed_sheet(g, &th, s.x, s.y + off, s.w, s.h, d.d(24));

        g.panel(s.cx() - d.d(18), s.y + off + d.d(8), d.d(36), d.d(4), d.d(2), th.subtle, 140, 0, 0);
        g.text(s.x + d.d(16), s.y + off + d.d(18), d.d(11), th.ink, 2, tr(ev, "Ana Ekranı Özelleştir", "Customize Home"));

        let wp_names_en = ["Slate", "Ocean", "Dusk", "Forest", "Obsidian", "Custom"];
        let wp_names_tr = ["Arduvaz", "Okyanus", "Alacakaranlık", "Orman", "Obsidyen", "Özel"];
        let wp_label = if en { wp_names_en[(wp as usize).min(5)] } else { wp_names_tr[(wp as usize).min(5)] };

        let rows: [(&str, &str); 8] = [
            (tr(ev, "Görünüm", "Appearance"), if dark { tr(ev, "Koyu", "Dark") } else { tr(ev, "Açık", "Light") }),
            (tr(ev, "Duvar Kağıdı", "Wallpaper"), wp_label),
            (tr(ev, "Düzen", "Layout"), self.prefs.density.label(en)),
            (tr(ev, "Simge Boyutu", "Icons"), self.prefs.icon_size.label(en)),
            (tr(ev, "Uygulama Adları", "App Labels"), if self.prefs.show_labels { tr(ev, "Göster", "Show") } else { tr(ev, "Gizle", "Hide") }),
            (tr(ev, "Sayfa Göstergesi", "Page Indicator"), if self.prefs.pill_indicator { tr(ev, "Kapsül", "Pill") } else { tr(ev, "Nokta", "Dots") }),
            (tr(ev, "Geçiş Stili", "Transition"), self.prefs.transition.label(en)),
            (tr(ev, "Azaltılmış Hareket", "Reduced Motion"), if self.reduced_motion() { tr(ev, "Açık", "On") } else { tr(ev, "Kapalı", "Off") }),
        ];

        for (i, (k, v)) in rows.iter().enumerate() {
            let rr = geo.cust_row(i);
            if rr.bottom() + off > s.bottom() + off - d.d(6) {
                break;
            }
            g.panel(rr.x, rr.y + off, rr.w, rr.h, d.d(12), th.card_bg, th.card_alpha, th.rim, th.rim_alpha);
            g.text(rr.x + d.d(14), rr.cy() + off - d.d(5), d.d(9), th.ink, 2, k);
            let pill_w = d.d(96);
            let pill_h = d.d(26);
            let px = rr.right() - pill_w - d.d(10);
            let py = rr.cy() + off - pill_h / 2;
            g.panel(px, py, pill_w, pill_h, pill_h / 2, th.tile_off_bg, 220, 0, 0);
            g.text_centered(px + pill_w / 2, py + d.d(8), d.d(8).max(8), th.ink, 1, v);
        }
    }

    pub fn draw_drawer(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.drawer_p;
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let th = GlassTheme::for_dark(unpack_dark_theme(ev.flags));
        let sheet_h = geo.h - geo.drawer_y;
        let off = (256 - p.min(256)) * sheet_h / 256;
        themed_sheet(g, &th, 0, geo.drawer_y + off, geo.w, sheet_h + d.d(40), d.d(24));
        g.panel(geo.w / 2 - d.d(18), geo.drawer_y + off + d.d(9), d.d(36), d.d(4), d.d(2), th.subtle, 130, 0, 0);

        let sb = geo.drawer_search();
        g.panel(sb.x, sb.y + off, sb.w, sb.h, sb.h / 2, th.tile_off_bg, 200, 0, 0);
        search_glyph(g, sb.x + d.d(22), sb.cy() + off, d.d(6), th.muted);
        g.text(sb.x + d.d(42), sb.cy() + off - d.d(5), d.d(9), th.muted, 2, tr(ev, "Uygulama ara", "Search apps"));

        let en = ev.lang == 1;
        let pe = p.min(256);
        let mut k = 0usize;
        for app in self.apps_slice().iter() {
            if !is_launcher_visible(app) {
                continue;
            }
            let (x, y) = geo.drawer_cell(k);
            let row = (k / 4) as i32;
            let s = stag(pe, 1 + row, 18);
            let rise = (256 - s) * d.d(16) / 256;
            tile_icon(g, app_icon_id(app), x, y + off + rise, geo.icon, false);
            label(g, x + geo.icon / 2, y + off + rise + geo.icon + d.d(6), d.d(8), fade_to(th.ink, th.sheet_bg, s), app_name(app, en));
            k += 1;
        }
    }

    pub fn draw_folder(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.folder_p;
        if p <= 0 {
            return;
        }
        let fp = (self.folder_page as usize).min(MAX_PAGES - 1);
        let Cell::Folder(slots) = self.pages[fp][self.folder_cell] else { return };
        let d = &geo.dp;
        let th = GlassTheme::for_dark(unpack_dark_theme(ev.flags));
        let base = geo.folder_card();
        let sc = p.clamp(40, 270);
        let cw = base.w * sc / 256;
        let ch = base.h * sc / 256;
        let cx = base.cx() - cw / 2;
        let cy = base.cy() - ch / 2;
        themed_sheet(g, &th, cx, cy, cw, ch, d.d(24));
        if p > 140 {
            g.text_centered(base.cx(), cy + d.d(14), d.d(10), th.ink, 2, tr(ev, "Klasör", "Folder"));
        }
        let en = ev.lang == 1;
        let apps = self.apps_slice();
        let mut k = 0;
        for slot in slots.iter() {
            let Some(a) = *slot else { continue };
            let Some(app) = apps.get(a as usize) else { continue };
            let (tx, ty) = geo.folder_slot(k);
            let ix = base.cx() + (tx - base.cx()) * sc / 256;
            let iy = base.cy() + (ty - base.cy()) * sc / 256;
            let s = geo.icon * p.clamp(40, 264) / 256;
            tile_icon(g, app_icon_id(app), ix - s / 2, iy - s / 2, s, false);
            if p > 180 {
                label(g, ix, iy + s / 2 + d.d(5), d.d(8), th.ink, app_name(app, en));
            }
            k += 1;
        }
        if p > 180 {
            let u = geo.folder_ungroup();
            g.panel(u.x, u.y, u.w, u.h, u.h / 2, th.sheet_bg, 220, th.rim, th.rim_alpha);
            g.text_centered(u.cx(), u.cy() - d.d(5), d.d(8), th.ink, 2, tr(ev, "Grubu Çöz", "Ungroup"));
        }
    }

    pub fn draw_search(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.search_p;
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let th = GlassTheme::for_dark(unpack_dark_theme(ev.flags));
        let s = geo.sheet;
        let off = -(256 - p.min(256)) * (s.bottom() + d.d(12)) / 256;
        themed_sheet(g, &th, s.x, s.y + off, s.w, s.h, d.d(24));
        let f = geo.search_field();
        g.panel(f.x, f.y + off, f.w, f.h, f.h / 2, th.tile_off_bg, 200, 0, 0);
        search_glyph(g, f.x + d.d(24), f.cy() + off, d.d(7), th.ink);
        let q = self.query_str();
        if q.is_empty() {
            g.text(f.x + d.d(44), f.cy() + off - d.d(5), d.d(9), th.muted, 2, tr(ev, "Uygulama ara", "Search apps"));
        } else {
            g.text(f.x + d.d(44), f.cy() + off - d.d(5), d.d(10), th.ink, 2, q);
        }
        let en = ev.lang == 1;
        let apps = self.apps_slice();
        if q.is_empty() {
            g.text(s.x + d.d(18), s.y + off + d.d(68), d.d(7).max(8), th.muted, 1, tr(ev, "Uygulamalar", "Apps"));
            let mut k = 0usize;
            for app in apps.iter() {
                if !is_launcher_visible(app) {
                    continue;
                }
                if k >= 8 {
                    break;
                }
                let (x, y) = geo.search_suggest(k);
                let sz = geo.icon * 9 / 10;
                let st = stag(p.min(256), 1 + k as i32, 18);
                let rise = (256 - st) * d.d(10) / 256;
                tile_icon(g, app_icon_id(app), x, y + off + rise, sz, false);
                label(g, x + sz / 2, y + off + rise + sz + d.d(4), d.d(7).max(8), fade_to(th.ink, th.sheet_bg, st), app_name(app, en));
                k += 1;
            }
        } else {
            let mut found = [0u8; 8];
            let n = self.matches(ev, &mut found);
            if n == 0 {
                g.text_centered(s.cx(), s.y + off + d.d(100), d.d(9), th.muted, 2, tr(ev, "Sonuç bulunamadı", "No results"));
            }
            for i in 0..n {
                let app = &apps[found[i] as usize];
                let r = geo.search_row(i);
                mini_tile_icon(g, app_icon_id(app), r.x + d.d(6), r.y + off + d.d(5), d.d(40));
                g.text(r.x + d.d(60), r.cy() + off - d.d(6), d.d(10), th.ink, 2, app_name(app, en));
            }
        }
    }

    pub fn draw_recents(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        let p = self.rec_p;
        if p <= 0 {
            return;
        }
        let d = &geo.dp;
        let pal = Palette::for_dark(unpack_dark_theme(ev.flags));
        g.panel(0, 0, geo.w, geo.h, 0, DEEP, (p.min(256) * 204 / 256) as u32, 0, 0);
        let en = ev.lang == 1;
        let list = self.apps_slice();
        let n = list.iter().filter(|a| a.state != apps::STATE_STOPPED && a.surface_id != 0).count();
        let pe = p.min(256);
        let s0 = stag(pe, 0, 18);
        let rise0 = (256 - s0) * d.d(18) / 256;
        g.text_centered(geo.w / 2, geo.status_h + d.d(24) + rise0, d.d(11), fade_to(WHITE, DEEP, s0), 2, tr(ev, "Son Uygulamalar", "Recent Apps"));
        if n == 0 {
            let r = R::new(d.x(48), geo.h * 38 / 100 + rise0, d.d(264), d.d(80));
            frost_sheet(g, r.x, r.y, r.w, r.h, d.d(20), 190);
            g.text_centered(r.cx(), r.cy() - d.d(5), d.d(9), fade_to(SOFT_WHITE, FROST_SHEET, s0), 2, tr(ev, "Arka planda uygulama yok", "No background apps"));
            return;
        }
        let mut k = 0;
        for (i, app) in list.iter().enumerate() {
            if app.state == apps::STATE_STOPPED || app.surface_id == 0 {
                continue;
            }
            let s = stag(pe, 1 + k as i32, 18);
            let rise = (256 - s) * d.d(32) / 256;
            let mut c = geo.recent_card(k, n);
            c.y += rise + if self.swipe_card == i as i32 { self.swipe_dy } else { 0 };
            frost_sheet(g, c.x, c.y, c.w, c.h, d.d(18), 205);
            mini_tile_icon(g, app_icon_id(app), c.x + d.d(10), c.y + d.d(5), d.d(22));
            g.text(c.x + d.d(40), c.y + d.d(10), d.d(8), WHITE, 2, app_name(app, en));
            let pv = geo.recent_preview(c);
            g.panel(pv.x, pv.y, pv.w, pv.h, d.d(12), pal.bg, 255, 0, 0);
            g.embed_surface(app.surface_id, pv.x, pv.y, pv.w, pv.h);
            k += 1;
        }
        let sc = stag(pe, 1 + n as i32, 18);
        let r = geo.recents_clear();
        let ry = r.y + (256 - sc) * d.d(14) / 256;
        g.panel(r.x, ry, r.w, r.h, r.h / 2, FROST_CARD, 190, WHITE, 28);
        g.text_centered(r.cx(), ry + r.h / 2 - d.d(5), d.d(8), fade_to(WHITE, FROST_CARD, sc), 2, tr(ev, "Tümünü Temizle", "Clear All"));
    }
}
