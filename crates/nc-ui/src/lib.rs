#![no_std]

#[cfg(test)]
extern crate std;

#[cfg(feature = "remote")]
pub mod remote;

use nc_font::{draw_text, text_width, Style};
use nc_gfx::{lerp_color, rgb, Canvas, Rect};
use nc_math::integer_sqrt;

pub const BACKDROP_TOP: u32 = rgb(248, 250, 252);
pub const BACKDROP_BOTTOM: u32 = rgb(238, 242, 246);
pub const ACCENT_LIGHT: u32 = rgb(59, 130, 246);
pub const ACCENT_LINE: u32 = rgb(37, 99, 235);
pub const ACCENT_DARK: u32 = rgb(29, 78, 216);
pub const ACCENT_BG: u32 = rgb(239, 246, 255);
pub const AQUA_LIGHT: u32 = rgb(112, 226, 206);
pub const AQUA: u32 = rgb(30, 184, 166);
pub const AQUA_DARK: u32 = rgb(14, 146, 132);
pub const SUCCESS: u32 = rgb(16, 185, 129);
pub const SUCCESS_BG: u32 = rgb(236, 253, 245);
pub const TEXT_PRIMARY: u32 = rgb(15, 23, 42);
pub const TEXT_MUTED: u32 = rgb(100, 116, 139);
pub const TEXT_DIM: u32 = rgb(148, 163, 184);
pub const PANEL_FILL: u32 = rgb(255, 255, 255);
pub const PANEL_BORDER: u32 = rgb(226, 232, 240);
pub const SHADOW: u32 = rgb(15, 23, 42);
pub const RAIL: u32 = rgb(226, 232, 240);
pub const RAIL_DOT: u32 = rgb(203, 213, 225);
pub const WHITE: u32 = rgb(255, 255, 255);

pub fn text_style(cap_height: i32, tracking_percent: i32) -> Style {
    Style {
        cap_height,
        stroke_sixteenths: (cap_height * 16 / 18).max(22),
        tracking: cap_height * tracking_percent / 100,
    }
}

pub fn draw_backdrop(canvas: &mut Canvas<'_>) {
    canvas.vertical_gradient(BACKDROP_TOP, BACKDROP_BOTTOM);
}

pub fn draw_text_gradient(
    canvas: &mut Canvas<'_>,
    text: &str,
    x: i32,
    y: i32,
    style: &Style,
    top: u32,
    bottom: u32,
) {
    let cap = style.cap_height.max(1);
    let mut plot = |px: i32, py: i32, coverage: u8| {
        let t = ((py - y) * 255 / cap).clamp(0, 255) as u32;
        canvas.blend_pixel(px, py, lerp_color(top, bottom, t), coverage);
    };
    draw_text(text, x, y, style, &mut plot);
}

pub fn draw_text_color(canvas: &mut Canvas<'_>, text: &str, x: i32, y: i32, style: &Style, color: u32) {
    let mut plot = |px: i32, py: i32, coverage: u8| {
        canvas.blend_pixel(px, py, color, coverage);
    };
    draw_text(text, x, y, style, &mut plot);
}

pub fn draw_fading_line(canvas: &mut Canvas<'_>, x: i32, y: i32, width: i32, color: u32, alpha: u32) {
    let half = (width / 2).max(1);
    for offset in 0..width {
        let distance = (offset - width / 2).abs();
        let fade = (255 - distance * 255 / half).clamp(0, 255) as u32;
        let soft = fade * fade / 255;
        canvas.blend_pixel(x + offset, y, color, (soft * alpha / 255).min(255) as u8);
    }
}

fn circle_coverage(dx: i32, dy: i32, radius256: i64) -> u32 {
    let dx = dx as i64 * 256;
    let dy = dy as i64 * 256;
    let distance = integer_sqrt((dx * dx + dy * dy) as u64) as i64;
    (radius256 - distance + 128).clamp(0, 255) as u32
}

pub fn draw_dot(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, radius: i32, color: u32, strength: u32) {
    let radius256 = radius as i64 * 256;
    for y in center_y - radius - 1..=center_y + radius + 1 {
        for x in center_x - radius - 1..=center_x + radius + 1 {
            let coverage = circle_coverage(x - center_x, y - center_y, radius256);
            if coverage > 0 {
                canvas.blend_pixel(x, y, color, (coverage * strength.min(256) / 256).min(255) as u8);
            }
        }
    }
}

