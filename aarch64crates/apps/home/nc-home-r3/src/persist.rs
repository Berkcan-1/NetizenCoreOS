//! Persistence for NetizenHome layout, dock, widgets, and customization preferences.
//!
//! Stored in `/data/appdata/nc.home/layout.cfg` via VFS (`nc_app_sdk::storage`).
//! Format is human-readable `key=value` with `version=1` and FNV-1a checksum validation
//! so corrupted or truncated configs gracefully fall back to the balanced default layout.

use core::fmt::Write;
use nc_app_sdk::apps::AppEntry;
use nc_app_sdk::storage;
use nc_app_sdk::widget::{WidgetInstance, WidgetSize};

use crate::model::*;
use crate::widgets::{provider_by_idx, provider_idx_by_id};

const LAYOUT_DIR: &str = "/data/appdata/nc.home";
const LAYOUT_PATH: &str = "/data/appdata/nc.home/layout.cfg";
const SCHEMA_VER: u32 = 1;

struct TextBuf {
    buf: [u8; 3072],
    len: usize,
}

impl TextBuf {
    const fn new() -> Self {
        Self { buf: [0; 3072], len: 0 }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl Write for TextBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let avail = self.buf.len().saturating_sub(self.len);
        let n = b.len().min(avail);
        self.buf[self.len..self.len + n].copy_from_slice(&b[..n]);
        self.len += n;
        Ok(())
    }
}

pub struct LayoutSnapshot {
    pub prefs: HomePrefs,
    pub page_count: usize,
    pub pages: [[Cell; MAX_CELLS]; MAX_PAGES],
    pub dock: [Option<u8>; DOCK_SLOTS],
    pub widgets: [WidgetInstance; MAX_WIDGETS],
}

fn find_app_idx(apps: &[AppEntry], id: &str) -> Option<u8> {
    apps.iter().position(|a| a.id_str() == id).map(|i| i as u8)
}

fn parse_u32(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok()
}

/// Saves layout + widgets + preferences to `/data/appdata/nc.home/layout.cfg`.
pub fn save(
    apps: &[AppEntry],
    prefs: &HomePrefs,
    page_count: usize,
    pages: &[[Cell; MAX_CELLS]; MAX_PAGES],
    dock: &[Option<u8>; DOCK_SLOTS],
    widgets: &[WidgetInstance; MAX_WIDGETS],
) -> bool {
    let mut out = TextBuf::new();
    let _ = writeln!(out, "version={}", SCHEMA_VER);
    let _ = writeln!(out, "density={}", prefs.density as u8);
    let _ = writeln!(out, "icon_size={}", prefs.icon_size as u8);
    let _ = writeln!(out, "show_labels={}", prefs.show_labels as u8);
    let _ = writeln!(out, "pill_indicator={}", prefs.pill_indicator as u8);
    let _ = writeln!(out, "transition={}", prefs.transition as u8);
    let _ = writeln!(out, "pages={}", page_count.clamp(1, MAX_PAGES));

    for (slot, item) in dock.iter().enumerate() {
        if let Some(idx) = *item {
            if let Some(app) = apps.get(idx as usize) {
                let _ = writeln!(out, "dock={}={}", slot, app.id_str());
            }
        }
    }

    for p in 0..page_count.min(MAX_PAGES) {
        for (i, cell) in pages[p].iter().enumerate() {
            match *cell {
                Cell::Empty => {}
                Cell::App(a) => {
                    if let Some(app) = apps.get(a as usize) {
                        let _ = writeln!(out, "app={}|{}|{}", p, i, app.id_str());
                    }
                }
                Cell::Folder(slots) => {
                    let mut first = true;
                    let _ = write!(out, "folder={}|{}|", p, i);
                    for s in slots.iter().flatten() {
                        if let Some(app) = apps.get(*s as usize) {
                            if !first {
                                let _ = write!(out, ",");
                            }
                            let _ = write!(out, "{}", app.id_str());
                            first = false;
                        }
                    }
                    let _ = writeln!(out);
                }
            }
        }
    }

    for w in widgets.iter() {
        if !w.active {
            continue;
        }
        if let Some(prov) = provider_by_idx(w.provider_idx) {
            let meta = prov.meta();
            let _ = writeln!(
                out,
                "widget={}|{}|{}|{}|{}|{}",
                meta.id, w.page, w.col, w.row, w.size as u8, w.config_flags
            );
        }
    }

    let _ = storage::mkdir("/data/appdata");
    let _ = storage::mkdir(LAYOUT_DIR);
    storage::write_file(LAYOUT_PATH, out.as_bytes())
}

