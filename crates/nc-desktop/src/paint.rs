use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_gfx::{lerp_color, Canvas, Rect};

use crate::motion::{clamp01, ONE};

pub const WHITE: u32 = 0x00FF_FFFF;

pub fn a8(v: u32) -> u8 {
    v.min(255) as u8
}

pub fn mix(a: u32, b: u32, t_q12: i32) -> u32 {
    lerp_color(a, b, (clamp01(t_q12) as u32 * 255) / ONE as u32)
}

pub fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn bounds(canvas: &Canvas<'_>) -> (i32, i32, i32, i32) {
    let c = canvas.clip_rect();
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    (
        c.x.max(0),
        c.y.max(0),
        (c.x + c.width).min(cw),
        (c.y + c.height).min(ch),
    )
}

pub fn sample(canvas: &mut Canvas<'_>, x: i32, y: i32) -> u32 {
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    if x < 0 || y < 0 || x >= w || y >= h {
        return 0;
    }
    canvas.pixels_mut()[(y as usize) * (w as usize) + x as usize]
}

pub fn fade_text_color(canvas: &mut Canvas<'_>, x: i32, y: i32, color: u32, t_q12: i32) -> u32 {
    if t_q12 >= ONE {
        return color;
    }
    let bg = sample(canvas, x, y);
    mix(bg, color, t_q12)
}

fn clamp_radius(rect: Rect, r: i32) -> i32 {
    r.min(rect.width / 2).min(rect.height / 2).max(0)
}

fn dist16(x: i32, y: i32, rect: Rect, r: i32) -> i32 {
    let px = ((x - rect.x) * 16 + 8) - rect.width * 8;
    let py = ((y - rect.y) * 16 + 8) - rect.height * 8;
    let hw = rect.width * 8;
    let hh = rect.height * 8;
    let r16 = r * 16;
    let qx = px.abs() - (hw - r16);
    let qy = py.abs() - (hh - r16);
    let mx = qx.max(0) as i64;
    let my = qy.max(0) as i64;
    let len = if mx == 0 {
        my
    } else if my == 0 {
        mx
    } else {
        isqrt((mx * mx + my * my) as u64) as i64
    } as i32;
    len + qx.max(qy).min(0) - r16
}

fn cov(d16: i32) -> u32 {
    ((8 - d16).clamp(0, 16) * 255 / 16) as u32
}

pub fn paint_rr<F: FnMut(i32, i32) -> (u32, u32)>(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    f: F,
) {
    paint_rr_hole(canvas, rect, radius, None, f);
}

pub fn paint_rr_hole<F: FnMut(i32, i32) -> (u32, u32)>(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    hole: Option<Rect>,
    mut f: F,
) {
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let r = clamp_radius(rect, radius);
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let x0 = rect.x.max(bx0);
    let y0 = rect.y.max(by0);
    let x1 = (rect.x + rect.width).min(bx1);
    let y1 = (rect.y + rect.height).min(by1);
    let right = rect.x + rect.width - 1;
    let bottom = rect.y + rect.height - 1;
    for y in y0..y1 {
        let row_corner = (y - rect.y) < r || (bottom - y) < r;
        let (hx0, hx1) = match hole {
            Some(h) if y >= h.y && y < h.y + h.height => (h.x, h.x + h.width),
            _ => (x1, x1),
        };
        let mut x = x0;
        while x < x1 {
            if x >= hx0 && x < hx1 {
                x = hx1;
                continue;
            }
            let c = if row_corner && ((x - rect.x) < r || (right - x) < r) {
                cov(dist16(x, y, rect, r))
            } else {
                255
            };
            if c > 0 {
                let (col, a) = f(x, y);
                let al = c * a.min(255) / 255;
                if al > 0 {
                    canvas.blend_pixel(x, y, col, a8(al));
                }
            }
            x += 1;
        }
    }
}

