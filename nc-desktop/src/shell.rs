use nc_app::draw_icon;
use nc_app::text::{shadowed, Line};
use nc_font::text_width;
use nc_gfx::{Canvas, Rect};
use nc_ui::{draw_text_color, text_style, WHITE};

use crate::icons::{
    draw_doc_glyph, draw_dock_tile, draw_folder_glyph, draw_power_glyph, draw_reboot_glyph,
    draw_start_logo, draw_tile, draw_wifi, tile_radius,
};
use crate::layout::{
    context_item_rect, context_menu_rect, desktop_modal_rect, dock_bottom_gap, dock_height,
    dock_hint_rect, dock_pad, icon_tile, modal_buttons, start_cell, start_dot_rect, start_layout,
    start_tile, task_manager_action_btn, task_manager_close_rect, task_manager_rect,
    task_manager_row_rect, title_height, volume_osd_rect,
};

use crate::motion::{
    clamp01, ease_out_back, ease_out_cubic, lerp, scale_rect_about, scale_u32, staggered, ONE,
};
use crate::paint::{
    draw_line, fade_text_color, fill_rr, fill_rr_vgrad, fit_text, glass_surface, shadow_rr,
    stroke_rr,
};
use crate::theme::{
    dock_glass, hash_str, lighten, menu_glass, modal_glass, start_glass, tile_colors, BLACK,
    CLOSE_RED, CLOSE_RED_TOP, DANGER, FROST, INK, INK_MUTED, INK_SOFT, SNOW, WARN,
};
use crate::{ContextMenuKind, DesktopIconId, DesktopModal, DesktopState, Target};
use nc_gfx::rgb;

fn text_h(unit: i32, pct: i32) -> i32 {
    (unit * pct / 100).max(9)
}

fn draw_label(
    canvas: &mut Canvas<'_>,
    name: &str,
    cell: Rect,
    y: i32,
    unit: i32,
    selected: bool,
    accent: u32,
) {
    let style = text_style(text_h(unit, 46), 14);
    let max_w = cell.width - unit / 2;
    let text = fit_text(name, max_w, &|t| text_width(t, &style));
    let w = text_width(&text, &style);
    let x = cell.x + (cell.width - w) / 2;
    if selected {
        let plate = Rect::new(x - unit / 3, y - 2, w + unit * 2 / 3, style.cap_height + 6);
        fill_rr(canvas, plate, plate.height / 2, accent, 215);
    }
    shadowed(canvas, &text, x, y, &style, WHITE, BLACK);
}

fn draw_icon_cell(
    canvas: &mut Canvas<'_>,
    state: &DesktopState,
    id: DesktopIconId,
    cell: Rect,
    unit: i32,
    accent: u32,
    lifted: bool,
) {
    let selected = state.selected_icon == Some(id);
    let hover = match id {
        DesktopIconId::App(i) => state.hover == Target::AppIcon(i),
        DesktopIconId::Fs(i) => state.hover == Target::FsIcon(i),
    };
    if selected {
        fill_rr(canvas, cell, unit / 2, WHITE, 58);
        stroke_rr(canvas, cell, unit / 2, 1, WHITE, 170);
    } else if hover {
        fill_rr(canvas, cell, unit / 2, WHITE, 34);
        stroke_rr(canvas, cell, unit / 2, 1, WHITE, 90);
    }
    let mut tile = icon_tile(cell, unit);
    let lift = if lifted { unit / 3 } else { 0 };
    if lifted {
        let grow = unit / 5;
        tile = Rect::new(tile.x - grow, tile.y - grow - lift, tile.width + grow * 2, tile.height + grow * 2);
    }
    let label_y = icon_tile(cell, unit).y + icon_tile(cell, unit).height + unit / 3;
    match id {
        DesktopIconId::App(i) => {
            let info = state.apps[i].app.info();
            let (top, bottom) = tile_colors(hash_str(&info.id));
            let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
                draw_icon(c, info.icon, x, y, s, col)
            };
            draw_tile(canvas, tile, top, bottom, ONE, lift, &glyph, 56);
            let name: &str = &info.name;
            draw_label(canvas, name, cell, label_y, unit, selected, accent);
        }
        DesktopIconId::Fs(i) => {
            let item = &state.fs_items[i];
            if item.is_dir {
                let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
                    draw_folder_glyph(c, x, y, s, col)
                };
                draw_tile(canvas, tile, rgb(255, 214, 112), rgb(246, 154, 36), ONE, lift, &glyph, 58);
            } else {
                let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
                    draw_doc_glyph(c, x, y, s, col, rgb(120, 142, 190))
                };
                draw_tile(canvas, tile, rgb(168, 188, 222), rgb(84, 106, 150), ONE, lift, &glyph, 58);
            }
            let name: &str = &item.name;
            draw_label(canvas, name, cell, label_y, unit, selected, accent);
        }
    }
}

