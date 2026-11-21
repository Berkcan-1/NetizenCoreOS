use nc_abi::event::{unpack_dark_theme, unpack_fg_app, unpack_locked, unpack_tz_hours, EventBlock};
use nc_app_sdk::status::{self, SystemStatus};
use nc_app_sdk::widget::{WidgetContext, WidgetInstance, WidgetRect, WidgetSize};
use nc_app_sdk::{apps, security, sys};

use crate::layout::Geo;
use crate::model::*;
use crate::motion::*;
use crate::persist;
use crate::view::wall_clock;
use crate::widgets::{
    provider_by_idx, provider_meta, PROVIDER_BATTERY, PROVIDER_CLOCK, PROVIDER_SYSINFO,
};

pub const LONG_PRESS_MS: u32 = 300;
pub const STATUS_POLL_MS: u32 = 2500;

static mut REG_RESP: [u8; 4096] = [0; 4096];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    None,
    Consumed,
    Status,
    Nav(u8),
    Child,
    Cc,
    CcOut,
    Search,
    SearchOut,
    Folder(u8),
    RecCard(u8),
    RecClear,
    RecBg,
    Icon(Slot),
    Widget(u8),
    WidgetRemove(u8),
    WidgetResize(u8),
    EditAdd,
    EditCustomize,
    EditDone,
    WpickSheet,
    WpickOut,
    CustSheet,
    CustOut,
    DrawerApp(u8),
    DrawerSearch,
    DrawerSheet,
    Page,
    LockKey(u8),
}

#[derive(Clone, Copy)]
pub struct Gest {
    pub kind: Kind,
    pub sx: i32,
    pub sy: i32,
    pub t0: u32,
    pub x: i32,
    pub y: i32,
    pub moved: bool,
    pub lifted: bool,
    pub axis: u8,
    pub base: i32,
    pub fx: i32,
    pub fy: i32,
}

impl Gest {
    pub const fn new() -> Gest {
        Gest { kind: Kind::None, sx: 0, sy: 0, t0: 0, x: 0, y: 0, moved: false, lifted: false, axis: 0, base: 0, fx: -1, fy: -1 }
    }
}

pub struct HomeShell {
    pub stamp: i64,
    pub last_flags: u32,
    pub inset: (i32, i32),
    pub launch_ms: u32,
    pub last_status_ms: u32,
    pub sys_status: SystemStatus,

    pub registry: [apps::AppEntry; MAX_APPS],
    pub reg_count: usize,
    pub reg_gen: u32,

    pub prefs: HomePrefs,
    pub layout_loaded: bool,
    pub page: usize,
    pub page_count: usize,
    pub pages: [[Cell; MAX_CELLS]; MAX_PAGES],
    pub dock: [Option<u8>; DOCK_SLOTS],
    pub widgets: [WidgetInstance; MAX_WIDGETS],
    pub next_widget_id: u16,

    // Lock screen state (enforced in kernel; UI renders prompt & sends OP_LOCK_AUTH)
    pub locked: bool,
    pub lock_enabled: bool,
    pub lock_kind: u8,
    pub lock_fails: u32,
    pub lock_lockout_s: u32,
    pub lock_show_info: bool,
    pub lock_buf: [u8; 32],
    pub lock_len: usize,
    pub lock_err: bool,

    pub t_app: Tween,
    pub t_nav: Tween,
    pub t_cc: Tween,
    pub t_rec: Tween,
    pub t_drawer: Tween,
    pub t_folder: Tween,
    pub t_search: Tween,
    pub t_lift: Tween,
    pub t_page: Tween,
    pub t_edit: Tween,
    pub t_wpick: Tween,
    pub t_cust: Tween,

    pub app_p: i32,
    pub nav_p: i32,
    pub cc_p: i32,
    pub rec_p: i32,
    pub drawer_p: i32,
    pub folder_p: i32,
    pub search_p: i32,
    pub lift_p: i32,
    pub page_scroll_px: i32,
    pub edit_p: i32,
    pub wpick_p: i32,
    pub cust_p: i32,

