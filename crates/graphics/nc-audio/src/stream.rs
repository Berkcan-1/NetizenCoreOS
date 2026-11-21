extern crate alloc;

use alloc::vec::Vec;

pub const MAX_STREAMS: usize = 4;
pub const MAX_STREAMS_PER_OWNER: usize = 2;
const MIN_RATE: u32 = 8_000;
const MAX_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    NoDevice,
    BadFormat,
    Limit,
    NoStream,
    NoMemory,
}

pub struct AppStream {
    pub(crate) owner: u32,
    pub(crate) id: u32,
    channels: usize,
    ring: Vec<i16>,
    capacity: usize,
    read: usize,
    len: usize,
    step: u32,
    position: u32,
    previous: (i32, i32),
    next: (i32, i32),
    primed: bool,
    gain: i32,
    closing: bool,
}

impl AppStream {
    fn new(owner: u32, id: u32, rate: u32, channels: usize) -> Result<Self, StreamError> {
        if !(MIN_RATE..=MAX_RATE).contains(&rate) || !(1..=2).contains(&channels) {
            return Err(StreamError::BadFormat);
        }
        let capacity = (rate / 2) as usize;
        let mut ring = Vec::new();
        ring.try_reserve_exact(capacity * channels).map_err(|_| StreamError::NoMemory)?;
        ring.resize(capacity * channels, 0);
        Ok(Self {
            owner,
            id,
            channels,
            ring,
            capacity,
            read: 0,
            len: 0,
            step: (((rate as u64) << 16) / crate::synth::OUTPUT_RATE as u64) as u32,
            position: 0,
            previous: (0, 0),
            next: (0, 0),
            primed: false,
            gain: 256,
            closing: false,
        })
    }

    pub fn queued_frames(&self) -> usize {
        self.len
    }

    pub fn free_frames(&self) -> usize {
        self.capacity - self.len
    }

    pub fn has_pending(&self) -> bool {
        if self.primed {
            self.len > 0
        } else {
            self.len >= 2
        }
    }

    fn write(&mut self, pcm: &[u8]) -> usize {
        if self.closing {
            return 0;
        }
        let frame_bytes = self.channels * 2;
        let offered = pcm.len() / frame_bytes;
        let accepted = offered.min(self.free_frames());
        for frame in 0..accepted {
            let slot = (self.read + self.len) % self.capacity;
            for channel in 0..self.channels {
                let at = frame * frame_bytes + channel * 2;
                self.ring[slot * self.channels + channel] = i16::from_le_bytes([pcm[at], pcm[at + 1]]);
            }
            self.len += 1;
        }
        accepted
    }

    fn pop(&mut self) -> Option<(i32, i32)> {
        if self.len == 0 {
            return None;
        }
        let base = self.read * self.channels;
        let left = self.ring[base] as i32;
        let right = if self.channels == 2 { self.ring[base + 1] as i32 } else { left };
        self.read = (self.read + 1) % self.capacity;
        self.len -= 1;
        Some((left, right))
    }

    pub fn next_frame(&mut self) -> Option<(i32, i32)> {
        if !self.primed {
            if self.len < 2 {
                return None;
            }
            self.previous = self.pop()?;
            self.next = self.pop()?;
            self.position = 0;
            self.primed = true;
        }
        while self.position >= 65_536 {
            let Some(frame) = self.pop() else { return None; };
            self.position -= 65_536;
            self.previous = self.next;
            self.next = frame;
        }
        let fraction = (self.position >> 1) as i32;
        let left = self.previous.0 + (((self.next.0 - self.previous.0) * fraction) >> 15);
        let right = self.previous.1 + (((self.next.1 - self.previous.1) * fraction) >> 15);
        self.position += self.step;
        Some(((left * self.gain) >> 8, (right * self.gain) >> 8))
    }
}

pub struct Streams {
    list: Vec<AppStream>,
    counter: u32,
}

impl Streams {
    pub const fn new() -> Self {
        Self { list: Vec::new(), counter: 0 }
    }

    pub fn any_pending(&self) -> bool {
        self.list.iter().any(|stream| stream.has_pending())
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }

    pub fn open(&mut self, owner: u32, rate: u32, channels: u32) -> Result<u32, StreamError> {
        if self.list.len() >= MAX_STREAMS || self.list.iter().filter(|s| s.owner == owner && !s.closing).count() >= MAX_STREAMS_PER_OWNER {
            return Err(StreamError::Limit);
        }
        self.list.try_reserve(1).map_err(|_| StreamError::NoMemory)?;
        self.counter = self.counter.wrapping_add(1).max(1);
        let stream = AppStream::new(owner, self.counter, rate, channels as usize)?;
        self.list.push(stream);
        Ok(self.counter)
    }

    fn find(&mut self, owner: u32, id: u32) -> Result<&mut AppStream, StreamError> {
        self.list.iter_mut().find(|s| s.owner == owner && s.id == id).ok_or(StreamError::NoStream)
    }

    pub fn write(&mut self, owner: u32, id: u32, pcm: &[u8]) -> Result<usize, StreamError> {
        Ok(self.find(owner, id)?.write(pcm))
    }

    pub fn status(&mut self, owner: u32, id: u32) -> Result<(usize, usize), StreamError> {
        let stream = self.find(owner, id)?;
        Ok((stream.queued_frames(), stream.free_frames()))
    }

    pub fn set_volume(&mut self, owner: u32, id: u32, percent: u32) -> Result<(), StreamError> {
        let v = percent.min(100) as i32;
        self.find(owner, id)?.gain = v * v * 256 / 10_000;
        Ok(())
    }

    pub fn close(&mut self, owner: u32, id: u32, discard: bool) -> Result<(), StreamError> {
        let stream = self.find(owner, id)?;
        stream.closing = true;
        if discard {
            stream.len = 0;
        }
        Ok(())
    }

    pub fn release_owner(&mut self, owner: u32) {
        self.list.retain(|stream| stream.owner != owner);
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn mix_frame(&mut self) -> (i32, i32) {
        let (mut left, mut right) = (0i32, 0i32);
        for stream in self.list.iter_mut() {
            if let Some((l, r)) = stream.next_frame() {
                left += l;
                right += r;
            }
        }
        (left, right)
    }

    pub fn reap(&mut self) {
        if self.list.iter().any(|s| s.closing && s.len == 0) {
            self.list.retain(|s| !(s.closing && s.len == 0));
        }
    }
}