fn rounded_distance(x: i32, y: i32, area: Rect, radius: i32) -> i64 {
    let half_width = area.width as i64 * 128;
    let half_height = area.height as i64 * 128;
    let center_x = area.x as i64 * 256 + half_width;
    let center_y = area.y as i64 * 256 + half_height;
    let radius256 = radius as i64 * 256;
    let px = (x as i64 * 256 + 128 - center_x).abs();
    let py = (y as i64 * 256 + 128 - center_y).abs();
    let qx = px - (half_width - radius256);
    let qy = py - (half_height - radius256);
    let outside = if qx > 0 && qy > 0 {
        integer_sqrt((qx * qx + qy * qy) as u64) as i64
    } else {
        qx.max(qy)
    };
    outside - radius256
}

fn panel_pixel(
    canvas: &mut Canvas<'_>,
    x: i32,
    y: i32,
    area: Rect,
    radius: i32,
    fill: u32,
    fill_alpha: u32,
    border: u32,
    border_alpha: u32,
) {
    let distance = rounded_distance(x, y, area, radius);
    let fill_coverage = (128 - distance).clamp(0, 255) as u32;
    if fill_coverage > 0 {
        canvas.blend_pixel(x, y, fill, (fill_alpha * fill_coverage / 255).min(255) as u8);
    }
    let ring = (256 - (distance + 128).abs()).clamp(0, 255) as u32;
    if ring > 0 {
        canvas.blend_pixel(x, y, border, (border_alpha * ring / 255).min(255) as u8);
    }
}

pub fn draw_panel(
    canvas: &mut Canvas<'_>,
    area: Rect,
    radius: i32,
    fill: u32,
    fill_alpha: u32,
    border: u32,
    border_alpha: u32,
) {
    let margin = radius + 2;
    let inner_start = area.x + margin;
    let inner_end = area.x + area.width - margin;
    let right = area.x + area.width;
    for y in area.y..area.y + area.height {
        let deep_row = y >= area.y + margin && y < area.y + area.height - margin && inner_end > inner_start;
        if !deep_row {
            for x in area.x..right {
                panel_pixel(canvas, x, y, area, radius, fill, fill_alpha, border, border_alpha);
            }
            continue;
        }
        for x in area.x..inner_start {
            panel_pixel(canvas, x, y, area, radius, fill, fill_alpha, border, border_alpha);
        }
        if fill_alpha >= 255 {
            canvas.fill_rect(inner_start, y, inner_end - inner_start, 1, fill);
        } else if fill_alpha > 0 {
            for x in inner_start..inner_end {
                canvas.blend_pixel(x, y, fill, fill_alpha as u8);
            }
        }
        for x in inner_end..right {
            panel_pixel(canvas, x, y, area, radius, fill, fill_alpha, border, border_alpha);
        }
    }
}

pub fn draw_shadow(
    canvas: &mut Canvas<'_>,
    area: Rect,
    radius: i32,
    offset_y: i32,
    spread: i32,
    color: u32,
    strength: u32,
) {
    let shifted = Rect::new(area.x, area.y + offset_y, area.width, area.height);
    let reach = (spread as i64 * 256).max(1);
    let margin = radius + 2;
    for y in shifted.y - spread..shifted.y + shifted.height + spread {
        let deep_row = y >= area.y + margin && y < area.y + area.height - margin;
        for x in shifted.x - spread..shifted.x + shifted.width + spread {
            if deep_row && x >= area.x + margin && x < area.x + area.width - margin {
                continue;
            }
            let distance = rounded_distance(x, y, shifted, radius).max(0);
            if distance >= reach {
                continue;
            }
            let near = ((reach - distance) * 256 / reach) as u32;
            let falloff = near * near >> 8;
            canvas.blend_pixel(x, y, color, (falloff * strength >> 8).min(255) as u8);
        }
    }
}

pub fn draw_card(canvas: &mut Canvas<'_>, area: Rect, radius: i32) {
    draw_shadow(canvas, area, radius, 4, 12, SHADOW, 18);
    draw_panel(canvas, area, radius, PANEL_FILL, 255, PANEL_BORDER, 255);
}

pub fn draw_badge(canvas: &mut Canvas<'_>, area: Rect, text: &str, bg: u32, fg: u32) {
    let radius = area.height / 2;
    draw_panel(canvas, area, radius, bg, 255, bg, 0);
    let style = text_style(area.height * 55 / 100, 30);
    let tw = text_width(text, &style);
    let tx = area.x + (area.width - tw) / 2;
    let ty = area.y + (area.height - style.cap_height) / 2;
    draw_text_color(canvas, text, tx, ty, &style, fg);
}

pub fn draw_radio(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, radius: i32, selected: bool) {
    if selected {
        draw_dot(canvas, center_x, center_y, radius, ACCENT_LINE, 255);
        draw_dot(canvas, center_x, center_y, radius / 2, WHITE, 255);
    } else {
        draw_dot(canvas, center_x, center_y, radius, PANEL_BORDER, 255);
        draw_dot(canvas, center_x, center_y, radius - 2, PANEL_FILL, 255);
    }
}

