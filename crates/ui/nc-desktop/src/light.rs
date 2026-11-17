use nc_gfx::{Canvas, Rect};

use crate::paint::{paint_rr, paint_rr_hole};

#[derive(Clone, Copy)]
pub struct Light {
    pub x: i32,
    pub y: i32,
}

fn falloff(dx: i32, dy: i32, reach: i32) -> u32 {
    let d2 = dx as i64 * dx as i64 + dy as i64 * dy as i64;
    let r2 = reach as i64 * reach as i64;
    if r2 <= 0 || d2 >= r2 {
        return 0;
    }
    let t = (r2 - d2) * 255 / r2;
    (t * t / 255) as u32
}

fn window_of(rect: Rect, light: Light, reach: i32) -> Option<Rect> {
    let x0 = rect.x.max(light.x - reach);
    let y0 = rect.y.max(light.y - reach);
    let x1 = (rect.x + rect.width).min(light.x + reach);
    let y1 = (rect.y + rect.height).min(light.y + reach);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn restrict(canvas: &mut Canvas<'_>, area: Rect) -> Rect {
    let prev = canvas.clip_rect();
    let x0 = area.x.max(prev.x);
    let y0 = area.y.max(prev.y);
    let x1 = (area.x + area.width).min(prev.x + prev.width);
    let y1 = (area.y + area.height).min(prev.y + prev.height);
    canvas.set_clip(Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0)));
    prev
}

pub fn sheen(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    light: Light,
    reach: i32,
    color: u32,
    strength: u32,
) {
    if strength == 0 {
        return;
    }
    let Some(area) = window_of(rect, light, reach) else {
        return;
    };
    let prev = restrict(canvas, area);
    paint_rr(canvas, rect, radius, |x, y| {
        (color, falloff(x - light.x, y - light.y, reach) * strength / 255)
    });
    canvas.set_clip(prev);
}

pub fn reveal_edge(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    light: Light,
    reach: i32,
    color: u32,
    strength: u32,
) {
    if strength == 0 || rect.width < 3 || rect.height < 3 {
        return;
    }
    let Some(area) = window_of(rect, light, reach) else {
        return;
    };
    let hole = Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
    let prev = restrict(canvas, area);
    paint_rr_hole(canvas, rect, radius, Some(hole), |x, y| {
        (color, falloff(x - light.x, y - light.y, reach) * strength / 255)
    });
    canvas.set_clip(prev);
}
