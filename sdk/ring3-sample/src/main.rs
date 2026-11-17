//! Ring 3 örnek uygulaması: düğmeye basınca sayaç artar. Durum `static` içinde kalır; çekirdek her
//! olayda `_start`'tan girer. Çizim, `Gfx` ile komut akışı olarak gönderilir (pencereye kırpılır).
#![no_std]
#![no_main]

use nc_abi::event::{EventBlock, OP_DRAW, OP_UPDATE, RET_NONE, RET_REDRAW};
use nc_app_sdk::audio::{self, Sound, Stream};
use nc_app_sdk::notify::{self, Level};
use nc_app_sdk::{dispatch, pointer, sys, Gfx, Line, RingApp};

const TEXT: u32 = 0x000F_172A;

struct Counter {
    clicks: u32,
}

fn button(ev: &EventBlock) -> (i32, i32, i32, i32) {
    let u = ev.unit;
    (ev.area_w / 2 - u * 5, ev.area_h - u * 4, u * 10, u * 3)
}

fn beep() {
    let Ok(stream) = Stream::open(22_050, 1) else { return; };
    let mut pcm = [0i16; 2_400];
    for (n, sample) in pcm.iter_mut().enumerate() {
        let phase = (n * 880 * 4 / 22_050) % 4;
        let wave = match phase { 0 => 6_000, 1 | 3 => 0, _ => -6_000 };
        let fade = (pcm_len_left(n) as i32).min(400);
        *sample = (wave * fade / 400) as i16;
    }
    let _ = stream.write(&pcm);
    let _ = stream.close(false);
}

fn pcm_len_left(n: usize) -> usize {
    2_400 - n
}

impl RingApp for Counter {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        let (bx, by, bw, bh) = button(ev);
        match ev.op {
            OP_UPDATE => match pointer(ev) {
                Some(p) if p.clicked && p.x >= bx && p.x < bx + bw && p.y >= by && p.y < by + bh => {
                    self.clicks = self.clicks.wrapping_add(1);
                    let _ = audio::play(Sound::Click);
                    if self.clicks % 10 == 0 {
                        beep();
                        let _ = notify::post("Sayaç", "10 tıklama daha tamamlandı", Level::Success);
                    }
                    RET_REDRAW
                }
                _ => RET_NONE,
            },
            OP_DRAW => {
                let mut line = Line::new();
                line.number(self.clicks, 1);
                gfx.text_centered(ev.area_w / 2, ev.unit * 2, ev.unit * 3, TEXT, 20, line.as_str());
                gfx.button(bx, by, bw, bh, Some(ev.accent), false, "+1");
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: Counter = Counter { clicks: 0 };
static mut COMMANDS: [u8; 4096] = [0; 4096];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys::exit(101)
}
