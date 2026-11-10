//! Polling PS/2 keyboard/mouse fallback for legacy PC and VM controller emulation.
//! No controller reprogramming or reset command is issued during discovery.

use nc_arch::inb;
use nc_hal::{InputDevice, KeyCode, MouseState};

const STATUS_PORT: u16 = 0x64;
const DATA_PORT: u16 = 0x60;
const OUTPUT_FULL: u8 = 1 << 0;
const AUX_DATA: u8 = 1 << 5;
const STATUS_TIMEOUT: u8 = 1 << 6;
const STATUS_PARITY: u8 = 1 << 7;

pub struct Ps2Input {
    x: i32,
    y: i32,
    left_down: bool,
    right_down: bool,
    middle_down: bool,
    shift: bool,
    extended: bool,
    mouse_packet: [u8; 3],
    mouse_index: usize,
}

impl Ps2Input {
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            left_down: false,
            right_down: false,
            middle_down: false,
            shift: false,
            extended: false,
            mouse_packet: [0; 3],
            mouse_index: 0,
        }
    }

    fn keyboard_byte(&mut self, byte: u8) -> Option<KeyCode> {
        if byte == 0xE0 { self.extended = true; return None; }
        let release = (byte & 0x80) != 0;
        let code = byte & 0x7F;
        if code == 0x2A || code == 0x36 {
            self.shift = !release;
            self.extended = false;
            return None;
        }
        if release { self.extended = false; return None; }
        let extended = self.extended;
        self.extended = false;
        if extended {
            return match code { 0x1C => Some(KeyCode::Enter), 0x48 => Some(KeyCode::Up), 0x50 => Some(KeyCode::Down), 0x4B => Some(KeyCode::Left), 0x4D => Some(KeyCode::Right), _ => None };
        }
        Some(match code {
            0x01 => KeyCode::Escape,
            0x0F => KeyCode::Tab,
            0x1C => KeyCode::Enter,
            0x39 => KeyCode::Space,
            0x0E => KeyCode::Backspace,
            0x48 => KeyCode::Up,
            0x50 => KeyCode::Down,
            0x4B => KeyCode::Left,
            0x4D => KeyCode::Right,
            0x02 => KeyCode::Char(if self.shift { '!' } else { '1' }),
            0x03 => KeyCode::Char(if self.shift { '@' } else { '2' }),
            0x04 => KeyCode::Char(if self.shift { '#' } else { '3' }),
            0x05 => KeyCode::Char(if self.shift { '$' } else { '4' }),
            0x06 => KeyCode::Char(if self.shift { '%' } else { '5' }),
            0x07 => KeyCode::Char(if self.shift { '^' } else { '6' }),
            0x08 => KeyCode::Char(if self.shift { '&' } else { '7' }),
            0x09 => KeyCode::Char(if self.shift { '*' } else { '8' }),
            0x0A => KeyCode::Char(if self.shift { '(' } else { '9' }),
            0x0B => KeyCode::Char(if self.shift { ')' } else { '0' }),
            0x10..=0x19 => {
                const ROW: [char; 10] = ['q','w','e','r','t','y','u','i','o','p'];
                let c = ROW[(code - 0x10) as usize];
                KeyCode::Char(if self.shift { c.to_ascii_uppercase() } else { c })
            }
            0x1E..=0x26 => {
                const ROW: [char; 9] = ['a','s','d','f','g','h','j','k','l'];
                let c = ROW[(code - 0x1E) as usize];
                KeyCode::Char(if self.shift { c.to_ascii_uppercase() } else { c })
            }
            0x2C..=0x32 => {
                const ROW: [char; 7] = ['z','x','c','v','b','n','m'];
                let c = ROW[(code - 0x2C) as usize];
                KeyCode::Char(if self.shift { c.to_ascii_uppercase() } else { c })
            }
            0x1A => KeyCode::Char(if self.shift { '{' } else { '[' }),
            0x1B => KeyCode::Char(if self.shift { '}' } else { ']' }),
            0x27 => KeyCode::Char(if self.shift { ':' } else { ';' }),
            0x28 => KeyCode::Char(if self.shift { '"' } else { '\'' }),
            0x29 => KeyCode::Char(if self.shift { '~' } else { '`' }),
            0x2B => KeyCode::Char(if self.shift { '|' } else { '\\' }),
            0x33 => KeyCode::Char(if self.shift { '<' } else { ',' }),
            0x34 => KeyCode::Char(if self.shift { '>' } else { '.' }),
            0x35 => KeyCode::Char(if self.shift { '?' } else { '/' }),
            0x0C => KeyCode::Char(if self.shift { '_' } else { '-' }),
            0x0D => KeyCode::Char(if self.shift { '+' } else { '=' }),
            _ => return None,
        })
    }

    fn mouse_byte(&mut self, byte: u8, width: i32, height: i32) {
        if self.mouse_index == 0 && (byte & 0x08) == 0 { return; }
        self.mouse_packet[self.mouse_index] = byte;
        self.mouse_index += 1;
        if self.mouse_index < 3 { return; }
        self.mouse_index = 0;
        let flags = self.mouse_packet[0];
        if (flags & 0xC0) != 0 { return; }
        let dx = self.mouse_packet[1] as i8 as i32;
        let dy = self.mouse_packet[2] as i8 as i32;
        self.x = self.x.saturating_add(dx).clamp(0, width.saturating_sub(1).max(0));
        self.y = self.y.saturating_sub(dy).clamp(0, height.saturating_sub(1).max(0));
        self.left_down = (flags & 1) != 0;
        self.right_down = (flags & 2) != 0;
        self.middle_down = (flags & 4) != 0;
    }
}

impl Default for Ps2Input { fn default() -> Self { Self::new() } }

impl InputDevice for Ps2Input {
    fn name(&self) -> &'static str { "PS/2 polling keyboard + mouse (set-1 / 3-byte packet)" }

    fn poll(&mut self, width: i32, height: i32) -> (MouseState, Option<KeyCode>) {
        let old_left = self.left_down;
        let old_right = self.right_down;
        let mut key = None;
        // Bounded polling prevents a missing/stuck controller from hanging the UI loop.
        for _ in 0..32 {
            let status = unsafe { inb(STATUS_PORT) };
            if (status & OUTPUT_FULL) == 0 { break; }
            let byte = unsafe { inb(DATA_PORT) };
            // Drop bytes flagged by the controller as parity/timeout errors.
            if (status & (STATUS_TIMEOUT | STATUS_PARITY)) != 0 {
                self.mouse_index = 0;
                continue;
            }
            if (status & AUX_DATA) != 0 {
                self.mouse_byte(byte, width, height);
            } else if key.is_none() {
                key = self.keyboard_byte(byte);
            }
        }
        let state = MouseState {
            x: self.x,
            y: self.y,
            left_down: self.left_down,
            left_clicked: !old_left && self.left_down,
            right_down: self.right_down,
            right_clicked: !old_right && self.right_down,
        };
        let _ = self.middle_down;
        (state, key)
    }
}
