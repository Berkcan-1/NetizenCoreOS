//! Kronometre (Ring 3, olay-tabanlı). Görünümü ve davranışı Ring 0 sürümüyle (`nc-app-stopwatch`)
//! PİKSEL PİKSEL aynıdır; bu `tests/parity.rs` ile sınanır. Koordinatlar pencereye göredir.
#![no_std]

pub mod strings;

use nc_abi::event::{EventBlock, OP_DRAW, OP_UPDATE, RET_NONE, RET_REDRAW};
use nc_app_sdk::audio::{self, Sound};
use nc_app_sdk::notify::{self, Level, NotifyError};
use nc_app_sdk::{pointer, Gfx, Line, RingApp};

// nc_ui::TEXT_PRIMARY / TEXT_DIM (parity testi eşitliği doğrular).
pub const TEXT_PRIMARY: u32 = (15 << 16) | (23 << 8) | 42;
pub const TEXT_DIM: u32 = (148 << 16) | (163 << 8) | 184;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hover {
    Nothing,
    Toggle,
    Reset,
}

#[derive(Clone, Copy)]
struct Area {
    w: i32,
    h: i32,
}

#[derive(Clone, Copy)]
struct Box2 {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn toggle_button(a: Area, unit: i32) -> Box2 {
    Box2 { x: a.w / 2 - unit * 9 - unit / 2, y: a.h - unit * 5, w: unit * 9, h: unit * 3 }
}

fn reset_button(a: Area, unit: i32) -> Box2 {
    Box2 { x: a.w / 2 + unit / 2, y: a.h - unit * 5, w: unit * 9, h: unit * 3 }
}

fn inside(b: Box2, x: i32, y: i32) -> bool {
    x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h
}

const NOTE: [(&str, &str); 11] = [
    ("Kronometre", "dakika geçti"),
    ("Stopwatch", "minutes elapsed"),
    ("Chronomètre", "minutes écoulées"),
    ("Stoppuhr", "Minuten vergangen"),
    ("Cronómetro", "minutos transcurridos"),
    ("Cronômetro", "minutos decorridos"),
    ("Секундомер", "мин прошло"),
    ("秒表", "分钟已过"),
    ("ストップウォッチ", "分経過"),
    ("스톱워치", "분 경과"),
    ("स्टॉपवॉच", "मिनट बीते"),
];

pub struct StopwatchApp {
    running: bool,
    started_ms: u32,
    banked_ms: u32,
    hover: Hover,
    notified_minutes: u32,
    notify_blocked: bool,
}

impl StopwatchApp {
    pub const fn new() -> Self {
        Self { running: false, started_ms: 0, banked_ms: 0, hover: Hover::Nothing, notified_minutes: 0, notify_blocked: false }
    }

    fn elapsed_ms(&self, now: u32) -> u32 {
        if self.running {
            self.banked_ms.wrapping_add(now.wrapping_sub(self.started_ms))
        } else {
            self.banked_ms
        }
    }

    fn update(&mut self, ev: &EventBlock) -> u32 {
        let area = Area { w: ev.area_w, h: ev.area_h };
        let pointer = pointer(ev);
        let hover = match pointer {
            Some(p) if inside(toggle_button(area, ev.unit), p.x, p.y) => Hover::Toggle,
            Some(p) if inside(reset_button(area, ev.unit), p.x, p.y) => Hover::Reset,
            _ => Hover::Nothing,
        };
        let mut redraw = hover != self.hover || self.running;
        self.hover = hover;

        if let Some(p) = pointer {
            if p.clicked {
                match hover {
                    Hover::Toggle => {
                        if self.running {
                            self.banked_ms = self.elapsed_ms(ev.uptime_ms);
                            self.running = false;
                        } else {
                            self.started_ms = ev.uptime_ms;
                            self.running = true;
                        }
                        let _ = audio::play(Sound::Click);
                    }
                    Hover::Reset => {
                        self.banked_ms = 0;
                        self.started_ms = ev.uptime_ms;
                        self.notified_minutes = 0;
                        let _ = audio::play(Sound::Tick);
                    }
                    Hover::Nothing => {}
                }
                redraw = true;
            }
        }
        if self.running {
            self.announce_minutes(ev);
        }
        if redraw {
            RET_REDRAW
        } else {
            RET_NONE
        }
    }

    fn announce_minutes(&mut self, ev: &EventBlock) {
        let minutes = self.elapsed_ms(ev.uptime_ms) / 60_000;
        if minutes == 0 || minutes <= self.notified_minutes || self.notify_blocked {
            return;
        }
        self.notified_minutes = minutes;
        let (title, suffix) = NOTE[if (ev.lang as usize) < NOTE.len() { ev.lang as usize } else { 1 }];
        let mut body = Line::new();
        body.number(minutes, 1).text(" ").text(suffix);
        if let Err(NotifyError::Denied) = notify::post(title, body.as_str(), Level::Info) {
            self.notify_blocked = true;
        }
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let unit = ev.unit;
        let area = Area { w: ev.area_w, h: ev.area_h };
        let elapsed = self.elapsed_ms(ev.uptime_ms);

        let mut line = Line::new();
        line.number(elapsed / 60000 % 100, 2)
            .text(":")
            .number(elapsed / 1000 % 60, 2)
            .text(".")
            .number(elapsed / 100 % 10, 1);

        let center_x = area.w / 2;
        gfx.text_centered(center_x, unit * 2, unit * 3, TEXT_PRIMARY, 20, line.as_str());

        let words = strings::for_language(ev.lang);
        let caption = if self.running { words[strings::RUNNING] } else { words[strings::STOPPED] };
        gfx.text_centered(center_x, unit * 6 + unit / 2, unit * 55 / 100, TEXT_DIM, 40, caption);

        let toggle = toggle_button(area, unit);
        let label = if self.running { words[strings::STOP] } else { words[strings::START] };
        gfx.button(toggle.x, toggle.y, toggle.w, toggle.h, Some(ev.accent), self.hover == Hover::Toggle, label);
        let reset = reset_button(area, unit);
        gfx.button(reset.x, reset.y, reset.w, reset.h, None, self.hover == Hover::Reset, words[strings::RESET]);
    }
}

impl Default for StopwatchApp {
    fn default() -> Self {
        Self::new()
    }
}

impl RingApp for StopwatchApp {
    fn handle(&mut self, event: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match event.op {
            OP_UPDATE => self.update(event),
            OP_DRAW => {
                self.draw(event, gfx);
                RET_NONE
            }
            nc_abi::event::OP_INIT => RET_REDRAW,
            _ => RET_NONE,
        }
    }
}