pub fn fill_rr(canvas: &mut Canvas<'_>, rect: Rect, radius: i32, color: u32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    paint_rr(canvas, rect, radius, |_, _| (color, alpha));
}

pub fn fill_rr_vgrad(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    top: u32,
    bottom: u32,
    alpha: u32,
) {
    if alpha == 0 {
        return;
    }
    let span = (rect.height - 1).max(1);
    let base = rect.y;
    paint_rr(canvas, rect, radius, |_, y| {
        let t = (((y - base) * 255) / span).clamp(0, 255) as u32;
        (lerp_color(top, bottom, t), alpha)
    });
}

pub fn stroke_rr(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    thickness: i32,
    color: u32,
    alpha: u32,
) {
    if alpha == 0 || rect.width <= 0 || rect.height <= 0 || thickness <= 0 {
        return;
    }
    let r = clamp_radius(rect, radius);
    let t = thickness;
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let x0 = rect.x.max(bx0);
    let y0 = rect.y.max(by0);
    let x1 = (rect.x + rect.width).min(bx1);
    let y1 = (rect.y + rect.height).min(by1);
    let right = rect.x + rect.width - 1;
    let bottom = rect.y + rect.height - 1;
    for y in y0..y1 {
        let dy_top = y - rect.y;
        let dy_bot = bottom - y;
        let corner_row = dy_top < r || dy_bot < r;
        let edge_row = dy_top < t || dy_bot < t;
        let full_row = edge_row || corner_row;
        let mut x = x0;
        while x < x1 {
            if !full_row {
                let near_left = x - rect.x < t;
                let near_right = right - x < t;
                if !near_left && !near_right {
                    let skip_to = (right - t + 1).max(x + 1);
                    x = skip_to;
                    continue;
                }
            }
            let in_corner = corner_row && ((x - rect.x) < r || (right - x) < r);
            let c = if in_corner {
                let d = dist16(x, y, rect, r);
                cov(d).saturating_sub(cov(d + t * 16))
            } else if edge_row || (x - rect.x) < t || (right - x) < t {
                255
            } else {
                0
            };
            if c > 0 {
                let al = c * alpha.min(255) / 255;
                if al > 0 {
                    canvas.blend_pixel(x, y, color, a8(al));
                }
            }
            x += 1;
        }
    }
}

pub fn shadow_rr(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    dy: i32,
    blur: i32,
    color: u32,
    alpha: u32,
) {
    if alpha == 0 || blur <= 0 || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let r = clamp_radius(rect, radius);
    let shifted = Rect::new(rect.x, rect.y + dy, rect.width, rect.height);
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let x0 = (rect.x - blur).max(bx0);
    let y0 = (rect.y.min(shifted.y) - blur).max(by0);
    let x1 = (rect.x + rect.width + blur).min(bx1);
    let y1 = (rect.y.max(shifted.y) + rect.height + blur).min(by1);
    let blur16 = blur * 16;
    let rx1 = rect.x + rect.width;
    let ry1 = rect.y + rect.height;
    for y in y0..y1 {
        for x in x0..x1 {
            if x >= rect.x && x < rx1 && y >= rect.y && y < ry1 {
                let in_corner = ((x - rect.x) < r || (rx1 - 1 - x) < r)
                    && ((y - rect.y) < r || (ry1 - 1 - y) < r);
                if !in_corner || cov(dist16(x, y, rect, r)) >= 255 {
                    continue;
                }
            }
            let d = dist16(x, y, shifted, r);
            if d >= blur16 {
                continue;
            }
            let t = (d.max(0) as i64 * 256 / blur16 as i64).clamp(0, 256);
            let s = t * t * (3 * 256 - 2 * t) / (256 * 256);
            let a = alpha as i64 * (256 - s) / 256;
            if a > 0 {
                canvas.blend_pixel(x, y, color, a8(a as u32));
            }
        }
    }
}

