#![allow(dead_code)]
#![no_std]
#![no_main]

mod aero;
mod clock;
mod glyphs;
mod input;
mod layout;
mod model;
mod motion;
mod overlay;
mod persist;
mod shell;
mod view;
mod widgets;

use nc_abi::event::{
    unpack_locked, EventBlock, OP_DRAW, OP_INIT, OP_RESUME, OP_UPDATE, RET_ANIMATING, RET_NONE,
    RET_REDRAW,
};
use nc_app_sdk::{dispatch, sys, Gfx, RingApp};

use layout::Geo;
use shell::{HomeShell, STATUS_POLL_MS};
use view::wall_clock;

impl HomeShell {
    fn draw(&self, ev: &EventBlock, g: &mut Gfx<'_>) {
        let geo = Geo::new(ev, &self.prefs);
        // Ekran kilitliyken hicbir uygulama veya ana ekran icerigi gosterilmez!
        if self.locked || unpack_locked(ev.flags) {
            self.draw_wallpaper(g, ev, &geo);
            self.draw_lock_screen(g, ev, &geo);
            self.draw_status(g, ev, &geo);
            return;
        }
        let covered = self.app_p >= 232 && !self.recents_open;
        if self.rec_p >= 250 {
            self.draw_wallpaper(g, ev, &geo);
            self.draw_recents(g, ev, &geo);
        } else if !covered {
            if self.drawer_p >= 250 {
                self.draw_wallpaper(g, ev, &geo);
            } else {
                self.draw_home(g, ev, &geo);
            }
            self.draw_drawer(g, ev, &geo);
            self.draw_recents(g, ev, &geo);
        }
        self.draw_launch(g, ev, &geo);
        self.draw_cc_dim(g, &geo);
        self.draw_folder(g, ev, &geo);
        self.draw_search(g, ev, &geo);
        self.draw_wpick(g, ev, &geo);
        self.draw_customize(g, ev, &geo);
        self.draw_status(g, ev, &geo);
        self.draw_cc(g, ev, &geo);
        self.draw_nav(g, ev, &geo);
    }
}

impl RingApp for HomeShell {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.refresh_registry();
                self.refresh_lock_status();
                self.refresh_status(ev.uptime_ms);
                let geo = Geo::new(ev, &self.prefs);
                self.sync(ev, &geo);
                self.stamp = wall_clock() / 60;
                self.last_flags = ev.flags;
                let moving = self.step(ev.uptime_ms, geo.w);
                RET_REDRAW | if moving { RET_ANIMATING } else { 0 }
            }
            OP_UPDATE => {
                let geo = Geo::new(ev, &self.prefs);
                let mut redraw = false;
                let stamp = wall_clock() / 60;
                if stamp != self.stamp || ev.flags != self.last_flags {
                    self.stamp = stamp;
                    self.last_flags = ev.flags;
                    if self.locked {
                        self.refresh_lock_status();
                    }
                    let _ = self.refresh_status(ev.uptime_ms);
                    redraw = true;
                } else if ev.uptime_ms.wrapping_sub(self.last_status_ms) >= STATUS_POLL_MS {
                    // Event-driven status refresh; skip expensive widget polling if covered by app unless CC open
                    let need_status = !self.app_open || self.cc_open || self.cc_p > 0;
                    if need_status && self.refresh_status(ev.uptime_ms) {
                        redraw = true;
                    } else {
                        self.last_status_ms = ev.uptime_ms;
                    }
                }
                self.sync(ev, &geo);
                if self.handle_key(ev) {
                    redraw = true;
                }
                if self.pointer_update(ev, &geo) {
                    redraw = true;
                }
                let moving = self.step(ev.uptime_ms, geo.w);
                if moving {
                    RET_REDRAW | RET_ANIMATING
                } else if redraw {
                    RET_REDRAW
                } else {
                    RET_NONE
                }
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                if self.animating() {
                    RET_ANIMATING
                } else {
                    RET_NONE
                }
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: HomeShell = HomeShell::new();
static mut COMMANDS: [u8; 16384] = [0; 16384];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
