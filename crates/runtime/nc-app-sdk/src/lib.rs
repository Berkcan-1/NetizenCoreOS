//! Ring 3 uygulama SDK'sı (olay-tabanlı model; sözleşme: `nc-abi`).
//!
//! Bir uygulama `RingApp::handle` yazar; SDK olayı okur, çizim akışını tamponlar, `OP_DRAW`
//! sonunda `SYS_GUI_DRAW` ile gönderir ve `SYS_APP_DONE` ile döner. Uygulama durumu çağrılar
//! arasında uygulamanın kendi belleğinde (`static`) kalır.
#![no_std]

pub use nc_abi;

pub mod audio;
pub mod ecosystem;
pub mod gfx;
pub mod line;
pub mod notify;
mod shell;
pub mod sys;
pub mod widget;

pub use ecosystem::{apps, camera, contacts, ime, media, perm, pkg, security, settings, status, storage, telephony};

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

#[cfg(test)]
mod shell_tests {
    extern crate std;
    use crate::audio::{self, AudioError, Sound};
    use crate::notify::{self, Level, NotifyError, Permission};
    use crate::shell;
    use nc_abi::shell as wire;
    use std::sync::Mutex;
    use std::vec::Vec;

    static LOCK: Mutex<()> = Mutex::new(());
    static LOG: Mutex<Vec<(u8, u32, u32, Vec<u8>)>> = Mutex::new(Vec::new());
    static MODE: Mutex<u8> = Mutex::new(0);

    fn reply(out: &mut [u8], status: i32, payload: &[u8]) -> i64 {
        wire::encode_response_header(out, status, payload.len()).unwrap();
        out[wire::RESP_HEADER..wire::RESP_HEADER + payload.len()].copy_from_slice(payload);
        (wire::RESP_HEADER + payload.len()) as i64
    }

    fn fake(request: &[u8], response: &mut [u8]) -> i64 {
        let parsed = wire::parse_request(request).unwrap();
        LOG.lock().unwrap().push((parsed.op, parsed.arg0, parsed.arg1, parsed.payload.to_vec()));
        let mode = *MODE.lock().unwrap();
        match parsed.op {
            wire::OP_STREAM_OPEN => reply(response, 0, &7u32.to_le_bytes()),
            wire::OP_STREAM_WRITE => {
                let frames = parsed.payload.len() as u32 / 4;
                let accepted = if mode == 1 { frames.min(1000) } else { frames };
                reply(response, 0, &accepted.to_le_bytes())
            }
            wire::OP_STREAM_STATUS => {
                let mut p = Vec::new();
                p.extend_from_slice(&123u32.to_le_bytes());
                p.extend_from_slice(&456u32.to_le_bytes());
                reply(response, 0, &p)
            }
            wire::OP_NOTIFY_STATE => reply(response, 0, &[wire::PERM_ALLOWED]),
            wire::OP_NOTIFY_POST if mode == 2 => reply(response, wire::ERR_DENIED, &[]),
            wire::OP_SOUND_PLAY if mode == 3 => reply(response, wire::ERR_NO_DEVICE, &[]),
            _ => reply(response, 0, &[]),
        }
    }

    fn setup(mode: u8) -> std::sync::MutexGuard<'static, ()> {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        LOG.lock().unwrap().clear();
        *MODE.lock().unwrap() = mode;
        shell::set_transport(fake);
        guard
    }

    #[test]
    fn sound_play_sends_the_right_id_and_maps_errors() {
        let _g = setup(0);
        audio::play(Sound::Error).unwrap();
        assert_eq!(LOG.lock().unwrap()[0].0, wire::OP_SOUND_PLAY);
        assert_eq!(LOG.lock().unwrap()[0].1, wire::SOUND_ERROR);
        drop(_g);
        let _g = setup(3);
        assert_eq!(audio::play(Sound::Click), Err(AudioError::NoDevice));
    }

    #[test]
    fn stream_write_chunks_large_buffers_and_counts_frames() {
        let _g = setup(0);
        let stream = audio::Stream::open(44_100, 2).unwrap();
        let pcm: Vec<i16> = (0..20_000).map(|n| (n % 3000) as i16).collect();
        let frames = stream.write(&pcm).unwrap();
        assert_eq!(frames, 10_000);
        let log = LOG.lock().unwrap();
        let writes: Vec<_> = log.iter().filter(|e| e.0 == wire::OP_STREAM_WRITE).collect();
        assert!(writes.len() >= 3);
        assert!(writes.iter().all(|w| w.1 == 7 && w.3.len() <= wire::MAX_PCM_WRITE / 2 && w.3.len() % 4 == 0));
        let total: usize = writes.iter().map(|w| w.3.len()).sum();
        assert_eq!(total, 40_000);
        assert_eq!(&writes[0].3[0..2], &0i16.to_le_bytes());
        assert_eq!(&writes[0].3[2..4], &1i16.to_le_bytes());
        drop(log);
        drop(stream);
    }

    #[test]
    fn stream_write_stops_when_kernel_buffer_is_full() {
        let _g = setup(1);
        let stream = audio::Stream::open(48_000, 1).unwrap();
        let pcm = [100i16; 6000];
        assert_eq!(stream.write(&pcm).unwrap(), 1000);
        let status = stream.status().unwrap();
        assert_eq!((status.queued_frames, status.free_frames), (123, 456));
        stream.set_volume(250).unwrap();
        assert_eq!(LOG.lock().unwrap().last().unwrap().2, 100);
        stream.close(true).unwrap();
        let last = LOG.lock().unwrap().last().cloned().unwrap();
        assert_eq!((last.0, last.2), (wire::OP_STREAM_CLOSE, 1));
    }

    #[test]
    fn dropping_a_stream_closes_it_without_discarding() {
        let _g = setup(0);
        {
            let _s = audio::Stream::open(8_000, 1).unwrap();
        }
        let last = LOG.lock().unwrap().last().cloned().unwrap();
        assert_eq!((last.0, last.2), (wire::OP_STREAM_CLOSE, 0));
    }

    #[test]
    fn notify_post_encodes_and_maps_denial() {
        let _g = setup(0);
        notify::post("Başlık", "gövde", Level::Warning).unwrap();
        let entry = LOG.lock().unwrap()[0].clone();
        assert_eq!((entry.0, entry.1), (wire::OP_NOTIFY_POST, wire::LEVEL_WARNING));
        assert_eq!(wire::parse_notify_payload(&entry.3), Some(("Başlık", "gövde")));
        assert_eq!(notify::permission(), Ok(Permission::Allowed));
        assert_eq!(notify::post(&"x".repeat(65), "", Level::Info), Err(NotifyError::BadRequest));
        drop(_g);
        let _g = setup(2);
        assert_eq!(notify::post("a", "b", Level::Info), Err(NotifyError::Denied));
    }
}