fn box_blur_line(src: &[u32], dst: &mut [u32], r: usize) {
    let n = src.len();
    if n == 0 {
        return;
    }
    let win = (2 * r + 1) as u32;
    let first = src[0];
    let (fr, fg, fb) = ((first >> 16) & 0xFF, (first >> 8) & 0xFF, first & 0xFF);
    let mut sr = fr * r as u32;
    let mut sg = fg * r as u32;
    let mut sb = fb * r as u32;
    for k in 0..=r {
        let p = src[k.min(n - 1)];
        sr += (p >> 16) & 0xFF;
        sg += (p >> 8) & 0xFF;
        sb += p & 0xFF;
    }
    for i in 0..n {
        dst[i] = ((sr / win) << 16) | ((sg / win) << 8) | (sb / win);
        let out_p = if i >= r { src[i - r] } else { first };
        let in_p = src[(i + r + 1).min(n - 1)];
        sr = sr + ((in_p >> 16) & 0xFF) - ((out_p >> 16) & 0xFF);
        sg = sg + ((in_p >> 8) & 0xFF) - ((out_p >> 8) & 0xFF);
        sb = sb + (in_p & 0xFF) - (out_p & 0xFF);
    }
}

pub fn blur_rect(canvas: &mut Canvas<'_>, rect: Rect, radius: usize, passes: usize) {
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    let x0 = rect.x.clamp(0, cw);
    let y0 = rect.y.clamp(0, ch);
    let x1 = (rect.x + rect.width).clamp(0, cw);
    let y1 = (rect.y + rect.height).clamp(0, ch);
    let rw = (x1 - x0) as usize;
    let rh = (y1 - y0) as usize;
    if rw <= 2 || rh <= 2 || radius == 0 || passes == 0 {
        return;
    }
    let r = radius.min(rw / 2).min(rh / 2).min(32).max(1);
    let stride = cw as usize;
    const BLOCK: usize = 8;
    let mut line: Vec<u32> = vec![0; rw.max(rh)];
    let mut cin: Vec<u32> = vec![0; rh * BLOCK];
    let mut cout: Vec<u32> = vec![0; rh * BLOCK];
    for _ in 0..passes {
        let pixels = canvas.pixels_mut();
        for y in (y0 as usize)..(y1 as usize) {
            let base = y * stride + x0 as usize;
            line[..rw].copy_from_slice(&pixels[base..base + rw]);
            box_blur_line(&line[..rw], &mut pixels[base..base + rw], r);
        }
        let mut bx = x0 as usize;
        while bx < x1 as usize {
            let bw = BLOCK.min(x1 as usize - bx);
            for j in 0..rh {
                let row = (y0 as usize + j) * stride + bx;
                for k in 0..bw {
                    cin[k * rh + j] = pixels[row + k];
                }
            }
            for k in 0..bw {
                box_blur_line(&cin[k * rh..(k + 1) * rh], &mut cout[k * rh..(k + 1) * rh], r);
            }
            for j in 0..rh {
                let row = (y0 as usize + j) * stride + bx;
                for k in 0..bw {
                    pixels[row + k] = cout[k * rh + j];
                }
            }
            bx += BLOCK;
        }
    }
}

fn corner_pixels<F: FnMut(i32, i32)>(
    rect: Rect,
    r: i32,
    cw: i32,
    ch: i32,
    bottom_round: bool,
    mut f: F,
) {
    if r <= 1 {
        return;
    }
    let right = rect.x + rect.width - 1;
    let bottom = rect.y + rect.height - 1;
    let count = if bottom_round { 4 } else { 2 };
    for j in 0..r {
        for i in 0..r {
            let pts = [
                (rect.x + i, rect.y + j),
                (right - i, rect.y + j),
                (rect.x + i, bottom - j),
                (right - i, bottom - j),
            ];
            for &(x, y) in pts.iter().take(count) {
                if x < 0 || y < 0 || x >= cw || y >= ch {
                    continue;
                }
                if cov(dist16(x, y, rect, r)) < 255 {
                    f(x, y);
                }
            }
        }
    }
}