pub fn draw_icons(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    let accent = state.settings.accent_color();
    if let Some(g) = state.drag_ghost(width, height, unit) {
        let r = Rect::new(g.x + 3, g.y + 3, g.width - 6, g.height - 6);
        fill_rr(canvas, r, unit / 2, WHITE, 34);
        stroke_rr(canvas, r, unit / 2, 1, WHITE, 130);
    }
    let rects = state.icon_rects(width, height, unit);
    let dragging = state.dragging_icon();
    for (id, cell) in rects.iter() {
        if Some(*id) == dragging {
            continue;
        }
        draw_icon_cell(canvas, state, *id, *cell, unit, accent, false);
    }
    for (id, cell) in rects.iter() {
        if Some(*id) == dragging {
            draw_icon_cell(canvas, state, *id, *cell, unit, accent, true);
        }
    }
}

fn draw_tooltip(canvas: &mut Canvas<'_>, text: &str, cx: i32, anchor_top: i32, width: i32, unit: i32) {
    let style = text_style(text_h(unit, 46), 15);
    let tw = text_width(text, &style);
    let w = tw + unit * 3 / 2;
    let h = style.cap_height + unit * 3 / 4 + 2;
    let x = (cx - w / 2).clamp(unit / 2, (width - w - unit / 2).max(unit / 2));
    let y = anchor_top - h - unit / 4;
    let r = Rect::new(x, y, w, h);
    shadow_rr(canvas, r, h / 2, 3, unit / 2, rgb(8, 14, 40), 90);
    fill_rr(canvas, r, h / 2, SNOW, 232);
    stroke_rr(canvas, r, h / 2, 1, WHITE, 200);
    draw_text_color(canvas, text, x + (w - tw) / 2, y + (h - style.cap_height) / 2, &style, INK);
}

fn draw_dock_chevron(canvas: &mut Canvas<'_>, rect: Rect, right: bool, color: u32, alpha: u32) {
    let cx = rect.x + rect.width / 2;
    let cy = rect.y + rect.height / 2;
    let arm = (rect.width / 5).max(2);
    let half = (rect.width / 4).max(2);
    let (x0, x1) = if right { (cx - arm, cx + arm) } else { (cx + arm, cx - arm) };
    draw_line(canvas, x0, cy - half, x1, cy, 24, color, alpha);
    draw_line(canvas, x1, cy, x0, cy + half, 24, color, alpha);
}

fn draw_dock_page_items<'a>(
    canvas: &mut Canvas<'_>,
    state: &'a DesktopState,
    items: &[usize],
    layout: &crate::layout::DockLayout,
    unit: i32,
    offset_x: i32,
    appear: i32,
    show_indicators: bool,
) -> Option<(i32, i32, &'a str)> {
    if appear <= 0 {
        return None;
    }
    let accent = state.settings.accent_color();
    let focused = state.focused();
    let mut hover_label = None;
    for (pos, base_rect) in layout.tiles.iter().enumerate() {
        let Some(&app_idx) = items.get(layout.first_index + pos) else {
            continue;
        };
        let slot = &state.apps[app_idx];
        let info = slot.app.info();
        let mut rect = *base_rect;
        rect.x += offset_x;
        rect.y -= state.bounce_offset(app_idx, unit);
        let (top, bottom) = tile_colors(hash_str(&info.id));
        let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
            draw_icon(c, info.icon, x, y, s, col)
        };
        draw_dock_tile(canvas, rect, top, bottom, appear, 0, &glyph, 56);

        if show_indicators && slot.window.open {
            let is_focus = focused == Some(app_idx);
            let cy = layout.dock.y + layout.dock.height - dock_pad(unit) / 2;
            let cx = rect.x + rect.width / 2;
            let w = if is_focus { (unit * 2 / 3).max(8) } else { (unit / 5).max(4) };
            let hh = (unit / 8).max(3);
            let col = if is_focus { accent } else { WHITE };
            fill_rr(
                canvas,
                Rect::new(cx - w / 2, cy - hh / 2, w, hh),
                hh / 2,
                col,
                scale_u32(240, appear),
            );
        }

        if show_indicators && state.hover == Target::DockApp(app_idx) {
            let name: &str = &info.name;
            hover_label = Some((rect.x + rect.width / 2, rect.y, name));
        }
    }
    hover_label
}

