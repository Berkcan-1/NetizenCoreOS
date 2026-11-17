use crate::synth::{note, Note, Timbre};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Sound {
    Startup = 0,
    Shutdown,
    Click,
    Open,
    Close,
    Notify,
    Success,
    Warning,
    Error,
    Lock,
    Unlock,
    VolumeTick,
    Connect,
    Disconnect,
}

static NOTIFY: [Note; 2] = [
    note(0, 220, 1175, 120, Timbre::Bell),
    note(110, 460, 1568, 140, Timbre::Bell),
];
static SUCCESS: [Note; 3] = [
    note(0, 170, 1047, 110, Timbre::Bell),
    note(80, 170, 1319, 115, Timbre::Bell),
    note(160, 420, 1568, 130, Timbre::Bell),
];
static WARNING: [Note; 2] = [
    note(0, 150, 880, 140, Timbre::Soft),
    note(190, 300, 880, 140, Timbre::Soft),
];
static ERROR: [Note; 4] = [
    note(0, 210, 196, 120, Timbre::Buzz),
    note(0, 210, 233, 100, Timbre::Buzz),
    note(230, 380, 147, 120, Timbre::Buzz),
    note(230, 380, 175, 100, Timbre::Buzz),
];
static CLICK: [Note; 1] = [note(0, 22, 2400, 90, Timbre::Tick)];
static OPEN: [Note; 2] = [
    note(0, 100, 700, 80, Timbre::Soft),
    note(45, 130, 1050, 80, Timbre::Soft),
];
static CLOSE: [Note; 2] = [
    note(0, 90, 1050, 70, Timbre::Soft),
    note(45, 130, 700, 70, Timbre::Soft),
];
static VOLUME_TICK: [Note; 1] = [note(0, 30, 1500, 100, Timbre::Tick)];
static LOCK: [Note; 2] = [
    note(0, 110, 880, 110, Timbre::Soft),
    note(80, 240, 587, 110, Timbre::Soft),
];
static UNLOCK: [Note; 2] = [
    note(0, 110, 587, 110, Timbre::Soft),
    note(80, 260, 880, 120, Timbre::Soft),
];
static CONNECT: [Note; 2] = [
    note(0, 110, 880, 110, Timbre::Bell),
    note(90, 260, 1319, 130, Timbre::Bell),
];
static DISCONNECT: [Note; 2] = [
    note(0, 110, 1319, 110, Timbre::Bell),
    note(90, 260, 880, 120, Timbre::Bell),
];
static SHUTDOWN: [Note; 6] = [
    note(0, 300, 784, 130, Timbre::Bell),
    note(170, 300, 659, 130, Timbre::Bell),
    note(340, 300, 523, 130, Timbre::Bell),
    note(510, 800, 392, 140, Timbre::Bell),
    note(510, 800, 262, 90, Timbre::Soft),
    note(510, 800, 196, 60, Timbre::Soft),
];

pub struct Profile {
    pub notes: &'static [Note],
    pub priority: u8,
    pub gain: u16,
}

pub fn profile(sound: Sound) -> Profile {
    let (notes, priority, gain): (&'static [Note], u8, u16) = match sound {
        Sound::Startup => (&SUCCESS, 9, 256),
        Sound::Shutdown => (&SHUTDOWN, 9, 256),
        Sound::Click => (&CLICK, 1, 256),
        Sound::Open => (&OPEN, 2, 256),
        Sound::Close => (&CLOSE, 2, 256),
        Sound::Notify => (&NOTIFY, 5, 256),
        Sound::Success => (&SUCCESS, 5, 256),
        Sound::Warning => (&WARNING, 6, 256),
        Sound::Error => (&ERROR, 7, 256),
        Sound::Lock => (&LOCK, 4, 256),
        Sound::Unlock => (&UNLOCK, 4, 256),
        Sound::VolumeTick => (&VOLUME_TICK, 3, 256),
        Sound::Connect => (&CONNECT, 4, 256),
        Sound::Disconnect => (&DISCONNECT, 4, 256),
    };
    Profile { notes, priority, gain }
}
