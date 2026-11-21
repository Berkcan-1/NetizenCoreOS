use nc_abi::event::{
    unpack_dark_theme, unpack_wallpaper, EventBlock, PTR_DOWN, PTR_HAS,
};
use nc_abi::key;
use nc_abi::sys as sys_wire;
use nc_app_sdk::widget::{WidgetAction, WidgetRect, WidgetSize};
use nc_app_sdk::{apps, pointer, security, sys};

use crate::layout::{Geo, R};
use crate::model::*;
use crate::overlay::lock_keypad_rect;
use crate::shell::*;
use crate::widgets::{provider_by_idx, PROVIDER_COUNT};

impl HomeShell {
    pub fn matches(&self, ev: &EventBlock, out: &mut [u8; 8]) -> usize {
        let en = ev.lang == 1;
        let mut n = 0;
        for (i, app) in self.apps_slice().iter().enumerate() {
            if !is_launcher_visible(app) {
                continue;
            }
            if (lower_contains(app_name(app, en), self.query_str())
                || lower_contains(app.id_str(), self.query_str()))
                && n < out.len()
            {
                out[n] = i as u8;
                n += 1;
            }
        }
        n
    }

    pub fn handle_key(&mut self, ev: &EventBlock) -> bool {
        let code = ev.key;
        if code == key::NONE {
            return false;
        }
        if self.locked {
            if code == key::BACKSPACE {
                self.lock_pop_byte();
                return true;
            }
            if code == key::ENTER {
                self.lock_submit();
                return true;
            }
            if let Some(c) = key::as_char(code) {
                if c.is_ascii() && !c.is_control() {
                    if self.lock_kind == 1 && !c.is_ascii_digit() {
                        return false;
                    }
                    self.lock_push_byte(c as u8);
                    return true;
                }
            }
            return false;
        }
        if code == key::ESCAPE {
            return self.close_top();
        }
        if !self.search_open {
            return false;
        }
        if code == key::BACKSPACE {
            self.pop_char();
            return true;
        }
        if let Some(c) = key::as_char(code) {
            if !c.is_control() {
                self.push_char(c);
                return true;
            }
        }
        false
    }