pub fn blur_rr(canvas: &mut Canvas<'_>, rect: Rect, radius: i32, blur: usize, passes: usize) {
    blur_rr_mask(canvas, rect, radius, blur, passes, true);
}

pub fn blur_rr_mask(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    blur: usize,
    passes: usize,
    bottom_round: bool,
) {
    if blur == 0 || passes == 0 {
        return;
    }
    let r = clamp_radius(rect, radius);
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    let stride = cw as usize;
    let mut saved: Vec<u32> = Vec::new();
    {
        let pixels = canvas.pixels_mut();
        corner_pixels(rect, r, cw, ch, bottom_round, |x, y| {
            saved.push(pixels[(y as usize) * stride + x as usize]);
        });
    }
    blur_rect(canvas, rect, blur, passes);
    let mut idx = 0usize;
    let pixels = canvas.pixels_mut();
    corner_pixels(rect, r, cw, ch, bottom_round, |x, y| {
        pixels[(y as usize) * stride + x as usize] = saved[idx];
        idx += 1;
    });
}

pub struct Glass {
    pub tint: u32,
    pub alpha_top: u32,
    pub alpha_bot: u32,
    pub blur: usize,
    pub passes: usize,
    pub shadow: u32,
    pub shadow_dy: i32,
    pub shadow_blur: i32,
    pub gloss: u32,
    pub edge: u32,
    pub hole: Option<Rect>,
}

pub fn glass_surface(canvas: &mut Canvas<'_>, rect: Rect, radius: i32, g: &Glass, appear: i32) {
    if rect.width < 4 || rect.height < 4 || appear <= 0 {
        return;
    }
    let ap = clamp01(appear);
    let sc = |v: u32| -> u32 { (v as u64 * ap as u64 / ONE as u64) as u32 };
    if g.shadow > 0 {
        shadow_rr(
            canvas,
            rect,
            radius,
            g.shadow_dy,
            g.shadow_blur,
            0x000A_1030,
            sc(g.shadow),
        );
    }
    if g.blur > 0 && ap > ONE / 4 {
        blur_rr(canvas, rect, radius, g.blur, g.passes);
    }
    let span = (rect.height - 1).max(1);
    let top_a = g.alpha_top as i32;
    let bot_a = g.alpha_bot as i32;
    let base_y = rect.y;
    let tint = g.tint;
    paint_rr_hole(canvas, rect, radius, g.hole, |_, y| {
        let t = ((y - base_y) * 255 / span).clamp(0, 255);
        let a = top_a + (bot_a - top_a) * t / 255;
        (tint, sc(a.max(0) as u32))
    });
    if g.gloss > 0 {
        let gloss = g.gloss;
        let rx = rect.x;
        let ry = rect.y;
        let rw = rect.width.max(1) as i64;
        let rh = rect.height.max(1) as i64;
        paint_rr_hole(canvas, rect, radius, g.hole, |x, y| {
            let v = (y - ry) as i64 * 256 / rh;
            let u = (x - rx) as i64 * 256 / rw;
            let vert = ((150 - v).max(0) * 255 / 150) as u32;
            let diag = ((200 - (u + v)).clamp(0, 56) * 255 / 56) as u32;
            let a = (vert * 120 / 255 + diag * vert / 255 * 70 / 255) * gloss / 255;
            (WHITE, sc(a))
        });
    }
    if g.edge > 0 {
        stroke_rr(canvas, rect, radius, 1, WHITE, sc(g.edge));
        let inner = Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
        stroke_rr(canvas, inner, (radius - 1).max(0), 1, WHITE, sc(g.edge / 4));
    }
}

