// kernel/src/ring3/shell.rs
//
// Kabuk hizmeti (`SYS_SHELL_CALL`, sozlesme: crates/foundation/nc-abi/src/shell.rs): uygulama sesi ve bildirimler.
// Bu dosya saf mantiktir (kullanici bellegine dokunmaz); kopyalama ve yetki sarmalayicisi runtime.rs'tedir.
#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_abi::shell as wire;
use nc_app::notify;
use nc_audio::{Sound, StreamError};

pub struct Caller<'a> {
    pub pid: u32,
    pub app_id: &'a str,
    pub caps: u32,
}

fn sound_for(id: u32) -> Option<Sound> {
    Some(match id {
        wire::SOUND_CLICK => Sound::Click,
        wire::SOUND_OPEN => Sound::Open,
        wire::SOUND_CLOSE => Sound::Close,
        wire::SOUND_NOTIFY => Sound::Notify,
        wire::SOUND_SUCCESS => Sound::Success,
        wire::SOUND_WARNING => Sound::Warning,
        wire::SOUND_ERROR => Sound::Error,
        wire::SOUND_TICK => Sound::VolumeTick,
        wire::SOUND_CONNECT => Sound::Connect,
        wire::SOUND_DISCONNECT => Sound::Disconnect,
        _ => return None,
    })
}

fn stream_error(error: StreamError) -> i32 {
    match error {
        StreamError::NoDevice => wire::ERR_NO_DEVICE,
        StreamError::BadFormat => wire::ERR_BAD_REQUEST,
        StreamError::Limit => wire::ERR_LIMIT,
        StreamError::NoStream => wire::ERR_NO_STREAM,
        StreamError::NoMemory => wire::ERR_NO_MEMORY,
    }
}

fn required_caps(op: u8) -> u32 {
    match op {
        wire::OP_NOTIFY_POST | wire::OP_NOTIFY_STATE => nc_caps::NOTIFY_POST,
        _ => nc_caps::AUDIO_PLAY,
    }
}

fn payload_u32(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn handle(caller: &Caller<'_>, op: u8, arg0: u32, arg1: u32, _arg2: u32, payload: &[u8]) -> (i32, Vec<u8>) {
    let required = required_caps(op);
    if caller.caps & required != required {
        return (wire::ERR_DENIED, Vec::new());
    }
    match op {
        wire::OP_SOUND_PLAY => match sound_for(arg0) {
            Some(sound) => {
                if nc_audio::play(sound) {
                    (0, Vec::new())
                } else if nc_audio::available() {
                    (0, Vec::new())
                } else {
                    (wire::ERR_NO_DEVICE, Vec::new())
                }
            }
            None => (wire::ERR_BAD_REQUEST, Vec::new()),
        },
        wire::OP_STREAM_OPEN => {
            if !(wire::MIN_RATE..=wire::MAX_RATE).contains(&arg0) || !(1..=2).contains(&arg1) {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            }
            match nc_audio::stream_open(caller.pid, arg0, arg1) {
                Ok(id) => (0, payload_u32(&[id])),
                Err(error) => (stream_error(error), Vec::new()),
            }
        }
        wire::OP_STREAM_WRITE => {
            if payload.len() > wire::MAX_PCM_WRITE {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            }
            match nc_audio::stream_write(caller.pid, arg0, payload) {
                Ok(frames) => (0, payload_u32(&[frames as u32])),
                Err(error) => (stream_error(error), Vec::new()),
            }
        }
        wire::OP_STREAM_STATUS => match nc_audio::stream_status(caller.pid, arg0) {
            Ok((queued, free)) => (0, payload_u32(&[queued as u32, free as u32])),
            Err(error) => (stream_error(error), Vec::new()),
        },
        wire::OP_STREAM_VOLUME => match nc_audio::stream_volume(caller.pid, arg0, arg1) {
            Ok(()) => (0, Vec::new()),
            Err(error) => (stream_error(error), Vec::new()),
        },
        wire::OP_STREAM_CLOSE => match nc_audio::stream_close(caller.pid, arg0, arg1 == 1) {
            Ok(()) => (0, Vec::new()),
            Err(error) => (stream_error(error), Vec::new()),
        },
        wire::OP_NOTIFY_POST => {
            let Some((title, body)) = wire::parse_notify_payload(payload) else {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            };
            if title.chars().any(|c| c.is_control()) || body.chars().any(|c| c.is_control() && c != '\n') {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            }
            if notify::permission(caller.app_id) == notify::Permission::Denied {
                return (wire::ERR_DENIED, Vec::new());
            }
            let posted = notify::post(notify::Notification {
                app_id: String::from(caller.app_id),
                title: String::from(title),
                body: String::from(body),
                level: notify::Level::from_u32(arg0),
            });
            if posted { (0, Vec::new()) } else { (wire::ERR_LIMIT, Vec::new()) }
        }
        wire::OP_NOTIFY_STATE => {
            let state = match notify::permission(caller.app_id) {
                notify::Permission::Ask => wire::PERM_ASK,
                notify::Permission::Allowed => wire::PERM_ALLOWED,
                notify::Permission::Denied => wire::PERM_DENIED,
            };
            (0, alloc::vec![state])
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

pub fn release(pid: u32) {
    nc_audio::stream_release_owner(pid);
}
