// crates/nc-app/src/text.rs
use nc_font::{text_width, Style};
use nc_gfx::{Canvas, Rect};
use nc_ui::draw_text_color;

pub struct Line {
    bytes: [u8; 128],
    length: usize,
}

impl Line {
    pub fn new() -> Line {
        Line { bytes: [0; 128], length: 0 }
    }

    pub fn text(&mut self, value: &str) -> &mut Line {
        for byte in value.bytes() {
            if self.length < self.bytes.len() {
                self.bytes[self.length] = byte;
                self.length += 1;
            }
        }
        self
    }

    pub fn number(&mut self, value: u32, digits: usize) -> &mut Line {
        let mut divisor = 1u32;
        let mut needed = 1usize;
        while value / divisor >= 10 {
            divisor *= 10;
            needed += 1;
        }
        let mut padding = digits.saturating_sub(needed);
        while padding > 0 && self.length < self.bytes.len() {
            self.bytes[self.length] = b'0';
            self.length += 1;
            padding -= 1;
        }
        while divisor > 0 && self.length < self.bytes.len() {
            self.bytes[self.length] = b'0' + (value / divisor % 10) as u8;
            self.length += 1;
            divisor /= 10;
        }
        self
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.length]).unwrap_or("")
    }
}

pub fn centered(canvas: &mut Canvas<'_>, text: &str, center_x: i32, y: i32, style: &Style, color: u32) {
    let width = text_width(text, style);
    draw_text_color(canvas, text, center_x - width / 2, y, style, color);
}

pub fn shadowed(canvas: &mut Canvas<'_>, text: &str, x: i32, y: i32, style: &Style, color: u32, shadow: u32) {
    draw_text_color(canvas, text, x + 1, y + 1, style, shadow);
    draw_text_color(canvas, text, x, y, style, color);
}

pub fn horizontal_line(canvas: &mut Canvas<'_>, x: i32, y: i32, width: i32, color: u32, alpha: u8) {
    for offset in 0..width {
        canvas.blend_pixel(x + offset, y, color, alpha);
    }
}

pub fn draw_text_clipped(
    canvas: &mut Canvas<'_>,
    text: &str,
    x: i32,
    y: i32,
    max_width: i32,
    style: &Style,
    color: u32,
) {
    if max_width <= 4 {
        return;
    }
    let prev_clip = canvas.clip_rect();
    let box_clip = Rect::new(x, y - 4, max_width, style.cap_height + 10);
    let cx0 = box_clip.x.max(prev_clip.x);
    let cy0 = box_clip.y.max(prev_clip.y);
    let cx1 = (box_clip.x + box_clip.width).min(prev_clip.x + prev_clip.width);
    let cy1 = (box_clip.y + box_clip.height).min(prev_clip.y + prev_clip.height);
    if cx1 > cx0 && cy1 > cy0 {
        canvas.set_clip(Rect::new(cx0, cy0, cx1 - cx0, cy1 - cy0));
        draw_text_color(canvas, text, x, y, style, color);
        canvas.set_clip(prev_clip);
    }
}
