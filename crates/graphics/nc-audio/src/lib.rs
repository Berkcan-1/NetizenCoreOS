#![cfg_attr(not(test), no_std)]

mod adpcm;
mod mixer;
mod stream;
mod synth;
mod theme;

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use mixer::Mixer;
#[cfg(target_arch = "x86_64")]
use nc_driver::ac97::Ac97Output as HardwareOutput;
use nc_hal::{AudioError, AudioStream};

#[cfg(not(target_arch = "x86_64"))]
struct HardwareOutput;

#[cfg(not(target_arch = "x86_64"))]
impl HardwareOutput {
    fn probe() -> Result<Self, AudioError> {
        Err(AudioError::DeviceNotReady)
    }
}

#[cfg(not(target_arch = "x86_64"))]
impl AudioStream for HardwareOutput {
    fn stream_open(&mut self, _rate: u32) -> Result<(), AudioError> {
        Err(AudioError::DeviceNotReady)
    }
    fn stream_slot_frames(&self) -> usize {
        0
    }
    fn stream_slot_count(&self) -> usize {
        0
    }
    fn stream_queued_slots(&mut self) -> usize {
        0
    }
    fn stream_push_slot(&mut self, _samples: &[i16]) -> Result<(), AudioError> {
        Err(AudioError::DeviceNotReady)
    }
    fn stream_take_halt(&mut self) -> bool {
        false
    }
    fn stream_kick(&mut self) {}
    fn stream_stop(&mut self) {}
}

pub use stream::{StreamError, MAX_STREAMS, MAX_STREAMS_PER_OWNER};
pub use theme::Sound;

const LEAD_START: usize = 12;
const LEAD_MAX: usize = 24;
const LEAD_STEP: usize = 4;
const SLOT_SAMPLES: usize = 1024;

static VOLUME: AtomicU8 = AtomicU8::new(80);
static MUTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub available: bool,
    pub underruns: u32,
    pub slots_pushed: u32,
    pub voices: usize,
    pub lead_slots: usize,
}

struct Engine {
    out: HardwareOutput,
    mixer: Mixer,
    scratch: [i16; SLOT_SAMPLES],
    underruns: u32,
    pushed: u32,
    pending: bool,
    lead: usize,
}

static mut ENGINE: Option<Engine> = None;

fn engine() -> Option<&'static mut Engine> {
    unsafe { (*core::ptr::addr_of_mut!(ENGINE)).as_mut() }
}

impl Engine {
    fn sync_gain(&mut self) {
        self.mixer.set_gain(VOLUME.load(Ordering::Relaxed), MUTED.load(Ordering::Relaxed));
    }

    fn pump(&mut self) {
        self.sync_gain();
        if !self.mixer.active() {
            self.pending = false;
            return;
        }
        let mut queued = self.out.stream_queued_slots();
        if self.out.stream_take_halt() && self.pending {
            self.underruns = self.underruns.wrapping_add(1);
            self.lead = (self.lead + LEAD_STEP).min(LEAD_MAX);
        }
        let lead = self.lead;
        let mut guard = 0;
        while queued < lead && self.mixer.active() && guard < lead {
            self.mixer.mix(&mut self.scratch);
            if self.out.stream_push_slot(&self.scratch).is_err() {
                break;
            }
            self.pushed = self.pushed.wrapping_add(1);
            queued += 1;
            guard += 1;
        }
        self.out.stream_kick();
        self.pending = self.mixer.active();
    }
}

pub fn init() -> Result<(), AudioError> {
    if engine().is_some() {
        return Ok(());
    }
    let mut out = HardwareOutput::probe()?;
    out.stream_open(synth::OUTPUT_RATE)?;
    let mut mixer = Mixer::new();
    mixer.set_gain(VOLUME.load(Ordering::Relaxed), MUTED.load(Ordering::Relaxed));
    unsafe {
        *core::ptr::addr_of_mut!(ENGINE) = Some(Engine {
            out,
            mixer,
            scratch: [0; SLOT_SAMPLES],
            underruns: 0,
            pushed: 0,
            pending: false,
            lead: LEAD_START,
        });
    }
    Ok(())
}

pub fn available() -> bool {
    engine().is_some()
}

pub fn play(sound: Sound) -> bool {
    let Some(engine) = engine() else { return false; };
    let accepted = match sound {
        Sound::Startup => match nc_assets::startup_sound() {
            Some(blob) => engine.mixer.play_adpcm(Sound::Startup, blob) || engine.mixer.play_synth(Sound::Startup),
            None => engine.mixer.play_synth(Sound::Startup),
        },
        other => engine.mixer.play_synth(other),
    };
    if accepted {
        engine.pump();
    }
    accepted
}

pub fn pump() {
    if let Some(engine) = engine() {
        engine.pump();
    }
}

pub fn busy() -> bool {
    match engine() {
        Some(engine) => engine.mixer.active(),
        None => false,
    }
}

pub fn stream_open(owner: u32, rate: u32, channels: u32) -> Result<u32, StreamError> {
    let engine = engine().ok_or(StreamError::NoDevice)?;
    engine.mixer.streams.open(owner, rate, channels)
}

pub fn stream_write(owner: u32, id: u32, pcm: &[u8]) -> Result<usize, StreamError> {
    let engine = engine().ok_or(StreamError::NoDevice)?;
    engine.mixer.streams.write(owner, id, pcm)
}

pub fn stream_status(owner: u32, id: u32) -> Result<(usize, usize), StreamError> {
    let engine = engine().ok_or(StreamError::NoDevice)?;
    engine.mixer.streams.status(owner, id)
}

pub fn stream_volume(owner: u32, id: u32, percent: u32) -> Result<(), StreamError> {
    let engine = engine().ok_or(StreamError::NoDevice)?;
    engine.mixer.streams.set_volume(owner, id, percent)
}

pub fn stream_close(owner: u32, id: u32, discard: bool) -> Result<(), StreamError> {
    let engine = engine().ok_or(StreamError::NoDevice)?;
    engine.mixer.streams.close(owner, id, discard)
}

pub fn stream_release_owner(owner: u32) {
    if let Some(engine) = engine() {
        engine.mixer.streams.release_owner(owner);
    }
}

pub fn stop_all() {
    if let Some(engine) = engine() {
        engine.mixer.stop_all();
        engine.out.stream_stop();
        engine.pending = false;
    }
}

pub fn set_volume(percent: u8) {
    VOLUME.store(percent.min(100), Ordering::Relaxed);
}

pub fn volume() -> u8 {
    VOLUME.load(Ordering::Relaxed)
}

pub fn set_muted(muted: bool) {
    MUTED.store(muted, Ordering::Relaxed);
}

pub fn muted() -> bool {
    MUTED.load(Ordering::Relaxed)
}

pub fn stats() -> Stats {
    match engine() {
        Some(engine) => Stats {
            available: true,
            underruns: engine.underruns,
            slots_pushed: engine.pushed,
            voices: engine.mixer.voice_count(),
            lead_slots: engine.lead,
        },
        None => Stats::default(),
    }
}

#[cfg(test)]
mod tests;
