use nc_gfx::{Canvas, Rect};
use nc_ui::{draw_dot, draw_panel, WHITE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Sliders,
    Monitor,
    Stopwatch,
    Generic,
}

pub fn draw_icon(canvas: &mut Canvas<'_>, icon: Icon, center_x: i32, center_y: i32, size: i32, color: u32) {
    match icon {
        Icon::Sliders => draw_sliders(canvas, center_x, center_y, size, color),
        Icon::Monitor => draw_monitor(canvas, center_x, center_y, size, color),
        Icon::Stopwatch => draw_stopwatch(canvas, center_x, center_y, size, color),
        Icon::Generic => draw_generic(canvas, center_x, center_y, size, color),
    }
}

pub fn draw_line(canvas: &mut Canvas<'_>, x0: i32, y0: i32, x1: i32, y1: i32, radius: i32, color: u32) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for step in 0..=steps {
        let x = x0 + (x1 - x0) * step / steps;
        let y = y0 + (y1 - y0) * step / steps;
        draw_dot(canvas, x, y, radius, color, 256);
    }
}

pub fn draw_cross(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, half: i32, color: u32) {
    draw_line(canvas, center_x - half, center_y - half, center_x + half, center_y + half, 1, color);
    draw_line(canvas, center_x - half, center_y + half, center_x + half, center_y - half, 1, color);
}

pub fn draw_dash(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, half: i32, color: u32) {
    canvas.fill_rect(center_x - half, center_y, half * 2 + 1, 2, color);
}

pub fn draw_grid(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, size: i32, color: u32) {
    let gap = (size / 6).max(1);
    let square = ((size - gap) / 2).max(2);
    let left = center_x - (square * 2 + gap) / 2;
    let top = center_y - (square * 2 + gap) / 2;
    for row in 0..2 {
        for column in 0..2 {
            let area = Rect::new(left + column * (square + gap), top + row * (square + gap), square, square);
            draw_panel(canvas, area, 1, color, 255, color, 0);
        }
    }
}

fn draw_sliders(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, size: i32, color: u32) {
    let left = center_x - size / 2;
    let knobs = [25, 70, 45];
    for row in 0..3 {
        let y = center_y - size / 3 + row * size / 3;
        canvas.fill_rect(left, y - 1, size, 2, color);
        draw_dot(canvas, left + size * knobs[row as usize] / 100, y, (size / 7).max(2), color, 256);
    }
}

fn draw_monitor(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, size: i32, color: u32) {
    let screen = Rect::new(center_x - size / 2, center_y - size * 2 / 5, size, size * 3 / 5);
    draw_panel(canvas, screen, (size / 10).max(1), color, 255, color, 0);
    canvas.fill_rect(center_x - size / 12, screen.y + screen.height, (size / 6).max(2), (size / 8).max(2), color);
    canvas.fill_rect(center_x - size / 4, screen.y + screen.height + (size / 8).max(2), size / 2, 2, color);
}

fn draw_stopwatch(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, size: i32, color: u32) {
    let radius = (size * 2 / 5).max(3);
    let middle_y = center_y + size / 10;
    let outer = Rect::new(center_x - radius, middle_y - radius, radius * 2, radius * 2);
    let inner = Rect::new(outer.x + 1, outer.y + 1, outer.width - 2, outer.height - 2);
    draw_panel(canvas, outer, radius, WHITE, 0, color, 255);
    draw_panel(canvas, inner, radius - 1, WHITE, 0, color, 255);
    canvas.fill_rect(center_x - (size / 10).max(1), center_y - size / 2, (size / 5).max(2), (size / 10).max(2), color);
    draw_line(canvas, center_x, middle_y, center_x + radius / 2, middle_y - radius / 2, 1, color);
}

fn draw_generic(canvas: &mut Canvas<'_>, center_x: i32, center_y: i32, size: i32, color: u32) {
    let area = Rect::new(center_x - size * 2 / 5, center_y - size * 2 / 5, size * 4 / 5, size * 4 / 5);
    draw_panel(canvas, area, (size / 5).max(2), WHITE, 0, color, 255);
    draw_dot(canvas, center_x, center_y, (size / 8).max(2), color, 256);
}
