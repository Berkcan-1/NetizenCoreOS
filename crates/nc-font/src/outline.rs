use core::cmp::Ordering as Cmp;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::data_outline::{OutGlyph, CLUSTERS, JP, KR, POINTS, SC};

const SUBSAMPLES: i64 = 4;
const MAX_WIDTH: usize = 256;
const MAX_CROSSINGS: usize = 64;
const EM_PER_CAP_NUM: i32 = 138;
const EM_PER_CAP_DEN: i32 = 100;

static SCRIPT_HINT: AtomicU8 = AtomicU8::new(0);

pub fn set_script_hint(value: u8) {
    SCRIPT_HINT.store(value, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
pub struct Shape {
    points: &'static [i16],
    pub advance: u16,
    even_odd: bool,
}

fn shape_of(glyph: &'static OutGlyph) -> Shape {
    let start = glyph.start as usize;
    let end = start + glyph.len as usize;
    Shape {
        points: &POINTS[start..end],
        advance: glyph.advance,
        even_odd: glyph.even_odd,
    }
}

fn search(table: &'static [OutGlyph], code: u32) -> Option<&'static OutGlyph> {
    let mut low = 0usize;
    let mut high = table.len();
    while low < high {
        let mid = (low + high) / 2;
        let found = table[mid].code;
        if found == code {
            return Some(&table[mid]);
        }
        if found < code {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    None
}

pub fn find(code: u32) -> Option<Shape> {
    let special = match SCRIPT_HINT.load(Ordering::Relaxed) {
        1 => search(&JP, code),
        2 => search(&KR, code),
        _ => None,
    };
    let glyph = match special {
        Some(g) => Some(g),
        None => search(&SC, code),
    };
    match glyph {
        Some(g) => Some(shape_of(g)),
        None => None,
    }
}

pub fn is_deva(code: u32) -> bool {
    code >= 0x0900 && code <= 0x097F
}

fn is_deva_consonant(code: u32) -> bool {
    (code >= 0x0915 && code <= 0x0939) || (code >= 0x0958 && code <= 0x095F)
}

pub fn deva_extends(prev: u32, next: u32) -> bool {
    if next == 0x200C || next == 0x200D {
        return prev == 0x094D || is_deva(prev);
    }
    if next <= 0x0903 && next >= 0x0900 {
        return true;
    }
    if next >= 0x093A && next <= 0x094F && next != 0x093D {
        return true;
    }
    if next == 0x093C || (next >= 0x0951 && next <= 0x0957) || next == 0x0962 || next == 0x0963 {
        return true;
    }
    if prev == 0x094D && is_deva_consonant(next) {
        return true;
    }
    if (prev == 0x200D || prev == 0x200C) && is_deva_consonant(next) {
        return true;
    }
    false
}

pub fn find_cluster(key: &str) -> Option<Shape> {
    let mut low = 0usize;
    let mut high = CLUSTERS.len();
    while low < high {
        let mid = (low + high) / 2;
        match CLUSTERS[mid].key.cmp(key) {
            Cmp::Equal => {
                let start = CLUSTERS[mid].start as usize;
                let end = start + CLUSTERS[mid].len as usize;
                return Some(Shape {
                    points: &POINTS[start..end],
                    advance: CLUSTERS[mid].advance,
                    even_odd: true,
                });
            }
            Cmp::Less => low = mid + 1,
            Cmp::Greater => high = mid,
        }
    }
    None
}

pub fn em_pixels(cap: i32) -> i32 {
    (cap * EM_PER_CAP_NUM + EM_PER_CAP_DEN / 2) / EM_PER_CAP_DEN
}

pub fn advance_pixels(advance: u16, cap: i32) -> i32 {
    ((advance as i32 * em_pixels(cap) + 500) / 1000).max(1)
}

fn add_span(cover: &mut [i32; MAX_WIDTH], width: i64, from: i64, to: i64) {
    let limit = width * 256;
    let a = from.max(0).min(limit);
    let b = to.max(0).min(limit);
    if b <= a {
        return;
    }
    let first = (a >> 8) as usize;
    let last = (b >> 8) as usize;
    if first == last {
        cover[first] += (b - a) as i32;
        return;
    }
    cover[first] += (256 - (a & 255)) as i32;
    let mut index = first + 1;
    while index < last {
        cover[index] += 256;
        index += 1;
    }
    if last < MAX_WIDTH {
        cover[last] += (b & 255) as i32;
    }
}

pub fn draw(shape: &Shape, x: i32, y: i32, cap: i32, plot: &mut dyn FnMut(i32, i32, u8)) {
    let em = em_pixels(cap) as i64;
    let points = shape.points;
    let base = (y as i64 + cap as i64) * 256;
    let origin = x as i64 * 256;
    let tx = |u: i16| -> i64 { origin + (u as i64 * em * 256) / 1000 };
    let ty = |u: i16| -> i64 { base - (u as i64 * em * 256) / 1000 };

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = i64::MAX;
    let mut max_y = i64::MIN;
    let mut index = 0usize;
    while index < points.len() {
        let count = points[index] as usize;
        index += 1;
        let mut k = 0usize;
        while k < count {
            let px = tx(points[index + k * 2]);
            let py = ty(points[index + k * 2 + 1]);
            if px < min_x {
                min_x = px;
            }
            if px > max_x {
                max_x = px;
            }
            if py < min_y {
                min_y = py;
            }
            if py > max_y {
                max_y = py;
            }
            k += 1;
        }
        index += count * 2;
    }
    if min_x > max_x {
        return;
    }

    let x_first = min_x.div_euclid(256);
    let x_last = (max_x + 255).div_euclid(256);
    let width = x_last - x_first + 1;
    if width <= 0 || width as usize > MAX_WIDTH {
        return;
    }
    let x_origin = x_first * 256;
    let row_first = min_y.div_euclid(256);
    let row_last = (max_y + 255).div_euclid(256);

    let mut cover = [0i32; MAX_WIDTH];
    let mut row = row_first;
    while row <= row_last {
        let mut used = false;
        let mut sub = 0i64;
        while sub < SUBSAMPLES {
            let sample = row * 256 + (2 * sub + 1) * 256 / (2 * SUBSAMPLES);
            let mut crossings = [(0i64, 0i32); MAX_CROSSINGS];
            let mut found = 0usize;

            let mut cursor = 0usize;
            while cursor < points.len() {
                let count = points[cursor] as usize;
                cursor += 1;
                let mut j = 0usize;
                while j < count {
                    let next = if j + 1 == count { 0 } else { j + 1 };
                    let x0 = tx(points[cursor + j * 2]);
                    let y0 = ty(points[cursor + j * 2 + 1]);
                    let x1 = tx(points[cursor + next * 2]);
                    let y1 = ty(points[cursor + next * 2 + 1]);
                    j += 1;
                    if y0 == y1 {
                        continue;
                    }
                    let (low, high, dir) = if y0 < y1 { (y0, y1, 1) } else { (y1, y0, -1) };
                    if sample < low || sample >= high {
                        continue;
                    }
                    if found == MAX_CROSSINGS {
                        continue;
                    }
                    let cx = x0 + (sample - y0) * (x1 - x0) / (y1 - y0);
                    let mut slot = found;
                    while slot > 0 && crossings[slot - 1].0 > cx {
                        crossings[slot] = crossings[slot - 1];
                        slot -= 1;
                    }
                    crossings[slot] = (cx, dir);
                    found += 1;
                }
                cursor += count * 2;
            }

            let mut winding = 0i32;
            let mut c = 0usize;
            while c + 1 < found {
                winding += crossings[c].1;
                let inside = if shape.even_odd { winding & 1 != 0 } else { winding != 0 };
                if inside {
                    add_span(&mut cover, width, crossings[c].0 - x_origin, crossings[c + 1].0 - x_origin);
                    used = true;
                }
                c += 1;
            }
            sub += 1;
        }

        if used {
            let mut column = 0usize;
            while (column as i64) < width {
                let value = cover[column];
                if value > 0 {
                    let alpha = (value as i64 * 255 + (256 * SUBSAMPLES) / 2) / (256 * SUBSAMPLES);
                    let alpha = if alpha > 255 { 255 } else { alpha };
                    if alpha > 0 {
                        plot((x_first + column as i64) as i32, row as i32, alpha as u8);
                    }
                    cover[column] = 0;
                }
                column += 1;
            }
        }
        row += 1;
    }
}
