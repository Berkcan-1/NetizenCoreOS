use nc_gfx::{rgb, Canvas, Rect};
use nc_assets::logo;
use nc_ui::{draw_dot, WHITE};

use crate::motion::{clamp01, ONE};
use crate::paint::{
    draw_line, fill_rr, fill_rr_vgrad, mix, paint_rr, ring, sample, shadow_rr, stroke_rr,
};
use crate::theme::{darken, lighten};

static MINLOGO_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/minlogo.bin"));

pub fn tile_radius(size: i32) -> i32 {
    size * 27 / 100
}

pub fn draw_tile(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    top: u32,
    bottom: u32,
    appear: i32,
    lift: i32,
    glyph: &dyn Fn(&mut Canvas<'_>, i32, i32, i32, u32),
    glyph_pct: i32,
) {
    let h = rect.height;
    draw_tile_with_shadow(
        canvas,
        rect,
        top,
        bottom,
        appear,
        lift,
        glyph,
        glyph_pct,
        h / 10 + lift / 2,
        h / 4 + lift / 2,
        darken(bottom, 140),
        88,
    );
}

/// Dock tiles use a short, low-opacity shadow so adjacent app icons do not
/// merge into a dark horizontal strip on the translucent dock surface.
pub fn draw_dock_tile(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    top: u32,
    bottom: u32,
    appear: i32,
    lift: i32,
    glyph: &dyn Fn(&mut Canvas<'_>, i32, i32, i32, u32),
    glyph_pct: i32,
) {
    let h = rect.height;
    draw_tile_with_shadow(
        canvas,
        rect,
        top,
        bottom,
        appear,
        lift,
        glyph,
        glyph_pct,
        (h / 18).max(1) + lift / 4,
        (h / 12).max(1) + lift / 4,
        darken(bottom, 105),
        28,
    );
}

fn draw_tile_with_shadow(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    top: u32,
    bottom: u32,
    appear: i32,
    lift: i32,
    glyph: &dyn Fn(&mut Canvas<'_>, i32, i32, i32, u32),
    glyph_pct: i32,
    shadow_dy: i32,
    shadow_blur: i32,
    shadow_color: u32,
    shadow_alpha: u32,
) {
    if rect.width < 6 || appear <= 0 {
        return;
    }
    let ap = clamp01(appear);
    let sc = |v: u32| -> u32 { (v as u64 * ap as u64 / ONE as u64) as u32 };
    let r = tile_radius(rect.width);
    let h = rect.height;

    shadow_rr(
        canvas,
        rect,
        r,
        shadow_dy,
        shadow_blur,
        shadow_color,
        sc(shadow_alpha),
    );
    fill_rr_vgrad(canvas, rect, r, top, bottom, sc(255));

    let glow_y = rect.y + h * 62 / 100;
    let glow_h = (rect.y + h - glow_y).max(1);
    let bottom_light = lighten(bottom, 120);
    paint_rr(canvas, rect, r, |_, y| {
        let a = ((y - glow_y).max(0) * 90 / glow_h) as u32;
        (bottom_light, sc(a))
    });

    let gloss = Rect::new(rect.x + 2, rect.y + 2, rect.width - 4, h * 48 / 100);
    let gh = gloss.height.max(1);
    paint_rr(canvas, gloss, (r - 2).max(2), |_, y| {
        let t = (y - gloss.y).max(0) * 255 / gh;
        let a = ((255 - t).max(0) as u32 * 150 / 255).min(255);
        (WHITE, sc(a))
    });
    stroke_rr(canvas, rect, r, 1, WHITE, sc(110));

    let size = rect.width * glyph_pct / 100;
    let cx = rect.x + rect.width / 2;
    let cy = rect.y + rect.height / 2;
    if ap > ONE * 3 / 4 {
        glyph(canvas, cx, cy + (h / 26).max(1), size, darken(bottom, 110));
    }
    let body = sample(canvas, cx, cy);
    glyph(canvas, cx, cy, size, mix(body, WHITE, ap));
}

pub fn draw_folder_glyph(canvas: &mut Canvas<'_>, cx: i32, cy: i32, size: i32, color: u32) {
    let w = size;
    let h = size * 4 / 5;
    let x = cx - w / 2;
    let y = cy - h / 2;
    fill_rr(canvas, Rect::new(x, y, w * 2 / 5, h / 3), (h / 8).max(2), color, 255);
    fill_rr(
        canvas,
        Rect::new(x, y + h / 5, w, h * 4 / 5),
        (h / 6).max(2),
        color,
        255,
    );
}

pub fn draw_doc_glyph(canvas: &mut Canvas<'_>, cx: i32, cy: i32, size: i32, color: u32, line: u32) {
    let w = size * 3 / 4;
    let h = size;
    let x = cx - w / 2;
    let y = cy - h / 2;
    fill_rr(canvas, Rect::new(x, y, w, h), (w / 8).max(2), color, 255);
    let lh = (h / 12).max(2);
    for i in 0..3 {
        let lw = if i == 2 { w * 5 / 12 } else { w * 2 / 3 };
        fill_rr(
            canvas,
            Rect::new(x + w / 6, y + h / 4 + i * (lh * 2 + 1), lw, lh),
            lh / 2,
            line,
            255,
        );
    }
}

pub fn draw_power_glyph(canvas: &mut Canvas<'_>, cx: i32, cy: i32, r: i32, color: u32, alpha: u32) {
    let t = (r / 4).max(2);
    ring(canvas, cx, cy + r / 8, r, t, color, alpha, t);
    fill_rr(
        canvas,
        Rect::new(cx - t / 2, cy - r - r / 6, t, r + r / 6),
        t / 2,
        color,
        alpha,
    );
}

pub fn draw_reboot_glyph(canvas: &mut Canvas<'_>, cx: i32, cy: i32, r: i32, color: u32, alpha: u32) {
    let t = (r / 4).max(2);
    ring(canvas, cx, cy, r, t, color, alpha, t * 2);
    let ax = cx + r / 2;
    let ay = cy - r;
    let s = (r / 2).max(3);
    draw_line(canvas, ax, ay - s / 2, ax + s / 2 + 1, ay, t * 16, color, alpha);
    draw_line(canvas, ax, ay + s / 2, ax + s / 2 + 1, ay, t * 16, color, alpha);
}

pub fn draw_wifi(
    canvas: &mut Canvas<'_>,
    x: i32,
    base_y: i32,
    height: i32,
    color: u32,
    active: bool,
) -> i32 {
    let bw = (height / 5).max(2);
    let gap = (bw / 2).max(1);
    for i in 0..4 {
        let bh = height * (3 + i * 2) / 10;
        let rx = x + i * (bw + gap);
        let on = !active && i > 0;
        fill_rr(
            canvas,
            Rect::new(rx, base_y - bh, bw, bh),
            bw / 2,
            color,
            if on { 70 } else { 235 },
        );
    }
    4 * (bw + gap) - gap
}

pub fn draw_win_glyph_min(canvas: &mut Canvas<'_>, cx: i32, cy: i32, s: i32, color: u32) {
    fill_rr(canvas, Rect::new(cx - s, cy - 1, s * 2, 2), 1, color, 235);
}

pub fn draw_win_glyph_max(canvas: &mut Canvas<'_>, cx: i32, cy: i32, s: i32, color: u32, restore: bool) {
    if restore {
        stroke_rr(
            canvas,
            Rect::new(cx - s + 2, cy - s - 1, s * 2 - 2, s * 2 - 2),
            2,
            1,
            color,
            200,
        );
        fill_rr(canvas, Rect::new(cx - s - 1, cy - s + 3, s * 2 - 2, s * 2 - 2), 2, rgb(255, 255, 255), 90);
        stroke_rr(
            canvas,
            Rect::new(cx - s - 1, cy - s + 3, s * 2 - 2, s * 2 - 2),
            2,
            1,
            color,
            235,
        );
    } else {
        stroke_rr(canvas, Rect::new(cx - s, cy - s, s * 2, s * 2), 3, 1, color, 235);
    }
}

pub fn draw_win_glyph_close(canvas: &mut Canvas<'_>, cx: i32, cy: i32, s: i32, color: u32) {
    draw_line(canvas, cx - s, cy - s, cx + s, cy + s, 28, color, 240);
    draw_line(canvas, cx - s, cy + s, cx + s, cy - s, 28, color, 240);
}

pub fn draw_start_logo(canvas: &mut Canvas<'_>, cx: i32, cy: i32, target_size: i32) {
    let dst = target_size.max(12);
    let x0 = cx - dst / 2;
    let y0 = cy - dst / 2;

    if MINLOGO_BIN.len() > 8 {
        let src_w = u32::from_le_bytes([MINLOGO_BIN[0], MINLOGO_BIN[1], MINLOGO_BIN[2], MINLOGO_BIN[3]])
            as usize;
        let src_h = u32::from_le_bytes([MINLOGO_BIN[4], MINLOGO_BIN[5], MINLOGO_BIN[6], MINLOGO_BIN[7]])
            as usize;
        let pixels = &MINLOGO_BIN[8..];
        if src_w > 0 && src_h > 0 && pixels.len() >= src_w * src_h * 4 {
            blit_rgba_scaled(canvas, x0, y0, dst as usize, dst as usize, pixels, src_w, src_h);
            return;
        }
    }

    if let Some(asset_logo) = logo() {
        let src_w = asset_logo.width as usize;
        let src_h = asset_logo.height as usize;
        if src_w > 0 && src_h > 0 {
            blit_rgba_scaled(
                canvas,
                x0,
                y0,
                dst as usize,
                dst as usize,
                asset_logo.pixels,
                src_w,
                src_h,
            );
            return;
        }
    }

    draw_dot(canvas, cx, cy, dst / 3, WHITE, 240);
}

fn blit_rgba_scaled(
    canvas: &mut Canvas<'_>,
    dst_x: i32,
    dst_y: i32,
    dst_w: usize,
    dst_h: usize,
    src: &[u8],
    src_w: usize,
    src_h: usize,
) {
    for dy in 0..dst_h {
        let sy0 = dy * src_h / dst_h;
        let sy1 = ((dy + 1) * src_h / dst_h).max(sy0 + 1).min(src_h);
        for dx in 0..dst_w {
            let sx0 = dx * src_w / dst_w;
            let sx1 = ((dx + 1) * src_w / dst_w).max(sx0 + 1).min(src_w);

            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut a_sum = 0u32;
            let mut count = 0u32;

            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let idx = (sy * src_w + sx) * 4;
                    let a = src[idx + 3] as u32;
                    r_sum += src[idx] as u32 * a;
                    g_sum += src[idx + 1] as u32 * a;
                    b_sum += src[idx + 2] as u32 * a;
                    a_sum += a;
                    count += 1;
                }
            }

            if count > 0 && a_sum > 0 {
                let alpha = (a_sum / count).min(255) as u8;
                if alpha > 4 {
                    let r = (r_sum / a_sum).min(255) as u8;
                    let g = (g_sum / a_sum).min(255) as u8;
                    let b = (b_sum / a_sum).min(255) as u8;
                    canvas.blend_pixel(dst_x + dx as i32, dst_y + dy as i32, rgb(r, g, b), alpha);
                }
            }
        }
    }
}