pub fn draw_dock(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    let items = state.dock_indices();
    let dl = state.dock_layout(width, height, unit);
    if state.dock_reveal < ONE / 12 {
        if state.settings.dock_auto_hide {
            let hint = dock_hint_rect(width, height, unit);
            fill_rr(canvas, hint, hint.height / 2, WHITE, 170);
        }
        return;
    }
    let reveal = ease_out_cubic(state.dock_reveal);
    let (blur, passes) = state.blur_params(unit);
    let full_e = crate::motion::ease_in_out_cubic(state.dock_full_t());
    let radius = lerp(tile_radius(dl.tile) + dock_pad(unit), 0, full_e);
    glass_surface(canvas, dl.dock, radius, &dock_glass(blur, passes), reveal);

    let sep_h = dl.tile * 6 / 10;
    let sep_y = dl.dock.y + (dl.dock.height - sep_h) / 2;
    let sep_a = scale_u32(100, reveal);
    fill_rr(canvas, Rect::new(dl.sep_a, sep_y, 2, sep_h), 1, WHITE, sep_a);
    fill_rr(canvas, Rect::new(dl.sep_b, sep_y, 2, sep_h), 1, WHITE, sep_a);

    let accent = state.settings.accent_color();
    let start_active = state.start_open || state.hover == Target::StartButton;
    let start_top = lighten(accent, if start_active { 110 } else { 70 });
    let start_bot = if start_active { accent } else { crate::theme::darken(accent, 20) };
    let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, _col: u32| draw_start_logo(c, x, y, s);
    let start_rect = if start_active {
        let g = dl.start.width / 14;
        Rect::new(dl.start.x - g, dl.start.y - g * 2, dl.start.width + g * 2, dl.start.height + g * 2)
    } else {
        dl.start
    };
    draw_tile(canvas, start_rect, start_top, start_bot, reveal, 0, &glyph, 66);

    let mut hover_label = None;
    let transitioning = dl.page_count > 1 && state.dock_page_anim < ONE;
    let clip = Rect::new(dl.app_viewport.x, 0, dl.app_viewport.width.max(1), height);
    canvas.set_clip(clip);
    if transitioning {
        let old_layout = state.dock_layout_for_page(width, height, unit, state.dock_page_from);
        let t = crate::motion::ease_in_out_cubic(state.dock_page_anim);
        let shift = (dl.app_viewport.width.max(dl.tile) as i64 * t as i64 / ONE as i64) as i32;
        let old_offset = if state.dock_page_direction >= 0 { -shift } else { shift };
        let new_offset = if state.dock_page_direction >= 0 {
            dl.app_viewport.width.max(dl.tile) - shift
        } else {
            -(dl.app_viewport.width.max(dl.tile) - shift)
        };
        let _ = draw_dock_page_items(
            canvas, state, &items, &old_layout, unit, old_offset, ONE - t, false,
        );
        hover_label = draw_dock_page_items(
            canvas, state, &items, &dl, unit, new_offset, t, true,
        );
    } else {
        hover_label = draw_dock_page_items(canvas, state, &items, &dl, unit, 0, reveal, true);
    }
    canvas.reset_clip();

    for (rect, is_right, target) in [
        (dl.prev, false, Target::DockPrev),
        (dl.next, true, Target::DockNext),
    ] {
        if let Some(r) = rect {
            let hovered = state.hover == target;
            fill_rr(
                canvas,
                r,
                r.width / 3,
                if hovered { WHITE } else { FROST },
                scale_u32(if hovered { 220 } else { 150 }, reveal),
            );
            stroke_rr(canvas, r, r.width / 3, 1, WHITE, scale_u32(210, reveal));
            draw_dock_chevron(canvas, r, is_right, INK_SOFT, scale_u32(220, reveal));
        }
    }

    if state.hover == Target::StartButton {
        hover_label = Some((dl.start.x + dl.start.width / 2, start_rect.y, "Baslat"));
    }

    // Keep the status readout inside the dock's main glass surface. A second
    // capsule here makes the right side look like a separate, mismatched bar.
    let st = dl.status;
    let style = text_style(text_h(unit, 48), 15);
    let secs = state.uptime_ms / 1000;
    let mut clock = Line::new();
    clock.number(secs / 60 % 60, 2).text(":").number(secs % 60, 2);
    let tw = text_width(clock.as_str(), &style);
    let ty = st.y + (st.height - style.cap_height) / 2;
    let wide = st.width >= unit * 6;
    let tx = if wide { st.x + st.width - tw - unit * 3 / 4 } else { st.x + (st.width - tw) / 2 };
    if wide {
        let gh = unit * 6 / 5;
        draw_wifi(
            canvas,
            st.x + unit * 3 / 4,
            st.y + st.height / 2 + gh / 2,
            gh,
            WHITE,
            state.settings.wifi_enabled,
        );
    }
    shadowed(canvas, clock.as_str(), tx, ty, &style, WHITE, rgb(5, 38, 58));

    if let Some((cx, top, text)) = hover_label {
        if !transitioning && state.dock_reveal >= ONE * 3 / 4 {
            draw_tooltip(canvas, text, cx, top, width, unit);
        }
    }
}