    pub app_open: bool,
    pub cc_open: bool,
    pub cc_wifi_detail: bool,
    pub drag_cc: bool,
    pub recents_open: bool,
    pub drawer_open: bool,
    pub search_open: bool,
    pub folder_open: bool,
    pub folder_page: u8,
    pub folder_cell: usize,
    pub drag_drawer: bool,
    pub drag_page: bool,
    pub edit_mode: bool,
    pub wpick_open: bool,
    pub wpick_size: WidgetSize,
    pub cust_open: bool,

    pub active_surface: u32,
    pub launch_app: u8,
    pub origin: (i32, i32, i32),

    pub is_down: bool,
    pub gest: Gest,
    pub pressed: Option<Slot>,
    pub lift: Option<Slot>,
    pub drag_x: i32,
    pub drag_y: i32,
    pub swipe_dy: i32,
    pub swipe_card: i32,
    pub brightness_touch: i32,

    pub query: [u8; 32],
    pub qlen: usize,
}

impl HomeShell {
    pub const fn new() -> Self {
        Self {
            stamp: -1,
            last_flags: 0,
            inset: (0, 0),
            launch_ms: 0,
            last_status_ms: 0,
            sys_status: SystemStatus::EMPTY,
            registry: [apps::AppEntry::EMPTY; MAX_APPS],
            reg_count: 0,
            reg_gen: 0,
            prefs: HomePrefs::DEFAULT,
            layout_loaded: false,
            page: 0,
            page_count: 2,
            pages: [[Cell::Empty; MAX_CELLS]; MAX_PAGES],
            dock: [None; DOCK_SLOTS],
            widgets: [WidgetInstance::EMPTY; MAX_WIDGETS],
            next_widget_id: 1,
            locked: false,
            lock_enabled: false,
            lock_kind: 1,
            lock_fails: 0,
            lock_lockout_s: 0,
            lock_show_info: true,
            lock_buf: [0; 32],
            lock_len: 0,
            lock_err: false,
            t_app: Tween::new(),
            t_nav: Tween::new(),
            t_cc: Tween::new(),
            t_rec: Tween::new(),
            t_drawer: Tween::new(),
            t_folder: Tween::new(),
            t_search: Tween::new(),
            t_lift: Tween::new(),
            t_page: Tween::new(),
            t_edit: Tween::new(),
            t_wpick: Tween::new(),
            t_cust: Tween::new(),
            app_p: 0,
            nav_p: 0,
            cc_p: 0,
            rec_p: 0,
            drawer_p: 0,
            folder_p: 0,
            search_p: 0,
            lift_p: 0,
            page_scroll_px: 0,
            edit_p: 0,
            wpick_p: 0,
            cust_p: 0,
            app_open: false,
            cc_open: false,
            cc_wifi_detail: false,
            drag_cc: false,
            recents_open: false,
            drawer_open: false,
            search_open: false,
            folder_open: false,
            folder_page: 0,
            folder_cell: 0,
            drag_drawer: false,
            drag_page: false,
            edit_mode: false,
            wpick_open: false,
            wpick_size: WidgetSize::Medium,
            cust_open: false,
            active_surface: 0,
            launch_app: 0,
            origin: (0, 0, 0),
            is_down: false,
            gest: Gest::new(),
            pressed: None,
            lift: None,
            drag_x: 0,
            drag_y: 0,
            swipe_dy: 0,
            swipe_card: -1,
            brightness_touch: -1,
            query: [0; 32],
            qlen: 0,
        }
    }

    pub fn apps_slice(&self) -> &[apps::AppEntry] {
        &self.registry[..self.reg_count]
    }

    pub fn reduced_motion(&self) -> bool {
        self.sys_status.reduced_motion()
    }

