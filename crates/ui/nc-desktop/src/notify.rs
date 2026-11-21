use alloc::string::String;
use alloc::vec::Vec;
use nc_app::notify::{self as shared, Level, Notification, Permission};
use nc_app::text::Line;
use nc_app::{tr, TextId};
use nc_font::text_width;
use nc_gfx::{rgb, Canvas, Rect};
use nc_ui::{draw_text_color, text_style, WHITE};

use crate::art::{art_rect_for_tile, draw_art, kind_for, ArtKind};
use crate::layout::inside;
use crate::motion::{approach, ease_out_cubic, scale_u32, ONE};
use crate::paint::{fill_rr, fit_text, glass_surface};
use crate::theme::{dock_glass, glass_text, ink_soft, CLOSE_RED, DANGER, WARN};
use crate::{DesktopState, Target};

const TOAST_MS: u32 = 5_500;
const MAX_TOASTS: usize = 3;
const MAX_HISTORY: usize = 12;
const PANEL_ROWS: usize = 6;
const PERM_ROWS: usize = 4;
const CONFIG_PATH: &str = "/data/system/notify.cfg";

struct Toast {
    id: u32,
    app_id: String,
    title: String,
    body: String,
    level: Level,
    age_ms: u32,
    anim: i32,
    leaving: bool,
}

struct Held {
    app_id: String,
    items: Vec<Notification>,
}

pub(crate) struct HistoryItem {
    app_id: String,
    title: String,
    body: String,
    level: Level,
    at_ms: u32,
}

pub(crate) struct NotifyState {
    toasts: Vec<Toast>,
    history: Vec<HistoryItem>,
    held: Vec<Held>,
    prompt: Option<String>,
    prompt_anim: i32,
    panel_open: bool,
    panel_anim: i32,
    unread: usize,
    next_id: u32,
}

impl NotifyState {
    pub(crate) fn new() -> Self {
        Self {
            toasts: Vec::new(),
            history: Vec::new(),
            held: Vec::new(),
            prompt: None,
            prompt_anim: 0,
            panel_open: false,
            panel_anim: 0,
            unread: 0,
            next_id: 1,
        }
    }

    pub(crate) fn panel_open(&self) -> bool {
        self.panel_open
    }
}

fn card_w(unit: i32) -> i32 {
    unit * 17
}

fn toast_h(unit: i32) -> i32 {
    unit * 39 / 10
}

fn anchor(width: i32, unit: i32, w: i32) -> (i32, i32) {
    (width - unit - w, unit)
}

fn toast_rect(width: i32, unit: i32, index: usize) -> Rect {
    let (x, y) = anchor(width, unit, card_w(unit));
    Rect::new(x, y + index as i32 * (toast_h(unit) + unit / 2), card_w(unit), toast_h(unit))
}

fn prompt_rect(width: i32, unit: i32, toasts: usize) -> Rect {
    let (x, y) = anchor(width, unit, card_w(unit));
    let top = y + toasts as i32 * (toast_h(unit) + unit / 2);
    Rect::new(x, top, card_w(unit), unit * 65 / 10)
}

fn prompt_buttons(card: Rect, unit: i32) -> (Rect, Rect) {
    let pad = unit * 6 / 5;
    let gap = unit / 2;
    let bw = (card.width - pad * 2 - gap) / 2;
    let bh = unit * 2 + unit / 5;
    let y = card.y + card.height - pad * 3 / 4 - bh;
    (Rect::new(card.x + pad, y, bw, bh), Rect::new(card.x + pad + bw + gap, y, bw, bh))
}

fn panel_w(unit: i32) -> i32 {
    unit * 19
}

fn panel_layout(state: &DesktopState, width: i32, unit: i32) -> (Rect, Rect, Vec<Rect>, Vec<Rect>) {
    let rows = state.notify.history.len().min(PANEL_ROWS);
    let perms = shared::permissions().len().min(PERM_ROWS);
    let w = panel_w(unit);
    let (x, y) = anchor(width, unit, w);
    let header = unit * 3;
    let row_h = unit * 7 / 2;
    let body = if rows == 0 { unit * 4 } else { rows as i32 * row_h };
    let perm_head = if perms > 0 { unit * 5 / 2 } else { 0 };
    let perm_rows = perms as i32 * (unit * 5 / 2);
    let h = header + body + perm_head + perm_rows + unit / 2;
    let panel = Rect::new(x, y, w, h);
    let clear = Rect::new(x + w - unit * 6 - unit / 2, y + unit / 2, unit * 6, unit * 2);
    let mut row_rects = Vec::new();
    for i in 0..rows {
        row_rects.push(Rect::new(x, y + header + i as i32 * row_h, w, row_h));
    }
    let mut perm_rects = Vec::new();
    let perm_top = y + header + body + perm_head;
    for i in 0..perms {
        perm_rects.push(Rect::new(x + unit / 2, perm_top + i as i32 * (unit * 5 / 2), w - unit, unit * 5 / 2));
    }
    (panel, clear, row_rects, perm_rects)
}

