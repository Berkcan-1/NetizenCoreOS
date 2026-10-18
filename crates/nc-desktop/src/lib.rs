#![no_std]

extern crate alloc;

mod icons;
mod layout;
mod motion;
mod paint;
mod shell;
mod theme;
mod wallpaper;
mod window;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_app::theme::WALLPAPER_COUNT;
use nc_app::{App, Context, Input, Pointer, Settings};
use nc_fs::{FileType, VfsManager};
use nc_gfx::{Canvas, Rect};
use nc_hal::{KeyCode, MouseState};
use nc_math::sin_milli;

use layout::{
    centered_pos, clamp_window_pos, close_button, content_area, context_item_rect,
    context_menu_rect, desktop_modal_rect, dock_area, dock_bottom_gap, dock_height, grid_cell,
    grid_for, grid_pick, inside, max_button, min_button, modal_buttons, start_cell, start_dot_rect,
    start_layout, task_manager_action_btn, task_manager_close_rect,
    task_manager_rect, task_manager_row_rect, title_bar, title_height, unit_for, usable_rect,
    window_size, DockLayout, Grid,
};
use motion::{
    approach, clamp01, ease_in_out_cubic, ease_out_cubic, ease_out_quart, lerp,
    scale_rect_about, scale_u32, ONE,
};
use paint::fill_rr;
use theme::{AZURE, DANGER, INDIGO, SNOW, TEAL, WARN};

