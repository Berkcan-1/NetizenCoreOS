#![no_std]

mod data;
mod raster;

use data::Entry;

#[derive(Clone, Copy)]
pub struct Style {
    pub cap_height: i32,
    pub stroke_sixteenths: i32,
    pub tracking: i32,
}

impl Style {
    pub const fn new(cap_height: i32) -> Style {
        let stroke = cap_height * 2 + 2;
        Style {
            cap_height,
            stroke_sixteenths: if stroke < 18 { 18 } else { stroke },
            tracking: 0,
        }
    }
}

enum Item {
    Skip,
    Blank(i32),
    Glyph(&'static Entry),
}

fn clamp_cap(cap_height: i32) -> i32 {
    cap_height.max(2).min(1024)
}

fn advance_pixels(units: i32, cap: i32) -> i32 {
    ((units * cap + 500) / 1000).max(1)
}

fn is_zero_width(code: u32) -> bool {
    (code >= 0x200B && code <= 0x200F)
        || code == 0x2060
        || code == 0xFEFF
        || (code >= 0xFE00 && code <= 0xFE0F)
}

fn is_space(code: u32) -> bool {
    (code >= 0x2000 && code <= 0x200A) || code == 0x202F || code == 0x205F || code == 0x3000
}

fn classify(code: u32) -> Item {
    if code < 0x20 {
        if code == 9 {
            return Item::Blank(data::SPACE_ADVANCE * 4);
        }
        return Item::Skip;
    }
    if code == 0x7F || (code >= 0x80 && code < 0xA0) {
        return Item::Skip;
    }
    if let Some(entry) = data::find(code) {
        return Item::Glyph(entry);
    }
    if is_zero_width(code) || data::is_combining(code) {
        return Item::Skip;
    }
    if is_space(code) {
        return Item::Blank(data::SPACE_ADVANCE);
    }
    Item::Glyph(&data::NOTDEF)
}

fn layout(text: &str, cap: i32, tracking: i32, visit: &mut dyn FnMut(&'static Entry, i32)) -> i32 {
    let mut pen = 0;
    let mut count = 0;
    let mut chars = text.chars().peekable();
    while let Some(first) = chars.next() {
        let mut code = first as u32;
        while let Some(&next) = chars.peek() {
            let next_code = next as u32;
            if !data::is_combining(next_code) {
                break;
            }
            chars.next();
            if let Some(composed) = data::compose(code, next_code) {
                code = composed;
            }
        }
        let advance = match classify(code) {
            Item::Skip => continue,
            Item::Blank(units) => advance_pixels(units, cap),
            Item::Glyph(entry) => {
                visit(entry, pen);
                advance_pixels(entry.advance as i32, cap)
            }
        };
        pen += advance + tracking;
        count += 1;
    }
    if count == 0 {
        0
    } else {
        (pen - tracking).max(0)
    }
}

pub fn text_width(text: &str, style: &Style) -> i32 {
    let cap = clamp_cap(style.cap_height);
    layout(text, cap, style.tracking, &mut |_, _| {})
}

pub fn draw_text(text: &str, x: i32, y: i32, style: &Style, plot: &mut dyn FnMut(i32, i32, u8)) {
    let cap = clamp_cap(style.cap_height);
    let stroke = style.stroke_sixteenths;
    layout(text, cap, style.tracking, &mut |entry, offset| {
        raster::draw_glyph(entry, x + offset, y, cap, stroke, &mut *plot);
    });
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;
