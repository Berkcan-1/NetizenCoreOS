use crate::adpcm::AdpcmReader;
use crate::stream::Streams;
use crate::synth::{SynthVoice, OUTPUT_RATE};
use crate::theme::{profile, Sound};

pub const MAX_VOICES: usize = 12;
const DUPLICATE_WINDOW_FRAMES: u32 = 1_920;
const LIMITER_KNEE: i32 = 22_000;

#[derive(Clone, Copy)]
struct AdpcmVoice {
    reader: AdpcmReader,
    step: u32,
    position: u32,
    previous: (i32, i32),
    next: (i32, i32),
    exhausted: bool,
}

impl AdpcmVoice {
    fn new(mut reader: AdpcmReader) -> Option<Self> {
        let previous = reader.next_frame()?;
        let next = reader.next_frame().unwrap_or((0, 0));
        let step = (((reader.rate() as u64) << 16) / OUTPUT_RATE as u64) as u32;
        Some(Self { reader, step, position: 0, previous, next, exhausted: false })
    }

    fn next_frame(&mut self) -> Option<(i32, i32)> {
        while self.position >= 65_536 {
            self.position -= 65_536;
            self.previous = self.next;
            match self.reader.next_frame() {
                Some(frame) => self.next = frame,
                None => {
                    if self.exhausted {
                        return None;
                    }
                    self.exhausted = true;
                    self.next = (0, 0);
                }
            }
        }
        let fraction = (self.position >> 1) as i32;
        let left = self.previous.0 + (((self.next.0 - self.previous.0) * fraction) >> 15);
        let right = self.previous.1 + (((self.next.1 - self.previous.1) * fraction) >> 15);
        self.position += self.step;
        Some((left, right))
    }
}

#[derive(Clone, Copy)]
enum Source {
    Empty,
    Synth(SynthVoice),
    Adpcm(AdpcmVoice),
}

#[derive(Clone, Copy)]
struct Voice {
    source: Source,
    sound: u8,
    priority: u8,
    gain: u16,
    serial: u32,
}

const EMPTY_VOICE: Voice = Voice { source: Source::Empty, sound: 0, priority: 0, gain: 0, serial: 0 };

pub struct Mixer {
    voices: [Voice; MAX_VOICES],
    serial: u32,
    master: i32,
    target: i32,
    pub streams: Streams,
}

pub fn limit(value: i32) -> i16 {
    let magnitude = value.abs();
    if magnitude <= LIMITER_KNEE {
        return value as i16;
    }
    let headroom = 32_767 - LIMITER_KNEE;
    let over = magnitude - LIMITER_KNEE;
    let shaped = LIMITER_KNEE + ((over as i64 * headroom as i64) / (over + headroom) as i64) as i32;
    let shaped = shaped.min(32_767);
    if value < 0 { -shaped as i16 } else { shaped as i16 }
}

impl Mixer {
    pub fn new() -> Self {
        Self { voices: [EMPTY_VOICE; MAX_VOICES], serial: 0, master: 0, target: 0, streams: Streams::new() }
    }

    pub fn set_gain(&mut self, volume_percent: u8, muted: bool) {
        let volume = volume_percent.min(100) as i32;
        self.target = if muted { 0 } else { volume * volume * 32_767 / 10_000 };
    }

    pub fn active(&self) -> bool {
        self.voices.iter().any(|voice| !matches!(voice.source, Source::Empty)) || self.streams.any_pending()
    }

    pub fn voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| !matches!(voice.source, Source::Empty)).count() + self.streams.count()
    }

    fn allocate(&mut self, priority: u8) -> Option<usize> {
        if let Some(index) = self.voices.iter().position(|voice| matches!(voice.source, Source::Empty)) {
            return Some(index);
        }
        let mut victim: Option<usize> = None;
        for (index, voice) in self.voices.iter().enumerate() {
            if voice.priority > priority {
                continue;
            }
            match victim {
                None => victim = Some(index),
                Some(current) => {
                    let best = &self.voices[current];
                    if voice.priority < best.priority
                        || (voice.priority == best.priority && voice.serial < best.serial)
                    {
                        victim = Some(index);
                    }
                }
            }
        }
        victim
    }

    pub fn play_synth(&mut self, sound: Sound) -> bool {
        let id = sound as u8;
        for voice in self.voices.iter() {
            if let Source::Synth(synth) = &voice.source {
                if voice.sound == id && synth.position_frames() < DUPLICATE_WINDOW_FRAMES {
                    return false;
                }
            }
        }
        let info = profile(sound);
        let Some(slot) = self.allocate(info.priority) else { return false; };
        self.serial = self.serial.wrapping_add(1);
        self.voices[slot] = Voice {
            source: Source::Synth(SynthVoice::new(info.notes)),
            sound: id,
            priority: info.priority,
            gain: info.gain,
            serial: self.serial,
        };
        true
    }

    pub fn play_adpcm(&mut self, sound: Sound, blob: &'static [u8]) -> bool {
        let Some(reader) = AdpcmReader::new(blob) else { return false; };
        let Some(voice) = AdpcmVoice::new(reader) else { return false; };
        let info = profile(sound);
        let Some(slot) = self.allocate(info.priority) else { return false; };
        self.serial = self.serial.wrapping_add(1);
        self.voices[slot] = Voice {
            source: Source::Adpcm(voice),
            sound: sound as u8,
            priority: info.priority,
            gain: info.gain,
            serial: self.serial,
        };
        true
    }

    pub fn stop_all(&mut self) {
        self.voices = [EMPTY_VOICE; MAX_VOICES];
        self.streams.clear();
    }

    pub fn mix(&mut self, out: &mut [i16]) {
        for frame in out.chunks_exact_mut(2) {
            let mut left = 0i32;
            let mut right = 0i32;
            for voice in self.voices.iter_mut() {
                match &mut voice.source {
                    Source::Empty => {}
                    Source::Synth(synth) => match synth.next_sample() {
                        Some(sample) => {
                            let scaled = (sample * voice.gain as i32) >> 8;
                            left += scaled;
                            right += scaled;
                        }
                        None => voice.source = Source::Empty,
                    },
                    Source::Adpcm(stream) => match stream.next_frame() {
                        Some((l, r)) => {
                            left += (l * voice.gain as i32) >> 8;
                            right += (r * voice.gain as i32) >> 8;
                        }
                        None => voice.source = Source::Empty,
                    },
                }
            }
            let (stream_left, stream_right) = self.streams.mix_frame();
            left += stream_left;
            right += stream_right;
            if self.master != self.target {
                let delta = (self.target - self.master) / 64;
                self.master = if delta == 0 { self.target } else { self.master + delta };
            }
            let left = (((left * 7) >> 3) as i64 * self.master as i64 >> 15) as i32;
            let right = (((right * 7) >> 3) as i64 * self.master as i64 >> 15) as i32;
            frame[0] = limit(left);
            frame[1] = limit(right);
        }
        self.streams.reap();
    }
}