    fn hit(&self, ev: &EventBlock, g: &Geo, x: i32, y: i32) -> Kind {
        if self.locked {
            for idx in 0..12 {
                if lock_keypad_rect(g, idx).contains(x, y) {
                    return Kind::LockKey(idx as u8);
                }
            }
            return Kind::Consumed;
        }
        // Persistent 3-button navigation bar is always active when unlocked.
        if y >= g.h - g.nav_h {
            return Kind::Nav(g.nav_button(x));
        }
        if self.cc_open {
            return if g.cc.contains(x, y) { Kind::Cc } else { Kind::CcOut };
        }
        if self.wpick_open {
            return if g.wpick_sheet().contains(x, y) { Kind::WpickSheet } else { Kind::WpickOut };
        }
        if self.cust_open {
            return if g.cust_sheet().contains(x, y) { Kind::CustSheet } else { Kind::CustOut };
        }
        if self.search_open {
            if !g.sheet.contains(x, y) {
                return Kind::SearchOut;
            }
            let mut found = [0u8; 8];
            let n = self.matches(ev, &mut found);
            if self.qlen > 0 {
                for i in 0..n {
                    if g.search_row(i).contains(x, y) {
                        return Kind::DrawerApp(found[i]);
                    }
                }
            } else {
                let mut k = 0usize;
                for (i, app) in self.apps_slice().iter().enumerate() {
                    if !is_launcher_visible(app) {
                        continue;
                    }
                    if k >= 8 {
                        break;
                    }
                    let (sx, sy) = g.search_suggest(k);
                    if R::new(sx - g.dp.d(6), sy - g.dp.d(4), g.icon + g.dp.d(12), g.icon + g.dp.d(24)).contains(x, y) {
                        return Kind::DrawerApp(i as u8);
                    }
                    k += 1;
                }
            }
            return Kind::Search;
        }
        if self.folder_open {
            if g.folder_ungroup().contains(x, y) {
                return Kind::Folder(2);
            }
            let fp = (self.folder_page as usize).min(MAX_PAGES - 1);
            if let Cell::Folder(slots) = self.pages[fp][self.folder_cell] {
                let mut k = 0;
                for slot in slots.iter() {
                    let Some(a) = *slot else { continue };
                    let (cx, cy) = g.folder_slot(k);
                    if R::new(cx - g.icon / 2 - g.dp.d(6), cy - g.icon / 2 - g.dp.d(4), g.icon + g.dp.d(12), g.icon + g.dp.d(26)).contains(x, y) {
                        return Kind::Folder(10 + a);
                    }
                    k += 1;
                }
            }
            if g.folder_card().contains(x, y) {
                return Kind::Folder(1);
            }
            return Kind::Folder(0);
        }
        if self.recents_open {
            if y >= g.h - g.nav_h {
                return Kind::Nav(g.nav_button(x));
            }
            let list = self.apps_slice();
            let n = list.iter().filter(|a| a.state != apps::STATE_STOPPED && a.surface_id != 0).count();
            if n > 0 {
                if g.recents_clear().contains(x, y) {
                    return Kind::RecClear;
                }
                let mut k = 0;
                for (i, app) in list.iter().enumerate() {
                    if app.state == apps::STATE_STOPPED || app.surface_id == 0 {
                        continue;
                    }
                    if g.recent_card(k, n).contains(x, y) {
                        return Kind::RecCard(i as u8);
                    }
                    k += 1;
                }
            }
            return Kind::RecBg;
        }
        if self.app_open {
            if y < g.status_h + g.dp.d(6) {
                return Kind::Status;
            }
            if y >= g.h - g.nav_h {
                return Kind::Nav(g.nav_button(x));
            }
            return if self.app_p >= 256 { Kind::Child } else { Kind::Consumed };
        }
        if y < g.status_h + g.dp.d(4) {
            return Kind::Status;
        }
        if self.drawer_p > 200 {
            if g.drawer_search().contains(x, y) {
                return Kind::DrawerSearch;
            }
            let mut k = 0usize;
            for (i, app) in self.apps_slice().iter().enumerate() {
                if !is_launcher_visible(app) {
                    continue;
                }
                let (ix, iy) = g.drawer_cell(k);
                if R::new(ix - g.dp.d(8), iy - g.dp.d(4), g.icon + g.dp.d(16), g.icon + g.dp.d(26)).contains(x, y) {
                    return Kind::DrawerApp(i as u8);
                }
                k += 1;
            }
            return Kind::DrawerSheet;
        }

        // Edit Mode top toolbar
        if self.edit_mode {
            if g.edit_bar_add().grow(g.dp.d(4)).contains(x, y) {
                return Kind::EditAdd;
            }
            if g.edit_bar_customize().grow(g.dp.d(4)).contains(x, y) {
                return Kind::EditCustomize;
            }
            if g.edit_bar_done().grow(g.dp.d(4)).contains(x, y) {
                return Kind::EditDone;
            }
        }

        let edit_dy = self.edit_p * g.dp.d(28) / 256;
        let page_dx = self.page as i32 * g.w - self.page_scroll_px;

        // Widgets on current page
        for (wi, w) in self.widgets.iter().enumerate() {
            if !w.active || w.page != self.page as u8 {
                continue;
            }
            let wr = g.widget_rect(w.col, w.row, w.size, page_dx, edit_dy);
            if self.edit_mode {
                if g.widget_remove_badge(wr).grow(g.dp.d(6)).contains(x, y) {
                    return Kind::WidgetRemove(wi as u8);
                }
                if g.widget_resize_badge(wr).grow(g.dp.d(4)).contains(x, y) {
                    return Kind::WidgetResize(wi as u8);
                }
            }
            if wr.contains(x, y) {
                return Kind::Widget(wi as u8);
            }
        }

        // App icons on current page
        let cap = g.capacity().min(MAX_CELLS);
        for i in 0..cap {
            if cell_covered_by_widgets(&self.widgets, self.page as u8, i, g.cols) {
                continue;
            }
            if self.pages[self.page][i] != Cell::Empty && g.cell_rect_ex(i, page_dx, edit_dy).contains(x, y) {
                return Kind::Icon(Slot::Grid(self.page as u8, i));
            }
        }
        for i in 0..DOCK_SLOTS {
            if self.dock[i].is_some() && g.dock_rect(i).contains(x, y) {
                return Kind::Icon(Slot::Dock(i));
            }
        }
        Kind::Page
    }

