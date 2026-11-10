use alloc::vec::Vec;
use nc_app::security::{LockState, KIND_PIN};
use nc_app::{tr, trf, TextId};
use nc_font::text_width;
use nc_gfx::{Canvas, Rect};
use nc_hal::{KeyCode, MouseState};
use nc_ui::{draw_text_color, text_style, WHITE};

use crate::light::{reveal_edge, sheen, Light};
use crate::motion::ONE;
use crate::paint::{draw_line, fill_rr, glass_surface, stroke_rr, Glass};
use crate::theme::{
    glass_shadow, glass_text, ink_muted, is_dark, light_reach, radius_large, radius_medium, snow,
    DANGER,
};
use crate::DesktopAction;

const FAILS_BEFORE_WAIT: u32 = 5;
const WAIT_STEP_MS: u32 = 30_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    None,
    Digit(u8),
    Backspace,
    Submit,
    Restart,
    Shutdown,
}

pub struct LockUi {
    pub locked: bool,
    pub state: LockState,
    pub input: Vec<u8>,
    pub fails: u32,
    pub wait_until_ms: u32,
    pub shake: i32,
    pub wrong: bool,
    pub attempt: Option<bool>,
    pub hover: Hit,
    pub light_x: i32,
    pub light_y: i32,
    last_ms: u32,
    last_wait_secs: u32,
}

impl LockUi {
    pub fn new() -> LockUi {
        LockUi {
            locked: false,
            state: LockState::Disabled,
            input: Vec::new(),
            fails: 0,
            wait_until_ms: 0,
            shake: 0,
            wrong: false,
            attempt: None,
            hover: Hit::None,
            light_x: 0,
            light_y: 0,
            last_ms: 0,
            last_wait_secs: 0,
        }
    }

    pub fn engage(&mut self, state: LockState, uptime_ms: u32) {
        self.state = state;
        if let LockState::Enabled { fails, .. } = state {
            self.locked = true;
            self.fails = fails;
            self.input.clear();
            self.wrong = false;
            if fails >= FAILS_BEFORE_WAIT {
                self.wait_until_ms = uptime_ms.wrapping_add(WAIT_STEP_MS);
            }
        }
    }

    pub fn wipe_input(&mut self) {
        nc_app::security::wipe(&mut self.input);
        self.input.clear();
    }
}

fn th(unit: i32, pct: i32) -> i32 {
    (unit * pct / 100).max(8)
}

struct Layout {
    card: Rect,
    avatar: Rect,
    title_y: i32,
    dots_y: i32,
    message_y: i32,
    keys: [Rect; 12],
    field: Rect,
    button: Rect,
    restart: Rect,
    shutdown: Rect,
    pin: bool,
}

fn layout(width: i32, height: i32, unit: i32, pin: bool) -> Layout {
    let lu = unit.min(height / 38).max(8);
    let card_w = lu * 24;
    let pad = lu * 2;
    let key_w = lu * 65 / 10;
    let key_h = lu * 34 / 10;
    let gap = lu * 3 / 4;
    let avatar_d = lu * 5;
    let keypad_h = key_h * 4 + gap * 3;
    let card_h = if pin {
        pad + avatar_d + lu + lu * 2 + lu + lu * 2 + lu + keypad_h + pad
    } else {
        pad + avatar_d + lu + lu * 2 + lu + lu * 7 / 2 + lu + lu * 2 + lu + lu * 7 / 2 + pad
    };
    let card = Rect::new((width - card_w) / 2, ((height - card_h) / 2).max(lu), card_w, card_h);
    let avatar = Rect::new(card.x + (card_w - avatar_d) / 2, card.y + pad, avatar_d, avatar_d);
    let title_y = avatar.y + avatar_d + lu;
    let dots_y = title_y + lu * 2 + lu * 4 / 5;
    let message_y = if pin { dots_y + lu * 2 + lu / 2 } else { dots_y + lu * 5 };
    let keys_top = message_y + lu * 2 + lu / 2;
    let grid_w = key_w * 3 + gap * 2;
    let grid_x = card.x + (card_w - grid_w) / 2;
    let mut keys = [Rect::new(0, 0, 0, 0); 12];
    let mut i = 0;
    while i < 12 {
        let col = (i % 3) as i32;
        let row = (i / 3) as i32;
        keys[i] = Rect::new(grid_x + col * (key_w + gap), keys_top + row * (key_h + gap), key_w, key_h);
        i += 1;
    }
    let field_w = card_w - pad * 2;
    let field = Rect::new(card.x + pad, title_y + lu * 3, field_w, lu * 7 / 2);
    let button = Rect::new(card.x + pad, message_y + lu * 2, field_w, lu * 7 / 2);
    let bs_w = lu * 9;
    let bs_h = lu * 5 / 2;
    let shutdown = Rect::new(width - lu * 2 - bs_w, height - lu * 2 - bs_h, bs_w, bs_h);
    let restart = Rect::new(shutdown.x - lu - bs_w, shutdown.y, bs_w, bs_h);
    Layout { card, avatar, title_y, dots_y, message_y, keys, field, button, restart, shutdown, pin }
}