pub fn soft_sphere(
    canvas: &mut Canvas<'_>,
    cx: i32,
    cy: i32,
    radius: i32,
    inner: u32,
    outer: u32,
    alpha: u32,
) {
    if radius <= 2 || alpha == 0 {
        return;
    }
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let r2 = (radius as i64) * (radius as i64);
    let x0 = (cx - radius).max(bx0);
    let x1 = (cx + radius + 1).min(bx1);
    let y0 = (cy - radius).max(by0);
    let y1 = (cy + radius + 1).min(by1);
    for y in y0..y1 {
        let dy = (y - cy) as i64;
        for x in x0..x1 {
            let dx = (x - cx) as i64;
            let d2 = dx * dx + dy * dy;
            if d2 >= r2 {
                continue;
            }
            let t = (d2 * 255 / r2) as u32;
            let u = 255 - t;
            let a = alpha as u64 * (u * u / 255) as u64 / 255;
            if a > 0 {
                canvas.blend_pixel(x, y, lerp_color(inner, outer, t), a8(a as u32));
            }
        }
    }
}

pub fn fit_text(text: &str, max_w: i32, measure: &dyn Fn(&str) -> i32) -> String {
    if measure(text) <= max_w {
        return String::from(text);
    }
    let dots = "...";
    let dots_w = measure(dots);
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if measure(&probe) + dots_w > max_w {
            break;
        }
        out.push(ch);
    }
    out.push_str(dots);
    out
}

pub fn draw_line(
    canvas: &mut Canvas<'_>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness16: i32,
    color: u32,
    alpha: u32,
) {
    if alpha == 0 {
        return;
    }
    let pad = thickness16 / 16 + 2;
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let minx = (x0.min(x1) - pad).max(bx0);
    let maxx = (x0.max(x1) + pad + 1).min(bx1);
    let miny = (y0.min(y1) - pad).max(by0);
    let maxy = (y0.max(y1) + pad + 1).min(by1);
    let ax = x0 as i64 * 16 + 8;
    let ay = y0 as i64 * 16 + 8;
    let bxx = x1 as i64 * 16 + 8;
    let byy = y1 as i64 * 16 + 8;
    let dx = bxx - ax;
    let dy = byy - ay;
    let len2 = (dx * dx + dy * dy).max(1);
    let half = (thickness16 / 2) as i64;
    for y in miny..maxy {
        for x in minx..maxx {
            let px = x as i64 * 16 + 8;
            let py = y as i64 * 16 + 8;
            let t = (((px - ax) * dx + (py - ay) * dy) * 256 / len2).clamp(0, 256);
            let cx = ax + dx * t / 256;
            let cy = ay + dy * t / 256;
            let ex = px - cx;
            let ey = py - cy;
            let d = isqrt((ex * ex + ey * ey) as u64) as i64;
            let c = ((half + 8 - d).clamp(0, 16) * 255 / 16) as u32;
            if c > 0 {
                let al = c * alpha.min(255) / 255;
                if al > 0 {
                    canvas.blend_pixel(x, y, color, a8(al));
                }
            }
        }
    }
}

pub fn ring(
    canvas: &mut Canvas<'_>,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    color: u32,
    alpha: u32,
    gap_top: i32,
) {
    if alpha == 0 || radius <= 1 {
        return;
    }
    let (bx0, by0, bx1, by1) = bounds(canvas);
    let x0 = (cx - radius - 1).max(bx0);
    let x1 = (cx + radius + 2).min(bx1);
    let y0 = (cy - radius - 1).max(by0);
    let y1 = (cy + radius + 2).min(by1);
    let outer = radius * 16;
    let inner = (radius - thickness) * 16;
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = (x - cx) as i64 * 16;
            let dy = (y - cy) as i64 * 16;
            if gap_top > 0 && y < cy && (x - cx).abs() < gap_top {
                continue;
            }
            let d = isqrt((dx * dx + dy * dy) as u64) as i32;
            let c = cov(d - outer).saturating_sub(cov(d - inner));
            if c > 0 {
                let al = c * alpha.min(255) / 255;
                if al > 0 {
                    canvas.blend_pixel(x, y, color, a8(al));
                }
            }
        }
    }
}
