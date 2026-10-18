#![no_std]

use nc_app::text::{centered, Line};
use nc_app::{App, AppInfo, Context, Icon, Input, Update};
use nc_gfx::{Canvas, Rect};
use nc_ui::{draw_accent_button, draw_button, text_style, TEXT_DIM, TEXT_PRIMARY};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hover {
    Nothing,
    Toggle,
    Reset,
}

pub struct StopwatchApp {
    info: AppInfo,
    running: bool,
    started_ms: u32,
    banked_ms: u32,
    hover: Hover,
}

impl StopwatchApp {
    pub fn new() -> StopwatchApp {
        StopwatchApp {
            info: AppInfo::new("nc.stopwatch", "STOPWATCH", env!("CARGO_PKG_VERSION"), Icon::Stopwatch, 26, 16),
            running: false,
            started_ms: 0,
            banked_ms: 0,
            hover: Hover::Nothing,
        }
    }

    fn elapsed_ms(&self, now: u32) -> u32 {
        if self.running {
            self.banked_ms.wrapping_add(now.wrapping_sub(self.started_ms))
        } else {
            self.banked_ms
        }
    }
}

fn toggle_button(area: Rect, unit: i32) -> Rect {
    Rect::new(area.x + area.width / 2 - unit * 9 - unit / 2, area.y + area.height - unit * 5, unit * 9, unit * 3)
}

fn reset_button(area: Rect, unit: i32) -> Rect {
    Rect::new(area.x + area.width / 2 + unit / 2, area.y + area.height - unit * 5, unit * 9, unit * 3)
}

fn inside(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

impl App for StopwatchApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let hover = match input.pointer {
            Some(pointer) if inside(toggle_button(input.area, ctx.unit), pointer.x, pointer.y) => Hover::Toggle,
            Some(pointer) if inside(reset_button(input.area, ctx.unit), pointer.x, pointer.y) => Hover::Reset,
            _ => Hover::Nothing,
        };
        let mut redraw = hover != self.hover || self.running;
        self.hover = hover;

        if let Some(pointer) = input.pointer {
            if pointer.clicked {
                match hover {
                    Hover::Toggle => {
                        if self.running {
                            self.banked_ms = self.elapsed_ms(ctx.uptime_ms);
                            self.running = false;
                        } else {
                            self.started_ms = ctx.uptime_ms;
                            self.running = true;
                        }
                    }
                    Hover::Reset => {
                        self.banked_ms = 0;
                        self.started_ms = ctx.uptime_ms;
                    }
                    Hover::Nothing => {}
                }
                redraw = true;
            }
        }
        Update { redraw, close: false, install_app: None }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        let elapsed = self.elapsed_ms(ctx.uptime_ms);

        let mut line = Line::new();
        line.number(elapsed / 60000 % 100, 2)
            .text(":")
            .number(elapsed / 1000 % 60, 2)
            .text(".")
            .number(elapsed / 100 % 10, 1);

        let center_x = area.x + area.width / 2;
        let time_style = text_style(unit * 3, 20);
        centered(canvas, line.as_str(), center_x, area.y + unit * 2, &time_style, TEXT_PRIMARY);

        let caption_style = text_style(unit * 55 / 100, 40);
        let caption = if self.running { "RUNNING" } else { "STOPPED" };
        centered(canvas, caption, center_x, area.y + unit * 6 + unit / 2, &caption_style, TEXT_DIM);

        let toggle_label = if self.running { "STOP" } else { "START" };
        draw_accent_button(canvas, toggle_button(area, unit), toggle_label, self.hover == Hover::Toggle, ctx.settings.accent_color());
        draw_button(canvas, reset_button(area, unit), "RESET", self.hover == Hover::Reset, false);
    }
}
