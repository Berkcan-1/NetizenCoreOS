use nc_abi::event::{unpack_dark_theme, EventBlock};
use nc_app_sdk::widget::WidgetRect;
use nc_app_sdk::{sys, Gfx};
use nc_mobile_ui::{
    draw_close_glyph, draw_plus_glyph, GlassTheme, CORAL, WHITE,
};

use crate::aero::*;
use crate::layout::Geo;
use crate::model::*;
use crate::shell::HomeShell;
use crate::widgets::provider_meta;

pub fn wall_clock() -> i64 {
    sys::clock_get(1)
}

pub fn tr(ev: &EventBlock, a: &'static str, b: &'static str) -> &'static str {
    if ev.lang == 1 {
        b
    } else {
        a
    }
}

impl HomeShell {
    pub fn draw_wallpaper(&self, _g: &mut Gfx<'_>, _ev: &EventBlock, _geo: &Geo) {}

    fn slot_hidden(&self, slot: Slot) -> bool {
        self.lift == Some(slot)
    }

    pub fn draw_home(&self, g: &mut Gfx<'_>, ev: &EventBlock, geo: &Geo) {
        self.draw_wallpaper(g, ev, geo);
        let d = &geo.dp;
        let en = ev.lang == 1;
        let dark = unpack_dark_theme(ev.flags);
        let th = GlassTheme::for_dark(dark);
        let apps = self.apps_slice();

        let edit_dy = self.edit_p * d.d(28) / 256;

        // Edit Mode top bar
        if self.edit_p > 12 {
            let s = self.edit_p.clamp(0, 256);
            let add_r = geo.edit_bar_add();
            let cust_r = geo.edit_bar_customize();
            let done_r = geo.edit_bar_done();
            let by = (256 - s) * d.d(12) / 256;

            g.panel(add_r.x, add_r.y - by, add_r.w, add_r.h, add_r.h / 2, th.sheet_bg, scale_alpha(th.sheet_alpha as i32, s), th.rim, scale_alpha(th.rim_alpha as i32, s));
            draw_plus_glyph(g, add_r.x + d.d(14), add_r.cy() - by, d.d(4), th.ink);
            g.text(add_r.x + d.d(24), add_r.cy() - by - d.d(5), d.d(8), th.ink, 1, tr(ev, "Widget", "Widgets"));

            g.panel(cust_r.x, cust_r.y - by, cust_r.w, cust_r.h, cust_r.h / 2, th.sheet_bg, scale_alpha(th.sheet_alpha as i32, s), th.rim, scale_alpha(th.rim_alpha as i32, s));
            g.text_centered(cust_r.cx(), cust_r.cy() - by - d.d(5), d.d(8), th.ink, 1, tr(ev, "Özelleştir", "Customize"));

            g.panel(done_r.x, done_r.y - by, done_r.w, done_r.h, done_r.h / 2, th.tile_on_bg, scale_alpha(240, s), 0, 0);
            g.text_centered(done_r.cx(), done_r.cy() - by - d.d(5), d.d(8), th.tile_on_fg, 2, tr(ev, "Bitti", "Done"));
        }

        // Draw only pages intersecting the visible viewport
        let page_w = geo.w.max(1);
        for p in 0..self.page_count.min(MAX_PAGES) {
            let page_dx = p as i32 * page_w - self.page_scroll_px;
            if page_dx <= -page_w || page_dx >= page_w {
                continue;
            }

            // Transition style adjustments (Depth/Fade subtle scaling)
            let norm = (page_dx.abs() * 256 / page_w).clamp(0, 256);
            let depth_scale = match self.prefs.transition {
                TransitionStyle::Depth => 256 - norm * 18 / 256,
                _ => 256,
            };

            // 1. Render widgets on this page
            for w in self.widgets.iter() {
                if !w.active || w.page != p as u8 {
                    continue;
                }
                let wr = geo.widget_rect(w.col, w.row, w.size, page_dx, edit_dy);
                if wr.right() <= 0 || wr.x >= geo.w {
                    continue;
                }
                self.render_widget_safe(g, ev, w, WidgetRect { x: wr.x, y: wr.y, w: wr.w, h: wr.h });

                if self.edit_mode {
                    // Subtle border highlight in edit mode
                    g.panel(wr.x, wr.y, wr.w, wr.h, (wr.h.min(wr.w) / 5).clamp(14, 22), 0, 0, th.ink, 90);
                    // Remove badge (top-left)
                    let rm = geo.widget_remove_badge(wr);
                    g.dot(rm.cx(), rm.cy(), rm.w / 2, CORAL, 1024);
                    draw_close_glyph(g, rm.cx(), rm.cy(), d.d(3), WHITE);
                    // Resize badge (bottom-right) if provider supports multiple sizes
                    if let Some(meta) = provider_meta(w.provider_idx) {
                        if meta.next_size(w.size) != w.size {
                            let rs = geo.widget_resize_badge(wr);
                            g.panel(rs.x, rs.y, rs.w, rs.h, rs.h / 2, th.tile_on_bg, 230, 0, 0);
                            g.text_centered(rs.cx(), rs.cy() - d.d(4), d.d(7), th.tile_on_fg, 1, w.size.label(en));
                        }
                    }
                }
            }

            // 2. Render app icons and folders on this page
            let cap = geo.capacity().min(MAX_CELLS);
            for i in 0..cap {
                if self.slot_hidden(Slot::Grid(p as u8, i)) {
                    continue;
                }
                if cell_covered_by_widgets(&self.widgets, p as u8, i, geo.cols) {
                    continue;
                }
                let (x, y) = geo.cell_origin_ex(i, page_dx, edit_dy);
                if x + geo.icon < 0 || x > geo.w {
                    continue;
                }
                let pressed = self.pressed == Some(Slot::Grid(p as u8, i));
                match self.pages[p][i] {
                    Cell::Empty => {}
                    Cell::App(a) => {
                        let Some(app) = apps.get(a as usize) else { continue };
                        let app_depth = 256 - self.app_p.clamp(0, 256) * 14 / 256;
                        let edit_shrink = 256 - self.edit_p.clamp(0, 256) * 10 / 256;
                        let base = geo.icon * app_depth / 256 * depth_scale / 256 * edit_shrink / 256;
                        let size = if pressed { base * 94 / 100 } else { base };
                        let off = (geo.icon - size) / 2;
                        tile_icon(g, app_icon_id(app), x + off, y + off, size, pressed);
                        if self.prefs.show_labels {
                            label(g, x + geo.icon / 2, y + geo.icon + d.d(5), d.d(8), WHITE, app_name(app, en));
                        }
                    }
                    Cell::Folder(slots) => {
                        let v = self.folder_icon_ids(&slots);
                        folder_tile_icons(g, x, y, geo.icon, &v, pressed);
                        if self.prefs.show_labels {
                            label(g, x + geo.icon / 2, y + geo.icon + d.d(5), d.d(8), WHITE, tr(ev, "Klasör", "Folder"));
                        }
                    }
                }
            }
        }

        // Page indicators & persistent Dock
        if self.drawer_p < 240 {
            self.draw_page_indicators(g, geo);
            self.draw_dock(g, ev, geo, &th);
        }

        // Lifted icon being dragged
        if let Some(slot) = self.lift {
            let scale = 100 + 10 * self.lift_p / 256;
            let size = geo.icon * scale / 100;
            if let Cell::App(a) = self.get(slot) {
                if let Some(app) = apps.get(a as usize) {
                    tile_icon(g, app_icon_id(app), self.drag_x - size / 2, self.drag_y - size / 2, size, false);
                }
            } else if let Cell::Folder(s) = self.get(slot) {
                let v = self.folder_icon_ids(&s);
                folder_tile_icons(g, self.drag_x - size / 2, self.drag_y - size / 2, size, &v, false);
            }
        }
    }

    fn draw_page_indicators(&self, g: &mut Gfx<'_>, geo: &Geo) {
        let d = &geo.dp;
        let count = self.page_count.clamp(1, MAX_PAGES) as i32;
        let ph = d.d(4).max(3);
        let active_w = if self.prefs.pill_indicator { d.d(14).max(10) } else { ph };
        let idle_w = ph;
        let gap = d.d(6).max(4);
        let total_w = active_w + (count - 1) * (idle_w + gap);
        let mut x = geo.w / 2 - total_w / 2;
        for p in 0..count {
            let is_cur = p as usize == self.page;
            let w = if is_cur { active_w } else { idle_w };
            let alpha = if is_cur { 240 } else { 110 };
            g.panel(x, geo.dots_y, w, ph, ph / 2, WHITE, alpha, 0, 0);
            x += w + gap;
        }
    }

    fn draw_dock(&self, g: &mut Gfx<'_>, _ev: &EventBlock, geo: &Geo, th: &GlassTheme) {
        let k = geo.dock;
        let d = &geo.dp;
        let apps = self.apps_slice();
        g.panel(k.x, k.y, k.w, k.h, d.d(22), th.dock_bg, th.dock_alpha, th.rim, th.rim_alpha);
        for i in 0..DOCK_SLOTS {
            let Some(a) = self.dock[i] else { continue };
            let Some(app) = apps.get(a as usize) else { continue };
            if self.slot_hidden(Slot::Dock(i)) {
                continue;
            }
            let (x, y, s) = geo.dock_origin(i);
            tile_icon(g, app_icon_id(app), x, y, s, self.pressed == Some(Slot::Dock(i)));
        }
    }

    pub fn folder_icon_ids(&self, slots: &[Option<u8>; FOLDER_SLOTS]) -> [Option<u32>; 4] {
        let mut out = [None; 4];
        let apps = self.apps_slice();
        for (i, s) in slots.iter().enumerate() {
            if let Some(a) = *s {
                if let Some(app) = apps.get(a as usize) {
                    out[i] = Some(app_icon_id(app));
                }
            }
        }
        out
    }
}
