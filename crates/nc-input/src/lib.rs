#![no_std]

use core::sync::atomic::{AtomicU8, Ordering};
use nc_arch::idt::pop_input_event;
use nc_arch::{inb, outb};
use nc_hal::{InputDevice, KeyCode, MouseState};

static KEYBOARD_LAYOUT: AtomicU8 = AtomicU8::new(0); // 0: US/TR-Q, 1: TR-F / Alternatif

pub fn set_keyboard_layout(code: u8) {
    KEYBOARD_LAYOUT.store(code, Ordering::Relaxed);
}

pub struct Ps2Input {
    mouse_x: i32,
    mouse_y: i32,
    mouse_left_down: bool,
    mouse_right_down: bool,
    mouse_packet: [u8; 3],
    mouse_cycle: usize,
    last_key: Option<KeyCode>,
    extended: bool,
    shift_down: bool,
    caps_lock: bool,
}

fn wait_write() {
    let mut spins = 0u32;
    while spins < 100_000 {
        if unsafe { inb(0x64) } & 0x02 == 0 {
            return;
        }
        spins += 1;
    }
}

fn wait_read() -> bool {
    let mut spins = 0u32;
    while spins < 100_000 {
        if unsafe { inb(0x64) } & 0x01 != 0 {
            return true;
        }
        spins += 1;
    }
    false
}

pub fn init() {
    // Yardimci (fare) ve klavye portlarini ac
    wait_write();
    unsafe { outb(0x64, 0xAE); }
    wait_write();
    unsafe { outb(0x64, 0xA8); }

    wait_write();
    unsafe { outb(0x64, 0x20); }
    if wait_read() {
        let mut status = unsafe { inb(0x60) };
        status |= 0x03;  // Bit 0 (IRQ1 Klavye) ve Bit 1 (IRQ12 Fare) kesmelerini ac
        status &= !0x30; // Port saatlerini etkinlestir
        wait_write();
        unsafe { outb(0x64, 0x60); }
        wait_write();
        unsafe { outb(0x60, status); }
    }

    // Fare varsayilanlarini yukle ve veri akisini baslat
    wait_write();
    unsafe { outb(0x64, 0xD4); }
    wait_write();
    unsafe { outb(0x60, 0xF6); }
    if wait_read() {
        let _ = unsafe { inb(0x60) };
    }

    wait_write();
    unsafe { outb(0x64, 0xD4); }
    wait_write();
    unsafe { outb(0x60, 0xF4); }
    if wait_read() {
        let _ = unsafe { inb(0x60) };
    }
}

impl Ps2Input {
    pub const fn new(initial_x: i32, initial_y: i32) -> Ps2Input {
        Ps2Input {
            mouse_x: initial_x,
            mouse_y: initial_y,
            mouse_left_down: false,
            mouse_right_down: false,
            mouse_packet: [0; 3],
            mouse_cycle: 0,
            last_key: None,
            extended: false,
            shift_down: false,
            caps_lock: false,
        }
    }