pub fn draw_start_menu(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    let t = state.start_anim;
    if t <= 0 {
        return;
    }
    let e = ease_out_cubic(t);
    let apps = state.start_indices();
    let count = apps.len();
    let dock_top = height - dock_bottom_gap(unit) - dock_height(unit);
    let sl = start_layout(width, height, unit, dock_top, count);
    let scale = lerp(ONE * 93 / 100, ONE, e);
    let ax = sl.panel.x + sl.panel.width / 2;
    let ay = sl.panel.y + sl.panel.height + unit;
    let xf = |r: Rect| -> Rect {
        let (x, y, w, h) = scale_rect_about(r.x, r.y, r.width, r.height, scale, ax, ay);
        Rect::new(x, y, w, h)
    };
    let (blur, passes) = state.blur_params(unit);
    glass_surface(canvas, xf(sl.panel), unit * 6 / 5, &start_glass(blur, passes), e);
    if e < ONE / 2 {
        return;
    }
    let content = clamp01((e - ONE / 2) * 2);
    let accent = state.settings.accent_color();
    let shadow = rgb(10, 20, 50);

    let title_style = text_style(text_h(unit, 72), 16);
    let sub_style = text_style(text_h(unit, 42), 15);
    let logo = xf(Rect::new(sl.panel.x + sl.pad, sl.panel.y + sl.pad - unit / 4, unit * 5 / 2, unit * 5 / 2));
    let logo_glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, _col: u32| draw_start_logo(c, x, y, s);
    draw_tile(canvas, logo, lighten(accent, 70), accent, content, 0, &logo_glyph, 66);
    let tx = logo.x + logo.width + unit * 2 / 3;
    let ty = logo.y + unit / 8;
    let c1 = fade_text_color(canvas, tx, ty, WHITE, content);
    let s1 = fade_text_color(canvas, tx, ty, shadow, content);
    shadowed(canvas, "NetizenCore", tx, ty, &title_style, c1, s1);
    let ty2 = ty + title_style.cap_height + unit / 4;
    let c2 = fade_text_color(canvas, tx, ty2, rgb(232, 242, 255), content);
    shadowed(canvas, "Uygulamalar", tx, ty2, &sub_style, c2, s1);

    for (rect, is_power) in [(sl.reboot, false), (sl.shut, true)] {
        let r = xf(rect);
        let hot = state.hover == if is_power { Target::PowerShutdown } else { Target::PowerReboot };
        if hot {
            let (a, b) = if is_power { (CLOSE_RED_TOP, CLOSE_RED) } else { (lighten(WARN, 60), WARN) };
            fill_rr_vgrad(canvas, r, r.width / 2, a, b, scale_u32(245, content));
        } else {
            fill_rr(canvas, r, r.width / 2, WHITE, scale_u32(56, content));
        }
        stroke_rr(canvas, r, r.width / 2, 1, WHITE, scale_u32(150, content));
        let cx = r.x + r.width / 2;
        let cy = r.y + r.height / 2;
        let gr = (unit * 3 / 8).max(4);
        let ga = scale_u32(255, content);
        if is_power {
            draw_power_glyph(canvas, cx, cy, gr, WHITE, ga);
        } else {
            draw_reboot_glyph(canvas, cx, cy, gr, WHITE, ga);
        }
    }

    let page = state.start_page.min(sl.pages - 1);
    let first = page * sl.per_page;
    let last = (first + sl.per_page).min(count);
    let label_style = text_style(text_h(unit, 46), 15);
    let n = last.saturating_sub(first).max(1);
    for local_idx in 0..(last - first) {
        let app_idx = apps[first + local_idx];
        let cell_raw = start_cell(&sl, local_idx);
        let cell = xf(cell_raw);
        let local = staggered(e, local_idx, n, ONE * 35 / 100);
        if local <= 0 {
            continue;
        }
        let hot = state.hover == Target::StartNode(app_idx);
        if hot {
            let plate = Rect::new(cell.x + 2, cell.y + 2, cell.width - 4, cell.height - 4);
            fill_rr(canvas, plate, unit / 2, WHITE, scale_u32(54, local));
            stroke_rr(canvas, plate, unit / 2, 1, WHITE, scale_u32(130, local));
        }
        let tile_raw = xf(start_tile(cell_raw, unit));
        let target_scale = if hot { ONE * 106 / 100 } else { ONE };
        let sc = lerp(ONE * 78 / 100, target_scale, ease_out_back(local));
        let (nx, ny, nw, nh) = scale_rect_about(
            tile_raw.x,
            tile_raw.y,
            tile_raw.width,
            tile_raw.height,
            sc,
            tile_raw.x + tile_raw.width / 2,
            tile_raw.y + tile_raw.height / 2,
        );
        let tile = Rect::new(nx, ny, nw, nh);
        let info = state.apps[app_idx].app.info();
        let (top, bottom) = tile_colors(hash_str(&info.id));
        let glyph = |c: &mut Canvas<'_>, x: i32, y: i32, s: i32, col: u32| {
            draw_icon(c, info.icon, x, y, s, col)
        };
        draw_tile(canvas, tile, top, bottom, local, 0, &glyph, 56);

        let name: &str = &info.name;
        let text = fit_text(name, cell.width - unit / 2, &|s| text_width(s, &label_style));
        let w = text_width(&text, &label_style);
        let lx = cell.x + (cell.width - w) / 2;
        let ly = tile_raw.y + tile_raw.height + unit / 3;
        let col = fade_text_color(canvas, lx, ly, WHITE, local);
        let sh = fade_text_color(canvas, lx, ly, shadow, local);
        shadowed(canvas, &text, lx, ly, &label_style, col, sh);
    }

    if sl.pages > 1 {
        for p in 0..sl.pages {
            let d = xf(start_dot_rect(&sl, p, unit));
            let r = if p == page { (unit / 5).max(3) } else { (unit / 8).max(2) };
            let cx = d.x + d.width / 2;
            let cy = d.y + d.height / 2;
            let a = if p == page { 240 } else { 120 };
            fill_rr(canvas, Rect::new(cx - r, cy - r, r * 2, r * 2), r, WHITE, scale_u32(a, content));
        }
    }
}