fn pin_mode(ui: &LockUi) -> bool {
    match ui.state {
        LockState::Enabled { record, .. } => record.kind == KIND_PIN,
        _ => true,
    }
}

fn hit_test(ui: &LockUi, lay: &Layout, x: i32, y: i32) -> Hit {
    let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    if inside(lay.restart) {
        return Hit::Restart;
    }
    if inside(lay.shutdown) {
        return Hit::Shutdown;
    }
    if lay.pin {
        let order: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 11];
        let mut i = 0;
        while i < 12 {
            if inside(lay.keys[i]) {
                return match order[i] {
                    10 => Hit::Backspace,
                    11 => Hit::Submit,
                    d => Hit::Digit(d),
                };
            }
            i += 1;
        }
    } else if inside(lay.button) {
        return Hit::Submit;
    }
    let _ = ui;
    Hit::None
}

fn remaining_secs(ui: &LockUi, uptime_ms: u32) -> u32 {
    let left = ui.wait_until_ms.wrapping_sub(uptime_ms);
    if left > 0x8000_0000 || left == 0 {
        0
    } else {
        (left + 999) / 1000
    }
}

fn try_submit(ui: &mut LockUi, uptime_ms: u32) {
    let record = match ui.state {
        LockState::Enabled { record, .. } => record,
        _ => return,
    };
    if ui.input.is_empty() || remaining_secs(ui, uptime_ms) > 0 {
        return;
    }
    if record.verify(&ui.input) {
        ui.wipe_input();
        ui.locked = false;
        ui.wrong = false;
        ui.fails = 0;
        ui.attempt = Some(true);
    } else {
        ui.wipe_input();
        ui.wrong = true;
        ui.shake = ONE;
        ui.fails = ui.fails.saturating_add(1);
        ui.attempt = Some(false);
        if ui.fails % FAILS_BEFORE_WAIT == 0 {
            let steps = (ui.fails / FAILS_BEFORE_WAIT).min(10);
            ui.wait_until_ms = uptime_ms.wrapping_add(WAIT_STEP_MS * steps);
        }
    }
}