    fn process_byte(
        &mut self,
        status: u8,
        data: u8,
        screen_width: i32,
        screen_height: i32,
        left_clicked: &mut bool,
        right_clicked: &mut bool,
    ) {
        if status & 0x20 != 0 {
            match self.mouse_cycle {
                0 => {
                    // Bit 3 her zaman 1 olmalidir; degilse paket senkronu kaymistir
                    if data & 0x08 != 0 && data & 0xC0 == 0 {
                        self.mouse_packet[0] = data;
                        self.mouse_cycle = 1;
                    }
                }
                1 => {
                    self.mouse_packet[1] = data;
                    self.mouse_cycle = 2;
                }
                2 => {
                    self.mouse_packet[2] = data;
                    self.mouse_cycle = 0;

                    let flags = self.mouse_packet[0];
                    let raw_dx = self.mouse_packet[1] as i32;
                    let raw_dy = self.mouse_packet[2] as i32;
                    let dx = if flags & 0x10 != 0 { raw_dx | !0xFF } else { raw_dx };
                    let dy = if flags & 0x20 != 0 { raw_dy | !0xFF } else { raw_dy };

                    self.mouse_x = (self.mouse_x + dx).clamp(0, screen_width - 1);
                    self.mouse_y = (self.mouse_y - dy).clamp(0, screen_height - 1);

                    let l_down = flags & 0x01 != 0;
                    if l_down && !self.mouse_left_down {
                        *left_clicked = true;
                    }
                    self.mouse_left_down = l_down;

                    let r_down = flags & 0x02 != 0;
                    if r_down && !self.mouse_right_down {
                        *right_clicked = true;
                    }
                    self.mouse_right_down = r_down;
                }
                _ => self.mouse_cycle = 0,
            }
        } else {
            if data == 0xE0 {
                self.extended = true;
                return;
            }
            let release = data & 0x80 != 0;
            let code = data & 0x7F;
            let ext = self.extended;
            self.extended = false;

            // Sol Shift (0x2A) ve Sag Shift (0x36)
            if !ext && (code == 0x2A || code == 0x36) {
                self.shift_down = !release;
                return;
            }
            // Caps Lock (0x3A)
            if !ext && code == 0x3A && !release {
                self.caps_lock = !self.caps_lock;
                return;
            }

            if !release {
                let upper = self.shift_down ^ self.caps_lock;
                let shift = self.shift_down;

                let key = match (ext, code) {
                    (false, 0x1C) => KeyCode::Enter,
                    (false, 0x01) => KeyCode::Escape,
                    (false, 0x39) => KeyCode::Space,
                    (false, 0x0F) => KeyCode::Tab,
                    (false, 0x0E) => KeyCode::Backspace,
                    (true, 0x48) | (false, 0x48) => KeyCode::Up,
                    (true, 0x50) | (false, 0x50) => KeyCode::Down,
                    (true, 0x4B) | (false, 0x4B) => KeyCode::Left,
                    (true, 0x4D) | (false, 0x4D) => KeyCode::Right,
                    (false, 0x02) => KeyCode::Char(if shift { '!' } else { '1' }),
                    (false, 0x03) => KeyCode::Char(if shift { '\'' } else { '2' }),
                    (false, 0x04) => KeyCode::Char(if shift { '^' } else { '3' }),
                    (false, 0x05) => KeyCode::Char(if shift { '+' } else { '4' }),
                    (false, 0x06) => KeyCode::Char(if shift { '%' } else { '5' }),
                    (false, 0x07) => KeyCode::Char(if shift { '&' } else { '6' }),
                    (false, 0x08) => KeyCode::Char(if shift { '/' } else { '7' }),
                    (false, 0x09) => KeyCode::Char(if shift { '(' } else { '8' }),
                    (false, 0x0A) => KeyCode::Char(if shift { ')' } else { '9' }),
                    (false, 0x0B) => KeyCode::Char(if shift { '=' } else { '0' }),
                    (false, 0x10) => KeyCode::Char(if upper { 'Q' } else { 'q' }),
                    (false, 0x11) => KeyCode::Char(if upper { 'W' } else { 'w' }),
                    (false, 0x12) => KeyCode::Char(if upper { 'E' } else { 'e' }),
                    (false, 0x13) => KeyCode::Char(if upper { 'R' } else { 'r' }),
                    (false, 0x14) => KeyCode::Char(if upper { 'T' } else { 't' }),
                    (false, 0x15) => KeyCode::Char(if upper { 'Y' } else { 'y' }),
                    (false, 0x16) => KeyCode::Char(if upper { 'U' } else { 'u' }),
                    (false, 0x17) => KeyCode::Char(if upper { 'I' } else { 'i' }),
                    (false, 0x18) => KeyCode::Char(if upper { 'O' } else { 'o' }),
                    (false, 0x19) => KeyCode::Char(if upper { 'P' } else { 'p' }),
                    (false, 0x1E) => KeyCode::Char(if upper { 'A' } else { 'a' }),
                    (false, 0x1F) => KeyCode::Char(if upper { 'S' } else { 's' }),
                    (false, 0x20) => KeyCode::Char(if upper { 'D' } else { 'd' }),
                    (false, 0x21) => KeyCode::Char(if upper { 'F' } else { 'f' }),
                    (false, 0x22) => KeyCode::Char(if upper { 'G' } else { 'g' }),
                    (false, 0x23) => KeyCode::Char(if upper { 'H' } else { 'h' }),
                    (false, 0x24) => KeyCode::Char(if upper { 'J' } else { 'j' }),
                    (false, 0x25) => KeyCode::Char(if upper { 'K' } else { 'k' }),
                    (false, 0x26) => KeyCode::Char(if upper { 'L' } else { 'l' }),
                    (false, 0x2C) => KeyCode::Char(if upper { 'Z' } else { 'z' }),
                    (false, 0x2D) => KeyCode::Char(if upper { 'X' } else { 'x' }),
                    (false, 0x2E) => KeyCode::Char(if upper { 'C' } else { 'c' }),
                    (false, 0x2F) => KeyCode::Char(if upper { 'V' } else { 'v' }),
                    (false, 0x30) => KeyCode::Char(if upper { 'B' } else { 'b' }),
                    (false, 0x31) => KeyCode::Char(if upper { 'N' } else { 'n' }),
                    (false, 0x32) => KeyCode::Char(if upper { 'M' } else { 'm' }),
                    (false, 0x0C) => KeyCode::Char(if shift { '_' } else { '-' }),
                    (false, 0x0D) => KeyCode::Char(if shift { '+' } else { '=' }),
                    (false, 0x33) => KeyCode::Char(if shift { ';' } else { ',' }),
                    (false, 0x34) => KeyCode::Char(if shift { ':' } else { '.' }),
                    (false, 0x35) => KeyCode::Char(if shift { '?' } else { '/' }),
                    (false, 0x27) => KeyCode::Char(if shift { ':' } else { ';' }),
                    (false, 0x28) => KeyCode::Char(if shift { '"' } else { '\'' }),
                    (false, 0x1A) => KeyCode::Char('['),
                    (false, 0x1B) => KeyCode::Char(']'),
                    (false, 0x2B) => KeyCode::Char('\\'),
                    _ => KeyCode::Unknown,
                };
                self.last_key = Some(key);
            }
        }
    }