    pub fn pointer_update(&mut self, ev: &EventBlock, g: &Geo) -> bool {
        let Some(p) = pointer(ev) else { return false };
        let mut dirty = false;
        if p.clicked || (p.down && !self.is_down) {
            dirty |= self.press(ev, g, p.x, p.y);
        }
        if self.is_down && p.down {
            dirty |= self.hold(ev, g, p.x, p.y);
        }
        if self.is_down && !p.down {
            dirty |= self.release(ev, g, p.x, p.y);
        }
        dirty
    }

    fn forward(&mut self, g: &Geo, x: i32, y: i32, flags: u32) {
        let cx = x.clamp(0, g.w - 1);
        let cy = (y - g.status_h).clamp(0, g.app.h - 1);
        let _ = sys::wm_forward_touch(cx, cy, flags);
    }

    fn press(&mut self, ev: &EventBlock, g: &Geo, x: i32, y: i32) -> bool {
        let kind = self.hit(ev, g, x, y);
        self.is_down = true;
        self.gest = Gest { kind, sx: x, sy: y, t0: ev.uptime_ms, x, y, moved: false, lifted: false, axis: 0, base: 0, fx: -1, fy: -1 };
        self.pressed = None;
        self.swipe_card = -1;
        self.swipe_dy = 0;
        match kind {
            Kind::Child => {
                self.forward(g, x, y, ev.pointer_flags);
                self.gest.fx = x;
                self.gest.fy = y;
            }
            Kind::Icon(slot) => self.pressed = Some(slot),
            Kind::Cc => {
                if g.cc_bright_card().grow(g.dp.d(4)).contains(x, y) {
                    self.gest.base = 1;
                    self.set_brightness(g, x);
                }
            }
            Kind::DrawerSheet | Kind::DrawerApp(_) | Kind::DrawerSearch => self.gest.base = self.drawer_p,
            Kind::Page => self.gest.base = self.page_scroll_px,
            Kind::RecCard(i) => self.swipe_card = i as i32,
            _ => {}
        }
        true
    }