pub fn update(
    ui: &mut LockUi,
    mouse: &MouseState,
    key: Option<KeyCode>,
    width: i32,
    height: i32,
    unit: i32,
    uptime_ms: u32,
) -> (DesktopAction, bool) {
    let mut redraw = false;
    let dt = uptime_ms.wrapping_sub(ui.last_ms).min(100) as i32;
    ui.last_ms = uptime_ms;
    if ui.shake > 0 {
        ui.shake = (ui.shake - dt * ONE / 420).max(0);
        redraw = true;
    }
    let wait = remaining_secs(ui, uptime_ms);
    if wait != ui.last_wait_secs {
        ui.last_wait_secs = wait;
        redraw = true;
    }

    let pin = pin_mode(ui);
    let lay = layout(width, height, unit, pin);
    let hit = hit_test(ui, &lay, mouse.x, mouse.y);
    if hit != ui.hover {
        ui.hover = hit;
        redraw = true;
    }
    if (mouse.x - ui.light_x).abs() >= 3 || (mouse.y - ui.light_y).abs() >= 3 {
        ui.light_x = mouse.x;
        ui.light_y = mouse.y;
        if hit != Hit::None {
            redraw = true;
        }
    }

    let mut action = DesktopAction::None;
    let mut digit: Option<u8> = None;
    let mut backspace = false;
    let mut submit = false;

    if mouse.left_clicked {
        match hit {
            Hit::Digit(d) => digit = Some(d),
            Hit::Backspace => backspace = true,
            Hit::Submit => submit = true,
            Hit::Restart => action = DesktopAction::Reboot,
            Hit::Shutdown => action = DesktopAction::Shutdown,
            Hit::None => {}
        }
    }
    match key {
        Some(KeyCode::Char(c)) => {
            if pin {
                if c.is_ascii_digit() {
                    digit = Some(c as u8 - b'0');
                }
            } else if c.is_ascii() && !c.is_ascii_control() && ui.input.len() < 32 {
                ui.input.push(c as u8);
                ui.wrong = false;
                redraw = true;
            }
        }
        Some(KeyCode::Space) => {
            if !pin && ui.input.len() < 32 {
                ui.input.push(b' ');
                ui.wrong = false;
                redraw = true;
            }
        }
        Some(KeyCode::Backspace) => backspace = true,
        Some(KeyCode::Enter) => submit = true,
        Some(KeyCode::Escape) => {
            ui.wipe_input();
            redraw = true;
        }
        _ => {}
    }

    if let Some(d) = digit {
        if ui.input.len() < 4 && remaining_secs(ui, uptime_ms) == 0 {
            ui.input.push(b'0' + d);
            ui.wrong = false;
            redraw = true;
            if ui.input.len() == 4 {
                submit = true;
            }
        }
    }
    if backspace && !ui.input.is_empty() {
        ui.input.pop();
        redraw = true;
    }
    if submit {
        try_submit(ui, uptime_ms);
        redraw = true;
    }
    (action, redraw)
}

fn key_glyph(canvas: &mut Canvas<'_>, hit: Hit, rect: Rect, color: u32, unit: i32) {
    let cx = rect.x + rect.width / 2;
    let cy = rect.y + rect.height / 2;
    let a = (unit / 2).max(4);
    match hit {
        Hit::Backspace => {
            draw_line(canvas, cx - a, cy, cx - a / 3, cy - a * 2 / 3, 30, color, 230);
            draw_line(canvas, cx - a, cy, cx - a / 3, cy + a * 2 / 3, 30, color, 230);
            draw_line(canvas, cx - a / 3, cy - a * 2 / 3, cx + a, cy - a * 2 / 3, 30, color, 230);
            draw_line(canvas, cx - a / 3, cy + a * 2 / 3, cx + a, cy + a * 2 / 3, 30, color, 230);
            draw_line(canvas, cx + a, cy - a * 2 / 3, cx + a, cy + a * 2 / 3, 30, color, 230);
        }
        Hit::Submit => {
            draw_line(canvas, cx - a, cy, cx - a / 4, cy + a * 2 / 3, 34, color, 240);
            draw_line(canvas, cx - a / 4, cy + a * 2 / 3, cx + a, cy - a * 2 / 3, 34, color, 240);
        }
        _ => {}
    }
}