    pub fn poll_state(&mut self, screen_width: i32, screen_height: i32) -> (MouseState, Option<KeyCode>) {
        let mut left_clicked = false;
        let mut right_clicked = false;
        self.last_key = None;

        // 1. Kesme (IRQ1/IRQ12) halka tamponundaki tum olaylari tuket
        while let Some((status, data)) = pop_input_event() {
            self.process_byte(status, data, screen_width, screen_height, &mut left_clicked, &mut right_clicked);
        }

        // 2. Tamponda kalan donanim verisi varsa onu da al
        while unsafe { inb(0x64) } & 0x01 != 0 {
            let status = unsafe { inb(0x64) };
            let data = unsafe { inb(0x60) };
            self.process_byte(status, data, screen_width, screen_height, &mut left_clicked, &mut right_clicked);
        }

        (
            MouseState {
                x: self.mouse_x,
                y: self.mouse_y,
                left_down: self.mouse_left_down,
                left_clicked,
                right_down: self.mouse_right_down,
                right_clicked,
            },
            self.last_key,
        )
    }
}

impl InputDevice for Ps2Input {
    fn name(&self) -> &'static str {
        "ps2-input"
    }

    fn poll(&mut self, screen_width: i32, screen_height: i32) -> (MouseState, Option<KeyCode>) {
        self.poll_state(screen_width, screen_height)
    }
}