pub fn draw_context_menu(
    canvas: &mut Canvas<'_>,
    state: &DesktopState,
    pos: (i32, i32),
    kind: ContextMenuKind,
    width: i32,
    height: i32,
    unit: i32,
) {
    let t = state.ctx_anim;
    if t <= 0 {
        return;
    }
    let e = ease_out_cubic(t);
    let count = state.context_menu_item_count(kind);
    let menu = context_menu_rect(pos.0, pos.1, count, width, height, unit);
    let ax = pos.0.clamp(menu.x, menu.x + menu.width);
    let ay = pos.1.clamp(menu.y, menu.y + menu.height);
    let scale = lerp(ONE * 90 / 100, ONE, e);
    let xf = |r: Rect| -> Rect {
        let (x, y, w, h) = scale_rect_about(r.x, r.y, r.width, r.height, scale, ax, ay);
        Rect::new(x, y, w, h)
    };
    let (blur, passes) = state.blur_params(unit);
    glass_surface(canvas, xf(menu), unit * 2 / 3, &menu_glass(blur, passes), e);
    if e < ONE / 2 {
        return;
    }
    let content = clamp01((e - ONE / 2) * 2);
    let accent = state.settings.accent_color();
    let style = text_style(text_h(unit, 50), 15);
    let chip_style = text_style(text_h(unit, 38), 16);

    for idx in 0..count {
        let item = xf(context_item_rect(menu, idx, unit));
        let hot = state.hover == Target::ContextItem(idx);
        let (label, badge, dot) = state.context_menu_entry(kind, idx);
        if hot {
            fill_rr_vgrad(canvas, item, unit / 3, lighten(accent, 60), accent, scale_u32(235, content));
        }
        let chip = (unit * 11 / 10).min(item.height - 6);
        let cr = Rect::new(item.x + unit / 3, item.y + (item.height - chip) / 2, chip, chip);
        fill_rr_vgrad(canvas, cr, chip / 4, lighten(dot, 70), dot, scale_u32(255, content));
        if let Some(letter) = badge.chars().next() {
            let mut buf = [0u8; 4];
            let s: &str = letter.encode_utf8(&mut buf);
            let lw = text_width(s, &chip_style);
            let lx = cr.x + (chip - lw) / 2;
            let ly = cr.y + (chip - chip_style.cap_height) / 2;
            let col = fade_text_color(canvas, lx, ly, WHITE, content);
            draw_text_color(canvas, s, lx, ly, &chip_style, col);
        } else {
            let d = chip / 3;
            fill_rr(canvas, Rect::new(cr.x + (chip - d) / 2, cr.y + (chip - d) / 2, d, d), d / 2, WHITE, scale_u32(235, content));
        }
        let tx = cr.x + chip + unit / 2;
        let ty = item.y + (item.height - style.cap_height) / 2;
        let base = if hot { WHITE } else { INK };
        let col = fade_text_color(canvas, tx, ty, base, content);
        draw_text_color(canvas, label, tx, ty, &style, col);
    }
}

