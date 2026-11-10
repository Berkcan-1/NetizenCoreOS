#![no_std]

mod data;
mod data_outline;
mod outline;
mod raster;

use data::Entry;

pub use outline::set_script_hint;

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
    Shape(outline::Shape),
}

enum Drawn {
    Stroke(&'static Entry),
    Shape(outline::Shape),
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
    if let Some(shape) = outline::find(code) {
        return Item::Shape(shape);
    }
    if is_zero_width(code) || data::is_combining(code) {
        return Item::Skip;
    }
    if is_space(code) {
        return Item::Blank(data::SPACE_ADVANCE);
    }
    Item::Glyph(&data::NOTDEF)
}

fn layout(text: &str, cap: i32, tracking: i32, visit: &mut dyn FnMut(Drawn, i32)) -> i32 {
    let mut pen = 0;
    let mut count = 0;
    let mut index = 0usize;
    while index < text.len() {
        let first = match text[index..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let mut code = first as u32;
        let mut next_index = index + first.len_utf8();

        if outline::is_deva(code) {
            let mut prev = code;
            loop {
                match text[next_index..].chars().next() {
                    Some(c) if outline::deva_extends(prev, c as u32) => {
                        prev = c as u32;
                        next_index += c.len_utf8();
                    }
                    _ => break,
                }
            }
            let advance = match outline::find_cluster(&text[index..next_index]) {
                Some(shape) => {
                    visit(Drawn::Shape(shape), pen);
                    outline::advance_pixels(shape.advance, cap)
                }
                None => {
                    visit(Drawn::Stroke(&data::NOTDEF), pen);
                    advance_pixels(data::NOTDEF.advance as i32, cap)
                }
            };
            pen += advance + tracking;
            count += 1;
            index = next_index;
            continue;
        }

        loop {
            match text[next_index..].chars().next() {
                Some(c) if data::is_combining(c as u32) => {
                    next_index += c.len_utf8();
                    if let Some(composed) = data::compose(code, c as u32) {
                        code = composed;
                    }
                }
                _ => break,
            }
        }
        index = next_index;

        let advance = match classify(code) {
            Item::Skip => continue,
            Item::Blank(units) => advance_pixels(units, cap),
            Item::Glyph(entry) => {
                visit(Drawn::Stroke(entry), pen);
                advance_pixels(entry.advance as i32, cap)
            }
            Item::Shape(shape) => {
                visit(Drawn::Shape(shape), pen);
                outline::advance_pixels(shape.advance, cap)
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

/// `true` when `ch` is drawn as a real glyph, composed from a combining sequence, or deliberately
/// invisible (zero-width / spacing). `false` means `draw_text` would paint the "undefined" box.
/// Callers use this to decide whether a character needs a fallback before measuring or painting.
pub fn has_glyph(ch: char) -> bool {
    let code = ch as u32;
    data::find(code).is_some()
        || outline::find(code).is_some()
        || outline::is_deva(code)
        || data::is_combining(code)
        || is_zero_width(code)
        || is_space(code)
}

pub fn text_width(text: &str, style: &Style) -> i32 {
    let cap = clamp_cap(style.cap_height);
    layout(text, cap, style.tracking, &mut |_, _| {})
}

pub fn draw_text(text: &str, x: i32, y: i32, style: &Style, plot: &mut dyn FnMut(i32, i32, u8)) {
    let cap = clamp_cap(style.cap_height);
    let stroke = style.stroke_sixteenths;
    layout(text, cap, style.tracking, &mut |drawn, offset| match drawn {
        Drawn::Stroke(entry) => raster::draw_glyph(entry, x + offset, y, cap, stroke, &mut *plot),
        Drawn::Shape(shape) => outline::draw(&shape, x + offset, y, cap, &mut *plot),
    });
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;