    pub fn refresh_status(&mut self, now_ms: u32) -> bool {
        self.last_status_ms = now_ms;
        if let Some(st) = status::query() {
            let changed = st.bat_available != self.sys_status.bat_available
                || st.bat_percent != self.sys_status.bat_percent
                || st.bat_charging != self.sys_status.bat_charging
                || st.wifi_state != self.sys_status.wifi_state
                || st.wifi_enabled != self.sys_status.wifi_enabled
                || st.bt_state != self.sys_status.bt_state
                || st.cell_state != self.sys_status.cell_state
                || st.dnd != self.sys_status.dnd
                || st.airplane != self.sys_status.airplane
                || st.dark_theme != self.sys_status.dark_theme
                || st.bright_pct != self.sys_status.bright_pct
                || st.ram_free_mib != self.sys_status.ram_free_mib
                || st.usb_mounted != self.sys_status.usb_mounted;
            self.sys_status = st;
            changed
        } else {
            false
        }
    }

    pub fn refresh_lock_status(&mut self) {
        if let Some(st) = security::lock_status() {
            self.locked = st.locked;
            self.lock_enabled = st.enabled;
            self.lock_kind = if st.kind == 0 { 1 } else { st.kind };
            self.lock_fails = st.fails;
            self.lock_lockout_s = st.lockout_s;
            self.lock_show_info = st.show_info;
        }
    }

    pub fn refresh_registry(&mut self) {
        let resp = unsafe { &mut *core::ptr::addr_of_mut!(REG_RESP) };
        let Ok((gen, _fg_surf, count)) = apps::query_registry(resp, &mut self.registry) else {
            return;
        };
        let changed = gen != self.reg_gen || count != self.reg_count;
        self.reg_gen = gen;
        self.reg_count = count;
        if !self.layout_loaded {
            self.layout_loaded = true;
            if let Some(snap) = persist::load(self.apps_slice()) {
                self.prefs = snap.prefs;
                self.page_count = snap.page_count.clamp(1, MAX_PAGES);
                self.pages = snap.pages;
                self.dock = snap.dock;
                self.widgets = snap.widgets;
                let max_id = self.widgets.iter().map(|w| w.instance_id).max().unwrap_or(0);
                self.next_widget_id = max_id.saturating_add(1);
            } else {
                self.init_default_widgets();
            }
            self.reconcile_layout();
        } else if changed {
            self.reconcile_layout();
        }
    }

    fn init_default_widgets(&mut self) {
        self.widgets = [WidgetInstance::EMPTY; MAX_WIDGETS];
        self.page_count = 2;
        // Page 0: Balanced Clock (4x1) at top + System (2x1) & Battery (2x1) on row 1
        self.widgets[0] = WidgetInstance {
            instance_id: 1,
            provider_idx: PROVIDER_CLOCK,
            page: 0,
            col: 0,
            row: 0,
            size: WidgetSize::Medium,
            config_flags: 0,
            active: true,
        };
        self.widgets[1] = WidgetInstance {
            instance_id: 2,
            provider_idx: PROVIDER_SYSINFO,
            page: 1,
            col: 0,
            row: 0,
            size: WidgetSize::Small,
            config_flags: 0,
            active: true,
        };
        self.widgets[2] = WidgetInstance {
            instance_id: 3,
            provider_idx: PROVIDER_BATTERY,
            page: 1,
            col: 2,
            row: 0,
            size: WidgetSize::Small,
            config_flags: 0,
            active: true,
        };
        self.next_widget_id = 4;
    }

    pub fn save_layout(&self) {
        let _ = persist::save(
            self.apps_slice(),
            &self.prefs,
            self.page_count,
            &self.pages,
            &self.dock,
            &self.widgets,
        );
    }

    fn is_placed_on_home(&self, app_idx: u8) -> bool {
        if self.dock.iter().any(|s| *s == Some(app_idx)) {
            return true;
        }
        for p in 0..MAX_PAGES {
            for cell in self.pages[p].iter() {
                match cell {
                    Cell::App(a) if *a == app_idx => return true,
                    Cell::Folder(slots) if slots.iter().any(|s| *s == Some(app_idx)) => return true,
                    _ => {}
                }
            }
        }
        false
    }

