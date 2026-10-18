#![no_std]

use nc_app::text::Line;
use nc_app::theme::SIDEBAR;
use nc_app::{App, AppInfo, Context, Icon, Input, Update};
use nc_gfx::{Canvas, Rect};
use nc_ui::{draw_panel, draw_text_color, text_style, PANEL_BORDER, TEXT_DIM, TEXT_MUTED, TEXT_PRIMARY};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct SystemApp {
    info: AppInfo,
}

impl SystemApp {
    pub fn new() -> SystemApp {
        SystemApp { info: AppInfo::new("nc.system", "SYSTEM", VERSION, Icon::Monitor, 30, 24) }
    }
}

impl App for SystemApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, _ctx: &mut Context, _input: &Input) -> Update {
        Update::NONE
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        let x = area.x + unit * 2;
        let top = area.y + unit * 3 / 2;
        let title_style = text_style(unit * 13 / 10, 20);
        draw_text_color(canvas, "NETIZENCORE", x, top, &title_style, TEXT_PRIMARY);
        let caption_style = text_style(unit * 55 / 100, 40);
        draw_text_color(canvas, "OPERATING SYSTEM", x, top + unit * 5 / 2, &caption_style, TEXT_DIM);

        let seconds = ctx.uptime_ms / 1000;
        let mut kernel = Line::new();
        kernel.text("NETIZENCORE ").text(VERSION);

        let mut mem_line = Line::new();
        mem_line.number(ctx.total_memory_mb, 1).text(" MB");

        let mut display = Line::new();
        display.number(ctx.screen_width as u32, 1).text(" X ").number(ctx.screen_height as u32, 1);

        let mut uptime = Line::new();
        uptime
            .number(seconds / 3600 % 100, 2)
            .text(":")
            .number(seconds / 60 % 60, 2)
            .text(":")
            .number(seconds % 60, 2);

        let rows = [
            ("KERNEL", kernel.as_str()),
            ("MEMORY", mem_line.as_str()),
            ("DISPLAY", display.as_str()),
            ("PLATFORM", ctx.platform),
            ("UPTIME", uptime.as_str()),
        ];

        let row_height = unit * 5 / 2;
        let first = area.y + unit * 13 / 2;
        let label_style = text_style(unit * 6 / 10, 30);
        for index in 0..rows.len() {
            let row = Rect::new(x, first + index as i32 * (row_height + unit / 4), area.width - unit * 4, row_height);
            draw_panel(canvas, row, unit / 2, SIDEBAR, 255, PANEL_BORDER, 255);
            let text_y = row.y + (row.height - label_style.cap_height) / 2;
            draw_text_color(canvas, rows[index].0, row.x + unit * 3 / 2, text_y, &label_style, TEXT_MUTED);
            draw_text_color(canvas, rows[index].1, row.x + unit * 10, text_y, &label_style, TEXT_PRIMARY);
        }
    }
}