pub(crate) fn clock_zone(status: Rect) -> Rect {
    Rect::new(status.x + status.width * 45 / 100, status.y, status.width - status.width * 45 / 100, status.height)
}

fn level_color(level: Level, accent: u32) -> u32 {
    match level {
        Level::Info => accent,
        Level::Success => rgb(60, 200, 120),
        Level::Warning => WARN,
        Level::Error => DANGER,
    }
}

impl DesktopState {
    pub(crate) fn load_notify_permissions(&mut self) {
        let Ok(bytes) = self.vfs.read_file_path(CONFIG_PATH) else { return; };
        let Ok(text) = core::str::from_utf8(&bytes) else { return; };
        for line in text.lines() {
            let Some((id, value)) = line.trim().rsplit_once('=') else { continue; };
            match value.trim() {
                "allow" => shared::set_permission(id.trim(), Permission::Allowed),
                "deny" => shared::set_permission(id.trim(), Permission::Denied),
                _ => {}
            }
        }
    }

    fn save_notify_permissions(&mut self) {
        let mut out = String::new();
        for (id, permission) in shared::permissions() {
            let word = match permission {
                Permission::Allowed => "allow",
                Permission::Denied => "deny",
                Permission::Ask => continue,
            };
            out.push_str(&id);
            out.push('=');
            out.push_str(word);
            out.push('\n');
        }
        let _ = self.vfs.create_dir_path("/data/system");
        let _ = self.vfs.write_file_path(CONFIG_PATH, out.as_bytes());
    }

    fn app_slot(&self, app_id: &str) -> Option<usize> {
        self.apps.iter().position(|slot| slot.app.info().id == app_id)
    }

    fn app_label(&self, app_id: &str) -> String {
        match self.app_slot(app_id) {
            Some(index) => String::from(self.apps[index].app.info().display_name(self.settings.language)),
            None => String::from(app_id),
        }
    }

    fn show_toast(&mut self, item: Notification) {
        let now = self.uptime_ms;
        self.notify.history.insert(
            0,
            HistoryItem { app_id: item.app_id.clone(), title: item.title.clone(), body: item.body.clone(), level: item.level, at_ms: now },
        );
        self.notify.history.truncate(MAX_HISTORY);
        self.notify.unread = (self.notify.unread + 1).min(99);
        if self.notify.panel_open {
            nc_audio::play(nc_audio::Sound::Notify);
            return;
        }
        if self.notify.toasts.len() >= MAX_TOASTS {
            self.notify.toasts.remove(0);
        }
        let id = self.notify.next_id;
        self.notify.next_id = self.notify.next_id.wrapping_add(1);
        self.notify.toasts.push(Toast {
            id,
            app_id: item.app_id,
            title: item.title,
            body: item.body,
            level: item.level,
            age_ms: 0,
            anim: 0,
            leaving: false,
        });
        nc_audio::play(nc_audio::Sound::Notify);
    }

    pub(crate) fn notify_decide(&mut self, allow: bool) {
        let Some(app_id) = self.notify.prompt.take() else { return; };
        shared::set_permission(&app_id, if allow { Permission::Allowed } else { Permission::Denied });
        self.save_notify_permissions();
        if let Some(position) = self.notify.held.iter().position(|held| held.app_id == app_id) {
            let held = self.notify.held.remove(position);
            if allow {
                for item in held.items {
                    self.show_toast(item);
                }
            }
        }
        self.notify.prompt_anim = 0;
        self.dirty = true;
    }

    pub(crate) fn notify_toggle_permission(&mut self, index: usize) {
        let list = shared::permissions();
        if let Some((id, permission)) = list.get(index) {
            let next = if *permission == Permission::Allowed { Permission::Denied } else { Permission::Allowed };
            shared::set_permission(id, next);
            self.save_notify_permissions();
            self.dirty = true;
        }
    }

