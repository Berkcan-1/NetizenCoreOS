use alloc::vec::Vec;
use nc_gfx::Rect;
use nc_math::smoothstep256;

use crate::motion::{ease_out_cubic, lerp};

pub fn unit_for(width: i32, height: i32) -> i32 {
    (height / 36).min(width / 44).max(10)
}

pub fn inside(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

pub fn dock_tile(unit: i32) -> i32 {
    unit * 11 / 5
}

pub fn dock_pad(unit: i32) -> i32 {
    unit * 2 / 5
}

pub fn dock_gap(unit: i32) -> i32 {
    unit / 3
}

pub fn dock_height(unit: i32) -> i32 {
    dock_tile(unit) + dock_pad(unit) * 2
}

pub fn dock_bottom_gap(unit: i32) -> i32 {
    unit * 2 / 3
}

pub fn dock_area(unit: i32) -> i32 {
    dock_height(unit)
}

pub fn dock_hint_rect(width: i32, height: i32, unit: i32) -> Rect {
    let w = (unit * 6).min(width / 3).max(40);
    let h = (unit / 5).max(4);
    let x = (width - w) / 2;
    let y = height - h - (unit / 4).max(2);
    Rect::new(x, y, w, h)
}

pub fn usable_rect(width: i32, height: i32, unit: i32, auto_hide: bool) -> Rect {
    let reserve = if auto_hide { 0 } else { dock_height(unit) };
    Rect::new(0, 0, width, (height - reserve).max(unit * 10))
}

pub fn status_width(width: i32, unit: i32) -> i32 {
    if width < unit * 46 {
        unit * 4
    } else {
        unit * 7
    }
}

pub struct DockLayout {
    pub dock: Rect,
    pub start: Rect,
    pub tiles: Vec<Rect>,
    pub status: Rect,
    pub sep_a: i32,
    pub sep_b: i32,
    pub tile: i32,
}


pub fn dock_layout(
    width: i32,
    height: i32,
    unit: i32,
    count: usize,
    reveal: i32,
    focus_x: i32,
    mag: i32,
    full_t: i32,
) -> DockLayout {
    let pad = dock_pad(unit);
    let gap = dock_gap(unit);
    let sepw = unit * 3 / 4;
    let sw = status_width(width, unit);
    let avail = width - unit * 2;
    let n = count as i32;
    let nominal = dock_tile(unit);

    let fixed = |t: i32| pad * 2 + t + gap + sepw * 2 + sw + n * (t + gap);
    let mut t = nominal;
    while t > unit * 3 / 2 && fixed(t) > avail {
        t -= 1;
    }

    // Tam ekran (full_t) arttikca buyutme (magnification) efektini yumusakca bastir
    let eff_mag = (mag as i64 * (crate::motion::ONE - full_t.clamp(0, crate::motion::ONE)) as i64
        / crate::motion::ONE as i64) as i32;

    let rest_total = fixed(t);
    let rest_x0 = (width - rest_total) / 2;
    let apps_rest_x = rest_x0 + pad + t + gap + sepw;
    let reach = ((t + gap) * 5 / 2).max(1);

    let mut sizes: Vec<i32> = Vec::with_capacity(count);
    for i in 0..count {
        let center = apps_rest_x + i as i32 * (t + gap) + t / 2;
        let d = (center - focus_x).abs();
        let u = (d * 256 / reach).min(256) as u32;
        let w256 = 256 - smoothstep256(u) as i32;
        let extra = w256 * 118 / 256;
        let sc256 = 256 + (extra as i64 * eff_mag as i64 / crate::motion::ONE as i64) as i32;
        sizes.push(t * sc256 / 256);
    }

    let apps_w: i32 = sizes.iter().map(|s| s + gap).sum();
    let float_total = pad * 2 + t + gap + sepw * 2 + sw + apps_w;
    let float_x0 = ((width - float_total) / 2).max(unit / 2);

    // Tam ekran animasyon orani (0..ONE)
    let expand = crate::motion::ease_in_out_cubic(full_t);

    // Bar x0 -> 0'a, genislik -> tam ekran width'e uzar, alt bosluk -> 0'a iner
    let dock_x = lerp(float_x0, 0, expand);
    let dock_w = lerp(float_total, width, expand);
    let dock_h = dock_height(unit);
    let bot_gap = lerp(dock_bottom_gap(unit), 0, expand);

    let full_y = height - bot_gap - dock_h;
    let hidden_y = height + unit;
    let y = lerp(hidden_y, full_y, ease_out_cubic(reveal));

    let tile_top = y + (dock_h - t) / 2;
    let bottom = tile_top + t;

    // Baslat dugmesi sol kenarla birlikte sola kayar
    let start_x = dock_x + lerp(pad, unit / 2, expand);
    let start = Rect::new(start_x, bottom - t, t, t);
    let sep_a = start.x + t + gap + sepw / 2;

    // Uygulama simgeleri bar uzarken akici sekilde konumlanir
    let float_apps_x = float_x0 + pad + t + gap + sepw;
    let full_apps_x = sep_a + sepw / 2 + gap;
    let mut x = lerp(float_apps_x, full_apps_x, expand);

    let mut tiles = Vec::with_capacity(count);
    for s in sizes.iter() {
        tiles.push(Rect::new(x, bottom - *s, *s, *s));
        x += *s + gap;
    }

    // Sag durum cubugu (Wi-Fi + Saat) sag kenarla birlikte en saga uzar
    let status_x = dock_x + dock_w - sw - lerp(pad, unit / 2, expand);
    let sep_b = status_x - sepw / 2;
    let status = Rect::new(status_x, y + pad, sw, dock_h - pad * 2);

    DockLayout {
        dock: Rect::new(dock_x, y, dock_w, dock_h),
        start,
        tiles,
        status,
        sep_a,
        sep_b,
        tile: t,
    }
}

pub fn border(unit: i32) -> i32 {
    (unit * 2 / 5).max(3)
}

pub fn title_height(unit: i32) -> i32 {
    unit * 7 / 4 + 2
}

pub fn window_radius(unit: i32) -> i32 {
    unit * 2 / 3
}

pub fn window_size(width_units: i32, height_units: i32, unit: i32, usable: Rect) -> (i32, i32) {
    let b = border(unit);
    let w = (width_units * unit + 2 * b)
        .min(usable.width - unit * 2)
        .max(unit * 12);
    let h = (height_units * unit + b)
        .min(usable.height - unit * 2)
        .max(unit * 8);
    (w, h)
}

pub fn clamp_window_pos(x: i32, y: i32, w: i32, h: i32, unit: i32, usable: Rect) -> (i32, i32) {
    let min_x = -(w - unit * 6);
    let max_x = (usable.width - unit * 6).max(min_x);
    let max_y = (usable.height - title_height(unit) - unit / 2).max(0);
    let _ = h;
    (x.clamp(min_x, max_x), y.clamp(0, max_y))
}

pub fn centered_pos(w: i32, h: i32, unit: i32, usable: Rect, cascade: i32) -> (i32, i32) {
    let x = (usable.width - w) / 2 - unit * 2 + cascade * unit * 2;
    let y = (usable.height - h) / 2 - unit + cascade * unit;
    clamp_window_pos(x, y.max(unit / 2), w, h, unit, usable)
}

pub fn title_bar(win: Rect, unit: i32) -> Rect {
    Rect::new(win.x, win.y, win.width, title_height(unit))
}

pub fn content_area(win: Rect, unit: i32) -> Rect {
    let b = border(unit);
    let title = title_height(unit);
    Rect::new(
        win.x + b,
        win.y + title,
        (win.width - b * 2).max(0),
        (win.height - title - b).max(0),
    )
}

pub fn button_size(unit: i32) -> (i32, i32) {
    (unit * 5 / 2, title_height(unit) - unit * 3 / 4)
}

pub fn close_button(win: Rect, unit: i32) -> Rect {
    let (w, h) = button_size(unit);
    let b = border(unit);
    Rect::new(
        win.x + win.width - b - unit / 2 - w,
        win.y + (title_height(unit) - h) / 2,
        w,
        h,
    )
}

pub fn max_button(win: Rect, unit: i32) -> Rect {
    let c = close_button(win, unit);
    Rect::new(c.x - c.width, c.y, c.width, c.height)
}

pub fn min_button(win: Rect, unit: i32) -> Rect {
    let m = max_button(win, unit);
    Rect::new(m.x - m.width, m.y, m.width, m.height)
}

pub fn button_group(win: Rect, unit: i32) -> Rect {
    let mn = min_button(win, unit);
    let cl = close_button(win, unit);
    Rect::new(mn.x, mn.y, cl.x + cl.width - mn.x, mn.height)
}

pub struct Grid {
    pub x0: i32,
    pub y0: i32,
    pub cw: i32,
    pub ch: i32,
    pub cols: i32,
    pub rows: i32,
}

pub fn grid_for(width: i32, height: i32, unit: i32, auto_hide: bool) -> Grid {
    let m = unit;
    let cw = unit * 11 / 2;
    let ch = unit * 9 / 2;
    let reserve = if auto_hide { unit } else { dock_area(unit) };
    let usable_h = height - reserve - m;
    Grid {
        x0: m,
        y0: m,
        cw,
        ch,
        cols: ((width - 2 * m) / cw).max(1),
        rows: ((usable_h - m) / ch).max(1),
    }
}

pub fn grid_cell(g: &Grid, col: i32, row: i32) -> Rect {
    Rect::new(g.x0 + col * g.cw, g.y0 + row * g.ch, g.cw, g.ch)
}

pub fn grid_pick(g: &Grid, px: i32, py: i32) -> (i32, i32) {
    let col = ((px - g.x0).max(0) / g.cw).min(g.cols - 1);
    let row = ((py - g.y0).max(0) / g.ch).min(g.rows - 1);
    (col, row)
}

pub fn icon_tile(cell: Rect, unit: i32) -> Rect {
    let size = unit * 13 / 5;
    Rect::new(cell.x + (cell.width - size) / 2, cell.y + unit / 4, size, size)
}

pub struct StartLayout {
    pub panel: Rect,
    pub cols: i32,
    #[allow(dead_code)]
    pub rows: i32,
    pub per_page: usize,
    pub pages: usize,
    pub cell_w: i32,
    pub cell_h: i32,
    pub grid_x: i32,
    pub grid_y: i32,
    pub reboot: Rect,
    pub shut: Rect,
    #[allow(dead_code)]
    pub header_h: i32,
    pub pad: i32,
    pub dots_y: i32,
}

pub fn start_layout(width: i32, height: i32, unit: i32, dock_top: i32, count: usize) -> StartLayout {
    let _ = height;
    let pad = unit * 3 / 2;
    let cell_w = unit * 6;
    let cell_h = unit * 5;
    let header_h = unit * 3;
    let avail_w = width - unit * 2;
    let fit_cols = ((avail_w - pad * 2) / cell_w).clamp(1, 6);
    let cols = fit_cols.min((count as i32).max(3));
    let panel_w = cols * cell_w + pad * 2;
    let top_limit = unit;
    let avail_h = dock_top - unit * 2 / 3 - top_limit;
    let max_rows = ((avail_h - pad * 2 - header_h - unit * 2) / cell_h).max(1);
    let need_rows = ((count as i32 + cols - 1) / cols).max(1);
    let rows = need_rows.min(max_rows);
    let per_page = (cols * rows) as usize;
    let pages = ((count + per_page - 1) / per_page.max(1)).max(1);
    let dots_h = if pages > 1 { unit * 2 } else { 0 };
    let panel_h = pad + header_h + rows * cell_h + dots_h + pad;
    let x = (width - panel_w) / 2;
    let y = (dock_top - unit * 2 / 3 - panel_h).max(unit / 2);
    let panel = Rect::new(x, y, panel_w, panel_h);
    let bs = unit * 2;
    let shut = Rect::new(
        panel.x + panel.width - pad - bs,
        panel.y + pad + (header_h - bs) / 2 - unit / 4,
        bs,
        bs,
    );
    let reboot = Rect::new(shut.x - bs - unit / 2, shut.y, bs, bs);
    let grid_x = panel.x + pad;
    let grid_y = panel.y + pad + header_h;
    StartLayout {
        panel,
        cols,
        rows,
        per_page,
        pages,
        cell_w,
        cell_h,
        grid_x,
        grid_y,
        reboot,
        shut,
        header_h,
        pad,
        dots_y: grid_y + rows * cell_h + unit / 2,
    }
}

pub fn start_cell(sl: &StartLayout, local_index: usize) -> Rect {
    let i = local_index as i32;
    Rect::new(
        sl.grid_x + (i % sl.cols) * sl.cell_w,
        sl.grid_y + (i / sl.cols) * sl.cell_h,
        sl.cell_w,
        sl.cell_h,
    )
}

pub fn start_tile(cell: Rect, unit: i32) -> Rect {
    let size = unit * 13 / 5;
    Rect::new(cell.x + (cell.width - size) / 2, cell.y + unit / 4, size, size)
}

pub fn start_dot_rect(sl: &StartLayout, page: usize, unit: i32) -> Rect {
    let s = unit * 2;
    let total = sl.pages as i32 * s;
    Rect::new(
        sl.panel.x + (sl.panel.width - total) / 2 + page as i32 * s,
        sl.dots_y,
        s,
        s,
    )
}

pub fn context_menu_rect(
    x: i32,
    y: i32,
    item_count: usize,
    width: i32,
    height: i32,
    unit: i32,
) -> Rect {
    let w = (unit * 15).min(width - unit);
    let pad = unit / 3;
    let h = pad * 2 + item_count as i32 * context_row(unit);
    let cx = x.min(width - w - unit / 2).max(unit / 2);
    let cy = y.min(height - h - unit / 2).max(unit / 2);
    Rect::new(cx, cy, w, h)
}

pub fn context_row(unit: i32) -> i32 {
    unit * 8 / 5
}

pub fn context_item_rect(menu: Rect, index: usize, unit: i32) -> Rect {
    let pad = unit / 3;
    Rect::new(
        menu.x + pad,
        menu.y + pad + index as i32 * context_row(unit),
        menu.width - pad * 2,
        context_row(unit),
    )
}

pub fn desktop_modal_rect(width: i32, height: i32, unit: i32, tall: bool) -> Rect {
    let w = (unit * 28).min(width - unit * 2);
    let h = if tall {
        (unit * 18).min(height - unit * 2)
    } else {
        (unit * 11).min(height - unit * 2)
    };
    Rect::new((width - w) / 2, (height - h) / 2, w, h)
}

pub fn modal_buttons(m: Rect, unit: i32) -> (Rect, Rect) {
    let btn_w = unit * 7;
    let btn_h = unit * 2;
    let btn_y = m.y + m.height - btn_h - unit;
    let ok = Rect::new(m.x + m.width - btn_w - unit * 3 / 2, btn_y, btn_w, btn_h);
    let cancel = Rect::new(ok.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
    (ok, cancel)
}

pub fn task_manager_rect(x: i32, y: i32, width: i32, height: i32, unit: i32) -> Rect {
    let w = (unit * 34).min(width - unit);
    let h = (unit * 23).min(height - unit);
    Rect::new(x, y, w, h)
}

pub fn task_manager_close_rect(tm: Rect, unit: i32) -> Rect {
    Rect::new(
        tm.x + tm.width - unit * 3 - unit / 2,
        tm.y + unit / 2,
        unit * 3,
        title_height(unit) - unit,
    )
}

pub fn task_manager_row_rect(tm: Rect, idx: usize, unit: i32) -> Rect {
    let top = tm.y + unit * 9 + (idx as i32) * (unit * 3 / 2);
    Rect::new(tm.x + unit, top, tm.width - unit * 2, unit * 4 / 3)
}

pub fn task_manager_action_btn(tm: Rect, primary: bool, unit: i32) -> Rect {
    let w = unit * 10;
    let h = unit * 2;
    let y = tm.y + tm.height - h - unit * 3 / 4;
    if primary {
        Rect::new(tm.x + tm.width - w - unit, y, w, h)
    } else {
        Rect::new(tm.x + unit, y, w, h)
    }
}

pub fn volume_osd_rect(width: i32, height: i32, unit: i32, anim: i32) -> Rect {
    let w = (unit * 14).min(width - unit * 2);
    let h = unit * 3;
    let x = (width - w) / 2;
    let shown_y = height - dock_height(unit) - dock_bottom_gap(unit) - h - unit * 5 / 4;
    let hidden_y = shown_y + unit;
    let y = lerp(hidden_y, shown_y, ease_out_cubic(anim));
    Rect::new(x, y, w, h)
}