pub fn draw_task_manager(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    if !state.task_mgr.open {
        return;
    }
    let tm = task_manager_rect(state.task_mgr.x, state.task_mgr.y, width, height, unit);
    let (blur, passes) = state.blur_params(unit);
    glass_surface(canvas, tm, unit * 3 / 4, &modal_glass(blur, passes), ONE);
    let accent = state.settings.accent_color();

    let title_h = title_height(unit);
    let title_st = text_style(text_h(unit, 58), 16);
    let label_st = text_style(text_h(unit, 50), 15);
    let cap_st = text_style(text_h(unit, 44), 16);

    fill_rr(canvas, Rect::new(tm.x + unit, tm.y + title_h / 2 - 4, 8, 8), 4, accent, 255);
    draw_text_color(
        canvas,
        "Gorev Yoneticisi ve Arka Plan Islemleri",
        tm.x + unit * 5 / 2,
        tm.y + (title_h - title_st.cap_height) / 2,
        &title_st,
        INK,
    );

    let close_r = task_manager_close_rect(tm, unit);
    let close_hot = state.hover == Target::TaskMgrClose;
    if close_hot {
        fill_rr_vgrad(canvas, close_r, close_r.height / 2, CLOSE_RED_TOP, CLOSE_RED, 255);
    } else {
        fill_rr(canvas, close_r, close_r.height / 2, WHITE, 120);
    }
    crate::icons::draw_win_glyph_close(
        canvas,
        close_r.x + close_r.width / 2,
        close_r.y + close_r.height / 2,
        (unit * 3 / 10).max(3),
        if close_hot { WHITE } else { INK_SOFT },
    );

    let graph_card = Rect::new(tm.x + unit, tm.y + title_h + unit / 4, tm.width - unit * 2, unit * 11 / 2);
    fill_rr(canvas, graph_card, unit / 3, WHITE, 190);
    stroke_rr(canvas, graph_card, unit / 3, 1, WHITE, 230);

    let mut m_line = Line::new();
    m_line
        .text("Toplam CPU Yuku: %")
        .number(state.total_cpu_percent(), 1)
        .text("    |    Bellek Kullanimi: ")
        .number(state.used_ram_mb(), 1)
        .text(" / ")
        .number(state.total_memory_mb, 1)
        .text(" MB");
    draw_text_color(canvas, m_line.as_str(), graph_card.x + unit * 3 / 4, graph_card.y + unit / 2, &label_st, INK_SOFT);

    let bars_x = graph_card.x + unit * 3 / 4;
    let bars_y = graph_card.y + unit * 2;
    let bars_w = graph_card.width - unit * 3 / 2;
    let bars_h = unit * 3;
    fill_rr(canvas, Rect::new(bars_x, bars_y, bars_w, bars_h), unit / 4, FROST, 255);
    let step_w = (bars_w / 32).max(2);
    for i in 0..32usize {
        let hist_idx = (state.task_mgr.hist_cursor + i) % 32;
        let val = state.task_mgr.cpu_history[hist_idx].min(100) as i32;
        let bh = (bars_h * val / 100).max(3);
        let bx = bars_x + (i as i32) * step_w;
        let by = bars_y + bars_h - bh;
        let (a, b) = if val > 70 { (lighten(WARN, 60), WARN) } else { (lighten(accent, 80), accent) };
        fill_rr_vgrad(canvas, Rect::new(bx + 1, by, (step_w - 2).max(1), bh), (step_w / 3).max(1), a, b, 255);
    }

    let hdr_y = tm.y + unit * 8 + unit / 5;
    draw_text_color(canvas, "Islem Adi", tm.x + unit * 3 / 2, hdr_y, &cap_st, INK_MUTED);
    draw_text_color(canvas, "PID", tm.x + unit * 15, hdr_y, &cap_st, INK_MUTED);
    draw_text_color(canvas, "Durum", tm.x + unit * 19, hdr_y, &cap_st, INK_MUTED);
    draw_text_color(canvas, "CPU", tm.x + unit * 26, hdr_y, &cap_st, INK_MUTED);
    draw_text_color(canvas, "Bellek", tm.x + unit * 29, hdr_y, &cap_st, INK_MUTED);

    let rows = state.collect_task_rows();
    for (idx, row) in rows.iter().take(7).enumerate() {
        let rr = task_manager_row_rect(tm, idx, unit);
        let sel = state.task_mgr.selected_row == idx;
        let hov = state.hover == Target::TaskMgrRow(idx);
        if sel {
            fill_rr(canvas, rr, unit / 4, accent, 60);
            stroke_rr(canvas, rr, unit / 4, 1, accent, 200);
        } else if hov {
            fill_rr(canvas, rr, unit / 4, WHITE, 150);
        } else {
            fill_rr(canvas, rr, unit / 4, WHITE, 90);
        }
        let dot_c = if row.is_app { accent } else { rgb(40, 190, 150) };
        fill_rr(canvas, Rect::new(rr.x + unit / 2 - 3, rr.y + rr.height / 2 - 3, 6, 6), 3, dot_c, 255);
        let ty = rr.y + (rr.height - cap_st.cap_height) / 2;
        draw_text_color(canvas, row.name, rr.x + unit, ty, &cap_st, INK);
        let mut pid = Line::new();
        pid.number(row.pid, 1);
        draw_text_color(canvas, pid.as_str(), tm.x + unit * 15, ty, &cap_st, INK_SOFT);
        draw_text_color(canvas, row.status, tm.x + unit * 19, ty, &cap_st, dot_c);
        let mut cpu = Line::new();
        cpu.text("%").number(row.cpu_pct as u32, 1);
        draw_text_color(canvas, cpu.as_str(), tm.x + unit * 26, ty, &cap_st, INK);
        let mut mem = Line::new();
        mem.number(row.mem_mb as u32, 1).text(" MB");
        draw_text_color(canvas, mem.as_str(), tm.x + unit * 29, ty, &cap_st, INK_SOFT);
    }

    let opt = task_manager_action_btn(tm, false, unit);
    let kill = task_manager_action_btn(tm, true, unit);
    let opt_hot = state.hover == Target::TaskMgrOptimize;
    fill_rr(canvas, opt, opt.height / 2, WHITE, if opt_hot { 255 } else { 190 });
    stroke_rr(canvas, opt, opt.height / 2, 1, accent, 200);
    let ow = text_width("Arka Plani Dengele", &cap_st);
    draw_text_color(
        canvas,
        "Arka Plani Dengele",
        opt.x + (opt.width - ow) / 2,
        opt.y + (opt.height - cap_st.cap_height) / 2,
        &cap_st,
        INK,
    );
    let kill_hot = state.hover == Target::TaskMgrKill;
    fill_rr_vgrad(
        canvas,
        kill,
        kill.height / 2,
        if kill_hot { CLOSE_RED_TOP } else { lighten(DANGER, 50) },
        if kill_hot { CLOSE_RED } else { DANGER },
        255,
    );
    stroke_rr(canvas, kill, kill.height / 2, 1, WHITE, 120);
    let kw = text_width("Gorevi Sonlandir", &cap_st);
    draw_text_color(
        canvas,
        "Gorevi Sonlandir",
        kill.x + (kill.width - kw) / 2,
        kill.y + (kill.height - cap_st.cap_height) / 2,
        &cap_st,
        WHITE,
    );
}