    pub(crate) fn notify_toggle_panel(&mut self) {
        self.notify.panel_open = !self.notify.panel_open;
        if self.notify.panel_open {
            self.notify.unread = 0;
            self.notify.toasts.clear();
        }
        self.dirty = true;
    }

    pub(crate) fn notify_close_panel(&mut self) {
        if self.notify.panel_open {
            self.notify.panel_open = false;
            self.dirty = true;
        }
    }

    pub(crate) fn notify_clear(&mut self) {
        self.notify.history.clear();
        self.notify.unread = 0;
        self.dirty = true;
    }

    pub(crate) fn notify_dismiss_toast(&mut self, index: usize) {
        if let Some(toast) = self.notify.toasts.get_mut(index) {
            toast.leaving = true;
            self.dirty = true;
        }
    }

    pub(crate) fn notify_tick(&mut self, dt: u32) -> bool {
        let mut dirty = false;
        for item in shared::drain() {
            match shared::permission(&item.app_id) {
                Permission::Allowed => self.show_toast(item),
                Permission::Denied => {}
                Permission::Ask => {
                    let app_id = item.app_id.clone();
                    match self.notify.held.iter_mut().find(|held| held.app_id == app_id) {
                        Some(held) => {
                            if held.items.len() < 4 {
                                held.items.push(item);
                            }
                        }
                        None => self.notify.held.push(Held { app_id, items: alloc::vec![item] }),
                    }
                }
            }
            dirty = true;
        }
        if self.notify.prompt.is_none() {
            let next = self
                .notify
                .held
                .iter()
                .find(|held| shared::permission(&held.app_id) == Permission::Ask)
                .map(|held| held.app_id.clone());
            if let Some(app_id) = next {
                self.notify.prompt = Some(app_id);
                self.notify.prompt_anim = 0;
                nc_audio::play(nc_audio::Sound::Notify);
                dirty = true;
            }
        }
        if self.notify.prompt.is_some() && self.notify.prompt_anim < ONE {
            self.notify.prompt_anim = approach(self.notify.prompt_anim, ONE, dt, 220);
            dirty = true;
        }
        for toast in self.notify.toasts.iter_mut() {
            toast.age_ms = toast.age_ms.saturating_add(dt);
            if toast.age_ms >= TOAST_MS {
                toast.leaving = true;
            }
            let target = if toast.leaving { 0 } else { ONE };
            if toast.anim != target {
                toast.anim = approach(toast.anim, target, dt, if toast.leaving { 200 } else { 240 });
                dirty = true;
            }
        }
        let before = self.notify.toasts.len();
        self.notify.toasts.retain(|toast| !(toast.leaving && toast.anim <= 0));
        dirty |= before != self.notify.toasts.len();
        let panel_target = if self.notify.panel_open { ONE } else { 0 };
        if self.notify.panel_anim != panel_target {
            self.notify.panel_anim = approach(self.notify.panel_anim, panel_target, dt, 200);
            dirty = true;
        }
        dirty
    }

    pub(crate) fn notify_hit(&self, x: i32, y: i32, width: i32, unit: i32) -> Option<Target> {
        if self.notify.panel_open && self.notify.panel_anim > ONE / 4 {
            let (panel, clear, rows, perms) = panel_layout(self, width, unit);
            if inside(clear, x, y) && !self.notify.history.is_empty() {
                return Some(Target::NotifyClear);
            }
            for (index, rect) in perms.iter().enumerate() {
                if inside(*rect, x, y) {
                    return Some(Target::NotifyPerm(index));
                }
            }
            let _ = rows;
            if inside(panel, x, y) {
                return Some(Target::NotifyPanel);
            }
        }
        if self.notify.prompt.is_some() && !self.notify.panel_open {
            let card = prompt_rect(width, unit, self.notify.toasts.len());
            let (allow, deny) = prompt_buttons(card, unit);
            if inside(allow, x, y) {
                return Some(Target::NotifyAllow);
            }
            if inside(deny, x, y) {
                return Some(Target::NotifyDeny);
            }
            if inside(card, x, y) {
                return Some(Target::NotifyPanel);
            }
        }
        for index in 0..self.notify.toasts.len() {
            if inside(toast_rect(width, unit, index), x, y) {
                return Some(Target::NotifyToast(index));
            }
        }
        None
    }
}