const DOUBLE_CLICK_MS: u32 = 460;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DesktopAction {
    None,
    Shutdown,
    Reboot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DesktopIconId {
    App(usize),
    Fs(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuKind {
    DesktopSurface,
    AppItem(usize),
    FsItem(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Nothing,
    Desktop,
    AppIcon(usize),
    FsIcon(usize),
    DockBody,
    StartButton,
    DockApp(usize),
    DockStatus,
    StartPanel,
    StartNode(usize),
    StartPage(usize),
    PowerReboot,
    PowerShutdown,
    WinBody(usize),
    WinTitle(usize),
    WinClose(usize),
    WinMax(usize),
    WinMin(usize),
    ContextMenu,
    ContextItem(usize),
    ModalBody,
    ModalConfirm,
    ModalCancel,
    TaskMgrTitle,
    TaskMgrClose,
    TaskMgrRow(usize),
    TaskMgrKill,
    TaskMgrOptimize,
    TaskMgrBody,
}

#[derive(Clone)]
pub struct DesktopFsItem {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub cell: Option<(i32, i32)>,
}

pub enum DesktopModal {
    None,
    CreateFolder {
        name_buf: String,
    },
    CreateTextFile {
        name_buf: String,
    },
    RenameItem {
        old_name: String,
        new_buf: String,
        is_dir: bool,
    },
    EditTextFile {
        name: String,
        content: String,
    },
}

#[derive(Clone, Copy)]
pub struct BgService {
    pub pid: u32,
    pub name: &'static str,
    pub cpu_pct: u8,
    pub mem_mb: u16,
    pub ticks: u64,
}

pub struct TaskRowView<'a> {
    pub pid: u32,
    pub name: &'a str,
    pub status: &'static str,
    pub cpu_pct: u8,
    pub mem_mb: u16,
    pub is_app: bool,
    pub app_index: Option<usize>,
}

pub struct TaskManagerState {
    pub open: bool,
    pub x: i32,
    pub y: i32,
    pub selected_row: usize,
    pub cpu_history: [u8; 32],
    pub hist_cursor: usize,
    pub dragging: Option<(i32, i32)>,
}

#[derive(Clone, Copy)]
pub(crate) struct WindowState {
    pub open: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub placed: bool,
    pub x: i32,
    pub y: i32,
    pub open_anim: i32,
    pub leave: i32,
    pub leave_kind: u8,
    pub max_t: i32,
    pub from_dock: bool,
}

pub(crate) struct Slot {
    pub app: Box<dyn App>,
    pub window: WindowState,
    pub pinned_dock: bool,
    pub pinned_start: bool,
    pub icon_cell: Option<(i32, i32)>,
}

#[derive(Clone, Copy)]
struct Drag {
    app: usize,
    offset_x: i32,
    offset_y: i32,
}

#[derive(Clone, Copy)]
struct IconDrag {
    id: DesktopIconId,
    offset_x: i32,
    offset_y: i32,
    start_x: i32,
    start_y: i32,
    px: i32,
    py: i32,
    moved: bool,
}

#[derive(Clone, Copy)]
struct Settle {
    id: DesktopIconId,
    from_x: i32,
    from_y: i32,
    t: i32,
}

pub(crate) struct Visual {
    pub rect: Rect,
    pub appear: i32,
    pub content: i32,
}

pub struct DesktopState {
    platform: &'static str,
    pub(crate) total_memory_mb: u32,
    pub(crate) apps: Vec<Slot>,
    pub(crate) fs_items: Vec<DesktopFsItem>,
    pub(crate) bg_services: [BgService; 5],
    pub(crate) task_mgr: TaskManagerState,
    vfs: VfsManager,
    desktop_dir: &'static str,
    pub(crate) settings: Settings,
    pub(crate) start_open: bool,
    pub(crate) start_anim: i32,
    pub(crate) start_page: usize,
    pub(crate) dock_reveal: i32,
    pub(crate) dock_mag: i32,
    pub(crate) dock_focus_x: i32,
    pub(crate) pointer_x: i32,
    pub(crate) pointer_y: i32,
    pub(crate) context_menu: Option<((i32, i32), ContextMenuKind)>,
    pub(crate) ctx_last: Option<((i32, i32), ContextMenuKind)>,
    pub(crate) ctx_anim: i32,
    pub(crate) modal: DesktopModal,
    pub(crate) modal_anim: i32,
    pub(crate) volume: u8,
    pub(crate) muted: bool,
    pub(crate) vol_osd_ms: u32,
    pub(crate) vol_osd_anim: i32,
    pub(crate) vol_disp: i32,
    pub(crate) hover: Target,
    pub(crate) selected_icon: Option<DesktopIconId>,
    last_click_icon: Option<DesktopIconId>,
    last_click_ms: u32,
    last_title_app: Option<usize>,
    last_title_ms: u32,
    pub(crate) uptime_ms: u32,
    order: Vec<usize>,
    drag: Option<Drag>,
    icon_drag: Option<IconDrag>,
    settle: Option<Settle>,
    dirty: bool,
    wallpaper_dirty: bool,
}

impl DesktopState {
    pub fn new(
        platform: &'static str,
        total_memory_mb: u32,
        apps: Vec<Box<dyn App>>,
    ) -> DesktopState {
        let mut vfs = VfsManager::new();
        let desktop_dir = if vfs.create_dir_path("/data/desktop").is_ok()
            || vfs.list_dir_path("/data/desktop").is_ok()
        {
            "/data/desktop"
        } else {
            let _ = vfs.create_dir_path("/ram/desktop");
            "/ram/desktop"
        };

        let bg = |pid: u32, name: &'static str, cpu: u8, mem: u16| BgService {
            pid,
            name,
            cpu_pct: cpu,
            mem_mb: mem,
            ticks: 0,
        };

        let mut state = DesktopState {
            platform,
            total_memory_mb,
            apps: Vec::new(),
            fs_items: Vec::new(),
            bg_services: [
                bg(1, "cekirdek.zamanlayici", 2, 14),
                bg(2, "vfs.indeksleyici", 1, 8),
                bg(3, "ag.anahtarlama", 2, 10),
                bg(4, "grafik.besteci", 4, 18),
                bg(5, "donanim.gozcusu", 1, 6),
            ],
            task_mgr: TaskManagerState {
                open: false,
                x: 180,
                y: 80,
                selected_row: 0,
                cpu_history: [8; 32],
                hist_cursor: 0,
                dragging: None,
            },
            vfs,
            desktop_dir,
            settings: Settings::new(),
            start_open: false,
            start_anim: 0,
            start_page: 0,
            dock_reveal: ONE,
            dock_mag: 0,
            dock_focus_x: 0,
            pointer_x: 512,
            pointer_y: 384,
            context_menu: None,
            ctx_last: None,
            ctx_anim: 0,
            modal: DesktopModal::None,
            modal_anim: 0,
            volume: 80,
            muted: false,
            vol_osd_ms: 0,
            vol_osd_anim: 0,
            vol_disp: ONE * 80 / 100,
            hover: Target::Nothing,
            selected_icon: None,
            last_click_icon: None,
            last_click_ms: 0,
            last_title_app: None,
            last_title_ms: 0,
            uptime_ms: 0,
            order: Vec::new(),
            drag: None,
            icon_drag: None,
            settle: None,
            dirty: true,
            wallpaper_dirty: true,
        };

        for app in apps {
            state.install_app(app);
        }

        if let Ok(cfg_bytes) = state.vfs.read_file_path("/data/system/locale.cfg") {
            if let Ok(cfg_str) = core::str::from_utf8(&cfg_bytes) {
                for line in cfg_str.lines() {
                    if let Some(val) = line.strip_prefix("lang=") {
                        state.settings.language =
                            nc_app::Language::from_u8(val.trim().parse().unwrap_or(0));
                    } else if let Some(val) = line.strip_prefix("kbd=") {
                        state.settings.keyboard =
                            nc_app::KeyboardLayout::from_u8(val.trim().parse().unwrap_or(0));
                    }
                }
            }
        }

        state.sync_desktop_fs();
        state
    }

    pub fn adjust_volume(&mut self, delta: i32, toggle_mute: bool) {
        if toggle_mute {
            self.muted = !self.muted;
        } else {
            self.muted = false;
            self.volume = (self.volume as i32 + delta).clamp(0, 100) as u8;
        }
        self.vol_osd_ms = 1800; // 1.8 saniye ekranda kalir
        self.dirty = true;
    }

    pub(crate) fn blur_params(&self, unit: i32) -> (usize, usize) {
        let base = (unit / 4).max(3) as usize;
        match self.settings.glass_blur {
            0 => (0, 0),
            1 => ((base / 2).max(2), 1),
            2 => (base, 2),
            _ => ((base * 5 / 4).min(14), 2),
        }
    }

    fn open_ms(&self) -> u32 {
        match self.settings.launch_anim_speed {
            1 => 440,
            2 => 320,
            _ => 150,
        }
    }

    fn leave_ms(&self) -> u32 {
        (self.open_ms() * 2 / 3).max(110)
    }

    pub(crate) fn total_cpu_percent(&self) -> u32 {
        let mut sum = 0u32;
        for s in &self.bg_services {
            sum += s.cpu_pct as u32;
        }
        for slot in &self.apps {
            if slot.window.open && slot.window.open_anim < ONE {
                sum += 18;
            } else if slot.window.open && !slot.window.minimized {
                sum += 5;
            } else if slot.window.open {
                sum += 1;
            }
        }
        sum.min(100)
    }

    pub(crate) fn used_ram_mb(&self) -> u32 {
        let mut sum = 0u32;
        for s in &self.bg_services {
            sum += s.mem_mb as u32;
        }
        for slot in &self.apps {
            if slot.window.open {
                sum += 12;
            }
        }
        sum.min(self.total_memory_mb)
    }

    pub(crate) fn collect_task_rows(&self) -> Vec<TaskRowView<'_>> {
        let mut list = Vec::new();
        for (idx, slot) in self.apps.iter().enumerate() {
            if slot.window.open {
                let loading = slot.window.open_anim < ONE;
                let status = if loading {
                    "Yukleniyor"
                } else if slot.window.minimized {
                    "Simge Durumu"
                } else {
                    "Calisiyor"
                };
                let cpu = if loading {
                    18
                } else if slot.window.minimized {
                    1
                } else {
                    5
                };
                list.push(TaskRowView {
                    pid: 100 + idx as u32,
                    name: &slot.app.info().name,
                    status,
                    cpu_pct: cpu,
                    mem_mb: 12,
                    is_app: true,
                    app_index: Some(idx),
                });
            }
        }
        for s in &self.bg_services {
            list.push(TaskRowView {
                pid: s.pid,
                name: s.name,
                status: "Arka Plan",
                cpu_pct: s.cpu_pct,
                mem_mb: s.mem_mb,
                is_app: false,
                app_index: None,
            });
        }
        list
    }

    fn tick_background_processes(&mut self, uptime_ms: u32) {
        let phase = (uptime_ms / 250) as usize;
        let launching = self
            .apps
            .iter()
            .any(|s| s.window.open && s.window.open_anim < ONE);
        for (i, s) in self.bg_services.iter_mut().enumerate() {
            s.ticks = s.ticks.wrapping_add(1);
            let jitter = ((phase + i * 3) % 4) as u8;
            s.cpu_pct = match i {
                0 => 2 + (jitter & 1),
                1 => {
                    if launching {
                        12 + jitter
                    } else {
                        1 + (jitter & 1)
                    }
                }
                2 => 2 + (jitter % 2),
                3 => {
                    if self.settings.wallpaper_animated {
                        6 + jitter
                    } else {
                        3
                    }
                }
                _ => 1,
            };
        }
        let total = self.total_cpu_percent() as u8;
        let cur = self.task_mgr.hist_cursor;
        self.task_mgr.cpu_history[cur] = total;
        self.task_mgr.hist_cursor = (cur + 1) % 32;
        if self.task_mgr.open {
            self.dirty = true;
        }
    }

    pub fn sync_desktop_fs(&mut self) {
        let mut next_items = Vec::new();
        if let Ok(entries) = self.vfs.list_dir_path(self.desktop_dir) {
            for e in entries {
                if e.name.ends_with(".ntzc") {
                    continue;
                }
                let prev = self
                    .fs_items
                    .iter()
                    .find(|old| old.name == e.name)
                    .and_then(|old| old.cell);
                next_items.push(DesktopFsItem {
                    name: e.name,
                    is_dir: e.file_type == FileType::Directory,
                    size: e.size,
                    cell: prev,
                });
            }
        }
        let changed = self.fs_items.len() != next_items.len()
            || self
                .fs_items
                .iter()
                .zip(next_items.iter())
                .any(|(a, b)| a.name != b.name || a.is_dir != b.is_dir || a.size != b.size);
        self.fs_items = next_items;
        if changed {
            self.dirty = true;
        }
    }

    fn grid(&self, width: i32, height: i32, unit: i32) -> Grid {
        grid_for(width, height, unit, self.settings.dock_auto_hide)
    }

    fn icon_ids(&self) -> Vec<(DesktopIconId, Option<(i32, i32)>)> {
        let mut ids = Vec::new();
        for (i, s) in self.apps.iter().enumerate() {
            ids.push((DesktopIconId::App(i), s.icon_cell));
        }
        for (i, f) in self.fs_items.iter().enumerate() {
            ids.push((DesktopIconId::Fs(i), f.cell));
        }
        ids
    }

    fn icon_cells(&self, g: &Grid) -> Vec<(DesktopIconId, i32, i32)> {
        let ids = self.icon_ids();
        let cap = (g.cols * g.rows) as usize;
        let mut occ = vec![false; cap];
        let mut placed: Vec<Option<(i32, i32)>> = vec![None; ids.len()];
        for (n, (_, cell)) in ids.iter().enumerate() {
            if let Some((c, r)) = *cell {
                let c = c.clamp(0, g.cols - 1);
                let r = r.clamp(0, g.rows - 1);
                let idx = (c * g.rows + r) as usize;
                if !occ[idx] {
                    occ[idx] = true;
                    placed[n] = Some((c, r));
                }
            }
        }
        let mut cursor = 0usize;
        for n in 0..ids.len() {
            if placed[n].is_some() {
                continue;
            }
            while cursor < cap && occ[cursor] {
                cursor += 1;
            }
            let slot = if cursor < cap {
                occ[cursor] = true;
                cursor
            } else {
                cursor += 1;
                cursor - 1
            };
            let c = slot as i32 / g.rows;
            let r = slot as i32 % g.rows;
            placed[n] = Some((c, r));
        }
        ids.iter()
            .enumerate()
            .map(|(n, (id, _))| {
                let (c, r) = placed[n].unwrap_or((0, 0));
                (*id, c, r)
            })
            .collect()
    }

    pub(crate) fn icon_rects(&self, width: i32, height: i32, unit: i32) -> Vec<(DesktopIconId, Rect)> {
        let g = self.grid(width, height, unit);
        let mut out = Vec::new();
        for (id, c, r) in self.icon_cells(&g) {
            let mut rect = grid_cell(&g, c, r);
            if let Some(d) = self.icon_drag {
                if d.moved && d.id == id {
                    rect = Rect::new(d.px, d.py, rect.width, rect.height);
                }
            }
            if let Some(s) = self.settle {
                if s.id == id {
                    let e = ease_out_cubic(s.t);
                    rect = Rect::new(
                        lerp(s.from_x, rect.x, e),
                        lerp(s.from_y, rect.y, e),
                        rect.width,
                        rect.height,
                    );
                }
            }
            out.push((id, rect));
        }
        out
    }

    pub(crate) fn dragging_icon(&self) -> Option<DesktopIconId> {
        match self.icon_drag {
            Some(d) if d.moved => Some(d.id),
            _ => None,
        }
    }

    fn resolve_drop(&self, g: &Grid, id: DesktopIconId, want: (i32, i32)) -> (i32, i32) {
        let cells = self.icon_cells(g);
        let taken = |c: i32, r: i32| cells.iter().any(|(i, cc, rr)| *i != id && *cc == c && *rr == r);
        if !taken(want.0, want.1) {
            return want;
        }
        let max_d = g.cols + g.rows;
        for d in 1..=max_d {
            for dc in -d..=d {
                for dr in -d..=d {
                    if dc.abs() + dr.abs() != d {
                        continue;
                    }
                    let c = want.0 + dc;
                    let r = want.1 + dr;
                    if c < 0 || r < 0 || c >= g.cols || r >= g.rows {
                        continue;
                    }
                    if !taken(c, r) {
                        return (c, r);
                    }
                }
            }
        }
        want
    }

    pub(crate) fn drag_ghost(&self, width: i32, height: i32, unit: i32) -> Option<Rect> {
        let d = self.icon_drag?;
        if !d.moved {
            return None;
        }
        let g = self.grid(width, height, unit);
        let want = grid_pick(&g, d.px + g.cw / 2, d.py + g.ch / 2);
        let (c, r) = self.resolve_drop(&g, d.id, want);
        Some(grid_cell(&g, c, r))
    }

    fn finish_icon_drag(&mut self, d: IconDrag, width: i32, height: i32, unit: i32) {
        let g = self.grid(width, height, unit);
        let want = grid_pick(&g, d.px + g.cw / 2, d.py + g.ch / 2);
        let cell = self.resolve_drop(&g, d.id, want);
        match d.id {
            DesktopIconId::App(i) => {
                if i < self.apps.len() {
                    self.apps[i].icon_cell = Some(cell);
                }
            }
            DesktopIconId::Fs(i) => {
                if i < self.fs_items.len() {
                    self.fs_items[i].cell = Some(cell);
                }
            }
        }
        self.settle = Some(Settle {
            id: d.id,
            from_x: d.px,
            from_y: d.py,
            t: 0,
        });
        self.dirty = true;
    }

    pub fn install_app(&mut self, app: Box<dyn App>) {
        let target_id = app.info().id.clone();
        if let Some(existing) = self
            .apps
            .iter()
            .position(|slot| slot.app.info().id == target_id)
        {
            self.apps[existing].app = app; 
            let w = &mut self.apps[existing].window;
            w.open = true;
            w.minimized = false;
            w.open_anim = 0;
            w.from_dock = false;
            w.leave = 0;
            self.raise(existing);
            self.dirty = true;
            return;
        }

        let index = self.apps.len();
        self.apps.push(Slot {
            app,
            window: WindowState {
                open: false,
                minimized: false,
                maximized: false,
                placed: false,
                x: 0,
                y: 0,
                open_anim: ONE,
                leave: 0,
                leave_kind: 0,
                max_t: 0,
                from_dock: false,
            },
            pinned_dock: index < 4,
            pinned_start: true,
            icon_cell: None,
        });
        self.order.push(index);
        self.dirty = true;
    }

    pub fn app_count(&self) -> usize {
        self.apps.len()
    }

    pub fn wallpaper_index(&self) -> usize {
        self.settings.wallpaper
    }

    pub fn take_dirty(&mut self) -> bool {
        core::mem::replace(&mut self.dirty, false)
    }

    pub fn take_wallpaper_dirty(&mut self) -> bool {
        core::mem::replace(&mut self.wallpaper_dirty, false)
    }

    pub(crate) fn dock_indices(&self) -> Vec<usize> {
        let mut list = Vec::new();
        for (i, slot) in self.apps.iter().enumerate() {
            if slot.pinned_dock || slot.window.open {
                list.push(i);
            }
        }
        list
    }

    pub(crate) fn start_indices(&self) -> Vec<usize> {
        let mut list = Vec::new();
        for (i, slot) in self.apps.iter().enumerate() {
            if slot.pinned_start {
                list.push(i);
            }
        }
        if list.is_empty() {
            for i in 0..self.apps.len() {
                list.push(i);
            }
        }
        list
    }

    pub(crate) fn dock_full_t(&self) -> i32 {
        self.apps
            .iter()
            .filter(|s| s.window.open && !s.window.minimized)
            .map(|s| s.window.max_t)
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn dock_layout(&self, width: i32, height: i32, unit: i32) -> DockLayout {
        layout::dock_layout(
            width,
            height,
            unit,
            self.dock_indices().len(),
            self.dock_reveal,
            self.dock_focus_x,
            self.dock_mag,
            self.dock_full_t(),
        )
    }

    fn dock_tile_rect(&self, app: usize, width: i32, height: i32, unit: i32) -> Rect {
        let items = self.dock_indices();
        let dl = layout::dock_layout(
            width,
            height,
            unit,
            items.len(),
            ONE,
            -100000,
            0,
            self.dock_full_t(),
        );
        match items.iter().position(|&i| i == app) {
            Some(pos) => dl.tiles[pos],
            None => Rect::new(width / 2 - dl.tile / 2, dl.dock.y, dl.tile, dl.tile),
        }
    }

    pub(crate) fn bounce_offset(&self, app: usize, unit: i32) -> i32 {
        let w = &self.apps[app].window;
        if !(w.open && w.open_anim < ONE && !w.from_dock) {
            return 0;
        }
        let p = clamp01(w.open_anim) as i64;
        let deg = (p * 360 / ONE as i64) as i32;
        let s = sin_milli(deg).abs() as i64;
        (s * unit as i64 * (ONE as i64 - p) / ONE as i64 / 1000) as i32
    }

    pub(crate) fn context_menu_item_count(&self, kind: ContextMenuKind) -> usize {
        match kind {
            ContextMenuKind::DesktopSurface => 8,
            ContextMenuKind::AppItem(_) => 5,
            ContextMenuKind::FsItem(_) => 4,
        }
    }

    pub(crate) fn context_menu_entry(
        &self,
        kind: ContextMenuKind,
        idx: usize,
    ) -> (&'static str, &'static str, u32) {
        use nc_app::{tr, TextId};
        let lang = self.settings.language;
        match kind {
            ContextMenuKind::DesktopSurface => match idx {
                0 => (tr(lang, TextId::CtxOpenTaskMgr), "S", TEAL),
                1 => (tr(lang, TextId::CtxNewFolder), "K", AZURE),
                2 => (tr(lang, TextId::CtxNewTextFile), "T", TEAL),
                3 => (tr(lang, TextId::CtxRefreshIcons), "G", INDIGO),
                4 => (tr(lang, TextId::CtxNextWallpaper), "D", INDIGO),
                5 => (
                    if self.settings.wallpaper_animated {
                        tr(lang, TextId::CtxLiveWpOn)
                    } else {
                        tr(lang, TextId::CtxLiveWpOff)
                    },
                    "C",
                    WARN,
                ),
                6 => (tr(lang, TextId::CtxOpenFiles), "F", AZURE),
                _ => (tr(lang, TextId::CtxOpenSettings), "A", TEAL),
            },
            ContextMenuKind::AppItem(app_idx) => {
                let pinned_dock = self.apps.get(app_idx).map(|s| s.pinned_dock).unwrap_or(false);
                let pinned_start = self.apps.get(app_idx).map(|s| s.pinned_start).unwrap_or(false);
                let is_open = self.apps.get(app_idx).map(|s| s.window.open).unwrap_or(false);
                match idx {
                    0 => (tr(lang, TextId::CtxOpenApp), "", AZURE),
                    1 => (
                        if pinned_dock {
                            tr(lang, TextId::CtxUnpinDock)
                        } else {
                            tr(lang, TextId::CtxPinDock)
                        },
                        "",
                        TEAL,
                    ),
                    2 => (
                        if pinned_start {
                            tr(lang, TextId::CtxUnpinStart)
                        } else {
                            tr(lang, TextId::CtxPinStart)
                        },
                        "",
                        WARN,
                    ),
                    3 => (tr(lang, TextId::CtxOpenTaskMgr), "", INDIGO),
                    _ => (
                        if is_open {
                            tr(lang, TextId::CtxCloseWin)
                        } else {
                            tr(lang, TextId::CtxReadyState)
                        },
                        "",
                        DANGER,
                    ),
                }
            }
            ContextMenuKind::FsItem(_) => match idx {
                0 => (tr(lang, TextId::CtxOpenEdit), "", AZURE),
                1 => (tr(lang, TextId::CtxRename), "", TEAL),
                2 => (tr(lang, TextId::CtxShowInFiles), "", WARN),
                _ => (tr(lang, TextId::CtxDelete), "", DANGER),
            },
        }
    }

    pub(crate) fn focused(&self) -> Option<usize> {
        for position in (0..self.order.len()).rev() {
            let index = self.order[position];
            if self.interactive(index) {
                return Some(index);
            }
        }
        None
    }

    pub(crate) fn interactive(&self, index: usize) -> bool {
        let w = &self.apps[index].window;
        w.open && !w.minimized && w.leave == 0
    }

    pub(crate) fn visible(&self, index: usize) -> bool {
        let w = &self.apps[index].window;
        (w.open && !w.minimized) || w.leave > 0
    }

    fn usable(&self, width: i32, height: i32, unit: i32) -> Rect {
        usable_rect(width, height, unit, self.settings.dock_auto_hide)
    }

    fn normal_rect(&self, index: usize, unit: i32, usable: Rect) -> Rect {
        let slot = &self.apps[index];
        let info = slot.app.info();
        let (w, h) = window_size(info.width_units, info.height_units, unit, usable);
        let (x, y) = clamp_window_pos(slot.window.x, slot.window.y, w, h, unit, usable);
        Rect::new(x, y, w, h)
    }

    pub(crate) fn window_rect(&self, index: usize, unit: i32, width: i32, height: i32) -> Rect {
        let usable = self.usable(width, height, unit);
        let normal = self.normal_rect(index, unit, usable);
        let t = self.apps[index].window.max_t;
        if t <= 0 {
            return normal;
        }
        let e = ease_in_out_cubic(t);
        Rect::new(
            lerp(normal.x, usable.x, e),
            lerp(normal.y, usable.y, e),
            lerp(normal.width, usable.width, e),
            lerp(normal.height, usable.height, e),
        )
    }

    pub(crate) fn window_visual(&self, index: usize, unit: i32, width: i32, height: i32) -> Visual {
        let base = self.window_rect(index, unit, width, height);
        let win = &self.apps[index].window;
        if win.leave > 0 {
            let p = clamp01(win.leave);
            if win.leave_kind == 1 {
                let tile = self.dock_tile_rect(index, width, height, unit);
                let e = ease_in_out_cubic(p);
                let rect = Rect::new(
                    lerp(base.x, tile.x, e),
                    lerp(base.y, tile.y, e),
                    lerp(base.width, tile.width, e),
                    lerp(base.height, tile.height, e),
                );
                let fade = clamp01((p - ONE * 40 / 100) * 5 / 3);
                return Visual {
                    rect,
                    appear: ONE - fade,
                    content: ONE - clamp01(p * 4),
                };
            }
            let e = ease_out_cubic(p);
            let scale = lerp(ONE, ONE * 92 / 100, e);
            let (x, y, w, h) = scale_rect_about(
                base.x,
                base.y,
                base.width,
                base.height,
                scale,
                base.x + base.width / 2,
                base.y + base.height / 2,
            );
            return Visual {
                rect: Rect::new(x, y, w, h),
                appear: ONE - e,
                content: ONE - clamp01(p * 3),
            };
        }
        if win.open_anim < ONE {
            let e = ease_out_quart(win.open_anim);
            if win.from_dock {
                let tile = self.dock_tile_rect(index, width, height, unit);
                let rect = Rect::new(
                    lerp(tile.x, base.x, e),
                    lerp(tile.y, base.y, e),
                    lerp(tile.width, base.width, e),
                    lerp(tile.height, base.height, e),
                );
                return Visual {
                    rect,
                    appear: clamp01(e * 2),
                    content: clamp01((e - ONE * 80 / 100) * 5),
                };
            }
            let scale = lerp(ONE * 90 / 100, ONE, e);
            let (x, y, w, h) = scale_rect_about(
                base.x,
                base.y,
                base.width,
                base.height,
                scale,
                base.x + base.width / 2,
                base.y + base.height / 2,
            );
            let dy = lerp(unit, 0, e);
            return Visual {
                rect: Rect::new(x, y + dy, w, h),
                appear: ease_out_cubic(clamp01(win.open_anim * 2)),
                content: clamp01((e - ONE * 55 / 100) * 100 / 45),
            };
        }
        Visual {
            rect: base,
            appear: ONE,
            content: ONE,
        }
    }

    fn context(&self, width: i32, height: i32) -> Context {
        Context {
            platform: self.platform,
            screen_width: width,
            screen_height: height,
            unit: unit_for(width, height),
            uptime_ms: self.uptime_ms,
            settings: self.settings,
            total_memory_mb: self.total_memory_mb,
        }
    }

    fn raise(&mut self, app: usize) {
        if let Some(position) = self.order.iter().position(|&index| index == app) {
            self.order.remove(position);
        }
        self.order.push(app);
        self.dirty = true;
    }

    fn apply_settings(&mut self, settings: Settings) {
        if settings != self.settings {
            if settings.wallpaper != self.settings.wallpaper {
                self.wallpaper_dirty = true;
            }
            self.settings = settings;
            self.dirty = true;
        }
    }

    fn open_context(&mut self, pos: (i32, i32), kind: ContextMenuKind) {
        self.context_menu = Some((pos, kind));
        self.ctx_last = Some((pos, kind));
        self.ctx_anim = 0;
        self.start_open = false;
        self.dirty = true;
    }

    fn open_app(&mut self, app: usize, width: i32, height: i32, unit: i32) {
        if app >= self.apps.len() {
            return;
        }
        if !self.apps[app].window.open {
            let usable = self.usable(width, height, unit);
            let info = self.apps[app].app.info();
            let (w, h) = window_size(info.width_units, info.height_units, unit, usable);
            if !self.apps[app].window.placed {
                let (x, y) = centered_pos(w, h, unit, usable, (app % 4) as i32);
                self.apps[app].window.x = x;
                self.apps[app].window.y = y;
                self.apps[app].window.placed = true;
            }
            let win = &mut self.apps[app].window;
            win.open = true;
            win.minimized = false;
            win.open_anim = 0;
            win.from_dock = false;
            win.leave = 0;
            win.leave_kind = 0;
            let mut ctx = self.context(width, height);
            self.apps[app].app.opened(&mut ctx);
            self.apply_settings(ctx.settings);
            self.raise(app);
            self.dirty = true;
            return;
        }
        if self.apps[app].window.minimized {
            self.restore_app(app);
        }
        self.raise(app);
    }

    fn restore_app(&mut self, app: usize) {
        let win = &mut self.apps[app].window;
        win.minimized = false;
        win.open_anim = 0;
        win.from_dock = true;
        win.leave = 0;
        win.leave_kind = 0;
        self.raise(app);
    }

    fn minimize_app(&mut self, app: usize) {
        let win = &mut self.apps[app].window;
        win.minimized = true;
        win.leave = 1;
        win.leave_kind = 1;
        self.dirty = true;
    }

    fn open_app_by_id(&mut self, target_id: &str, width: i32, height: i32, unit: i32) {
        if let Some(idx) = self
            .apps
            .iter()
            .position(|slot| slot.app.info().id == target_id)
        {
            self.open_app(idx, width, height, unit);
        } else if !self.apps.is_empty() {
            self.open_app(0, width, height, unit);
        }
    }

    fn close_app(&mut self, app: usize, width: i32, height: i32) {
        if app < self.apps.len() && self.apps[app].window.open {
            let win = &mut self.apps[app].window;
            win.open = false;
            win.minimized = false;
            win.leave = 1;
            win.leave_kind = 2;
            let mut ctx = self.context(width, height);
            self.apps[app].app.closed(&mut ctx);
            self.apply_settings(ctx.settings);
            self.sync_desktop_fs();
            self.dirty = true;
        }
    }

    fn open_fs_item(&mut self, fs_idx: usize, width: i32, height: i32, unit: i32) {
        if fs_idx >= self.fs_items.len() {
            return;
        }
        let item = self.fs_items[fs_idx].clone();
        if item.is_dir {
            self.open_app_by_id("nc.files", width, height, unit);
        } else {
            let full = alloc::format!("{}/{}", self.desktop_dir, item.name);
            let bytes = self.vfs.read_file_path(&full).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            self.modal = DesktopModal::EditTextFile {
                name: item.name,
                content: text,
            };
            self.modal_anim = 0;
            self.dirty = true;
        }
    }

    fn open_modal(&mut self, modal: DesktopModal) {
        self.modal = modal;
        self.modal_anim = 0;
        self.dirty = true;
    }

    fn commit_modal(&mut self) {
        let current = core::mem::replace(&mut self.modal, DesktopModal::None);
        match current {
            DesktopModal::CreateFolder { name_buf } => {
                let clean = if name_buf.trim().is_empty() {
                    "Yeni_Klasor"
                } else {
                    name_buf.trim()
                };
                let path = alloc::format!("{}/{}", self.desktop_dir, clean);
                let _ = self.vfs.create_dir_path(&path);
                self.sync_desktop_fs();
            }
            DesktopModal::CreateTextFile { name_buf } => {
                let raw = if name_buf.trim().is_empty() {
                    "notlar.txt"
                } else {
                    name_buf.trim()
                };
                let final_name = if raw.contains('.') {
                    String::from(raw)
                } else {
                    alloc::format!("{}.txt", raw)
                };
                let path = alloc::format!("{}/{}", self.desktop_dir, final_name);
                let _ = self.vfs.write_file_path(
                    &path,
                    b"Masaustu metin belgesi.\nCift tiklayarak duzenleyebilirsiniz.\n",
                );
                self.sync_desktop_fs();
            }
            DesktopModal::RenameItem {
                old_name,
                new_buf,
                is_dir,
            } => {
                let clean = new_buf.trim();
                if !clean.is_empty() && clean != old_name {
                    let old_path = alloc::format!("{}/{}", self.desktop_dir, old_name);
                    let new_path = alloc::format!("{}/{}", self.desktop_dir, clean);
                    if is_dir {
                        if self.vfs.create_dir_path(&new_path).is_ok() {
                            let _ = self.vfs.remove_path(&old_path);
                        }
                    } else if let Ok(data) = self.vfs.read_file_path(&old_path) {
                        if self.vfs.write_file_path(&new_path, &data).is_ok() {
                            let _ = self.vfs.remove_path(&old_path);
                        }
                    }
                    self.sync_desktop_fs();
                }
            }
            DesktopModal::EditTextFile { name, content } => {
                let path = alloc::format!("{}/{}", self.desktop_dir, name);
                let _ = self.vfs.write_file_path(&path, content.as_bytes());
                self.sync_desktop_fs();
            }
            DesktopModal::None => {}
        }
        self.dirty = true;
    }

    fn hit(&self, x: i32, y: i32, width: i32, height: i32) -> Target {
        let unit = unit_for(width, height);

        if !matches!(self.modal, DesktopModal::None) {
            let tall = matches!(self.modal, DesktopModal::EditTextFile { .. });
            let m_rect = desktop_modal_rect(width, height, unit, tall);
            let (ok_r, cancel_r) = modal_buttons(m_rect, unit);
            if inside(ok_r, x, y) {
                return Target::ModalConfirm;
            }
            if inside(cancel_r, x, y) {
                return Target::ModalCancel;
            }
            if inside(m_rect, x, y) {
                return Target::ModalBody;
            }
        }

        if let Some((pos, kind)) = self.context_menu {
            let count = self.context_menu_item_count(kind);
            let menu = context_menu_rect(pos.0, pos.1, count, width, height, unit);
            for idx in 0..count {
                if inside(context_item_rect(menu, idx, unit), x, y) {
                    return Target::ContextItem(idx);
                }
            }
            if inside(menu, x, y) {
                return Target::ContextMenu;
            }
        }

        if self.start_open && self.start_anim > ONE / 2 {
            let apps = self.start_indices();
            let dock_top = height - dock_bottom_gap(unit) - dock_height(unit);
            let sl = start_layout(width, height, unit, dock_top, apps.len());
            if inside(sl.reboot, x, y) {
                return Target::PowerReboot;
            }
            if inside(sl.shut, x, y) {
                return Target::PowerShutdown;
            }
            if sl.pages > 1 {
                for p in 0..sl.pages {
                    if inside(start_dot_rect(&sl, p, unit), x, y) {
                        return Target::StartPage(p);
                    }
                }
            }
            let page = self.start_page.min(sl.pages - 1);
            let first = page * sl.per_page;
            let last = (first + sl.per_page).min(apps.len());
            for local in 0..last.saturating_sub(first) {
                if inside(start_cell(&sl, local), x, y) {
                    return Target::StartNode(apps[first + local]);
                }
            }
            if inside(sl.panel, x, y) {
                return Target::StartPanel;
            }
        }

        if self.dock_reveal >= ONE / 8 {
            let items = self.dock_indices();
            let dl = self.dock_layout(width, height, unit);
            if inside(dl.start, x, y) {
                return Target::StartButton;
            }
            for (pos, &app_idx) in items.iter().enumerate() {
                if inside(dl.tiles[pos], x, y) {
                    return Target::DockApp(app_idx);
                }
            }
            if inside(dl.status, x, y) {
                return Target::DockStatus;
            }
            if inside(dl.dock, x, y) {
                return Target::DockBody;
            }
        }

        if self.task_mgr.open {
            let tm = task_manager_rect(self.task_mgr.x, self.task_mgr.y, width, height, unit);
            if inside(tm, x, y) {
                if inside(task_manager_close_rect(tm, unit), x, y) {
                    return Target::TaskMgrClose;
                }
                if inside(title_bar(tm, unit), x, y) {
                    return Target::TaskMgrTitle;
                }
                if inside(task_manager_action_btn(tm, true, unit), x, y) {
                    return Target::TaskMgrKill;
                }
                if inside(task_manager_action_btn(tm, false, unit), x, y) {
                    return Target::TaskMgrOptimize;
                }
                let row_cnt = self.collect_task_rows().len().min(7);
                for r in 0..row_cnt {
                    if inside(task_manager_row_rect(tm, r, unit), x, y) {
                        return Target::TaskMgrRow(r);
                    }
                }
                return Target::TaskMgrBody;
            }
        }

        for position in (0..self.order.len()).rev() {
            let index = self.order[position];
            if !self.interactive(index) {
                continue;
            }
            let area = self.window_rect(index, unit, width, height);
            if !inside(area, x, y) {
                continue;
            }
            if inside(close_button(area, unit), x, y) {
                return Target::WinClose(index);
            }
            if inside(max_button(area, unit), x, y) {
                return Target::WinMax(index);
            }
            if inside(min_button(area, unit), x, y) {
                return Target::WinMin(index);
            }
            if inside(title_bar(area, unit), x, y) {
                return Target::WinTitle(index);
            }
            return Target::WinBody(index);
        }

        for (id, rect) in self.icon_rects(width, height, unit).iter().rev() {
            if self.dragging_icon() == Some(*id) {
                continue;
            }
            if inside(*rect, x, y) {
                return match id {
                    DesktopIconId::App(i) => Target::AppIcon(*i),
                    DesktopIconId::Fs(i) => Target::FsIcon(*i),
                };
            }
        }

        Target::Desktop
    }

    fn open_task_manager(&mut self, width: i32, height: i32, unit: i32) {
        self.task_mgr.open = true;
        let tm = task_manager_rect(0, 0, width, height, unit);
        self.task_mgr.x = ((width - tm.width) / 2).max(unit / 2);
        self.task_mgr.y = ((height - tm.height) / 2 - unit).max(unit / 2);
        self.dirty = true;
    }

    fn execute_context_action(
        &mut self,
        kind: ContextMenuKind,
        idx: usize,
        width: i32,
        height: i32,
        unit: i32,
    ) {
        match kind {
            ContextMenuKind::DesktopSurface => match idx {
                0 => self.open_task_manager(width, height, unit),
                1 => {
                    let next_num = self.fs_items.len() + 1;
                    self.open_modal(DesktopModal::CreateFolder {
                        name_buf: alloc::format!("Klasor_{}", next_num),
                    });
                }
                2 => {
                    let next_num = self.fs_items.len() + 1;
                    self.open_modal(DesktopModal::CreateTextFile {
                        name_buf: alloc::format!("Not_{}.txt", next_num),
                    });
                }
                3 => {
                    for slot in &mut self.apps {
                        slot.icon_cell = None;
                    }
                    self.sync_desktop_fs();
                    for item in &mut self.fs_items {
                        item.cell = None;
                    }
                }
                4 => {
                    self.settings.wallpaper = (self.settings.wallpaper + 1) % WALLPAPER_COUNT;
                    self.wallpaper_dirty = true;
                }
                5 => {
                    self.settings.wallpaper_animated = !self.settings.wallpaper_animated;
                }
                6 => self.open_app_by_id("nc.files", width, height, unit),
                _ => self.open_app_by_id("nc.settings", width, height, unit),
            },
            ContextMenuKind::AppItem(app_idx) => {
                if app_idx >= self.apps.len() {
                    return;
                }
                match idx {
                    0 => self.open_app(app_idx, width, height, unit),
                    1 => self.apps[app_idx].pinned_dock = !self.apps[app_idx].pinned_dock,
                    2 => self.apps[app_idx].pinned_start = !self.apps[app_idx].pinned_start,
                    3 => self.open_task_manager(width, height, unit),
                    _ => self.close_app(app_idx, width, height),
                }
            }
            ContextMenuKind::FsItem(fs_idx) => {
                if fs_idx >= self.fs_items.len() {
                    return;
                }
                match idx {
                    0 => self.open_fs_item(fs_idx, width, height, unit),
                    1 => {
                        let item = &self.fs_items[fs_idx];
                        let modal = DesktopModal::RenameItem {
                            old_name: item.name.clone(),
                            new_buf: item.name.clone(),
                            is_dir: item.is_dir,
                        };
                        self.open_modal(modal);
                    }
                    2 => self.open_app_by_id("nc.files", width, height, unit),
                    _ => {
                        let path =
                            alloc::format!("{}/{}", self.desktop_dir, self.fs_items[fs_idx].name);
                        let _ = self.vfs.remove_path(&path);
                        self.sync_desktop_fs();
                    }
                }
            }
        }
        self.dirty = true;
    }

    fn handle_desktop_icon_click(
        &mut self,
        icon_id: DesktopIconId,
        mouse: &MouseState,
        width: i32,
        height: i32,
        unit: i32,
    ) {
        let is_double = self.last_click_icon == Some(icon_id)
            && self.uptime_ms.wrapping_sub(self.last_click_ms) <= DOUBLE_CLICK_MS;

        self.selected_icon = Some(icon_id);
        self.dirty = true;

        if is_double {
            self.last_click_icon = None;
            self.icon_drag = None;
            match icon_id {
                DesktopIconId::App(app_idx) => self.open_app(app_idx, width, height, unit),
                DesktopIconId::Fs(fs_idx) => self.open_fs_item(fs_idx, width, height, unit),
            }
        } else {
            self.last_click_icon = Some(icon_id);
            self.last_click_ms = self.uptime_ms;
            let rect = self
                .icon_rects(width, height, unit)
                .iter()
                .find(|(id, _)| *id == icon_id)
                .map(|(_, r)| *r)
                .unwrap_or(Rect::new(mouse.x, mouse.y, 0, 0));
            self.icon_drag = Some(IconDrag {
                id: icon_id,
                offset_x: mouse.x - rect.x,
                offset_y: mouse.y - rect.y,
                start_x: mouse.x,
                start_y: mouse.y,
                px: rect.x,
                py: rect.y,
                moved: false,
            });
        }
    }

    fn click(
        &mut self,
        target: Target,
        mouse: &MouseState,
        width: i32,
        height: i32,
        unit: i32,
    ) -> DesktopAction {
        if !matches!(self.modal, DesktopModal::None) {
            match target {
                Target::ModalConfirm => self.commit_modal(),
                Target::ModalCancel => {
                    self.modal = DesktopModal::None;
                    self.dirty = true;
                }
                Target::ModalBody => {}
                _ => {
                    self.modal = DesktopModal::None;
                    self.dirty = true;
                }
            }
            return DesktopAction::None;
        }

        let keeps_menu = matches!(
            target,
            Target::StartButton
                | Target::StartNode(_)
                | Target::StartPanel
                | Target::StartPage(_)
                | Target::PowerReboot
                | Target::PowerShutdown
        );
        if !keeps_menu && self.start_open {
            self.start_open = false;
            self.dirty = true;
        }

        let keeps_ctx = matches!(target, Target::ContextMenu | Target::ContextItem(_));
        let active_ctx = self.context_menu;
        if !keeps_ctx && self.context_menu.is_some() {
            self.context_menu = None;
            self.dirty = true;
        }

        if !matches!(target, Target::AppIcon(_) | Target::FsIcon(_)) && self.selected_icon.is_some()
        {
            self.selected_icon = None;
            self.dirty = true;
        }

        match target {
            Target::TaskMgrClose => {
                self.task_mgr.open = false;
                self.dirty = true;
            }
            Target::TaskMgrTitle => {
                self.task_mgr.dragging =
                    Some((mouse.x - self.task_mgr.x, mouse.y - self.task_mgr.y));
            }
            Target::TaskMgrRow(r) => {
                self.task_mgr.selected_row = r;
                self.dirty = true;
            }
            Target::TaskMgrOptimize => {
                for s in &mut self.bg_services {
                    s.cpu_pct = 1;
                }
                for h in &mut self.task_mgr.cpu_history {
                    *h = (*h / 2).max(4);
                }
                self.dirty = true;
            }
            Target::TaskMgrKill => {
                let target_app = {
                    let rows = self.collect_task_rows();
                    rows.get(self.task_mgr.selected_row)
                        .and_then(|r| r.app_index)
                };
                if let Some(app_idx) = target_app {
                    self.close_app(app_idx, width, height);
                    self.task_mgr.selected_row = 0;
                } else {
                    for s in &mut self.bg_services {
                        s.cpu_pct = 1;
                    }
                }
                self.dirty = true;
            }
            Target::StartButton => {
                self.start_open = !self.start_open;
                if self.start_open {
                    self.start_page = 0;
                }
                self.dirty = true;
            }
            Target::StartPage(p) => {
                self.start_page = p;
                self.dirty = true;
            }
            Target::StartNode(app_idx) => {
                self.start_open = false;
                self.open_app(app_idx, width, height, unit);
                self.dirty = true;
            }
            Target::PowerReboot => return DesktopAction::Reboot,
            Target::PowerShutdown => return DesktopAction::Shutdown,

            Target::DockStatus => {
                let next = if self.volume >= 100 { 0 } else { self.volume as i32 + 10 };
                self.volume = next as u8;
                self.muted = self.volume == 0;
                self.vol_osd_ms = 1800;
                self.dirty = true;
            }

            Target::DockApp(app) => {
                if !self.apps[app].window.open {
                    self.open_app(app, width, height, unit);
                } else if self.apps[app].window.minimized {
                    self.restore_app(app);
                } else if self.focused() == Some(app) {
                    self.minimize_app(app);
                } else {
                    self.raise(app);
                }
                self.dirty = true;
            }
            Target::AppIcon(app) => {
                self.handle_desktop_icon_click(DesktopIconId::App(app), mouse, width, height, unit);
            }
            Target::FsIcon(fs_idx) => {
                self.handle_desktop_icon_click(
                    DesktopIconId::Fs(fs_idx),
                    mouse,
                    width,
                    height,
                    unit,
                );
            }
            Target::WinClose(app) => self.close_app(app, width, height),
            Target::WinMax(app) => {
                self.apps[app].window.maximized = !self.apps[app].window.maximized;
                self.raise(app);
                self.dirty = true;
            }
            Target::WinMin(app) => self.minimize_app(app),
            Target::WinTitle(app) => {
                self.raise(app);
                let double = self.last_title_app == Some(app)
                    && self.uptime_ms.wrapping_sub(self.last_title_ms) <= DOUBLE_CLICK_MS;
                if double {
                    self.last_title_app = None;
                    self.apps[app].window.maximized = !self.apps[app].window.maximized;
                    self.dirty = true;
                } else {
                    self.last_title_app = Some(app);
                    self.last_title_ms = self.uptime_ms;
                    if !self.apps[app].window.maximized {
                        let area = self.window_rect(app, unit, width, height);
                        self.drag = Some(Drag {
                            app,
                            offset_x: mouse.x - area.x,
                            offset_y: mouse.y - area.y,
                        });
                    }
                }
            }
            Target::WinBody(app) => self.raise(app),
            Target::ContextItem(idx) => {
                self.context_menu = None;
                if let Some((_, kind)) = active_ctx {
                    self.execute_context_action(kind, idx, width, height, unit);
                }
            }
            _ => {}
        }

        DesktopAction::None
    }

    fn dispatch(
        &mut self,
        target: Target,
        mouse: &MouseState,
        key: Option<KeyCode>,
        width: i32,
        height: i32,
        unit: i32,
    ) {
        let pointer_app = match target {
            Target::WinBody(app)
                if self.drag.is_none()
                    && self.icon_drag.is_none()
                    && self.context_menu.is_none() =>
            {
                Some(app)
            }
            _ => None,
        };
        let focused = self.focused();
        let mut to_close: Option<usize> = None;
        let mut new_app_to_install: Option<Box<dyn App>> = None;

        for index in 0..self.apps.len() {
            if !self.interactive(index) {
                continue;
            }
            let area = content_area(self.window_rect(index, unit, width, height), unit);
            let pointer = if pointer_app == Some(index) {
                Some(Pointer {
                    x: mouse.x,
                    y: mouse.y,
                    down: mouse.left_down,
                    clicked: mouse.left_clicked,
                    right_clicked: mouse.right_clicked,
                })
            } else {
                None
            };
            let input = Input {
                area,
                pointer,
                key: if focused == Some(index) { key } else { None },
                focused: focused == Some(index),
            };
            let mut ctx = self.context(width, height);
            let update = self.apps[index].app.update(&mut ctx, &input);
            if update.redraw {
                self.dirty = true;
            }
            if update.close {
                to_close = Some(index);
            }
            if let Some(app) = update.install_app {
                new_app_to_install = Some(app);
            }
            self.apply_settings(ctx.settings);
        }

        if let Some(index) = to_close {
            self.close_app(index, width, height);
        }
        if let Some(app) = new_app_to_install {
            self.install_app(app);
            self.dirty = true;
        }
    }

    fn handle_modal_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Escape => {
                self.modal = DesktopModal::None;
                self.dirty = true;
            }
            KeyCode::Enter => {
                if let DesktopModal::EditTextFile {
                    ref mut content, ..
                } = self.modal
                {
                    content.push('\n');
                    self.dirty = true;
                } else {
                    self.commit_modal();
                }
            }
            KeyCode::Backspace => match self.modal {
                DesktopModal::CreateFolder { ref mut name_buf }
                | DesktopModal::CreateTextFile { ref mut name_buf } => {
                    name_buf.pop();
                    self.dirty = true;
                }
                DesktopModal::RenameItem {
                    ref mut new_buf, ..
                } => {
                    new_buf.pop();
                    self.dirty = true;
                }
                DesktopModal::EditTextFile {
                    ref mut content, ..
                } => {
                    content.pop();
                    self.dirty = true;
                }
                DesktopModal::None => {}
            },
            KeyCode::Space => match self.modal {
                DesktopModal::CreateFolder { ref mut name_buf }
                | DesktopModal::CreateTextFile { ref mut name_buf } => {
                    if name_buf.len() < 26 {
                        name_buf.push('_');
                        self.dirty = true;
                    }
                }
                DesktopModal::RenameItem {
                    ref mut new_buf, ..
                } => {
                    if new_buf.len() < 26 {
                        new_buf.push('_');
                        self.dirty = true;
                    }
                }
                DesktopModal::EditTextFile {
                    ref mut content, ..
                } => {
                    content.push(' ');
                    self.dirty = true;
                }
                DesktopModal::None => {}
            },
            KeyCode::Char(c) => match self.modal {
                DesktopModal::CreateFolder { ref mut name_buf }
                | DesktopModal::CreateTextFile { ref mut name_buf } => {
                    if c != '/' && c != '\\' && name_buf.len() < 26 {
                        name_buf.push(c);
                        self.dirty = true;
                    }
                }
                DesktopModal::RenameItem {
                    ref mut new_buf, ..
                } => {
                    if c != '/' && c != '\\' && new_buf.len() < 26 {
                        new_buf.push(c);
                        self.dirty = true;
                    }
                }
                DesktopModal::EditTextFile {
                    ref mut content, ..
                } => {
                    content.push(c);
                    self.dirty = true;
                }
                DesktopModal::None => {}
            },
            _ => {}
        }
    }

    fn advance_animations(&mut self, dt: u32, mouse: &MouseState, height: i32, unit: i32) {
        let open_ms = self.open_ms();
        let leave_ms = self.leave_ms();
        let leave_step = ((dt as i64 * ONE as i64) / leave_ms.max(1) as i64).max(1) as i32;
        let mut dirty = false;
        for slot in self.apps.iter_mut() {
            let w = &mut slot.window;
            if w.leave > 0 {
                w.leave += leave_step;
                if w.leave >= ONE {
                    w.leave = 0;
                    if w.leave_kind == 2 {
                        w.maximized = false;
                        w.max_t = 0;
                    }
                    w.leave_kind = 0;
                }
                dirty = true;
            }
            if w.open && w.open_anim < ONE {
                w.open_anim = approach(w.open_anim, ONE, dt, open_ms);
                dirty = true;
            }
            let target = if w.maximized { ONE } else { 0 };
            if w.max_t != target {
                w.max_t = approach(w.max_t, target, dt, open_ms.max(260));
                dirty = true;
            }
        }

        let dock_top = height - dock_area(unit) - unit;
        let want_dock = !self.settings.dock_auto_hide
            || self.start_open
            || mouse.y >= height - unit
            || (self.dock_reveal > ONE / 4 && mouse.y >= dock_top);
        let reveal_target = if want_dock { ONE } else { 0 };
        if self.dock_reveal != reveal_target {
            self.dock_reveal = approach(self.dock_reveal, reveal_target, dt, 240);
            dirty = true;
        }

        let busy = self.drag.is_some()
            || self.icon_drag.map(|d| d.moved).unwrap_or(false)
            || self.task_mgr.dragging.is_some();
        let want_mag = !self.start_open
            && !busy
            && self.dock_reveal > ONE / 2
            && mouse.y >= dock_top - unit
            && matches!(self.hover, Target::DockApp(_) | Target::DockBody | Target::StartButton | Target::DockStatus);
        let mag_target = if want_mag { ONE } else { 0 };
        if self.dock_mag != mag_target {
            self.dock_mag = approach(self.dock_mag, mag_target, dt, if want_mag { 170 } else { 220 });
            dirty = true;
        }
        if self.dock_mag > 0 {
            if self.dock_focus_x != mouse.x {
                let a = ((dt as i64 * ONE as i64) / 60).clamp(0, ONE as i64) as i32;
                let diff = mouse.x - self.dock_focus_x;
                let step = (diff as i64 * a as i64 / ONE as i64) as i32;
                self.dock_focus_x = if step == 0 { mouse.x } else { self.dock_focus_x + step };
                dirty = true;
            }
        } else {
            self.dock_focus_x = mouse.x;
        }

        let start_target = if self.start_open { ONE } else { 0 };
        if self.start_anim != start_target {
            let dur = if self.start_open { 280 } else { 190 };
            self.start_anim = approach(self.start_anim, start_target, dt, dur);
            dirty = true;
        }

        let ctx_target = if self.context_menu.is_some() { ONE } else { 0 };
        if self.ctx_anim != ctx_target {
            self.ctx_anim = approach(self.ctx_anim, ctx_target, dt, if ctx_target > 0 { 170 } else { 110 });
            dirty = true;
        }

        if matches!(self.modal, DesktopModal::None) {
            self.modal_anim = 0;
        } else if self.modal_anim < ONE {
            self.modal_anim = approach(self.modal_anim, ONE, dt, 200);
            dirty = true;
        }

        if let Some(mut s) = self.settle {
            s.t = approach(s.t, ONE, dt, 190);
            self.settle = if s.t >= ONE { None } else { Some(s) };
            dirty = true;
        }

        if self.vol_osd_ms > 0 {
            self.vol_osd_ms = self.vol_osd_ms.saturating_sub(dt);
        }
        let osd_target = if self.vol_osd_ms > 0 { ONE } else { 0 };
        if self.vol_osd_anim != osd_target {
            self.vol_osd_anim = approach(self.vol_osd_anim, osd_target, dt, if osd_target > 0 { 160 } else { 240 });
            dirty = true;
        }
        let target_disp = if self.muted { 0 } else { ONE * (self.volume as i32) / 100 };
        if self.vol_disp != target_disp {
            self.vol_disp = approach(self.vol_disp, target_disp, dt, 110);
            dirty = true;
        }

        if dirty {
            self.dirty = true;
        }
    }

    pub fn update(
        &mut self,
        mouse: &MouseState,
        key: Option<KeyCode>,
        width: i32,
        height: i32,
        uptime_ms: u32,
    ) -> DesktopAction {
        let unit = unit_for(width, height);
        let dt = uptime_ms.wrapping_sub(self.uptime_ms).min(100);
        self.pointer_x = mouse.x;
        self.pointer_y = mouse.y;

        if uptime_ms / 1000 != self.uptime_ms / 1000 {
            self.dirty = true;
        }
        if uptime_ms / 250 != self.uptime_ms / 250 {
            self.tick_background_processes(uptime_ms);
        }
        if self.settings.wallpaper_animated && uptime_ms / 64 != self.uptime_ms / 64 {
            self.dirty = true;
        }
        if !matches!(self.modal, DesktopModal::None) && uptime_ms / 500 != self.uptime_ms / 500 {
            self.dirty = true;
        }
        self.uptime_ms = uptime_ms;

        self.advance_animations(dt, mouse, height, unit);

        if !matches!(self.modal, DesktopModal::None) {
            if let Some(k) = key {
                self.handle_modal_key(k);
            }
            let target = self.hit(mouse.x, mouse.y, width, height);
            if target != self.hover {
                self.hover = target;
                self.dirty = true;
            }
            if mouse.left_clicked {
                let _ = self.click(target, mouse, width, height, unit);
            }
            return DesktopAction::None;
        }

        let menu_was_open = self.start_open;

        match key {
            Some(KeyCode::Up) if !self.start_open && !self.task_mgr.open => {
                self.adjust_volume(5, false);
            }
            Some(KeyCode::Down) if !self.start_open && !self.task_mgr.open => {
                self.adjust_volume(-5, false);
            }
            _ => {}
        }

        if key == Some(KeyCode::Escape) {
            if self.start_open {
                self.start_open = false;
                self.dirty = true;
            }
            if self.context_menu.is_some() {
                self.context_menu = None;
                self.dirty = true;
            }
            if self.selected_icon.is_some() {
                self.selected_icon = None;
                self.dirty = true;
            }
        }

        if key == Some(KeyCode::Enter) && self.focused().is_none() {
            if let Some(sel) = self.selected_icon {
                match sel {
                    DesktopIconId::App(idx) => self.open_app(idx, width, height, unit),
                    DesktopIconId::Fs(idx) => self.open_fs_item(idx, width, height, unit),
                }
            }
        }

        if let Some((ox, oy)) = self.task_mgr.dragging {
            if mouse.left_down {
                let tm = task_manager_rect(0, 0, width, height, unit);
                let nx = (mouse.x - ox).clamp(4, (width - tm.width).max(4));
                let ny = (mouse.y - oy).clamp(4, (height - title_height(unit) - unit).max(4));
                if nx != self.task_mgr.x || ny != self.task_mgr.y {
                    self.task_mgr.x = nx;
                    self.task_mgr.y = ny;
                    self.dirty = true;
                }
            } else {
                self.task_mgr.dragging = None;
            }
        }

        if let Some(drag) = self.drag {
            if mouse.left_down {
                let usable = self.usable(width, height, unit);
                let rect = self.normal_rect(drag.app, unit, usable);
                let (x, y) = clamp_window_pos(
                    mouse.x - drag.offset_x,
                    mouse.y - drag.offset_y,
                    rect.width,
                    rect.height,
                    unit,
                    usable,
                );
                if x != self.apps[drag.app].window.x || y != self.apps[drag.app].window.y {
                    self.apps[drag.app].window.x = x;
                    self.apps[drag.app].window.y = y;
                    self.dirty = true;
                }
            } else {
                self.drag = None;
            }
        }

        if let Some(mut idrag) = self.icon_drag {
            if mouse.left_down {
                let dist = (mouse.x - idrag.start_x).abs() + (mouse.y - idrag.start_y).abs();
                if dist > 6 || idrag.moved {
                    idrag.moved = true;
                    idrag.px = (mouse.x - idrag.offset_x).clamp(0, (width - unit * 4).max(0));
                    idrag.py = (mouse.y - idrag.offset_y).clamp(0, (height - unit * 4).max(0));
                    self.icon_drag = Some(idrag);
                    self.dirty = true;
                }
            } else {
                if idrag.moved {
                    self.finish_icon_drag(idrag, width, height, unit);
                }
                self.icon_drag = None;
            }
        }

        let target = self.hit(mouse.x, mouse.y, width, height);

        if mouse.right_clicked {
            match target {
                Target::WinBody(app_idx) => {
                    self.context_menu = None;
                    self.start_open = false;
                    self.raise(app_idx);
                }
                Target::AppIcon(app_idx) => {
                    self.selected_icon = Some(DesktopIconId::App(app_idx));
                    self.open_context((mouse.x, mouse.y), ContextMenuKind::AppItem(app_idx));
                }
                Target::DockApp(app_idx) | Target::StartNode(app_idx) => {
                    self.open_context((mouse.x, mouse.y), ContextMenuKind::AppItem(app_idx));
                }
                Target::FsIcon(fs_idx) => {
                    self.selected_icon = Some(DesktopIconId::Fs(fs_idx));
                    self.open_context((mouse.x, mouse.y), ContextMenuKind::FsItem(fs_idx));
                }
                Target::DockStatus => {
                    self.adjust_volume(0, true);
                }
                _ => self.open_context((mouse.x, mouse.y), ContextMenuKind::DesktopSurface),
            }
        }

        let hover = if self.drag.is_some()
            || self.icon_drag.map(|d| d.moved).unwrap_or(false)
            || self.task_mgr.dragging.is_some()
        {
            Target::Nothing
        } else {
            target
        };
        if hover != self.hover {
            self.hover = hover;
            self.dirty = true;
        }

        let mut action = DesktopAction::None;
        if mouse.left_clicked {
            action = self.click(target, mouse, width, height, unit);
        }

        let app_key = if menu_was_open && key == Some(KeyCode::Escape) {
            None
        } else {
            key
        };
        self.dispatch(target, mouse, app_key, width, height, unit);

        action
    }
}