pub fn draw_desktop_modal(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    let (title, subtitle, text_val, tall) = match state.modal {
        DesktopModal::None => return,
        DesktopModal::CreateFolder { ref name_buf } => (
            "Masaustunde Yeni Klasor Olustur",
            "Klasor adini yazip Enter veya Kaydet'e basin",
            name_buf.as_str(),
            false,
        ),
        DesktopModal::CreateTextFile { ref name_buf } => (
            "Masaustunde Yeni Metin Belgesi",
            "Dosya adini yazin (Ornek: notlarim.txt)",
            name_buf.as_str(),
            false,
        ),
        DesktopModal::RenameItem { ref old_name, ref new_buf, .. } => (
            "Masaustu Ogesini Yeniden Adlandir",
            old_name.as_str(),
            new_buf.as_str(),
            false,
        ),
        DesktopModal::EditTextFile { ref name, ref content } => (
            "Masaustu Hizli Not Duzenleyici",
            name.as_str(),
            content.as_str(),
            true,
        ),
    };
    let t = state.modal_anim;
    if t <= 0 {
        return;
    }
    let e = ease_out_cubic(t);
    let m_rect = desktop_modal_rect(width, height, unit, tall);
    let scale = lerp(ONE * 94 / 100, ONE, e);
    let cx = m_rect.x + m_rect.width / 2;
    let cy = m_rect.y + m_rect.height / 2;
    let (sx, sy, sw, sh) = scale_rect_about(m_rect.x, m_rect.y, m_rect.width, m_rect.height, scale, cx, cy);
    let (blur, passes) = state.blur_params(unit);
    glass_surface(canvas, Rect::new(sx, sy, sw, sh), unit * 3 / 4, &modal_glass(blur, passes), e);
    if e < ONE * 6 / 10 {
        return;
    }
    let content = clamp01((e - ONE * 6 / 10) * 5 / 2);
    let accent = state.settings.accent_color();

    let title_st = text_style(text_h(unit, 68), 14);
    let sub_st = text_style(text_h(unit, 46), 14);
    let body_st = text_style(text_h(unit, 52), 14);

    let tx = m_rect.x + unit * 2;
    let c_title = fade_text_color(canvas, tx, m_rect.y + unit, INK, content);
    draw_text_color(canvas, title, tx, m_rect.y + unit, &title_st, c_title);
    let c_sub = fade_text_color(canvas, tx, m_rect.y + unit * 5 / 2, accent, content);
    draw_text_color(canvas, subtitle, tx, m_rect.y + unit * 5 / 2, &sub_st, c_sub);

    let box_h = if tall { m_rect.height - unit * 8 } else { unit * 5 / 2 };
    let input_r = Rect::new(m_rect.x + unit * 3 / 2, m_rect.y + unit * 4, m_rect.width - unit * 3, box_h);
    fill_rr(canvas, input_r, unit / 3, WHITE, scale_u32(255, content));
    stroke_rr(canvas, input_r, unit / 3, 2, accent, scale_u32(220, content));

    let prev_clip = canvas.clip_rect();
    let inner_clip = Rect::new(input_r.x + 6, input_r.y + 4, (input_r.width - 12).max(1), (input_r.height - 8).max(1));
    canvas.set_clip(inner_clip);
    let max_line_w = (input_r.width - 20).max(20);
    let caret_on = (state.uptime_ms / 500) % 2 == 0;

    if tall {
        let step_y = (body_st.cap_height + 6).max(14);
        let max_lines = ((input_r.height - 12) / step_y).max(1) as usize;
        let mut wrapped: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
        for raw_line in text_val.split('\n') {
            if raw_line.is_empty() {
                wrapped.push("");
                continue;
            }
            let mut start = 0usize;
            while start < raw_line.len() {
                let rest = &raw_line[start..];
                if text_width(rest, &body_st) <= max_line_w {
                    wrapped.push(rest);
                    break;
                }
                let mut split_at = 0usize;
                for (idx, ch) in rest.char_indices() {
                    let next_idx = idx + ch.len_utf8();
                    if text_width(&rest[..next_idx], &body_st) > max_line_w {
                        break;
                    }
                    split_at = next_idx;
                }
                if split_at == 0 {
                    split_at = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                }
                wrapped.push(&rest[..split_at]);
                start += split_at;
            }
        }
        if wrapped.is_empty() {
            wrapped.push("");
        }
        let total = wrapped.len();
        let skip = total.saturating_sub(max_lines);
        for (i, line) in wrapped.iter().skip(skip).take(max_lines).enumerate() {
            let ly = input_r.y + 8 + (i as i32) * step_y;
            draw_text_color(canvas, line, input_r.x + 10, ly, &body_st, INK);
            if skip + i + 1 == total && caret_on {
                let lw = text_width(line, &body_st).min(max_line_w);
                fill_rr(canvas, Rect::new(input_r.x + 12 + lw, ly - 1, 2, body_st.cap_height + 2), 1, accent, 255);
            }
        }
    } else {
        let ty = input_r.y + (input_r.height - body_st.cap_height) / 2;
        let tw = text_width(text_val, &body_st);
        let draw_x = if tw > max_line_w { input_r.x + 10 - (tw - max_line_w) } else { input_r.x + 10 };
        draw_text_color(canvas, text_val, draw_x, ty, &body_st, INK);
        if caret_on {
            let cur_x = (draw_x + tw + 2).min(input_r.x + input_r.width - 8);
            fill_rr(canvas, Rect::new(cur_x, ty - 1, 2, body_st.cap_height + 2), 1, accent, 255);
        }
    }
    canvas.set_clip(prev_clip);

    let (ok_r, cancel_r) = modal_buttons(m_rect, unit);
    let cancel_hot = state.hover == Target::ModalCancel;
    let ok_hot = state.hover == Target::ModalConfirm;
    fill_rr(canvas, cancel_r, cancel_r.height / 2, WHITE, if cancel_hot { 255 } else { 190 });
    stroke_rr(canvas, cancel_r, cancel_r.height / 2, 1, INK_MUTED, 90);
    let cw = text_width("Vazgec", &body_st);
    draw_text_color(
        canvas,
        "Vazgec",
        cancel_r.x + (cancel_r.width - cw) / 2,
        cancel_r.y + (cancel_r.height - body_st.cap_height) / 2,
        &body_st,
        INK,
    );
    fill_rr_vgrad(
        canvas,
        ok_r,
        ok_r.height / 2,
        lighten(accent, if ok_hot { 90 } else { 60 }),
        accent,
        255,
    );
    stroke_rr(canvas, ok_r, ok_r.height / 2, 1, WHITE, 130);
    let kw = text_width("Kaydet", &body_st);
    draw_text_color(
        canvas,
        "Kaydet",
        ok_r.x + (ok_r.width - kw) / 2,
        ok_r.y + (ok_r.height - body_st.cap_height) / 2,
        &body_st,
        WHITE,
    );
}

