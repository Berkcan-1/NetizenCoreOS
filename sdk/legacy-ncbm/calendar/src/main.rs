#![no_std]
#![no_main]

use nc_sdk::{declare_ntzc_app, rgb, Gui, NtzcInput, RET_NONE, RET_REDRAW};

const MONTH_NAMES: [&str; 12] = [
    "Ocak", "Subat", "Mart", "Nisan", "Mayis", "Haziran",
    "Temmuz", "Agustos", "Eylul", "Ekim", "Kasim", "Aralik",
];

const DAY_NAMES: [&str; 7] = ["Pzt", "Sal", "Car", "Per", "Cum", "Cmt", "Paz"];

const DAY_NUMS: [&str; 32] = [
    "", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10",
    "11", "12", "13", "14", "15", "16", "17", "18", "19", "20",
    "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
];

#[repr(C)]
pub struct CalendarState {
    pub year: i32,
    pub month: u8,        // 1..=12
    pub selected_day: u8, // 1..=31
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Ayin 1. gununun haftanin hangi gunune denk geldigini bulur (0 = Pazartesi .. 6 = Pazar)
fn first_weekday_monday_based(year: i32, month: u8) -> i32 {
    let mut y = year;
    let mut m = month as i32;
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let k = y % 100;
    let j = y / 100;
    let h = (1 + (13 * (m + 1)) / 5 + k + (k / 4) + (j / 4) + 5 * j) % 7;
    // Zeller: 0=Cmt, 1=Paz, 2=Pzt ... -> Pazartesi=0 yapalim:
    (h + 5) % 7
}

fn prev_month(state: &mut CalendarState) {
    if state.month <= 1 {
        state.month = 12;
        state.year -= 1;
    } else {
        state.month -= 1;
    }
    let max_d = days_in_month(state.year, state.month);
    if state.selected_day > max_d {
        state.selected_day = max_d;
    }
}

fn next_month(state: &mut CalendarState) {
    if state.month >= 12 {
        state.month = 1;
        state.year += 1;
    } else {
        state.month += 1;
    }
    let max_d = days_in_month(state.year, state.month);
    if state.selected_day > max_d {
        state.selected_day = max_d;
    }
}

/// 1. Baslangic (pencere ilk acildiginda bir kez cagrilir)
fn init(state: &mut CalendarState, _input: &NtzcInput) {
    state.year = 2026;
    state.month = 10;
    state.selected_day = 16;
}

/// 2. Girdi ve Mantik Guncellemesi (her karede cagrilir)
fn update(state: &mut CalendarState, input: &NtzcInput) -> u32 {
    // Klavye sol (7) ve sag (8) yon tuslariyla ay degistirme
    if input.key_code == 7 {
        prev_month(state);
        return RET_REDRAW;
    } else if input.key_code == 8 {
        next_month(state);
        return RET_REDRAW;
    }

    if input.has_pointer == 0 || input.pointer_clicked == 0 {
        return RET_NONE;
    }

    let u = input.unit.max(10);
    let lx = input.pointer_x - input.area_x;
    let ly = input.pointer_y - input.area_y;

    // Ust bar sol/sag ok butonlari (< ve >)
    if ly >= u && ly <= u * 3 {
        if lx >= u * 3 / 2 && lx <= u * 4 {
            prev_month(state);
            return RET_REDRAW;
        }
        if lx >= input.area_w - u * 4 && lx <= input.area_w - u * 3 / 2 {
            next_month(state);
            return RET_REDRAW;
        }
    }

    // Gun izgarasina tiklama kontrolu
    let grid_x = u * 3 / 2;
    let grid_y = u * 6;
    let cell_w = (input.area_w - u * 3) / 7;
    let cell_h = (input.area_h - u * 9) / 6;

    if lx >= grid_x && ly >= grid_y && cell_w > 0 && cell_h > 0 {
        let col = (lx - grid_x) / cell_w;
        let row = (ly - grid_y) / cell_h;
        if (0..7).contains(&col) && (0..6).contains(&row) {
            let slot = row * 7 + col;
            let start_off = first_weekday_monday_based(state.year, state.month);
            let day = slot - start_off + 1;
            let max_d = days_in_month(state.year, state.month) as i32;
            if day >= 1 && day <= max_d {
                state.selected_day = day as u8;
                return RET_REDRAW;
            }
        }
    }

    RET_NONE
}

/// Yili 4 haneli metne donusturen kucuk yardimci
fn format_header<'a>(buf: &'a mut [u8; 32], month: u8, year: i32) -> &'a str {
    let m_name = MONTH_NAMES[(month.saturating_sub(1) as usize) % 12].as_bytes();
    let mut pos = 0;
    for &b in m_name {
        buf[pos] = b;
        pos += 1;
    }
    buf[pos] = b' ';
    pos += 1;

    let y = year.clamp(1900, 9999);
    buf[pos] = b'0' + ((y / 1000) % 10) as u8;
    buf[pos + 1] = b'0' + ((y / 100) % 10) as u8;
    buf[pos + 2] = b'0' + ((y / 10) % 10) as u8;
    buf[pos + 3] = b'0' + (y % 10) as u8;
    pos += 4;

    core::str::from_utf8(&buf[..pos]).unwrap_or("Takvim")
}

