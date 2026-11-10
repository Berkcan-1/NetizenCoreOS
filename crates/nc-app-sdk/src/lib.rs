//! Ring 3 uygulama SDK'sı (olay-tabanlı model; sözleşme: `nc-abi`).
//!
//! Bir uygulama `RingApp::handle` yazar; SDK olayı okur, çizim akışını tamponlar, `OP_DRAW`
//! sonunda `SYS_GUI_DRAW` ile gönderir ve `SYS_APP_DONE` ile döner. Uygulama durumu çağrılar
//! arasında uygulamanın kendi belleğinde (`static`) kalır.
#![no_std]

pub use nc_abi;

pub mod gfx;
pub mod line;
pub mod sys;

use nc_abi::event::{EventBlock, EVENT_SIZE, OP_DRAW, PTR_CLICKED, PTR_DOWN, PTR_HAS, PTR_RIGHT_CLICKED};

pub use gfx::Gfx;
pub use line::Line;

/// Bir Ring 3 uygulaması. Dönüş değeri `nc_abi::event::RET_*` bayraklarıdır.
pub trait RingApp {
    fn handle(&mut self, event: &EventBlock, gfx: &mut Gfx<'_>) -> u32;
}

/// İşaretçi, pencere içeriğine GÖRE (sol üst = 0,0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub down: bool,
    pub clicked: bool,
    pub right_clicked: bool,
}

pub fn pointer(event: &EventBlock) -> Option<Pointer> {
    if event.pointer_flags & PTR_HAS == 0 {
        return None;
    }
    Some(Pointer {
        x: event.pointer_x.wrapping_sub(event.area_x),
        y: event.pointer_y.wrapping_sub(event.area_y),
        down: event.pointer_flags & PTR_DOWN != 0,
        clicked: event.pointer_flags & PTR_CLICKED != 0,
        right_clicked: event.pointer_flags & PTR_RIGHT_CLICKED != 0,
    })
}

/// Çekirdeğin yazdığı olay bloğunu okur ve doğrular.
pub fn read_event() -> Option<EventBlock> {
    let bytes = unsafe { core::slice::from_raw_parts(nc_abi::USER_EVENT_PAGE as *const u8, EVENT_SIZE) };
    EventBlock::decode(bytes)
}

/// Bir olayı işler ve `SYS_APP_DONE` ile döner. `tampon`: çizim akışı için (≤ 16 KiB).
pub fn dispatch<A: RingApp>(app: &mut A, buffer: &mut [u8]) -> ! {
    let ret = match read_event() {
        Some(event) => run(app, &event, buffer),
        None => 0,
    };
    sys::app_done(ret)
}

/// `dispatch`'in çekirdeği: çizim akışını `send(yüzey, bayt)` ile iletir (ana makinede test edilebilir).
pub fn run_with<A: RingApp, S: FnOnce(u64, &[u8])>(app: &mut A, event: &EventBlock, buffer: &mut [u8], send: S) -> u32 {
    let mut gfx = Gfx::new(buffer);
    let ret = app.handle(event, &mut gfx);
    if event.op == OP_DRAW && !gfx.is_empty() {
        send(event.surface, gfx.as_bytes());
    }
    ret
}

pub fn run<A: RingApp>(app: &mut A, event: &EventBlock, buffer: &mut [u8]) -> u32 {
    run_with(app, event, buffer, |surface, bytes| {
        sys::gui_draw(surface, bytes);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_abi::event::{OP_UPDATE, RET_REDRAW};

    struct Probe {
        calls: u32,
    }

    impl RingApp for Probe {
        fn handle(&mut self, event: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
            self.calls += 1;
            if event.op == OP_DRAW {
                gfx.fill_rect(1, 2, 3, 4, 5);
            }
            RET_REDRAW
        }
    }

    fn ev(op: u32) -> EventBlock {
        EventBlock { op, surface: 3, ..Default::default() }
    }

    #[test]
    fn draw_event_sends_stream_once_and_state_persists() {
        let mut app = Probe { calls: 0 };
        let mut buf = [0u8; 256];
        let mut sent: Option<(u64, usize)> = None;
        let ret = run_with(&mut app, &ev(OP_DRAW), &mut buf, |s, b| sent = Some((s, b.len())));
        assert_eq!(ret, RET_REDRAW);
        assert_eq!(sent, Some((3, 24)));
        let mut sent2 = false;
        run_with(&mut app, &ev(OP_UPDATE), &mut buf, |_, _| sent2 = true);
        assert!(!sent2, "yalnizca OP_DRAW cizim gonderir");
        assert_eq!(app.calls, 2);
    }

    #[test]
    fn pointer_is_window_relative() {
        let e = EventBlock {
            area_x: 100,
            area_y: 50,
            pointer_flags: PTR_HAS | PTR_CLICKED,
            pointer_x: 130,
            pointer_y: 45,
            ..Default::default()
        };
        assert_eq!(pointer(&e), Some(Pointer { x: 30, y: -5, down: false, clicked: true, right_clicked: false }));
        assert_eq!(pointer(&EventBlock::default()), None);
    }

    #[test]
    fn gfx_clamps_instead_of_poisoning_the_stream() {
        let mut buf = [0u8; 512];
        let mut gfx = Gfx::new(&mut buf);
        gfx.text(0, 0, 100_000, 0, 250, "x");
        gfx.fill_rect(i32::MIN, i32::MAX, -5, 99_999, 0);
        gfx.panel(0, 0, 0, 100_000, 999, 0, 999, 0, 999);
        gfx.dot(0, 0, i32::MAX, 0, u32::MAX);
        gfx.button(0, 0, -1, 0, None, false, "x");
        assert!(!gfx.dropped());
        assert!(nc_abi::draw::validate(gfx.as_bytes()).is_ok(), "sikistirilmis akis gecerli olmali");
    }
}