    fn reconcile_layout(&mut self) {
        for p in 0..MAX_PAGES {
            for cell in self.pages[p].iter_mut() {
                match cell {
                    Cell::App(a) if (*a as usize) >= self.reg_count => *cell = Cell::Empty,
                    Cell::Folder(slots) => {
                        for s in slots.iter_mut() {
                            if let Some(a) = *s {
                                if (a as usize) >= self.reg_count {
                                    *s = None;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for d in self.dock.iter_mut() {
            if let Some(a) = *d {
                if (a as usize) >= self.reg_count {
                    *d = None;
                }
            }
        }
        if self.dock.iter().all(|s| s.is_none()) {
            let mut slot = 0;
            for want in ["nc.phone", "nc.messages", "nc.gallery", "nc.camera", "nc.files", "nc.settings"] {
                if let Some(idx) = self.find_app_idx(want) {
                    if slot < DOCK_SLOTS {
                        self.dock[slot] = Some(idx as u8);
                        slot += 1;
                    }
                }
            }
        }
        let cols = self.prefs.density.cols();
        let cap = cols * self.prefs.density.max_rows();
        for i in 0..self.reg_count {
            if !is_launcher_visible(&self.registry[i]) {
                continue;
            }
            if !self.is_placed_on_home(i as u8) {
                'place: for p in 0..self.page_count.min(MAX_PAGES) {
                    for idx in 0..cap.min(MAX_CELLS) {
                        if self.pages[p][idx] == Cell::Empty
                            && !cell_covered_by_widgets(&self.widgets, p as u8, idx, cols)
                        {
                            self.pages[p][idx] = Cell::App(i as u8);
                            break 'place;
                        }
                    }
                }
            }
        }
    }

    pub fn find_app_idx(&self, id: &str) -> Option<usize> {
        self.apps_slice().iter().position(|a| a.id_str() == id)
    }

    pub fn widget_area_free(&self, page: u8, col: usize, row: usize, size: WidgetSize, ignore_idx: Option<usize>, geo: &Geo) -> bool {
        let (sc, sr) = size.grid_span(geo.cols);
        let sc = sc as usize;
        let sr = sr as usize;
        if col + sc > geo.cols || row + sr > geo.rows {
            return false;
        }
        for r in row..row + sr {
            for c in col..col + sc {
                let idx = r * geo.cols + c;
                if self.pages[page as usize][idx] != Cell::Empty {
                    return false;
                }
                for (wi, w) in self.widgets.iter().enumerate() {
                    if ignore_idx == Some(wi) {
                        continue;
                    }
                    if widget_covers(w, page, c, r, geo.cols) {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn add_widget(&mut self, provider_idx: u8, preferred_size: WidgetSize, geo: &Geo) -> bool {
        let Some(meta) = provider_meta(provider_idx) else { return false };
        let size = if meta.supports(preferred_size) { preferred_size } else { meta.default_size };
        let Some(slot) = self.widgets.iter().position(|w| !w.active) else { return false };

        // Try current page first, then other pages, then create a new page if available.
        let mut try_pages = [self.page; MAX_PAGES];
        let mut n_try = 1usize;
        for p in 0..self.page_count.min(MAX_PAGES) {
            if p != self.page && n_try < MAX_PAGES {
                try_pages[n_try] = p;
                n_try += 1;
            }
        }
        if self.page_count < MAX_PAGES && n_try < MAX_PAGES {
            try_pages[n_try] = self.page_count;
            n_try += 1;
        }

        for &p in &try_pages[..n_try] {
            for r in 0..geo.rows {
                for c in 0..geo.cols {
                    if self.widget_area_free(p as u8, c, r, size, None, geo) {
                        if p >= self.page_count {
                            self.page_count = (p + 1).min(MAX_PAGES);
                        }
                        self.widgets[slot] = WidgetInstance {
                            instance_id: self.next_widget_id,
                            provider_idx,
                            page: p as u8,
                            col: c as u8,
                            row: r as u8,
                            size,
                            config_flags: 0,
                            active: true,
                        };
                        self.next_widget_id = self.next_widget_id.saturating_add(1);
                        self.page = p;
                        self.save_layout();
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn remove_widget(&mut self, w_idx: usize) {
        if w_idx < MAX_WIDGETS {
            self.widgets[w_idx] = WidgetInstance::EMPTY;
            self.save_layout();
        }
    }

    pub fn resize_widget(&mut self, w_idx: usize, geo: &Geo) -> bool {
        if w_idx >= MAX_WIDGETS || !self.widgets[w_idx].active {
            return false;
        }
        let w = self.widgets[w_idx];
        let Some(meta) = provider_meta(w.provider_idx) else { return false };
        let next = meta.next_size(w.size);
        if next == w.size {
            return false;
        }
        // Try keeping origin; if too wide for current col, shift left to col 0.
        let (sc, _) = next.grid_span(geo.cols);
        let try_col = if (w.col as usize) + (sc as usize) <= geo.cols { w.col as usize } else { 0 };
        if self.widget_area_free(w.page, try_col, w.row as usize, next, Some(w_idx), geo) {
            self.widgets[w_idx].col = try_col as u8;
            self.widgets[w_idx].size = next;
            self.save_layout();
            return true;
        }
        false
    }

    pub fn toggle_widget_config(&mut self, w_idx: usize, bit: u16) {
        if w_idx < MAX_WIDGETS && self.widgets[w_idx].active {
            self.widgets[w_idx].config_flags ^= bit;
            self.save_layout();
        }
    }

    pub fn build_widget_ctx(&self, ev: &EventBlock, w: &WidgetInstance, rect: WidgetRect) -> WidgetContext {
        let st = &self.sys_status;
        WidgetContext {
            rect,
            size: w.size,
            config_flags: w.config_flags,
            dark_theme: unpack_dark_theme(ev.flags),
            accent: ev.accent,
            lang_en: ev.lang == 1,
            wall_clock_unix: wall_clock(),
            tz_hours: unpack_tz_hours(ev.flags),
            uptime_s: st.uptime_s,
            bat_available: st.bat_available,
            bat_percent: st.bat_percent,
            bat_charging: st.bat_charging,
            bat_full: st.bat_full,
            bat_voltage_mv: st.bat_voltage_mv,
            wifi_hw: st.wifi_hw,
            wifi_enabled: st.wifi_enabled,
            wifi_state: st.wifi_state,
            bt_hw: st.bt_hw,
            bt_enabled: st.bt_enabled,
            cell_hw: st.cell_hw,
            dnd: st.dnd,
            ram_total_mib: st.ram_total_mib,
            ram_free_mib: st.ram_free_mib,
            data_total_mib: st.data_total_mib,
            data_used_kib: st.data_used_kib,
            live_tasks: st.live_tasks,
            usb_mounted: st.usb_mounted,
            edit_mode: self.edit_mode,
        }
    }

    pub fn render_widget_safe(&self, g: &mut nc_app_sdk::Gfx<'_>, ev: &EventBlock, w: &WidgetInstance, rect: WidgetRect) {
        let ctx = self.build_widget_ctx(ev, w, rect);
        match provider_by_idx(w.provider_idx) {
            Some(prov) => prov.render(g, &ctx),
            None => crate::widgets::render_unavailable_provider(g, &ctx),
        }
    }

    pub fn lock_push_byte(&mut self, b: u8) {
        if self.lock_len < self.lock_buf.len() {
            self.lock_buf[self.lock_len] = b;
            self.lock_len += 1;
            self.lock_err = false;
            if self.lock_kind == 1 && self.lock_len == 4 {
                self.lock_submit();
            }
        }
    }

    pub fn lock_pop_byte(&mut self) {
        if self.lock_len > 0 {
            self.lock_len -= 1;
            self.lock_err = false;
        }
    }

    pub fn lock_submit(&mut self) {
        if self.lock_len == 0 {
            return;
        }
        let mut copy = [0u8; 32];
        let len = self.lock_len;
        copy[..len].copy_from_slice(&self.lock_buf[..len]);
        self.lock_len = 0;
        match security::authenticate(&copy[..len]) {
            Ok(()) => {
                self.locked = false;
                self.lock_err = false;
                self.lock_fails = 0;
                self.lock_lockout_s = 0;
            }
            Err((fails, lockout)) => {
                self.lock_err = true;
                self.lock_fails = fails;
                self.lock_lockout_s = lockout;
            }
        }
    }

    pub fn query_str(&self) -> &str {
        core::str::from_utf8(&self.query[..self.qlen]).unwrap_or("")
    }

    pub fn push_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        if self.qlen + s.len() <= self.query.len() {
            self.query[self.qlen..self.qlen + s.len()].copy_from_slice(s.as_bytes());
            self.qlen += s.len();
        }
    }

    pub fn pop_char(&mut self) {
        while self.qlen > 0 {
            self.qlen -= 1;
            if self.query[self.qlen] & 0xC0 != 0x80 {
                break;
            }
        }
    }

    pub fn close_overlays(&mut self) {
        self.cc_open = false;
        self.cc_wifi_detail = false;
        self.drag_cc = false;
        if self.search_open {
            self.search_open = false;
            let _ = nc_app_sdk::ecosystem::ime::hide();
        }
        self.drawer_open = false;
        self.folder_open = false;
        self.wpick_open = false;
        self.cust_open = false;
        self.qlen = 0;
    }

    pub fn close_top(&mut self) -> bool {
        if self.cc_open {
            self.cc_open = false;
            self.cc_wifi_detail = false;
        } else if self.wpick_open {
            self.wpick_open = false;
        } else if self.cust_open {
            self.cust_open = false;
        } else if self.search_open {
            if self.search_open {
            self.search_open = false;
            let _ = nc_app_sdk::ecosystem::ime::hide();
        }
            let _ = nc_app_sdk::ecosystem::ime::hide();
            self.qlen = 0;
        } else if self.folder_open {
            self.folder_open = false;
        } else if self.recents_open {
            self.recents_open = false;
        } else if self.drawer_open {
            self.drawer_open = false;
        } else if self.edit_mode {
            self.edit_mode = false;
        } else {
            return false;
        }
        true
    }

    pub fn sync(&mut self, ev: &EventBlock, geo: &Geo) {
        let ins = (geo.status_h, geo.nav_h);
        if ins != self.inset {
            self.inset = ins;
            let _ = sys::wm_set_viewport(ins.0, ins.1);
        }
        let now_locked = unpack_locked(ev.flags);
        if now_locked != self.locked {
            self.locked = now_locked;
            if now_locked {
                self.close_overlays();
                self.edit_mode = false;
                self.recents_open = false;
                self.app_open = false;
                self.active_surface = 0;
                self.lock_len = 0;
                self.lock_err = false;
                self.refresh_lock_status();
            }
        }
        if self.locked {
            return;
        }
        let fg = unpack_fg_app(ev.flags) as u32;
        if fg != 0 {
            if !self.app_open || self.active_surface != fg {
                self.refresh_registry();
                self.active_surface = fg;
                self.launch_app = self
                    .apps_slice()
                    .iter()
                    .position(|a| a.surface_id == fg)
                    .unwrap_or(0) as u8;
                let (dx, dy, ds) = self.rect_for_app(self.launch_app as usize, geo);
                self.origin = (dx, dy, ds);
                self.close_overlays();
                self.edit_mode = false;
                self.recents_open = false;
                self.app_open = true;
                self.launch_ms = ev.uptime_ms;
            }
        } else if self.app_open && ev.uptime_ms.wrapping_sub(self.launch_ms) > 700 {
            self.app_open = false;
        }
    }

    pub fn rect_for_app(&self, app: usize, geo: &Geo) -> (i32, i32, i32) {
        for (i, cell) in self.pages[self.page].iter().enumerate().take(geo.capacity()) {
            let hit = match cell {
                Cell::App(a) => *a as usize == app,
                Cell::Folder(slots) => slots.iter().any(|s| *s == Some(app as u8)),
                Cell::Empty => false,
            };
            if hit {
                let (x, y) = geo.cell_origin(i);
                return (x, y, geo.icon);
            }
        }
        for (i, slot) in self.dock.iter().enumerate() {
            if *slot == Some(app as u8) {
                return geo.dock_origin(i);
            }
        }
        if self.origin.2 > 0 {
            return self.origin;
        }
        (geo.dock.cx() - geo.icon / 2, geo.dock.y, geo.icon)
    }

    pub fn launch(&mut self, idx: usize, from: (i32, i32, i32), now: u32) {
        if self.locked || idx >= self.reg_count {
            return;
        }
        let mut id_copy = [0u8; 36];
        let id_len = self.registry[idx].id_len as usize;
        id_copy[..id_len].copy_from_slice(&self.registry[idx].id[..id_len]);
        let id_str = core::str::from_utf8(&id_copy[..id_len]).unwrap_or("");
        if id_str.is_empty() {
            return;
        }
        self.close_overlays();
        self.edit_mode = false;
        self.recents_open = false;
        self.launch_app = idx as u8;
        self.origin = from;
        self.launch_ms = now;
        if apps::launch(id_str) >= 0 {
            self.refresh_registry();
            self.active_surface = self.registry.get(idx).map(|a| a.surface_id).unwrap_or(0);
            self.app_open = true;
        }
    }

    pub fn go_home(&mut self) {
        self.close_overlays();
        self.edit_mode = false;
        self.recents_open = false;
        if self.app_open {
            let _ = sys::wm_pause_to_home();
        }
        self.app_open = false;
        self.refresh_registry();
    }

    pub fn go_back(&mut self) {
        if self.cc_open {
            self.cc_open = false;
            self.cc_wifi_detail = false;
        } else if self.recents_open {
            self.recents_open = false;
        } else if self.app_open {
            let _ = sys::wm_send_back();
        } else {
            self.close_top();
        }
    }

    pub fn toggle_recents(&mut self) {
        if self.app_open {
            let _ = sys::wm_pause_to_home();
            self.app_open = false;
        }
        self.close_overlays();
        self.edit_mode = false;
        self.refresh_registry();
        self.recents_open = !self.recents_open;
    }

    pub fn step(&mut self, now: u32, area_w: i32) -> bool {
        let rm = self.reduced_motion();
        self.app_p = self.t_app.step_ex(now, if self.app_open { ONE } else { 0 }, &S_APP, rm);
        if self.app_p == 0 && !self.app_open {
            self.active_surface = 0;
        }
        // Persistent 3-button smartphone navigation bar: always visible when unlocked
        // (Home screen, Drawer, Folders, Recent Apps, and running apps), hidden on Lock Screen.
        let nav_on = !self.locked;
        self.nav_p = self.t_nav.step_ex(now, if nav_on { ONE } else { 0 }, &S_NAV, rm);
        if self.drag_cc {
            self.t_cc.set(self.cc_p);
        } else {
            self.cc_p = self.t_cc.step_ex(now, if self.cc_open { ONE } else { 0 }, &S_CC, rm);
        }
        self.rec_p = self.t_rec.step_ex(now, if self.recents_open { ONE } else { 0 }, &S_REC, rm);
        if self.drag_drawer {
            self.t_drawer.set(self.drawer_p);
        } else {
            self.drawer_p = self.t_drawer.step_ex(now, if self.drawer_open { ONE } else { 0 }, &S_DRAWER, rm);
        }
        if self.drag_page {
            self.t_page.set(self.page_scroll_px);
        } else {
            let target_px = self.page as i32 * area_w.max(1);
            self.page_scroll_px = self.t_page.step_ex(now, target_px, &S_DRAWER, rm);
        }
        self.folder_p = self.t_folder.step_ex(now, if self.folder_open { ONE } else { 0 }, &S_FOLDER, rm);
        self.search_p = self.t_search.step_ex(now, if self.search_open { ONE } else { 0 }, &S_SEARCH, rm);
        self.lift_p = self.t_lift.step_ex(now, if self.lift.is_some() { ONE } else { 0 }, &S_LIFT, rm);
        self.edit_p = self.t_edit.step_ex(now, if self.edit_mode { ONE } else { 0 }, &S_NAV, rm);
        self.wpick_p = self.t_wpick.step_ex(now, if self.wpick_open { ONE } else { 0 }, &S_WIDGETS, rm);
        self.cust_p = self.t_cust.step_ex(now, if self.cust_open { ONE } else { 0 }, &S_WIDGETS, rm);
        self.animating()
    }

    pub fn animating(&self) -> bool {
        !(self.t_app.settled()
            && self.t_nav.settled()
            && self.t_cc.settled()
            && self.t_rec.settled()
            && self.t_drawer.settled()
            && self.t_page.settled()
            && self.t_folder.settled()
            && self.t_search.settled()
            && self.t_lift.settled()
            && self.t_edit.settled()
            && self.t_wpick.settled()
            && self.t_cust.settled())
    }

    pub fn get(&self, slot: Slot) -> Cell {
        match slot {
            Slot::Grid(p, i) => self
                .pages
                .get(p as usize)
                .and_then(|pg| pg.get(i))
                .copied()
                .unwrap_or(Cell::Empty),
            Slot::Dock(i) => match self.dock.get(i).copied().flatten() {
                Some(a) => Cell::App(a),
                None => Cell::Empty,
            },
        }
    }

    pub fn put(&mut self, slot: Slot, cell: Cell) {
        match slot {
            Slot::Grid(p, i) => {
                if (p as usize) < MAX_PAGES && i < MAX_CELLS {
                    self.pages[p as usize][i] = cell;
                }
            }
            Slot::Dock(i) => {
                if i < DOCK_SLOTS {
                    self.dock[i] = match cell {
                        Cell::App(a) => Some(a),
                        _ => None,
                    };
                }
            }
        }
    }

    pub fn move_or_group(&mut self, from: Slot, to: Slot) {
        if from == to {
            return;
        }
        let src = self.get(from);
        let dst = self.get(to);
        let dock_involved = matches!(from, Slot::Dock(_)) || matches!(to, Slot::Dock(_));
        let folder_into_dock = |c: Cell, s: Slot| matches!(c, Cell::Folder(_)) && matches!(s, Slot::Dock(_));
        match (src, dst) {
            (Cell::Empty, _) => return,
            (item, Cell::Empty) => {
                if folder_into_dock(item, to) {
                    return;
                }
                self.put(to, item);
                self.put(from, Cell::Empty);
            }
            (Cell::App(a), Cell::App(b)) if !dock_involved => {
                self.put(to, Cell::Folder([Some(b), Some(a), None, None]));
                self.put(from, Cell::Empty);
            }
            (Cell::App(a), Cell::Folder(mut slots)) if !dock_involved => {
                if let Some(free) = slots.iter_mut().find(|s| s.is_none()) {
                    *free = Some(a);
                    self.put(to, Cell::Folder(slots));
                    self.put(from, Cell::Empty);
                } else {
                    return;
                }
            }
            (a, b) => {
                if folder_into_dock(a, to) || folder_into_dock(b, from) {
                    return;
                }
                self.put(to, a);
                self.put(from, b);
            }
        }
        self.save_layout();
    }

    pub fn ungroup(&mut self, page: u8, cell: usize, cap: usize) {
        let p = page as usize;
        if p >= MAX_PAGES || cell >= MAX_CELLS {
            return;
        }
        let Cell::Folder(slots) = self.pages[p][cell] else { return };
        let cols = self.prefs.density.cols();
        let mut first = true;
        for app in slots.into_iter().flatten() {
            if first {
                self.pages[p][cell] = Cell::App(app);
                first = false;
            } else {
                for free in 0..cap.min(MAX_CELLS) {
                    if self.pages[p][free] == Cell::Empty
                        && !cell_covered_by_widgets(&self.widgets, page, free, cols)
                    {
                        self.pages[p][free] = Cell::App(app);
                        break;
                    }
                }
            }
        }
        if first {
            self.pages[p][cell] = Cell::Empty;
        }
        self.folder_open = false;
        self.save_layout();
    }
}