pub fn draw_volume_osd(canvas: &mut Canvas<'_>, state: &DesktopState, width: i32, height: i32, unit: i32) {
    if state.vol_osd_anim <= 0 {
        return;
    }
    let e = ease_out_cubic(state.vol_osd_anim);
    let raw = volume_osd_rect(width, height, unit, state.vol_osd_anim);
    let sc = lerp(ONE * 92 / 100, ONE, e);
    let (x, y, w, h) = scale_rect_about(raw.x, raw.y, raw.width, raw.height, sc, raw.x + raw.width / 2, raw.y + raw.height / 2);
    let r = Rect::new(x, y, w, h);

    let (blur, passes) = state.blur_params(unit);
    glass_surface(canvas, r, h / 2, &dock_glass(blur, passes), e);

    let accent = state.settings.accent_color();
    let vol = if state.muted { 0 } else { state.volume as i32 };
    let disp_v = (state.vol_disp * 100 / ONE).clamp(0, 100);

    // Sol hoparlör rozeti
    let badge_s = h - unit * 4 / 5;
    let bx = r.x + unit * 2 / 5;
    let by = r.y + (r.height - badge_s) / 2;
    let (b_top, b_bot) = if vol == 0 { (CLOSE_RED_TOP, CLOSE_RED) } else { (lighten(accent, 70), accent) };
    fill_rr_vgrad(canvas, Rect::new(bx, by, badge_s, badge_s), badge_s / 2, b_top, b_bot, scale_u32(245, e));

    let st = text_style(text_h(unit, 46), 16);
    let icon_txt = if vol == 0 { "X" } else if vol < 50 { ")" } else { "))" };
    let iw = text_width(icon_txt, &st);
    let badge_text_col = fade_text_color(canvas, bx, by, WHITE, e);
    draw_text_color(canvas, icon_txt, bx + (badge_s - iw) / 2, by + (badge_s - st.cap_height) / 2, &st, badge_text_col);

    // Sağ yüzde metni (%75 veya SESSIZ)
    let mut pct_line = Line::new();
    if vol == 0 {
        pct_line.text("SESSIZ");
    } else {
        pct_line.text("%").number(vol as u32, 1);
    }
    let pw = text_width(pct_line.as_str(), &st);
    let px = r.x + r.width - pw - unit * 3 / 4;
    let py = r.y + (r.height - st.cap_height) / 2;
    let text_col = fade_text_color(canvas, px, py, WHITE, e);
    let shadow_col = fade_text_color(canvas, px, py, BLACK, e);
    shadowed(canvas, pct_line.as_str(), px, py, &st, text_col, shadow_col);

    // Ortadaki akıcı ses çubuğu
    let bar_x = bx + badge_s + unit * 3 / 5;
    let bar_w = (px - bar_x - unit * 3 / 5).max(unit * 2);
    let bar_h = (unit / 3).max(6);
    let bar_y = r.y + (r.height - bar_h) / 2;
    fill_rr(canvas, Rect::new(bar_x, bar_y, bar_w, bar_h), bar_h / 2, WHITE, scale_u32(65, e));

    let fill_w = (bar_w * disp_v / 100).max(if disp_v > 0 { bar_h } else { 0 });
    if fill_w > 0 {
        fill_rr_vgrad(
            canvas,
            Rect::new(bar_x, bar_y, fill_w, bar_h),
            bar_h / 2,
            lighten(accent, 80),
            accent,
            scale_u32(250, e),
        );
    }
}
