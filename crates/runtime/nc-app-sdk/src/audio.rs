//! Uygulama sesi. Yetki: manifestte `audio.play`.
//!
//! * `play(Sound)`: sistem ses temasından kısa bir ses (açılış/kapanış/kilit sesleri uygulamalara kapalıdır).
//! * `Stream`: kendi PCM'ini çalar (s16le, 8000–48000 Hz, mono ya da stereo). Bloklamaz: `write` tampon doluysa
//!   az kare kabul eder, kalanını sonraki olayda yeniden gönder. Tampon yaklaşık yarım saniyedir; her olayda
//!   `status().free` kadar yazmak yeterlidir. Uygulama kapanınca akışlar çekirdekçe kapatılır.
//!
//! Ana ses düzeyi (Ayarlar/ses tuşları/sessiz) tüm uygulama seslerine uygulanır.

use crate::shell;
use nc_abi::shell as wire;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Click,
    Open,
    Close,
    Notify,
    Success,
    Warning,
    Error,
    Tick,
    Connect,
    Disconnect,
}

impl Sound {
    fn id(self) -> u32 {
        match self {
            Sound::Click => wire::SOUND_CLICK,
            Sound::Open => wire::SOUND_OPEN,
            Sound::Close => wire::SOUND_CLOSE,
            Sound::Notify => wire::SOUND_NOTIFY,
            Sound::Success => wire::SOUND_SUCCESS,
            Sound::Warning => wire::SOUND_WARNING,
            Sound::Error => wire::SOUND_ERROR,
            Sound::Tick => wire::SOUND_TICK,
            Sound::Connect => wire::SOUND_CONNECT,
            Sound::Disconnect => wire::SOUND_DISCONNECT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioError {
    Denied,
    NoDevice,
    Limit,
    NoStream,
    BadRequest,
    Failed,
}

fn map(status: i32) -> AudioError {
    match status {
        wire::ERR_DENIED => AudioError::Denied,
        wire::ERR_NO_DEVICE => AudioError::NoDevice,
        wire::ERR_LIMIT => AudioError::Limit,
        wire::ERR_NO_STREAM => AudioError::NoStream,
        wire::ERR_BAD_REQUEST => AudioError::BadRequest,
        _ => AudioError::Failed,
    }
}

fn simple(op: u8, arg0: u32, arg1: u32, payload: &[u8]) -> Result<shell::Reply, AudioError> {
    let reply = shell::call(op, arg0, arg1, payload).map_err(map)?;
    if reply.status != 0 {
        return Err(map(reply.status));
    }
    Ok(reply)
}

pub fn play(sound: Sound) -> Result<(), AudioError> {
    simple(wire::OP_SOUND_PLAY, sound.id(), 0, &[]).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamStatus {
    pub queued_frames: u32,
    pub free_frames: u32,
}

#[derive(Debug)]
pub struct Stream {
    id: u32,
    channels: u32,
}

impl Stream {
    pub fn open(sample_rate: u32, channels: u32) -> Result<Stream, AudioError> {
        let reply = simple(wire::OP_STREAM_OPEN, sample_rate, channels, &[])?;
        let id = reply.u32_at(0).ok_or(AudioError::Failed)?;
        Ok(Stream { id, channels })
    }

    pub fn write(&self, pcm: &[i16]) -> Result<usize, AudioError> {
        let channels = self.channels as usize;
        let usable = pcm.len() / channels * channels;
        let mut sent = 0usize;
        let mut bytes = [0u8; wire::MAX_PCM_WRITE / 2];
        while sent < usable {
            let take = (bytes.len() / 2).min(usable - sent) / channels * channels;
            for (index, sample) in pcm[sent..sent + take].iter().enumerate() {
                bytes[index * 2..index * 2 + 2].copy_from_slice(&sample.to_le_bytes());
            }
            let reply = simple(wire::OP_STREAM_WRITE, self.id, 0, &bytes[..take * 2])?;
            let accepted = reply.u32_at(0).ok_or(AudioError::Failed)? as usize * channels;
            sent += accepted;
            if accepted < take {
                break;
            }
        }
        Ok(sent / channels)
    }

    pub fn status(&self) -> Result<StreamStatus, AudioError> {
        let reply = simple(wire::OP_STREAM_STATUS, self.id, 0, &[])?;
        Ok(StreamStatus {
            queued_frames: reply.u32_at(0).ok_or(AudioError::Failed)?,
            free_frames: reply.u32_at(1).ok_or(AudioError::Failed)?,
        })
    }

    pub fn set_volume(&self, percent: u8) -> Result<(), AudioError> {
        simple(wire::OP_STREAM_VOLUME, self.id, percent.min(100) as u32, &[]).map(|_| ())
    }

    pub fn close(self, discard_pending: bool) -> Result<(), AudioError> {
        let result = simple(wire::OP_STREAM_CLOSE, self.id, discard_pending as u32, &[]).map(|_| ());
        core::mem::forget(self);
        result
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = shell::call(wire::OP_STREAM_CLOSE, self.id, 0, &[]);
    }
}
