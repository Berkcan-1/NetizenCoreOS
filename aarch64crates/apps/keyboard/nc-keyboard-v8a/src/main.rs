//! NetizenCore Modular Ring 3 / EL0 System Input Method (`nc.keyboard`).
//!
//! Runs as an isolated Ring 3 application packaged as `nc.keyboard.ntzc`.
//! Hidden from NetizenHome's launcher grid; visible in Settings -> Apps.
//! Whenever ANY application requests a keyboard (`nc_app_sdk::ecosystem::ime::show()`),
//! the kernel composites this app's overlay surface at the bottom of the screen,
//! routes tray touches here, and injects committed keys (`ime::commit_key`) into
//! the requesting application's `EventBlock.key`.
#![no_std]
#![no_main]

use nc_abi::event::{
    unpack_dark_theme, EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, PTR_DOWN,
    RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_app_sdk::ecosystem::ime;
use nc_app_sdk::{dispatch, sys, Gfx, RingApp};
use nc_mobile_ui::keyboard::{KeyEvent, KeyboardLang, VirtualKeyboard};
use nc_mobile_ui::{Dp, Palette, BLUE};

static mut COMMANDS: [u8; 65_536] = [0u8; 65_536];

struct KeyboardApp {
    kb: VirtualKeyboard,
    was_down: bool,
}

impl KeyboardApp {
    const fn new() -> Self {
        let mut kb = VirtualKeyboard::new(KeyboardLang::TurkishQ);
        kb.visible = true;
        Self { kb, was_down: false }
    }
}

impl RingApp for KeyboardApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.kb.show();
                self.kb.clear_press();
                self.was_down = false;
                RET_REDRAW
            }
            OP_PAUSE => {
                self.kb.clear_press();
                self.was_down = false;
                RET_NONE
            }
            OP_UPDATE => {
                let down = (ev.pointer_flags & PTR_DOWN) != 0;
                let mut redraw = false;
                if down && !self.was_down {
                    let dp = Dp::new(ev.area_w as i32, ev.area_h as i32);
                    match self.kb.handle_tap(&dp, ev.area_h as i32, ev.pointer_x, ev.pointer_y) {
                        KeyEvent::Char(c) => {
                            let _ = ime::commit_key(key::CHAR_FLAG | (c as u32));
                            redraw = true;
                        }
                        KeyEvent::Backspace => {
                            let _ = ime::commit_key(key::BACKSPACE);
                            redraw = true;
                        }
                        KeyEvent::Enter => {
                            let _ = ime::commit_key(key::ENTER);
                            redraw = true;
                        }
                        KeyEvent::Dismiss => {
                            let _ = ime::hide();
                            redraw = true;
                        }
                        KeyEvent::StateChanged => {
                            redraw = true;
                        }
                        KeyEvent::None => {}
                    }
                } else if !down && self.was_down {
                    self.kb.clear_press();
                    redraw = true;
                }
                self.was_down = down;
                if redraw { RET_REDRAW } else { RET_NONE }
            }
            OP_DRAW => {
                let dark = unpack_dark_theme(ev.flags);
                let pal = Palette::for_dark(dark);
                let accent = if ev.accent != 0 { ev.accent } else { BLUE };
                let dp = Dp::new(ev.area_w as i32, ev.area_h as i32);
                self.kb.draw(gfx, &dp, ev.area_h as i32, &pal, accent, dark);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: KeyboardApp = KeyboardApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        dispatch(
            &mut *core::ptr::addr_of_mut!(APP),
            &mut *core::ptr::addr_of_mut!(COMMANDS),
        )
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