fn draw_app_icon(canvas: &mut Canvas<'_>, state: &DesktopState, app_id: &str, tile: Rect, alpha: u32) {
    let kind = match state.app_slot(app_id) {
        Some(index) => kind_for(state.apps[index].app.info().icon),
        None => ArtKind::App,
    };
    draw_art(canvas, kind, art_rect_for_tile(tile), alpha);
}

fn faded_text(canvas: &mut Canvas<'_>, text: &str, x: i32, y: i32, style: &nc_font::Style, sample_x: i32, sample_y: i32, color: u32, alpha: i32) {
    let color = crate::paint::fade_text_color(canvas, sample_x, sample_y, color, alpha);
    draw_text_color(canvas, text, x, y, style, color);
}

fn short_text(text: &str, max_w: i32, size: i32, weight: i32) -> (String, nc_font::Style) {
    let style = text_style(size, weight);
    let fitted = fit_text(text, max_w, &|s| text_width(s, &style));
    (fitted, style)
}

fn draw_card_text(canvas: &mut Canvas<'_>, state: &DesktopState, x: i32, y: i32, max_w: i32, app: &str, title: &str, body: &str, unit: i32, alpha: i32) {
    let (app_text, app_style) = short_text(app, max_w, (unit * 38 / 100).max(9), 15);
    let (title_text, title_style) = short_text(title, max_w, (unit * 52 / 100).max(10), 22);
    let (body_text, body_style) = short_text(body, max_w, (unit * 44 / 100).max(9), 15);
    let muted = ink_soft();
    let fade = |c: u32| scale_u32(255, alpha) * (c != 0) as u32 + 0;
    let _ = fade;
    let _ = state;
    faded_text(canvas, &app_text, x, y, &app_style, x, y, muted, alpha);
    faded_text(canvas, &title_text, x, y + unit * 9 / 10, &title_style, x, y, glass_text(), alpha);
    if !body_text.is_empty() {
        faded_text(canvas, &body_text, x, y + unit * 9 / 5 + unit / 8, &body_style, x, y, muted, alpha);
    }
}

pub fn draw_notifications(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, _height: i32, unit: i32) {
    let (blur, passes) = state.blur_params(unit);
    let accent = state.settings.accent_color();
    let lang = state.settings.language;

    if state.notify.panel_anim > 0 {
        draw_panel(canvas, state, width, unit, blur, passes, accent, lang);
    }

    for (index, toast) in state.notify.toasts.iter().enumerate() {
        if toast.anim <= 0 {
            continue;
        }
        let e = ease_out_cubic(toast.anim);
        let mut r = toast_rect(width, unit, index);
        r.x += (unit * 5) * (ONE - e) / ONE;
        glass_surface(canvas, r, unit * 7 / 10, &dock_glass(blur, passes), e);
        let tile = Rect::new(r.x + unit * 7 / 10, r.y + (r.height - unit * 5 / 2) / 2, unit * 5 / 2, unit * 5 / 2);
        let alpha = (e as i64 * 256 / ONE as i64) as u32;
        draw_app_icon(canvas, state, &toast.app_id, tile, alpha);
        if toast.level != Level::Info {
            let dot = unit * 7 / 10;
            fill_rr(canvas, Rect::new(tile.x + tile.width - dot / 2, tile.y + tile.height - dot / 2, dot, dot), dot / 2, level_color(toast.level, accent), scale_u32(255, e));
        }
        let text_x = tile.x + tile.width + unit * 7 / 10;
        let max_w = r.x + r.width - text_x - unit * 7 / 10;
        let label = state.app_label(&toast.app_id);
        draw_card_text(canvas, state, text_x, r.y + unit * 3 / 5, max_w, &label, &toast.title, &toast.body, unit, e);
        let _ = toast.id;
    }

    let prompt_app = if state.notify.panel_open { None } else { state.notify.prompt.as_ref() };
    if let Some(app_id) = prompt_app {
        let e = ease_out_cubic(state.notify.prompt_anim);
        let mut r = prompt_rect(width, unit, state.notify.toasts.len());
        r.x += (unit * 5) * (ONE - e) / ONE;
        glass_surface(canvas, r, unit * 7 / 10, &dock_glass(blur, passes), e);
        let alpha = (e as i64 * 256 / ONE as i64) as u32;
        let tile = Rect::new(r.x + unit * 7 / 10, r.y + unit * 7 / 10, unit * 5 / 2, unit * 5 / 2);
        draw_app_icon(canvas, state, app_id, tile, alpha);
        let text_x = tile.x + tile.width + unit * 7 / 10;
        let max_w = r.x + r.width - text_x - unit * 7 / 10;
        let label = state.app_label(app_id);
        let title_style = text_style((unit * 56 / 100).max(10), 22);
        let body_style = text_style((unit * 44 / 100).max(9), 15);
        let name = fit_text(&label, max_w, &|s| text_width(s, &title_style));
        let ask = fit_text(tr(lang, TextId::NotifAskText), max_w, &|s| text_width(s, &body_style));
        faded_text(canvas, &name, text_x, r.y + unit * 9 / 10, &title_style, text_x, r.y, glass_text(), e);
        faded_text(canvas, &ask, text_x, r.y + unit * 9 / 5 + unit / 4, &body_style, text_x, r.y, ink_soft(), e);
        let (allow, deny) = prompt_buttons(r, unit);
        let hover_allow = state.hover == Target::NotifyAllow;
        let hover_deny = state.hover == Target::NotifyDeny;
        let allow_fill = if hover_allow { crate::theme::lighten(accent, 30) } else { accent };
        fill_rr(canvas, allow, allow.height / 2, allow_fill, scale_u32(255, e));
        fill_rr(canvas, deny, deny.height / 2, WHITE, scale_u32(if hover_deny { 120 } else { 70 }, e));
        let btn = text_style((unit * 44 / 100).max(9), 22);
        let t1 = tr(lang, TextId::NotifAllow);
        let t2 = tr(lang, TextId::NotifDeny);
        let w1 = text_width(t1, &btn);
        let w2 = text_width(t2, &btn);
        faded_text(canvas, t1, allow.x + (allow.width - w1) / 2, allow.y + (allow.height - btn.cap_height) / 2, &btn, allow.x, allow.y, WHITE, e);
        faded_text(canvas, t2, deny.x + (deny.width - w2) / 2, deny.y + (deny.height - btn.cap_height) / 2, &btn, deny.x, deny.y, glass_text(), e);
    }
}