pub fn draw(
    canvas: &mut Canvas<'_>,
    wallpaper: &Canvas<'_>,
    ui: &LockUi,
    accent: u32,
    lang: nc_app::Language,
    blur: usize,
    passes: usize,
    unit: i32,
    uptime_ms: u32,
) {
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    canvas.reset_clip();
    canvas.copy_rect_from(wallpaper, 0, 0, 0, 0, width, height);
    let veil = if is_dark() { 150 } else { 70 };
    fill_rr(canvas, Rect::new(0, 0, width, height), 0, snow(), veil);

    let pin = pin_mode(ui);
    let mut lay = layout(width, height, unit, pin);
    let lu = unit.min(height / 38).max(8);
    let shake = (ui.shake * ui.shake / ONE) / 1;
    let swing = (shake * lu * 2 / ONE) * if (ui.shake / (ONE / 12)) % 2 == 0 { 1 } else { -1 };
    lay.card.x += swing;
    lay.avatar.x += swing;
    let mut moved = 0;
    while moved < 12 {
        lay.keys[moved].x += swing;
        moved += 1;
    }
    lay.field.x += swing;
    lay.button.x += swing;

    let glass = Glass {
        tint: snow(),
        alpha_top: if is_dark() { 200 } else { 176 },
        alpha_bot: if is_dark() { 184 } else { 156 },
        blur,
        passes,
        shadow: 120,
        shadow_dy: 10,
        shadow_blur: 36,
        gloss: 40,
        edge: 170,
        hole: None,
    };
    glass_surface(canvas, lay.card, radius_large(lu) * 2, &glass, ONE);
    let light = Light { x: ui.light_x, y: ui.light_y };
    sheen(canvas, lay.card, radius_large(lu) * 2, light, light_reach(lu) + lu * 4, WHITE, 40);

    let text = glass_text();
    let shadow = glass_shadow();
    let title_style = text_style(th(lu, 150), 16);
    let body_style = text_style(th(lu, 62), 15);
    let small_style = text_style(th(lu, 52), 15);

    fill_rr(canvas, lay.avatar, lay.avatar.width / 2, accent, 255);
    let ring = Rect::new(lay.avatar.x - 3, lay.avatar.y - 3, lay.avatar.width + 6, lay.avatar.height + 6);
    stroke_rr(canvas, ring, ring.width / 2, 1, WHITE, 140);
    let head = lay.avatar.width / 4;
    fill_rr(
        canvas,
        Rect::new(lay.avatar.x + lay.avatar.width / 2 - head / 2 - head / 4, lay.avatar.y + lay.avatar.height / 5, head + head / 2, head + head / 2),
        (head + head / 2) / 2,
        WHITE,
        235,
    );
    fill_rr(
        canvas,
        Rect::new(lay.avatar.x + lay.avatar.width / 6, lay.avatar.y + lay.avatar.height * 11 / 20, lay.avatar.width * 2 / 3, lay.avatar.height * 3 / 10),
        lay.avatar.width / 6,
        WHITE,
        235,
    );

    let name = "NetizenCore";
    let nw = text_width(name, &title_style);
    draw_text_color(canvas, name, lay.card.x + (lay.card.width - nw) / 2, lay.title_y, &title_style, text);

    let wait = remaining_secs(ui, uptime_ms);
    let prompt = if pin { tr(lang, TextId::LockPromptPin) } else { tr(lang, TextId::LockPromptPassword) };
    let prompt_w = text_width(prompt, &body_style);
    let prompt_y = lay.title_y + title_style.cap_height + lu / 2;
    draw_text_color(canvas, prompt, lay.card.x + (lay.card.width - prompt_w) / 2, prompt_y, &body_style, ink_muted());
    let _ = shadow;

    let count = ui.input.len();
    if pin {
        let d = lu * 6 / 5;
        let spacing = lu * 2;
        let total = d * 4 + spacing * 3;
        let start = lay.card.x + (lay.card.width - total) / 2;
        let mut i = 0;
        while i < 4 {
            let r = Rect::new(start + i as i32 * (d + spacing), lay.dots_y, d, d);
            if i < count {
                fill_rr(canvas, r, d / 2, if ui.wrong { DANGER } else { accent }, 255);
            } else {
                stroke_rr(canvas, r, d / 2, 2, text, 150);
            }
            i += 1;
        }
    } else {
        fill_rr(canvas, lay.field, lay.field.height / 2, WHITE, if is_dark() { 24 } else { 150 });
        stroke_rr(canvas, lay.field, lay.field.height / 2, 1, if ui.wrong { DANGER } else { accent }, 200);
        let d = lu * 3 / 5;
        let step = d + lu / 3;
        let shown = count.min(((lay.field.width - lu * 2) / step) as usize);
        let mut i = 0usize;
        while i < shown {
            let r = Rect::new(lay.field.x + lu + i as i32 * step, lay.field.y + (lay.field.height - d) / 2, d, d);
            fill_rr(canvas, r, d / 2, text, 235);
            i += 1;
        }
    }

    let message = if wait > 0 {
        Some(trf(lang, TextId::LockWaitFmt, &[alloc::format!("{}", wait).as_str()]))
    } else if ui.wrong {
        Some(alloc::string::String::from(tr(lang, TextId::LockWrong)))
    } else {
        None
    };
    if let Some(msg) = message {
        let w = text_width(&msg, &small_style);
        draw_text_color(canvas, &msg, lay.card.x + (lay.card.width - w) / 2, lay.message_y, &small_style, DANGER);
    }

    if pin {
        let order: [Hit; 12] = [
            Hit::Digit(1), Hit::Digit(2), Hit::Digit(3),
            Hit::Digit(4), Hit::Digit(5), Hit::Digit(6),
            Hit::Digit(7), Hit::Digit(8), Hit::Digit(9),
            Hit::Backspace, Hit::Digit(0), Hit::Submit,
        ];
        let digit_style = text_style(th(lu, 110), 16);
        let radius = radius_medium(lu) * 2;
        let mut i = 0;
        while i < 12 {
            let r = lay.keys[i];
            let hot = ui.hover == order[i];
            fill_rr(canvas, r, radius, WHITE, if is_dark() { if hot { 46 } else { 22 } } else { if hot { 190 } else { 120 } });
            if hot {
                sheen(canvas, r, radius, light, lu * 6, WHITE, 90);
                reveal_edge(canvas, r, radius, light, lu * 6, accent, 230);
            } else {
                stroke_rr(canvas, r, radius, 1, WHITE, 70);
            }
            match order[i] {
                Hit::Digit(d) => {
                    let mut buf = [0u8; 1];
                    buf[0] = b'0' + d;
                    let s = core::str::from_utf8(&buf).unwrap_or("0");
                    let w = text_width(s, &digit_style);
                    draw_text_color(canvas, s, r.x + (r.width - w) / 2, r.y + (r.height - digit_style.cap_height) / 2, &digit_style, text);
                }
                other => key_glyph(canvas, other, r, text, lu),
            }
            i += 1;
        }
    } else {
        let hot = ui.hover == Hit::Submit;
        let radius = lay.button.height / 2;
        fill_rr(canvas, lay.button, radius, accent, if hot { 255 } else { 230 });
        if hot {
            sheen(canvas, lay.button, radius, light, lu * 8, WHITE, 70);
        }
        let label = tr(lang, TextId::LockUnlock);
        let w = text_width(label, &body_style);
        draw_text_color(canvas, label, lay.button.x + (lay.button.width - w) / 2, lay.button.y + (lay.button.height - body_style.cap_height) / 2, &body_style, WHITE);
    }

    for (rect, hit, id) in [
        (lay.restart, Hit::Restart, TextId::PowerRestart),
        (lay.shutdown, Hit::Shutdown, TextId::PowerShutdown),
    ] {
        let hot = ui.hover == hit;
        let radius = rect.height / 2;
        fill_rr(canvas, rect, radius, snow(), if hot { 230 } else { 170 });
        stroke_rr(canvas, rect, radius, 1, WHITE, 120);
        if hot {
            reveal_edge(canvas, rect, radius, light, lu * 6, accent, 220);
        }
        let label = tr(lang, id);
        let w = text_width(label, &small_style);
        draw_text_color(canvas, label, rect.x + (rect.width - w) / 2, rect.y + (rect.height - small_style.cap_height) / 2, &small_style, text);
    }
}