pub const CURSOR_WIDTH: usize = 11;
pub const CURSOR_HEIGHT: usize = 16;
const CURSOR_PIXELS: [u8; CURSOR_WIDTH * CURSOR_HEIGHT] = [
    1,0,0,0,0,0,0,0,0,0,0,
    1,1,0,0,0,0,0,0,0,0,0,
    1,2,1,0,0,0,0,0,0,0,0,
    1,2,2,1,0,0,0,0,0,0,0,
    1,2,2,2,1,0,0,0,0,0,0,
    1,2,2,2,2,1,0,0,0,0,0,
    1,2,2,2,2,2,1,0,0,0,0,
    1,2,2,2,2,2,2,1,0,0,0,
    1,2,2,2,2,1,1,1,1,0,0,
    1,2,1,2,2,1,0,0,0,0,0,
    1,1,0,1,2,2,1,0,0,0,0,
    1,0,0,0,1,2,2,1,0,0,0,
    0,0,0,0,1,2,2,1,0,0,0,
    0,0,0,0,0,1,2,1,0,0,0,
    0,0,0,0,0,1,2,1,0,0,0,
    0,0,0,0,0,0,1,0,0,0,0,
];

pub fn draw_cursor(canvas: &mut Canvas<'_>, x: i32, y: i32) {
    for row in 0..CURSOR_HEIGHT {
        for col in 0..CURSOR_WIDTH {
            let px = x + col as i32;
            let py = y + row as i32;
            let val = CURSOR_PIXELS[row * CURSOR_WIDTH + col];
            if val == 1 {
                canvas.blend_pixel(px, py, rgb(15, 23, 42), 255);
            } else if val == 2 {
                canvas.blend_pixel(px, py, WHITE, 255);
            }
        }
    }
}

pub fn draw_button(
    canvas: &mut Canvas<'_>,
    area: Rect,
    text: &str,
    is_hovered: bool,
    is_primary: bool,
) {
    let radius = area.height / 3;
    if is_primary {
        let bg = if is_hovered { ACCENT_DARK } else { ACCENT_LINE };
        draw_panel(canvas, area, radius, bg, 255, bg, 0);
        let style = text_style(area.height * 42 / 100, 30);
        let tw = text_width(text, &style);
        let tx = area.x + (area.width - tw) / 2;
        let ty = area.y + (area.height - style.cap_height) / 2;
        draw_text_color(canvas, text, tx, ty, &style, WHITE);
    } else {
        let (bg, border) = if is_hovered {
            (rgb(241, 245, 249), rgb(203, 213, 225))
        } else {
            (PANEL_FILL, PANEL_BORDER)
        };
        draw_panel(canvas, area, radius, bg, 255, border, 255);
        let style = text_style(area.height * 42 / 100, 30);
        let tw = text_width(text, &style);
        let tx = area.x + (area.width - tw) / 2;
        let ty = area.y + (area.height - style.cap_height) / 2;
        draw_text_color(canvas, text, tx, ty, &style, TEXT_PRIMARY);
    }
}

pub fn draw_accent_button(canvas: &mut Canvas<'_>, area: Rect, text: &str, is_hovered: bool, accent: u32) {
    let radius = area.height / 3;
    let bg = if is_hovered { lerp_color(accent, rgb(0, 0, 0), 40) } else { accent };
    draw_panel(canvas, area, radius, bg, 255, bg, 0);
    let style = text_style(area.height * 42 / 100, 30);
    let tw = text_width(text, &style);
    let tx = area.x + (area.width - tw) / 2;
    let ty = area.y + (area.height - style.cap_height) / 2;
    draw_text_color(canvas, text, tx, ty, &style, WHITE);
}

pub fn draw_progress_bar(canvas: &mut Canvas<'_>, area: Rect, progress_percent: u32, accent: u32) {
    let radius = area.height / 3;
    draw_panel(canvas, area, radius, rgb(241, 245, 249), 255, PANEL_BORDER, 255);
    let fill_w = (area.width as u64 * progress_percent.min(100) as u64 / 100) as i32;
    if fill_w > radius {
        let fill_rect = Rect::new(area.x, area.y, fill_w, area.height);
        draw_panel(canvas, fill_rect, radius, accent, 255, accent, 0);
    }
}

pub fn draw_input_box(canvas: &mut Canvas<'_>, area: Rect, text: &str, focused: bool) {
    let radius = area.height / 4;
    let border = if focused { ACCENT_LINE } else { PANEL_BORDER };
    draw_panel(canvas, area, radius, PANEL_FILL, 255, border, 255);
    let style = text_style(area.height * 45 / 100, 30);
    let text_y = area.y + (area.height - style.cap_height) / 2;
    draw_text_color(canvas, text, area.x + area.height / 3, text_y, &style, TEXT_PRIMARY);
}