fn ago(state: &DesktopState, at_ms: u32, lang: nc_app::Language) -> String {
    let seconds = state.uptime_ms.wrapping_sub(at_ms) / 1000;
    if seconds < 60 {
        return String::from(tr(lang, TextId::NotifJustNow));
    }
    let mut line = Line::new();
    line.number(seconds / 60, 1).text(" ").text(tr(lang, TextId::NotifMinAgo));
    String::from(line.as_str())
}

fn draw_panel(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, unit: i32, blur: usize, passes: usize, accent: u32, lang: nc_app::Language) {
    let e = ease_out_cubic(state.notify.panel_anim);
    let (mut panel, clear, rows, perms) = panel_layout(state, width, unit);
    let slide = unit * 3 * (ONE - e) / ONE;
    let dy = -slide;
    panel.y += dy;
    glass_surface(canvas, panel, unit * 9 / 10, &dock_glass(blur, passes), e);
    let alpha = (e as i64 * 256 / ONE as i64) as u32;
    let head = text_style((unit * 62 / 100).max(11), 22);
    let small = text_style((unit * 40 / 100).max(9), 15);
    let body = text_style((unit * 44 / 100).max(9), 15);
    faded_text(canvas, tr(lang, TextId::NotifTitle), panel.x + unit, panel.y + unit * 3 / 4, &head, panel.x, panel.y, glass_text(), e);
    if !state.notify.history.is_empty() {
        let clear_r = Rect::new(clear.x, clear.y + dy, clear.width, clear.height);
        let hover = state.hover == Target::NotifyClear;
        fill_rr(canvas, clear_r, clear_r.height / 2, WHITE, scale_u32(if hover { 120 } else { 64 }, e));
        let label = tr(lang, TextId::NotifClear);
        let lw = text_width(label, &small);
        faded_text(canvas, label, clear_r.x + (clear_r.width - lw) / 2, clear_r.y + (clear_r.height - small.cap_height) / 2, &small, clear_r.x, clear_r.y, glass_text(), e);
    }
    if state.notify.history.is_empty() {
        let text = tr(lang, TextId::NotifEmpty);
        let tw = text_width(text, &body);
        let top = panel.y + unit * 3;
        faded_text(canvas, text, panel.x + (panel.width - tw) / 2, top + unit * 3 / 2, &body, panel.x, top, ink_soft(), e);
    }
    for (index, row) in rows.iter().enumerate() {
        let item = &state.notify.history[index];
        let r = Rect::new(row.x, row.y + dy, row.width, row.height);
        if index > 0 {
            fill_rr(canvas, Rect::new(r.x + unit, r.y, r.width - unit * 2, 1), 0, WHITE, scale_u32(60, e));
        }
        let tile = Rect::new(r.x + unit * 7 / 10, r.y + (r.height - unit * 21 / 10) / 2, unit * 21 / 10, unit * 21 / 10);
        draw_app_icon(canvas, state, &item.app_id, tile, alpha);
        if item.level != Level::Info {
            let dot = unit / 2;
            fill_rr(canvas, Rect::new(tile.x + tile.width - dot / 2, tile.y + tile.height - dot / 2, dot, dot), dot / 2, level_color(item.level, accent), scale_u32(255, e));
        }
        let text_x = tile.x + tile.width + unit * 3 / 5;
        let max_w = r.x + r.width - text_x - unit * 7 / 10;
        let mut label = state.app_label(&item.app_id);
        label.push_str(" · ");
        label.push_str(&ago(state, item.at_ms, lang));
        draw_card_text(canvas, state, text_x, r.y + unit * 3 / 10, max_w, &label, &item.title, &item.body, unit, e);
    }
    let list = shared::permissions();
    if !perms.is_empty() {
        let (_, _, _, _) = (0, 0, 0, 0);
        let first = perms[0];
        faded_text(canvas, tr(lang, TextId::NotifPermsTitle), panel.x + unit, first.y + dy - unit * 2, &small, panel.x, first.y, ink_soft(), e);
    }
    for (index, row) in perms.iter().enumerate() {
        let Some((id, permission)) = list.get(index) else { break; };
        let r = Rect::new(row.x, row.y + dy, row.width, row.height);
        let hover = state.hover == Target::NotifyPerm(index);
        if hover {
            fill_rr(canvas, r, unit / 2, WHITE, scale_u32(50, e));
        }
        let tile = Rect::new(r.x + unit / 4, r.y + (r.height - unit * 3 / 2) / 2, unit * 3 / 2, unit * 3 / 2);
        draw_app_icon(canvas, state, id, tile, alpha);
        let name = state.app_label(id);
        let max_w = r.width - unit * 9;
        let fitted = fit_text(&name, max_w, &|s| text_width(s, &body));
        faded_text(canvas, &fitted, tile.x + tile.width + unit / 2, r.y + (r.height - body.cap_height) / 2, &body, r.x, r.y, glass_text(), e);
        let on = *permission == Permission::Allowed;
        let sw = Rect::new(r.x + r.width - unit * 3 - unit / 4, r.y + (r.height - unit * 3 / 2) / 2, unit * 3, unit * 3 / 2);
        let fill = if on { accent } else { rgb(150, 156, 170) };
        fill_rr(canvas, sw, sw.height / 2, fill, scale_u32(255, e));
        let knob = sw.height - unit / 4;
        let kx = if on { sw.x + sw.width - knob - unit / 8 } else { sw.x + unit / 8 };
        fill_rr(canvas, Rect::new(kx, sw.y + unit / 8, knob, knob), knob / 2, WHITE, scale_u32(255, e));
        let _ = (CLOSE_RED, TextId::NotifPermOn, TextId::NotifPermOff);
    }
}

pub(crate) fn draw_unread_dot(canvas: &mut Canvas<'_>, state: &DesktopState, x: i32, y: i32, unit: i32) {
    if state.notify.unread == 0 {
        return;
    }
    let size = (unit * 2 / 3).max(5);
    fill_rr(canvas, Rect::new(x, y, size, size), size / 2, state.settings.accent_color(), 255);
}

impl DesktopState {
    pub fn preview_toggle_notification_panel(&mut self) {
        self.notify_toggle_panel();
    }

    pub fn preview_prompt_button(&self, allow: bool, width: i32, height: i32) -> Option<(i32, i32)> {
        self.notify.prompt.as_ref()?;
        let unit = crate::layout::unit_for(width, height);
        let card = prompt_rect(width, unit, self.notify.toasts.len());
        let (a, d) = prompt_buttons(card, unit);
        let r = if allow { a } else { d };
        Some((r.x + r.width / 2, r.y + r.height / 2))
    }

    pub fn preview_notification_state(&self) -> (usize, usize, bool) {
        (self.notify.toasts.len(), self.notify.history.len(), self.notify.prompt.is_some())
    }
}
