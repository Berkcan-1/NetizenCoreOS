//! Native NetizenCore On-Screen Virtual Keyboard & IME (Feature 45).
//!
//! Designed directly with NetizenCore's `Palette`, `Dp`, and `Gfx` tokens so it
//! integrates seamlessly in both Light and Dark themes:
//! - Soft tray surface with 1px top hairline & rounded upper corners
//! - Floating rounded pill character keys with subtle elevation shadow
//! - Receded tonal modifier keys (`pal.track`) & accent-colored Return pill
//! - Authentic mobile Turkish Q layout (proper `ı ↔ I`, `i ↔ İ`, `ğ ü ş ö ç`),
//!   English QWERTY layout, and Numbers/Symbols layer.

use crate::{Dp, Palette, UiRect, SHADOW, WHITE};
use nc_app_sdk::Gfx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLang {
    TurkishQ,
    EnglishQwerty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftState {
    Lower,
    ShiftOnce,
    CapsLock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLayer {
    Alpha,
    Symbols,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    None,
    Char(char),
    Backspace,
    Enter,
    Dismiss,
    StateChanged,
}

pub struct VirtualKeyboard {
    pub visible: bool,
    pub lang: KeyboardLang,
    pub shift: ShiftState,
    pub layer: KeyboardLayer,
    pressed_row: i8,
    pressed_col: i8,
}

impl VirtualKeyboard {
    pub const fn new(lang: KeyboardLang) -> Self {
        Self {
            visible: false,
            lang,
            shift: ShiftState::Lower,
            layer: KeyboardLayer::Alpha,
            pressed_row: -1,
            pressed_col: -1,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.clear_press();
    }

    pub fn clear_press(&mut self) {
        self.pressed_row = -1;
        self.pressed_col = -1;
    }

    /// Logical height of the keyboard tray in dp (216 dp fits cleanly on 360x640 mobile viewports).
    pub const fn height_dp() -> i32 {
        216
    }

    pub fn tray_rect(&self, dp: &Dp, screen_h: i32) -> UiRect {
        let h = dp.d(Self::height_dp());
        UiRect::new(dp.origin_x, (screen_h - h).max(0), dp.width, h)
    }

    fn is_upper(&self) -> bool {
        self.shift != ShiftState::Lower
    }

    fn turkish_upper(c: char) -> char {
        match c {
            'i' => 'İ',
            'ı' => 'I',
            'ğ' => 'Ğ',
            'ü' => 'Ü',
            'ş' => 'Ş',
            'ö' => 'Ö',
            'ç' => 'Ç',
            'a'..='z' => ((c as u8) - 32) as char,
            other => other,
        }
    }

    fn english_upper(c: char) -> char {
        match c {
            'a'..='z' => ((c as u8) - 32) as char,
            other => other,
        }
    }

    fn map_char(&self, lower: char) -> char {
        if !self.is_upper() {
            return lower;
        }
        match self.lang {
            KeyboardLang::TurkishQ => Self::turkish_upper(lower),
            KeyboardLang::EnglishQwerty => Self::english_upper(lower),
        }
    }

    fn row_chars(&self, row: usize) -> &'static [char] {
        match (self.layer, self.lang, row) {
            (KeyboardLayer::Symbols, _, 0) => &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
            (KeyboardLayer::Symbols, _, 1) => &['@', '#', '$', '%', '&', '-', '+', '(', ')', '/'],
            (KeyboardLayer::Symbols, _, 2) => &['*', '"', '\'', ':', ';', '!', '?'],
            (KeyboardLayer::Alpha, KeyboardLang::TurkishQ, 0) => {
                &['q', 'w', 'e', 'r', 't', 'y', 'u', 'ı', 'o', 'p', 'ğ', 'ü']
            }
            (KeyboardLayer::Alpha, KeyboardLang::TurkishQ, 1) => {
                &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'ş', 'i']
            }
            (KeyboardLayer::Alpha, KeyboardLang::TurkishQ, 2) => {
                &['z', 'x', 'c', 'v', 'b', 'n', 'm', 'ö', 'ç']
            }
            (KeyboardLayer::Alpha, KeyboardLang::EnglishQwerty, 0) => {
                &['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p']
            }
            (KeyboardLayer::Alpha, KeyboardLang::EnglishQwerty, 1) => {
                &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l']
            }
            (KeyboardLayer::Alpha, KeyboardLang::EnglishQwerty, 2) => {
                &['z', 'x', 'c', 'v', 'b', 'n', 'm']
            }
            _ => &[],
        }
    }

    fn row_geometry(&self, tray: UiRect, dp: &Dp, row: usize) -> (i32, i32) {
        let top_pad = dp.d(12);
        let row_h = dp.d(42);
        let row_gap = dp.d(8);
        let y = tray.y + top_pad + (row as i32) * (row_h + row_gap);
        (y, row_h)
    }

    /// Compute the bounding rectangle for a character key in rows 0..=2.
    fn alpha_key_rect(&self, tray: UiRect, dp: &Dp, row: usize, col: usize) -> UiRect {
        let chars = self.row_chars(row);
        let count = chars.len().max(1) as i32;
        let (y, h) = self.row_geometry(tray, dp, row);
        let pad_x = dp.d(6);
        let gap = dp.d(4);

        if row == 2 {
            // Row 2 has Shift/ABC on left and Backspace on right
            let mod_w = dp.d(42);
            let inner_x = tray.x + pad_x + mod_w + gap;
            let inner_w = tray.w - (pad_x + mod_w + gap) * 2;
            let key_w = (inner_w - gap * (count - 1)) / count;
            let x = inner_x + (col as i32) * (key_w + gap);
            UiRect::new(x, y, key_w, h)
        } else {
            let indent = if row == 1 && self.lang == KeyboardLang::EnglishQwerty && self.layer == KeyboardLayer::Alpha {
                dp.d(16)
            } else {
                0
            };
            let inner_x = tray.x + pad_x + indent;
            let inner_w = tray.w - (pad_x + indent) * 2;
            let key_w = (inner_w - gap * (count - 1)) / count;
            let x = inner_x + (col as i32) * (key_w + gap);
            UiRect::new(x, y, key_w, h)
        }
    }

    fn shift_rect(&self, tray: UiRect, dp: &Dp) -> UiRect {
        let (y, h) = self.row_geometry(tray, dp, 2);
        UiRect::new(tray.x + dp.d(6), y, dp.d(42), h)
    }

    fn backspace_rect(&self, tray: UiRect, dp: &Dp) -> UiRect {
        let (y, h) = self.row_geometry(tray, dp, 2);
        let w = dp.d(42);
        UiRect::new(tray.right() - dp.d(6) - w, y, w, h)
    }

    fn bottom_row_rects(&self, tray: UiRect, dp: &Dp) -> [UiRect; 5] {
        let (y, h) = self.row_geometry(tray, dp, 3);
        let pad = dp.d(6);
        let gap = dp.d(5);
        let usable = tray.w - pad * 2 - gap * 4;
        let w_sym = usable * 15 / 100;
        let w_lang = usable * 13 / 100;
        let w_dot = usable * 11 / 100;
        let w_enter = usable * 20 / 100;
        let w_space = usable - w_sym - w_lang - w_dot - w_enter;

        let x0 = tray.x + pad;
        let x1 = x0 + w_sym + gap;
        let x2 = x1 + w_lang + gap;
        let x3 = x2 + w_space + gap;
        let x4 = x3 + w_dot + gap;
        [
            UiRect::new(x0, y, w_sym, h),
            UiRect::new(x1, y, w_lang, h),
            UiRect::new(x2, y, w_space, h),
            UiRect::new(x3, y, w_dot, h),
            UiRect::new(x4, y, w_enter, h),
        ]
    }

    /// Handle a touch tap at `(px, py)` and return the resulting `KeyEvent`.
    pub fn handle_tap(&mut self, dp: &Dp, screen_h: i32, px: i32, py: i32) -> KeyEvent {
        if !self.visible {
            return KeyEvent::None;
        }
        let tray = self.tray_rect(dp, screen_h);
        if !tray.contains(px, py) {
            return KeyEvent::None;
        }

        // Check Shift & Backspace on row 2
        if self.shift_rect(tray, dp).contains(px, py) {
            self.pressed_row = 2;
            self.pressed_col = -2;
            if self.layer == KeyboardLayer::Symbols {
                self.layer = KeyboardLayer::Alpha;
            } else {
                self.shift = match self.shift {
                    ShiftState::Lower => ShiftState::ShiftOnce,
                    ShiftState::ShiftOnce => ShiftState::CapsLock,
                    ShiftState::CapsLock => ShiftState::Lower,
                };
            }
            return KeyEvent::StateChanged;
        }
        if self.backspace_rect(tray, dp).contains(px, py) {
            self.pressed_row = 2;
            self.pressed_col = -3;
            return KeyEvent::Backspace;
        }

        // Check character rows 0..=2
        for row in 0..3 {
            let chars = self.row_chars(row);
            for (col, &ch) in chars.iter().enumerate() {
                if self.alpha_key_rect(tray, dp, row, col).contains(px, py) {
                    self.pressed_row = row as i8;
                    self.pressed_col = col as i8;
                    let out = if self.layer == KeyboardLayer::Alpha {
                        let mapped = self.map_char(ch);
                        if self.shift == ShiftState::ShiftOnce {
                            self.shift = ShiftState::Lower;
                        }
                        mapped
                    } else {
                        ch
                    };
                    return KeyEvent::Char(out);
                }
            }
        }

        // Check bottom bar (row 3)
        let [r_sym, r_lang, r_space, r_dot, r_enter] = self.bottom_row_rects(tray, dp);
        if r_sym.contains(px, py) {
            self.layer = match self.layer {
                KeyboardLayer::Alpha => KeyboardLayer::Symbols,
                KeyboardLayer::Symbols => KeyboardLayer::Alpha,
            };
            return KeyEvent::StateChanged;
        }
        if r_lang.contains(px, py) {
            self.lang = match self.lang {
                KeyboardLang::TurkishQ => KeyboardLang::EnglishQwerty,
                KeyboardLang::EnglishQwerty => KeyboardLang::TurkishQ,
            };
            return KeyEvent::StateChanged;
        }
        if r_space.contains(px, py) {
            return KeyEvent::Char(' ');
        }
        if r_dot.contains(px, py) {
            return KeyEvent::Char('.');
        }
        if r_enter.contains(px, py) {
            return KeyEvent::Enter;
        }

        KeyEvent::None
    }

    fn draw_key(
        gfx: &mut Gfx<'_>,
        r: UiRect,
        radius: i32,
        bg: u32,
        fg: u32,
        cap: i32,
        label: &str,
        pressed: bool,
        dark: bool,
    ) {
        // Subtle 1px natural elevation shadow under keys in light mode
        if !dark {
            gfx.panel(r.x, r.y + 1, r.w, r.h, radius, SHADOW, 18, 0, 0);
        }
        gfx.panel(r.x, r.y, r.w, r.h, radius, bg, 255, 0, 0);
        if pressed {
            gfx.panel(r.x, r.y, r.w, r.h, radius, SHADOW, 30, 0, 0);
        }
        gfx.text_centered(r.cx(), r.y + (r.h - cap) / 2, cap, fg, 2, label);
    }

    /// Render the native NetizenCore virtual keyboard onto `gfx`.
    pub fn draw(&self, gfx: &mut Gfx<'_>, dp: &Dp, screen_h: i32, pal: &Palette, accent: u32, dark: bool) {
        if !self.visible {
            return;
        }
        let tray = self.tray_rect(dp, screen_h);
        let key_r = dp.d(8).max(5);
        let cap_char = dp.d(13).max(10);
        let cap_mod = dp.d(10).max(8);

        // Soft integrated tray backdrop with rounded upper corners + hairline top divider
        gfx.panel(tray.x, tray.y, tray.w, tray.h + dp.d(16), dp.d(18), pal.bg, 250, pal.divider, 180);
        gfx.fill_rect(tray.x, tray.y, tray.w, 1, pal.divider);

        // Subtle drag/grab pill indicator at top center
        let grab_w = dp.d(32);
        let grab_h = dp.d(3).max(2);
        gfx.panel(tray.cx() - grab_w / 2, tray.y + dp.d(4), grab_w, grab_h, grab_h / 2, pal.subtle, 110, 0, 0);

        // Character rows 0..=2
        let mut utf8 = [0u8; 4];
        for row in 0..3 {
            let chars = self.row_chars(row);
            for (col, &ch) in chars.iter().enumerate() {
                let r = self.alpha_key_rect(tray, dp, row, col);
                let out = if self.layer == KeyboardLayer::Alpha { self.map_char(ch) } else { ch };
                let s = out.encode_utf8(&mut utf8);
                let pressed = self.pressed_row == row as i8 && self.pressed_col == col as i8;
                Self::draw_key(gfx, r, key_r, pal.card, pal.ink, cap_char, s, pressed, dark);
            }
        }

        // Shift / ABC key on row 2 left
        let r_shift = self.shift_rect(tray, dp);
        let shift_bg = match (self.layer, self.shift) {
            (KeyboardLayer::Alpha, ShiftState::CapsLock) => accent,
            (KeyboardLayer::Alpha, ShiftState::ShiftOnce) => pal.card,
            _ => pal.track,
        };
        let shift_fg = if self.layer == KeyboardLayer::Alpha && self.shift == ShiftState::CapsLock {
            WHITE
        } else {
            pal.ink
        };
        let shift_label = if self.layer == KeyboardLayer::Symbols { "ABC" } else { "^" };
        Self::draw_key(gfx, r_shift, key_r, shift_bg, shift_fg, cap_mod, shift_label, self.pressed_row == 2 && self.pressed_col == -2, dark);
        if self.layer == KeyboardLayer::Alpha && self.shift == ShiftState::ShiftOnce {
            gfx.dot(r_shift.cx(), r_shift.bottom() - dp.d(5), dp.d(2).max(2), accent, 1024);
        }

        // Backspace key on row 2 right
        let r_bs = self.backspace_rect(tray, dp);
        Self::draw_key(gfx, r_bs, key_r, pal.track, pal.ink, cap_mod, "DEL", self.pressed_row == 2 && self.pressed_col == -3, dark);

        // Bottom bar (row 3)
        let [r_sym, r_lang, r_space, r_dot, r_enter] = self.bottom_row_rects(tray, dp);
        let sym_label = if self.layer == KeyboardLayer::Alpha { "123" } else { "ABC" };
        let lang_label = match self.lang {
            KeyboardLang::TurkishQ => "TR",
            KeyboardLang::EnglishQwerty => "EN",
        };
        let space_label = match self.lang {
            KeyboardLang::TurkishQ => "Türkçe Q",
            KeyboardLang::EnglishQwerty => "English",
        };
        Self::draw_key(gfx, r_sym, key_r, pal.track, pal.ink, cap_mod, sym_label, false, dark);
        Self::draw_key(gfx, r_lang, key_r, pal.track, pal.ink, cap_mod, lang_label, false, dark);
        Self::draw_key(gfx, r_space, key_r, pal.card, pal.muted, dp.d(9).max(8), space_label, false, dark);
        Self::draw_key(gfx, r_dot, key_r, pal.track, pal.ink, cap_char, ".", false, dark);
        // Primary accent-colored action pill for Enter/Return
        Self::draw_key(gfx, r_enter, key_r, accent, WHITE, cap_mod, "OK", false, dark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_q_uppercase_rules_and_hit_testing() {
        let mut kb = VirtualKeyboard::new(KeyboardLang::TurkishQ);
        kb.show();
        let dp = Dp::new(540, 960);

        // Verify exact Turkish I/ı and İ/i mapping
        kb.shift = ShiftState::ShiftOnce;
        assert_eq!(kb.map_char('i'), 'İ');
        assert_eq!(kb.map_char('ı'), 'I');
        assert_eq!(kb.map_char('ğ'), 'Ğ');
        assert_eq!(kb.map_char('ü'), 'Ü');
        assert_eq!(kb.map_char('ş'), 'Ş');
        assert_eq!(kb.map_char('ö'), 'Ö');
        assert_eq!(kb.map_char('ç'), 'Ç');

        // Tap row 0 col 7 ('ı' in Turkish Q) while ShiftOnce -> returns 'I' and auto-drops to Lower
        let tray = kb.tray_rect(&dp, 960);
        let r_dotless_i = kb.alpha_key_rect(tray, &dp, 0, 7);
        assert_eq!(kb.handle_tap(&dp, 960, r_dotless_i.cx(), r_dotless_i.cy()), KeyEvent::Char('I'));
        assert_eq!(kb.shift, ShiftState::Lower);

        // Tap Enter pill
        let [_, _, _, _, r_enter] = kb.bottom_row_rects(tray, &dp);
        assert_eq!(kb.handle_tap(&dp, 960, r_enter.cx(), r_enter.cy()), KeyEvent::Enter);
    }
}
