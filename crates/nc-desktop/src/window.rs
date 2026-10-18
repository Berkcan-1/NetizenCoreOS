use nc_app::{draw_icon, AppInfo};
use nc_gfx::{Canvas, Rect};
use nc_ui::{draw_text_color, text_style, WHITE};

use crate::icons::{
    draw_tile, draw_win_glyph_close, draw_win_glyph_max, draw_win_glyph_min,
};
use crate::layout::{
    border, button_group, close_button, content_area, max_button, min_button, title_height,
    window_radius,
};
use crate::motion::{clamp01, lerp, scale_u32, ONE};
use crate::paint::{blur_rr_mask, fade_text_color, fill_rr, fill_rr_vgrad, glass_surface, stroke_rr};
use crate::theme::{
    hash_str, tile_colors, window_glass, CLOSE_RED, CLOSE_RED_TOP, INK, INK_MUTED, INK_SOFT, SNOW,
};
use crate::Target;

pub fn draw_frame(
    canvas: &mut Canvas<'_>,
    info: &AppInfo,
    index: usize,
    hover: Target,
    accent: u32,
    win: Rect,
    focused: bool,
    max_t: i32,
    appear: i32,
    blur: usize,
    passes: usize,
    unit: i32,
) {
    if appear <= 0 {
        return;
    }
    let r = lerp(window_radius(unit), 0, max_t);
    let title = title_height(unit);
    let b = border(unit);
    let flat = ONE - clamp01(max_t);

    if blur > 0 && appear > ONE / 4 {
        blur_rr_mask(
            canvas,
            Rect::new(win.x, win.y, win.width, title),
            r,
            blur,
            passes,
            false,
        );
    }

    let client = content_area(win, unit);
    let mut glass = window_glass(focused, 0, 0);
    glass.shadow = scale_u32(glass.shadow, flat);
    if appear >= ONE {
        glass.hole = Some(client);
    }
    glass_surface(canvas, win, r, &glass, appear);

    if appear >= ONE {
        canvas.fill_rect(client.x, client.y, client.width, client.height, SNOW);
    } else {
        fill_rr(canvas, client, 0, SNOW, scale_u32(255, appear));
    }
    stroke_rr(
        canvas,
        Rect::new(client.x - 1, client.y - 1, client.width + 2, client.height + 2),
        0,
        1,
        INK,
        scale_u32(if focused { 46 } else { 30 }, appear),
    );

    if focused {
        let glint_w = (win.width * 36 / 100).max(24);
        let gx = win.x + (win.width - glint_w) / 2;
        let ga = scale_u32(170, appear);
        fill_rr(canvas, Rect::new(gx, win.y + 1, glint_w, 2), 1, accent, ga);
    }

    let (top, bottom) = tile_colors(hash_str(&info.id));
    let tile_size = (title * 62 / 100).max(10);
    let tile = Rect::new(
        win.x + b + unit / 2,
        win.y + (title - tile_size) / 2,
        tile_size,
        tile_size,
    );
    let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
        draw_icon(c, info.icon, x, y, s, col)
    };
    draw_tile(canvas, tile, top, bottom, appear, 0, &glyph, 58);

    let style = text_style((unit * 58 / 100).max(10), 18);
    let tx = tile.x + tile.width + unit * 2 / 3;
    let ty = win.y + (title - style.cap_height) / 2;
    let ink = if focused { INK } else { INK_MUTED };
    let color = fade_text_color(canvas, tx, ty, ink, appear);
    draw_text_color(canvas, &info.name, tx, ty, &style, color);

    let group = button_group(win, unit);
    let gh = group.height;
    fill_rr(canvas, group, gh / 2, WHITE, scale_u32(if focused { 96 } else { 64 }, appear));
    stroke_rr(canvas, group, gh / 2, 1, WHITE, scale_u32(170, appear));

    let mn = min_button(win, unit);
    let mx = max_button(win, unit);
    let cl = close_button(win, unit);
    let sep_a = scale_u32(34, appear);
    fill_rr(canvas, Rect::new(mx.x, group.y + gh / 5, 1, gh * 3 / 5), 0, INK, sep_a);
    fill_rr(canvas, Rect::new(cl.x, group.y + gh / 5, 1, gh * 3 / 5), 0, INK, sep_a);

    let inset = 2;
    if hover == Target::WinMin(index) {
        fill_rr(
            canvas,
            Rect::new(mn.x + inset, mn.y + inset, mn.width - inset * 2, mn.height - inset * 2),
            gh / 2 - inset,
            WHITE,
            scale_u32(150, appear),
        );
    }
    if hover == Target::WinMax(index) {
        fill_rr(
            canvas,
            Rect::new(mx.x + 1, mx.y + inset, mx.width - 2, mx.height - inset * 2),
            gh / 3,
            WHITE,
            scale_u32(150, appear),
        );
    }
    let close_hot = hover == Target::WinClose(index);
    if close_hot {
        let rect = Rect::new(cl.x + 1, cl.y + inset, cl.width - inset - 1, cl.height - inset * 2);
        fill_rr_vgrad(canvas, rect, gh / 2 - inset, CLOSE_RED_TOP, CLOSE_RED, scale_u32(255, appear));
        stroke_rr(canvas, rect, gh / 2 - inset, 1, WHITE, scale_u32(150, appear));
    }

    let s = (unit * 3 / 10).max(3);
    let glyph_col = if focused { INK_SOFT } else { INK_MUTED };
    draw_win_glyph_min(canvas, mn.x + mn.width / 2, mn.y + mn.height / 2, s, glyph_col);
    draw_win_glyph_max(
        canvas,
        mx.x + mx.width / 2,
        mx.y + mx.height / 2,
        s,
        glyph_col,
        max_t > ONE / 2,
    );
    draw_win_glyph_close(
        canvas,
        cl.x + cl.width / 2,
        cl.y + cl.height / 2,
        s,
        if close_hot { WHITE } else { glyph_col },
    );
}