    fn set_brightness(&mut self, g: &Geo, x: i32) {
        let t = g.cc_bright_card();
        let rel = (x - t.x).clamp(0, t.w);
        let pct = 20 + rel * 80 / t.w.max(1);
        self.brightness_touch = pct;
        let rounded = (((pct + 5) / 10) * 10).clamp(20, 100) as u32;
        if rounded as i32 != self.gest.fy {
            self.gest.fy = rounded as i32;
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_BRIGHTNESS, rounded);
        }
    }

    fn hold(&mut self, ev: &EventBlock, g: &Geo, x: i32, y: i32) -> bool {
        let dx = x - self.gest.sx;
        let dy = y - self.gest.sy;
        let slop = g.dp.d(8);
        let mut dirty = false;
        self.gest.x = x;
        self.gest.y = y;
        if !self.gest.moved && (dx.abs() > slop || dy.abs() > slop) {
            self.gest.moved = true;
            self.gest.axis = if dx.abs() > dy.abs() { 1 } else { 2 };
        }
        let held = ev.uptime_ms.wrapping_sub(self.gest.t0);
        let kind = self.gest.kind;
        match kind {
            Kind::Child => {
                if self.gest.fx != x || self.gest.fy != y {
                    self.gest.fx = x;
                    self.gest.fy = y;
                    self.forward(g, x, y, PTR_HAS | PTR_DOWN);
                }
            }
            Kind::Status => {
                // Smooth touch-following Control Center pull-down from top edge
                if dy > g.dp.d(6) {
                    self.drag_cc = true;
                    let span = g.cc.h.max(120);
                    self.cc_p = (dy * 256 / span).clamp(0, 256);
                    dirty = true;
                }
            }
            Kind::Cc => {
                if self.gest.base == 1 {
                    self.set_brightness(g, x);
                    dirty = true;
                } else if !self.gest.moved && held >= LONG_PRESS_MS && g.cc_tile(0).contains(x, y) {
                    // Long-press Wi-Fi tile toggles inline hardware detail
                    self.cc_wifi_detail = !self.cc_wifi_detail;
                    self.gest.kind = Kind::Consumed;
                    dirty = true;
                } else if dy < -g.dp.d(12) {
                    self.drag_cc = true;
                    let span = g.cc.h.max(120);
                    self.cc_p = (256 + dy * 256 / span).clamp(0, 256);
                    dirty = true;
                }
            }
            Kind::Icon(slot) => {
                if self.gest.lifted {
                    self.drag_x = x;
                    self.drag_y = y;
                    dirty = true;
                } else if !self.gest.moved && held >= LONG_PRESS_MS && self.app_p == 0 {
                    self.gest.lifted = true;
                    self.lift = Some(slot);
                    self.pressed = None;
                    self.drag_x = x;
                    self.drag_y = y;
                    dirty = true;
                } else if self.gest.moved {
                    self.gest.kind = Kind::Page;
                    self.gest.base = self.page_scroll_px;
                    self.pressed = None;
                }
            }
            Kind::Page => {
                if !self.gest.moved && held >= LONG_PRESS_MS + 120 && self.app_p == 0 && !self.edit_mode {
                    // Long-press empty home space enters Edit Mode
                    self.edit_mode = true;
                    self.gest.kind = Kind::Consumed;
                    dirty = true;
                } else if self.gest.moved && self.gest.axis == 1 {
                    // Horizontal multi-page swipe with rubber-band edge resistance
                    self.drag_page = true;
                    let max_px = (self.page_count.saturating_sub(1)) as i32 * g.w;
                    let raw = self.gest.base - dx;
                    let clamped = if raw < 0 {
                        raw / 3
                    } else if raw > max_px {
                        max_px + (raw - max_px) / 3
                    } else {
                        raw
                    };
                    self.page_scroll_px = clamped;
                    dirty = true;
                } else if self.gest.moved && self.gest.axis == 2 && !self.edit_mode {
                    if dy < 0 {
                        self.drag_drawer = true;
                        self.drawer_p = (-dy * 256 / (g.h * 52 / 100).max(1)).clamp(0, 256);
                        dirty = true;
                    } else if dy > g.dp.d(24) && !self.drag_drawer {
                        self.drag_cc = true;
                        let span = g.cc.h.max(120);
                        self.cc_p = ((dy - g.dp.d(24)) * 256 / span).clamp(0, 256);
                        dirty = true;
                    }
                }
            }
            Kind::DrawerSheet | Kind::DrawerApp(_) | Kind::DrawerSearch => {
                if self.gest.moved && self.gest.axis == 2 && dy > 0 {
                    self.drag_drawer = true;
                    self.drawer_p = (256 - dy * 256 / (g.h * 52 / 100).max(1)).clamp(0, 256);
                    dirty = true;
                }
            }
            Kind::RecCard(_) => {
                self.swipe_dy = dy.min(g.dp.d(20));
                dirty = true;
            }
            _ => {}
        }
        dirty
    }

    fn release(&mut self, ev: &EventBlock, g: &Geo, x: i32, y: i32) -> bool {
        self.is_down = false;
        let gest = self.gest;
        let kind = gest.kind;
        self.pressed = None;
        let tap = !gest.moved;
        let now = ev.uptime_ms;
        let same = self.hit(ev, g, x, y) == kind;

        match kind {
            Kind::LockKey(k) => {
                if tap {
                    match k {
                        0..=8 => self.lock_push_byte(b'1' + k),
                        9 => self.lock_pop_byte(),
                        10 => self.lock_push_byte(b'0'),
                        11 => self.lock_submit(),
                        _ => {}
                    }
                }
            }
            Kind::Child => {
                self.forward(g, x, y, PTR_HAS);
            }
            Kind::Status => {
                if self.drag_cc {
                    self.drag_cc = false;
                    self.cc_open = self.cc_p > 72;
                    if self.cc_open {
                        self.refresh_status(now);
                        self.refresh_lock_status();
                    }
                } else if tap {
                    self.cc_open = !self.cc_open;
                    self.cc_wifi_detail = false;
                    self.refresh_status(now);
                    self.refresh_lock_status();
                }
            }
            Kind::CcOut => {
                if tap {
                    self.cc_open = false;
                    self.cc_wifi_detail = false;
                }
            }
            Kind::Cc => {
                self.brightness_touch = -1;
                if self.drag_cc {
                    self.drag_cc = false;
                    self.cc_open = self.cc_p > 170;
                    if !self.cc_open {
                        self.cc_wifi_detail = false;
                    }
                } else if tap && gest.base == 0 {
                    self.handle_cc_tap(ev, g, x, y, now);
                }
            }
            Kind::EditAdd => {
                if tap {
                    self.wpick_open = true;
                }
            }
            Kind::EditCustomize => {
                if tap {
                    self.cust_open = true;
                }
            }
            Kind::EditDone => {
                if tap {
                    self.edit_mode = false;
                    self.wpick_open = false;
                    self.cust_open = false;
                }
            }
            Kind::WpickOut => {
                if tap {
                    self.wpick_open = false;
                }
            }
            Kind::WpickSheet => {
                if tap {
                    let sizes = [WidgetSize::Small, WidgetSize::Medium, WidgetSize::Large];
                    for (i, sz) in sizes.iter().enumerate() {
                        if g.wpick_size_chip(i).contains(x, y) {
                            self.wpick_size = *sz;
                        }
                    }
                    for i in 0..PROVIDER_COUNT {
                        if g.wpick_row(i).contains(x, y) {
                            if self.add_widget(i as u8, self.wpick_size, g) {
                                self.wpick_open = false;
                            }
                            break;
                        }
                    }
                }
            }
            Kind::CustOut => {
                if tap {
                    self.cust_open = false;
                }
            }
            Kind::CustSheet => {
                if tap {
                    self.handle_customize_tap(ev, g, x, y, now);
                }
            }
            Kind::WidgetRemove(wi) => {
                if tap {
                    self.remove_widget(wi as usize);
                }
            }
            Kind::WidgetResize(wi) => {
                if tap {
                    self.resize_widget(wi as usize, g);
                }
            }
            Kind::Widget(wi) => {
                if tap {
                    let idx = wi as usize;
                    if idx < MAX_WIDGETS && self.widgets[idx].active {
                        let w = self.widgets[idx];
                        let edit_dy = self.edit_p * g.dp.d(28) / 256;
                        let page_dx = self.page as i32 * g.w - self.page_scroll_px;
                        let wr = g.widget_rect(w.col, w.row, w.size, page_dx, edit_dy);
                        let ctx = self.build_widget_ctx(ev, &w, WidgetRect { x: wr.x, y: wr.y, w: wr.w, h: wr.h });
                        if let Some(prov) = provider_by_idx(w.provider_idx) {
                            match prov.on_tap(x - wr.x, y - wr.y, &ctx) {
                                WidgetAction::OpenControlCenter => {
                                    self.cc_open = true;
                                    self.refresh_status(now);
                                }
                                WidgetAction::ToggleWifi => {
                                    if self.sys_status.wifi_hw {
                                        let next = !self.sys_status.wifi_enabled;
                                        let _ = sys::mobile_set(sys_wire::MOBILE_SET_WIFI, next as u32);
                                        self.refresh_status(now);
                                    }
                                }
                                WidgetAction::ToggleTheme => {
                                    let next = !unpack_dark_theme(ev.flags);
                                    let _ = sys::mobile_set(sys_wire::MOBILE_SET_DARK_THEME, next as u32);
                                    self.refresh_status(now);
                                }
                                WidgetAction::LaunchApp(app_id) => {
                                    if let Some(aidx) = self.find_app_idx(app_id) {
                                        self.launch(aidx, (wr.cx() - g.icon / 2, wr.cy() - g.icon / 2, g.icon), now);
                                    }
                                }
                                WidgetAction::ToggleConfigBit(bit) => {
                                    self.toggle_widget_config(idx, bit);
                                }
                                WidgetAction::Redraw | WidgetAction::None => {}
                            }
                        }
                    }
                }
            }
            Kind::SearchOut => {
                if tap {
                    self.search_open = false;
                    self.qlen = 0;
                    let _ = nc_app_sdk::ecosystem::ime::hide();
                }
            }
            Kind::Folder(code) => {
                if tap {
                    match code {
                        0 => self.folder_open = false,
                        1 => {}
                        2 => {
                            let cap = g.capacity();
                            self.ungroup(self.folder_page, self.folder_cell, cap);
                        }
                        c if c >= 10 => {
                            let app = (c - 10) as usize;
                            let fp = (self.folder_page as usize).min(MAX_PAGES - 1);
                            let slots = match self.pages[fp][self.folder_cell] {
                                Cell::Folder(s) => s,
                                _ => [None; FOLDER_SLOTS],
                            };
                            let k = slots.iter().flatten().position(|a| *a as usize == app).unwrap_or(0);
                            let (cx, cy) = g.folder_slot(k);
                            self.launch(app, (cx - g.icon / 2, cy - g.icon / 2, g.icon), now);
                        }
                        _ => {}
                    }
                }
            }
            Kind::DrawerSearch => {
                if self.drag_drawer {
                    self.finish_drawer_drag();
                } else if tap {
                    self.drawer_open = false;
                    self.search_open = true;
                    self.qlen = 0;
                    let _ = nc_app_sdk::ecosystem::ime::show();
                }
            }
            Kind::DrawerApp(i) => {
                if gest.moved || self.drag_drawer {
                    self.finish_drawer_drag();
                } else if tap {
                    let origin = if self.search_open {
                        let (sx, sy) = g.search_suggest(0);
                        (sx, sy, g.icon)
                    } else {
                        let (ix, iy) = g.drawer_cell(i as usize);
                        (ix, iy, g.icon)
                    };
                    self.launch(i as usize, origin, now);
                }
            }
            Kind::DrawerSheet => {
                if self.drag_drawer {
                    self.finish_drawer_drag();
                } else if tap && y < g.drawer_y + g.dp.d(26) {
                    self.drawer_open = false;
                }
            }
            Kind::Nav(i) => {
                if tap && g.nav_button(x) == i {
                    match i {
                        0 => self.go_back(),
                        1 => self.go_home(),
                        _ => self.toggle_recents(),
                    }
                }
            }
            Kind::RecClear => {
                if tap {
                    for i in 0..self.reg_count {
                        let mut id_copy = [0u8; 36];
                        let len = self.registry[i].id_len as usize;
                        id_copy[..len].copy_from_slice(&self.registry[i].id[..len]);
                        if let Ok(id) = core::str::from_utf8(&id_copy[..len]) {
                            let _ = apps::stop(id);
                        }
                    }
                    self.refresh_registry();
                    self.recents_open = false;
                }
            }
            Kind::RecBg => {
                if tap {
                    self.recents_open = false;
                }
            }
            Kind::RecCard(i) => {
                let dy = self.swipe_dy;
                self.swipe_card = -1;
                self.swipe_dy = 0;
                if dy < -g.dp.d(36) {
                    if let Some(app) = self.registry.get(i as usize) {
                        let mut id_copy = [0u8; 36];
                        let len = app.id_len as usize;
                        id_copy[..len].copy_from_slice(&app.id[..len]);
                        if let Ok(id) = core::str::from_utf8(&id_copy[..len]) {
                            let _ = apps::stop(id);
                        }
                        self.refresh_registry();
                    }
                } else if dy.abs() < g.dp.d(12) {
                    let list = self.apps_slice();
                    let n = list.iter().filter(|a| a.state != apps::STATE_STOPPED && a.surface_id != 0).count();
                    let k = list.iter().take(i as usize).filter(|a| a.state != apps::STATE_STOPPED && a.surface_id != 0).count();
                    let r = g.recent_card(k, n);
                    self.launch(i as usize, (r.x, r.y, r.w), now);
                }
            }
            Kind::Icon(slot) => {
                if gest.lifted {
                    self.drop_icon(g, slot, x, y);
                } else if tap && same && !self.edit_mode {
                    match self.get(slot) {
                        Cell::App(a) => {
                            let from = match slot {
                                Slot::Grid(_, i) => {
                                    let (ix, iy) = g.cell_origin(i);
                                    (ix, iy, g.icon)
                                }
                                Slot::Dock(i) => g.dock_origin(i),
                            };
                            self.launch(a as usize, from, now);
                        }
                        Cell::Folder(_) => {
                            if let Slot::Grid(p, i) = slot {
                                self.folder_page = p;
                                self.folder_cell = i;
                                self.folder_open = true;
                            }
                        }
                        Cell::Empty => {}
                    }
                }
                self.lift = None;
            }
            Kind::Page => {
                if self.drag_page {
                    self.drag_page = false;
                    let dx = x - gest.sx;
                    let threshold = g.w / 5;
                    if dx < -threshold && self.page + 1 < self.page_count {
                        self.page += 1;
                    } else if dx > threshold && self.page > 0 {
                        self.page -= 1;
                    }
                } else if self.drag_cc {
                    self.drag_cc = false;
                    self.cc_open = self.cc_p > 72;
                    if self.cc_open {
                        self.refresh_status(now);
                        self.refresh_lock_status();
                    }
                } else if self.drag_drawer {
                    self.finish_drawer_drag();
                }
            }
            _ => {}
        }
        self.lift = None;
        self.gest.kind = Kind::None;
        true
    }

    fn handle_cc_tap(&mut self, ev: &EventBlock, g: &Geo, x: i32, y: i32, now: u32) {
        if self.lock_enabled && g.cc_lock_btn().grow(g.dp.d(4)).contains(x, y) {
            self.cc_open = false;
            self.cc_wifi_detail = false;
            let _ = security::lock_now();
            self.refresh_lock_status();
            return;
        }
        // Tile 0: Wi-Fi
        if g.cc_tile(0).contains(x, y) {
            if self.sys_status.wifi_hw {
                let next = !self.sys_status.wifi_enabled;
                let _ = sys::mobile_set(sys_wire::MOBILE_SET_WIFI, next as u32);
                self.refresh_status(now);
            }
            return;
        }
        // Tile 1: Bluetooth (only if hardware exists)
        if g.cc_tile(1).contains(x, y) {
            if self.sys_status.bt_hw {
                let next = !self.sys_status.bt_enabled;
                let _ = sys::mobile_set(sys_wire::MOBILE_SET_BT, next as u32);
                self.refresh_status(now);
            }
            return;
        }
        // Tile 2: Mobile Data (only if modem exists)
        if g.cc_tile(2).contains(x, y) {
            // No modem hardware active yet; intentionally non-mutating
            return;
        }
        // Tile 3: Do Not Disturb
        if g.cc_tile(3).contains(x, y) {
            let next = !self.sys_status.dnd;
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_DND, next as u32);
            self.refresh_status(now);
            return;
        }
        // Quick Action: Light / Dark Theme
        if g.cc_theme_btn().contains(x, y) {
            let next = !unpack_dark_theme(ev.flags);
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_DARK_THEME, next as u32);
            self.refresh_status(now);
            return;
        }
        // Quick Action: Airplane Mode
        if g.cc_airplane_btn().contains(x, y) {
            let next = !self.sys_status.airplane;
            let _ = sys::mobile_set(sys_wire::MOBILE_SET_AIRPLANE, next as u32);
            self.refresh_status(now);
            return;
        }
        // Footer: Settings or dismiss Wi-Fi detail
        if g.cc_settings().contains(x, y) {
            if self.cc_wifi_detail {
                self.cc_wifi_detail = false;
            } else if let Some(sidx) = self.find_app_idx("nc.settings") {
                self.launch(sidx, (g.cc_settings().cx() - g.icon / 2, g.cc_settings().y, g.icon), now);
            }
        }
    }

    fn handle_customize_tap(&mut self, ev: &EventBlock, g: &Geo, x: i32, y: i32, now: u32) {
        for i in 0..8usize {
            if !g.cust_row(i).contains(x, y) {
                continue;
            }
            match i {
                0 => {
                    let next = !unpack_dark_theme(ev.flags);
                    let _ = sys::mobile_set(sys_wire::MOBILE_SET_DARK_THEME, next as u32);
                    self.refresh_status(now);
                }
                1 => {
                    let next = (unpack_wallpaper(ev.flags) + 1) % 5;
                    let _ = sys::mobile_set(sys_wire::MOBILE_SET_WALLPAPER, next as u32);
                }
                2 => {
                    self.prefs.density = match self.prefs.density {
                        GridDensity::Balanced => GridDensity::Compact,
                        GridDensity::Compact => GridDensity::Large,
                        GridDensity::Large => GridDensity::Balanced,
                    };
                    self.save_layout();
                }
                3 => {
                    self.prefs.icon_size = match self.prefs.icon_size {
                        IconSizePref::Small => IconSizePref::Medium,
                        IconSizePref::Medium => IconSizePref::Large,
                        IconSizePref::Large => IconSizePref::Small,
                    };
                    self.save_layout();
                }
                4 => {
                    self.prefs.show_labels = !self.prefs.show_labels;
                    self.save_layout();
                }
                5 => {
                    self.prefs.pill_indicator = !self.prefs.pill_indicator;
                    self.save_layout();
                }
                6 => {
                    self.prefs.transition = match self.prefs.transition {
                        TransitionStyle::Slide => TransitionStyle::Fade,
                        TransitionStyle::Fade => TransitionStyle::Depth,
                        TransitionStyle::Depth => TransitionStyle::Slide,
                    };
                    self.save_layout();
                }
                7 => {
                    let next_scale = if self.reduced_motion() { 100 } else { 0 };
                    let _ = sys::mobile_set(sys_wire::MOBILE_SET_ANIM_SCALE, next_scale);
                    self.refresh_status(now);
                }
                _ => {}
            }
            return;
        }
    }

    fn finish_drawer_drag(&mut self) {
        self.drag_drawer = false;
        self.drawer_open = self.drawer_p > 88;
        if self.drawer_open {
            self.refresh_registry();
        }
    }

    fn drop_icon(&mut self, g: &Geo, from: Slot, x: i32, y: i32) {
        let mut target = None;
        if g.dock.grow(g.dp.d(6)).contains(x, y) {
            for i in 0..DOCK_SLOTS {
                if g.dock_rect(i).contains(x, y) {
                    target = Some(Slot::Dock(i));
                }
            }
        } else {
            let edit_dy = self.edit_p * g.dp.d(28) / 256;
            let page_dx = self.page as i32 * g.w - self.page_scroll_px;
            let cap = g.capacity().min(MAX_CELLS);
            for i in 0..cap {
                if cell_covered_by_widgets(&self.widgets, self.page as u8, i, g.cols) {
                    continue;
                }
                if g.cell_rect_ex(i, page_dx, edit_dy).grow(g.dp.d(4)).contains(x, y) {
                    target = Some(Slot::Grid(self.page as u8, i));
                }
            }
        }
        if let Some(to) = target {
            self.move_or_group(from, to);
        }
    }
}
