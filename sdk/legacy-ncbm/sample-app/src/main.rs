#![no_std]
#![no_main]

use nc_sdk::{declare_ntzc_app, rgb, Gui, NtzcInput, RET_NONE, RET_REDRAW};

const GRID_W: usize = 24;
const GRID_H: usize = 14;
const PALETTE: [u32; 6] = [
    rgb(36, 134, 250),
    rgb(18, 206, 186),
    rgb(250, 166, 26),
    rgb(242, 62, 62),
    rgb(168, 85, 247),
    rgb(16, 28, 44),
];

#[repr(C)]
pub struct PaintState {
    pub pixels: [u8; GRID_W * GRID_H],
    pub selected_color: u8,
}

fn init(state: &mut PaintState, _input: &NtzcInput) {
    state.pixels = [0xFF; GRID_W * GRID_H];
    state.selected_color = 0;
}

fn update(state: &mut PaintState, input: &NtzcInput) -> u32 {
    if input.has_pointer == 0 {
        return RET_NONE;
    }
    let u = input.unit.max(10);
    let px = input.pointer_x - input.area_x;
    let py = input.pointer_y - input.area_y;

    // Ust renk paleti secimi
    if input.pointer_clicked != 0 && py >= u / 2 && py <= u * 2 {
        for i in 0..PALETTE.len() {
            let bx = u + (i as i32) * (u * 2);
            if px >= bx && px <= bx + u * 3 / 2 {
                state.selected_color = i as u8;
                return RET_REDRAW;
            }
        }
        // Temizle dugmesi
        let clear_x = input.area_w - u * 6;
        if px >= clear_x && px <= clear_x + u * 5 {
            state.pixels = [0xFF; GRID_W * GRID_H];
            return RET_REDRAW;
        }
    }

    // Tuval uzerinde cizim
    if input.pointer_down != 0 {
        let canvas_x = u;
        let canvas_y = u * 5 / 2;
        let cell = ((input.area_w - u * 2) / GRID_W as i32).min((input.area_h - u * 3) / GRID_H as i32).max(4);
        let gx = (px - canvas_x) / cell;
        let gy = (py - canvas_y) / cell;
        if gx >= 0 && (gx as usize) < GRID_W && gy >= 0 && (gy as usize) < GRID_H {
            let idx = (gy as usize) * GRID_W + (gx as usize);
            if state.pixels[idx] != state.selected_color {
                state.pixels[idx] = state.selected_color;
                return RET_REDRAW;
            }
        }
    }
    RET_NONE
}

fn draw(state: &mut PaintState, gui: &Gui) {
    let inp = gui.input;
    let u = inp.unit.max(10);
    let ax = inp.area_x;
    let ay = inp.area_y;

    gui.fill_rect(ax, ay, inp.area_w, inp.area_h, rgb(244, 249, 255));

    // Renk paleti
    for (i, &col) in PALETTE.iter().enumerate() {
        let bx = ax + u + (i as i32) * (u * 2);
        let by = ay + u * 2 / 3;
        let border = if state.selected_color == i as u8 {
            rgb(12, 22, 38)
        } else {
            col
        };
        gui.draw_panel(bx, by, u * 3 / 2, u * 4 / 3, u / 3, col, 255, border, 255);
    }

    // Temizle butonu
    let clear_x = ax + inp.area_w - u * 6;
    gui.draw_panel(
        clear_x,
        ay + u * 2 / 3,
        u * 5,
        u * 4 / 3,
        u / 3,
        rgb(242, 62, 62),
        240,
        rgb(242, 62, 62),
        255,
    );
    gui.draw_text("Temizle", clear_x + u * 3 / 4, ay + u, u / 2, rgb(255, 255, 255));

    // Piksel izgarasi
    let cell = ((inp.area_w - u * 2) / GRID_W as i32)
        .min((inp.area_h - u * 3) / GRID_H as i32)
        .max(4);
    let start_x = ax + u;
    let start_y = ay + u * 5 / 2;

    for gy in 0..GRID_H {
        for gx in 0..GRID_W {
            let c_idx = state.pixels[gy * GRID_W + gx];
            let col = if (c_idx as usize) < PALETTE.len() {
                PALETTE[c_idx as usize]
            } else {
                rgb(255, 255, 255)
            };
            gui.fill_rect(
                start_x + (gx as i32) * cell,
                start_y + (gy as i32) * cell,
                cell - 1,
                cell - 1,
                col,
            );
        }
    }
}

declare_ntzc_app!(PaintState, init, update, draw);