/// 3. Cizim (sadece degisiklik oldugunda cagrilir)
fn draw(state: &mut CalendarState, gui: &Gui) {
    let inp = gui.input;
    let u = inp.unit.max(10);
    let ax = inp.area_x;
    let ay = inp.area_y;

    // Arka plan
    gui.fill_rect(ax, ay, inp.area_w, inp.area_h, rgb(244, 249, 255));

    // Ust Baslik Karti
    gui.draw_panel(
        ax + u,
        ay + u * 3 / 4,
        inp.area_w - u * 2,
        u * 5 / 2,
        u / 2,
        rgb(16, 32, 58),
        245,
        rgb(96, 190, 255),
        220,
    );

    // Sol (<) ve Sag (>) Ay Degistirme Butonlari
    let btn_y = ay + u + 2;
    let btn_h = u * 2 - 4;
    gui.draw_panel(
        ax + u * 3 / 2,
        btn_y,
        u * 2,
        btn_h,
        btn_h / 2,
        rgb(36, 134, 250),
        235,
        rgb(255, 255, 255),
        180,
    );
    gui.draw_text("<", ax + u * 2 + 2, btn_y + u / 3, u * 3 / 5, rgb(255, 255, 255));

    gui.draw_panel(
        ax + inp.area_w - u * 7 / 2,
        btn_y,
        u * 2,
        btn_h,
        btn_h / 2,
        rgb(36, 134, 250),
        235,
        rgb(255, 255, 255),
        180,
    );
    gui.draw_text(
        ">",
        ax + inp.area_w - u * 3 + 2,
        btn_y + u / 3,
        u * 3 / 5,
        rgb(255, 255, 255),
    );

    // Ay ve Yil basligi
    let mut hdr_buf = [0u8; 32];
    let hdr_str = format_header(&mut hdr_buf, state.month, state.year);
    let title_cap = (u * 7 / 10).max(11);
    let tw = gui.text_width(hdr_str, title_cap);
    gui.draw_text(
        hdr_str,
        ax + (inp.area_w - tw) / 2,
        ay + u * 3 / 2,
        title_cap,
        rgb(238, 248, 255),
    );

    // Haftanin gun isimleri (Pzt .. Paz)
    let grid_x = ax + u * 3 / 2;
    let grid_y = ay + u * 6;
    let cell_w = (inp.area_w - u * 3) / 7;
    let cell_h = (inp.area_h - u * 9) / 6;
    let day_cap = (u / 2).max(9);

    for (col, &dname) in DAY_NAMES.iter().enumerate() {
        let cx = grid_x + (col as i32) * cell_w;
        let dw = gui.text_width(dname, day_cap);
        let col_color = if col >= 5 {
            rgb(236, 64, 60) // Hafta sonu mercan rengi
        } else {
            rgb(62, 80, 100)
        };
        gui.draw_text(
            dname,
            cx + (cell_w - dw) / 2,
            ay + u * 4 + u / 3,
            day_cap,
            col_color,
        );
    }

    // Gun Hucreleri (6 satir x 7 sutun)
    let start_off = first_weekday_monday_based(state.year, state.month);
    let total_days = days_in_month(state.year, state.month);

    for d in 1..=total_days {
        let slot = start_off + (d as i32) - 1;
        let row = slot / 7;
        let col = slot % 7;

        let cx = grid_x + col * cell_w + 2;
        let cy = grid_y + row * cell_h + 2;
        let cw = cell_w - 4;
        let ch = cell_h - 4;

        let is_selected = d == state.selected_day;
        let is_weekend = col >= 5;

        let (fill_c, border_c, text_c) = if is_selected {
            (rgb(18, 206, 186), rgb(36, 134, 250), rgb(255, 255, 255))
        } else if is_weekend {
            (rgb(252, 244, 246), rgb(224, 204, 212), rgb(210, 52, 52))
        } else {
            (rgb(255, 255, 255), rgb(206, 224, 238), rgb(18, 30, 46))
        };

        gui.draw_panel(cx, cy, cw, ch, u / 3, fill_c, 255, border_c, 255);

        let num_str = DAY_NUMS[d as usize];
        let nw = gui.text_width(num_str, day_cap);
        gui.draw_text(
            num_str,
            cx + (cw - nw) / 2,
            cy + (ch - day_cap) / 2,
            day_cap,
            text_c,
        );
    }

    // Alt Bilgi Cubugu
    let footer_y = ay + inp.area_h - u * 2;
    gui.draw_dot(ax + u * 2, footer_y + u / 3, 4, rgb(18, 206, 186), 255);
    gui.draw_text(
        "Ok tuslariyla veya < > butonlariyla aylari gezin",
        ax + u * 3,
        footer_y,
        (u * 45 / 100).max(8),
        rgb(96, 114, 136),
    );
}

declare_ntzc_app!(CalendarState, init, update, draw);
