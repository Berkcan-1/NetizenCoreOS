pub const OUTPUT_RATE: u32 = 48_000;
pub const MAX_NOTES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Timbre {
    Bell,
    Soft,
    Tick,
    Buzz,
}

#[derive(Clone, Copy)]
pub struct Note {
    pub start_ms: u16,
    pub len_ms: u16,
    pub freq: u16,
    pub level: u8,
    pub timbre: Timbre,
}

pub const fn note(start_ms: u16, len_ms: u16, freq: u16, level: u8, timbre: Timbre) -> Note {
    Note { start_ms, len_ms, freq, level, timbre }
}

pub fn sine_q15(phase: u16) -> i32 {
    let negative = phase & 0x8000 != 0;
    let x = (phase & 0x7FFF) as i32;
    let parabola = (x * (32768 - x)) >> 13;
    let squared = (parabola * parabola) >> 15;
    let value = parabola + (((squared - parabola) * 225) / 1000);
    let value = value.clamp(0, 32767);
    if negative { -value } else { value }
}

fn timbre_wave(timbre: Timbre, phase: u32) -> i32 {
    let p1 = (phase >> 16) as u16;
    let p2 = (phase.wrapping_mul(2) >> 16) as u16;
    let p3 = (phase.wrapping_mul(3) >> 16) as u16;
    let p5 = (phase.wrapping_mul(5) >> 16) as u16;
    match timbre {
        Timbre::Soft => sine_q15(p1),
        Timbre::Bell => (sine_q15(p1) * 20 + sine_q15(p2) * 7 + sine_q15(p3) * 3) / 30,
        Timbre::Tick => (sine_q15(p1) * 2 + sine_q15(p2)) / 3,
        Timbre::Buzz => (sine_q15(p1) * 10 + sine_q15(p3) * 5 + sine_q15(p5) * 3) / 18,
    }
}

fn envelope_q15(timbre: Timbre, position: u32, length: u32) -> i32 {
    if length == 0 || position >= length {
        return 0;
    }
    let attack = match timbre {
        Timbre::Tick => 48,
        Timbre::Buzz => 360,
        _ => 240,
    }
    .min(length / 2)
    .max(1);
    let fraction = (((length - position) as u64 * 32767) / length as u64) as i32;
    let squared = (fraction * fraction) >> 15;
    let decay = match timbre {
        Timbre::Tick => (squared * squared) >> 15,
        Timbre::Buzz => fraction,
        _ => squared,
    };
    if position < attack {
        ((decay as i64 * position as i64) / attack as i64) as i32
    } else {
        decay
    }
}

#[derive(Clone, Copy)]
pub struct SynthVoice {
    notes: &'static [Note],
    frame: u32,
    total: u32,
    phase: [u32; MAX_NOTES],
    increment: [u32; MAX_NOTES],
}

fn ms_to_frames(ms: u16) -> u32 {
    (ms as u32) * (OUTPUT_RATE / 1000)
}

impl SynthVoice {
    pub fn new(notes: &'static [Note]) -> Self {
        let count = notes.len().min(MAX_NOTES);
        let mut total = 0u32;
        let mut increment = [0u32; MAX_NOTES];
        for (index, entry) in notes.iter().take(count).enumerate() {
            let end = ms_to_frames(entry.start_ms) + ms_to_frames(entry.len_ms);
            total = total.max(end);
            increment[index] = (((entry.freq as u64) << 32) / OUTPUT_RATE as u64) as u32;
        }
        Self { notes: &notes[..count], frame: 0, total, phase: [0; MAX_NOTES], increment }
    }

    pub fn position_frames(&self) -> u32 {
        self.frame
    }

    pub fn next_sample(&mut self) -> Option<i32> {
        if self.frame >= self.total {
            return None;
        }
        let mut sum = 0i32;
        for (index, entry) in self.notes.iter().enumerate() {
            let start = ms_to_frames(entry.start_ms);
            let length = ms_to_frames(entry.len_ms);
            if self.frame < start || self.frame >= start + length {
                continue;
            }
            let wave = timbre_wave(entry.timbre, self.phase[index]);
            let envelope = envelope_q15(entry.timbre, self.frame - start, length);
            let shaped = ((wave as i64 * envelope as i64) >> 15) as i32;
            sum += (shaped * entry.level as i32) >> 8;
            self.phase[index] = self.phase[index].wrapping_add(self.increment[index]);
        }
        self.frame += 1;
        Some(sum)
    }
}