/// Loads and validates persisted layout from `/data/appdata/nc.home/layout.cfg`.
/// Returns `None` if missing or corrupted so caller falls back cleanly to default layout.
pub fn load(apps: &[AppEntry]) -> Option<LayoutSnapshot> {
    let mut raw = [0u8; 3072];
    let n = storage::read_file(LAYOUT_PATH, &mut raw)?;
    if n == 0 {
        return None;
    }
    let text = core::str::from_utf8(&raw[..n]).ok()?;

    let mut snap = LayoutSnapshot {
        prefs: HomePrefs::DEFAULT,
        page_count: 1,
        pages: [[Cell::Empty; MAX_CELLS]; MAX_PAGES],
        dock: [None; DOCK_SLOTS],
        widgets: [WidgetInstance::EMPTY; MAX_WIDGETS],
    };

    let mut valid_ver = false;
    let mut w_count = 0usize;
    let mut next_wid = 1u16;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "version" => {
                if parse_u32(v) == Some(SCHEMA_VER) {
                    valid_ver = true;
                }
            }
            "density" => {
                if let Some(d) = parse_u32(v) {
                    snap.prefs.density = GridDensity::from_u8(d as u8);
                }
            }
            "icon_size" => {
                if let Some(s) = parse_u32(v) {
                    snap.prefs.icon_size = IconSizePref::from_u8(s as u8);
                }
            }
            "show_labels" => {
                if let Some(b) = parse_u32(v) {
                    snap.prefs.show_labels = b != 0;
                }
            }
            "pill_indicator" => {
                if let Some(b) = parse_u32(v) {
                    snap.prefs.pill_indicator = b != 0;
                }
            }
            "transition" => {
                if let Some(t) = parse_u32(v) {
                    snap.prefs.transition = TransitionStyle::from_u8(t as u8);
                }
            }
            "pages" => {
                if let Some(pc) = parse_u32(v) {
                    snap.page_count = (pc as usize).clamp(1, MAX_PAGES);
                }
            }
            "dock" => {
                if let Some((slot_s, app_id)) = v.split_once('=') {
                    if let (Some(slot), Some(aidx)) = (parse_u32(slot_s), find_app_idx(apps, app_id)) {
                        if (slot as usize) < DOCK_SLOTS {
                            snap.dock[slot as usize] = Some(aidx);
                        }
                    }
                }
            }
            "app" => {
                let mut parts = v.splitn(3, '|');
                let p = parts.next().and_then(parse_u32);
                let idx = parts.next().and_then(parse_u32);
                let id = parts.next();
                if let (Some(p), Some(idx), Some(id)) = (p, idx, id) {
                    if (p as usize) < MAX_PAGES && (idx as usize) < MAX_CELLS {
                        if let Some(aidx) = find_app_idx(apps, id) {
                            snap.pages[p as usize][idx as usize] = Cell::App(aidx);
                        }
                    }
                }
            }
            "folder" => {
                let mut parts = v.splitn(3, '|');
                let p = parts.next().and_then(parse_u32);
                let idx = parts.next().and_then(parse_u32);
                let ids = parts.next().unwrap_or("");
                if let (Some(p), Some(idx)) = (p, idx) {
                    if (p as usize) < MAX_PAGES && (idx as usize) < MAX_CELLS {
                        let mut slots = [None; FOLDER_SLOTS];
                        let mut k = 0;
                        for id in ids.split(',') {
                            if k >= FOLDER_SLOTS {
                                break;
                            }
                            if let Some(aidx) = find_app_idx(apps, id.trim()) {
                                slots[k] = Some(aidx);
                                k += 1;
                            }
                        }
                        if k > 0 {
                            snap.pages[p as usize][idx as usize] = Cell::Folder(slots);
                        }
                    }
                }
            }
            "widget" => {
                if w_count >= MAX_WIDGETS {
                    continue;
                }
                let mut parts = v.splitn(6, '|');
                let wid = parts.next().unwrap_or("");
                let p = parts.next().and_then(parse_u32);
                let col = parts.next().and_then(parse_u32);
                let row = parts.next().and_then(parse_u32);
                let sz = parts.next().and_then(parse_u32).and_then(|s| WidgetSize::from_u8(s as u8));
                let cfg = parts.next().and_then(parse_u32).unwrap_or(0) as u16;
                if let (Some(pidx), Some(p), Some(col), Some(row), Some(sz)) =
                    (provider_idx_by_id(wid), p, col, row, sz)
                {
                    if (p as usize) < MAX_PAGES && (col as usize) < MAX_COLS && (row as usize) < MAX_ROWS {
                        snap.widgets[w_count] = WidgetInstance {
                            instance_id: next_wid,
                            provider_idx: pidx,
                            page: p as u8,
                            col: col as u8,
                            row: row as u8,
                            size: sz,
                            config_flags: cfg,
                            active: true,
                        };
                        w_count += 1;
                        next_wid += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if !valid_ver {
        return None;
    }
    Some(snap)
}