pub fn draw_wallpaper(canvas: &mut Canvas<'_>, index: usize) {
    wallpaper::draw_wallpaper(canvas, index);
}

pub fn draw(canvas: &mut Canvas<'_>, wallpaper: &Canvas<'_>, state: &DesktopState) {
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    let unit = unit_for(width, height);
    let ctx = state.context(width, height);
    let accent = state.settings.accent_color();
    let (blur, passes) = state.blur_params(unit);

    canvas.reset_clip();
    canvas.copy_rect_from(wallpaper, 0, 0, 0, 0, width, height);
    if state.settings.wallpaper_animated {
        wallpaper::draw_motion(
            canvas,
            state.uptime_ms,
            state.settings.wallpaper_speed.max(1) as u32,
            state.settings.wallpaper,
        );
    }
    shell::draw_icons(canvas, state, width, height, unit);

    let focused = state.focused();
    for position in 0..state.order.len() {
        let index = state.order[position];
        if !state.visible(index) {
            continue;
        }
        let slot = &state.apps[index];
        let visual = state.window_visual(index, unit, width, height);
        canvas.reset_clip();
        window::draw_frame(
            canvas,
            slot.app.info(),
            index,
            state.hover,
            accent,
            visual.rect,
            focused == Some(index),
            slot.window.max_t,
            visual.appear,
            blur,
            passes,
            unit,
        );
        if visual.content > 0 {
            let client_rect = content_area(visual.rect, unit);
            let clip_area = Rect::new(
                client_rect.x + 1,
                client_rect.y + 1,
                (client_rect.width - 2).max(0),
                (client_rect.height - 2).max(0),
            );
            canvas.set_clip(clip_area);
            slot.app.draw(canvas, client_rect, &ctx);
            canvas.reset_clip();
            if visual.content < ONE {
                fill_rr(
                    canvas,
                    client_rect,
                    0,
                    SNOW,
                    scale_u32(255, ONE - visual.content),
                );
            }
        }
    }

    shell::draw_task_manager(canvas, state, width, height, unit);
    shell::draw_dock(canvas, state, width, height, unit);

    if state.start_anim > 0 {
        shell::draw_start_menu(canvas, state, width, height, unit);
    }

    if let Some((pos, kind)) = state.context_menu.or(state.ctx_last) {
        if state.ctx_anim > 0 {
            shell::draw_context_menu(canvas, state, pos, kind, width, height, unit);
        }
    }

    if !matches!(state.modal, DesktopModal::None) {
        shell::draw_desktop_modal(canvas, state, width, height, unit);
    }
    if state.vol_osd_anim > 0 {
        shell::draw_volume_osd(canvas, state, width, height, unit);
    }
}
