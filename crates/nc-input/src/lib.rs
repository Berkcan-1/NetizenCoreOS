// crates/nc-input/src/lib.rs
#![no_std]

pub mod xhci;

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use nc_arch::idt::pop_input_event;
use nc_arch::{inb, outb};
use nc_hal::{InputDevice, KeyCode, MouseState};
use xhci::{init_and_probe_xhci, pop_usb_hid_event, UsbHidEvent};

static KEYBOARD_LAYOUT: AtomicU8 = AtomicU8::new(0); // 0: US/TR-Q, 1: TR-F / Alternatif
static I8042_PRESENT: AtomicBool = AtomicBool::new(true);

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

fn wait_write() -> bool {
    let mut spins = 0u32;
    while spins < 50_000 {
        let st = unsafe { inb(0x64) };
        if st == 0xFF {
            return false;
        }
        if st & 0x02 == 0 {
            return true;
        }
        spins += 1;
    }
    false
}

fn wait_read() -> bool {
    let mut spins = 0u32;
    while spins < 50_000 {
        let st = unsafe { inb(0x64) };
        if st == 0xFF {
            return false;
        }
        if st & 0x01 != 0 {
            return true;
        }
        spins += 1;
    }
    false
}

pub fn init() {
    // 1. Önce PCI üzerindeki xHCI (USB 3.x) denetleyicisini ve BIOS Legacy HID köprüsünü doğrula
    let _xhci_info = init_and_probe_xhci();

    // 2. Anakartta fiziksel veya SMM-emüle i8042 (0x64) olup olmadığını kontrol et
    let initial_status = unsafe { inb(0x64) };
    if initial_status == 0xFF {
        I8042_PRESENT.store(false, Ordering::Release);
        return;
    }
    I8042_PRESENT.store(true, Ordering::Release);

    if !wait_write() {
        return;
    }
    unsafe { outb(0x64, 0xAE); }
    if wait_write() {
        unsafe { outb(0x64, 0xA8); }
    }
    if wait_write() {
        unsafe { outb(0x64, 0x20); }
    }
    if wait_read() {
        let mut status = unsafe { inb(0x60) };
        status |= 0x03;  // IRQ1 ve IRQ12 aktif
        status &= !0x30; // Saatleri aç
        if wait_write() {
            unsafe { outb(0x64, 0x60); }
            if wait_write() {
                unsafe { outb(0x60, status); }
            }
        }
    }

    // Fare varsayılanlarını yükle ve veri akışını başlat
    if wait_write() {
        unsafe { outb(0x64, 0xD4); }
        if wait_write() {
            unsafe { outb(0x60, 0xF6); }
            if wait_read() {
                let _ = unsafe { inb(0x60) };
            }
        }
    }
    if wait_write() {
        unsafe { outb(0x64, 0xD4); }
        if wait_write() {
            unsafe { outb(0x60, 0xF4); }
            if wait_read() {
                let _ = unsafe { inb(0x60) };
            }
        }
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

    fn map_letter(code: u8, upper: bool, is_tr_f: bool) -> Option<char> {
        let ch_lower = if !is_tr_f {
            // Standart QWERTY / TR-Q
            match code {
                0x10 => 'q', 0x11 => 'w', 0x12 => 'e', 0x13 => 'r', 0x14 => 't',
                0x15 => 'y', 0x16 => 'u', 0x17 => 'i', 0x18 => 'o', 0x19 => 'p',
                0x1E => 'a', 0x1F => 's', 0x20 => 'd', 0x21 => 'f', 0x22 => 'g',
                0x23 => 'h', 0x24 => 'j', 0x25 => 'k', 0x26 => 'l',
                0x2C => 'z', 0x2D => 'x', 0x2E => 'c', 0x2F => 'v', 0x30 => 'b',
                0x31 => 'n', 0x32 => 'm',
                _ => return None,
            }
        } else {
            // Türkçe F (TR-F) dizilimi
            match code {
                0x10 => 'f', 0x11 => 'g', 0x12 => 'g', 0x13 => 'i', 0x14 => 'o',
                0x15 => 'd', 0x16 => 'r', 0x17 => 'n', 0x18 => 'h', 0x19 => 'p',
                0x1E => 'u', 0x1F => 'i', 0x20 => 'e', 0x21 => 'a', 0x22 => 'u',
                0x23 => 't', 0x24 => 'k', 0x25 => 'm', 0x26 => 'l',
                0x2C => 'j', 0x2D => 'o', 0x2E => 'v', 0x2F => 'c', 0x30 => 'c',
                0x31 => 'z', 0x32 => 's',
                _ => return None,
            }
        };
        Some(if upper {
            ((ch_lower as u8) - 32) as char
        } else {
            ch_lower
        })
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
        if status == 0xFF {
            return;
        }
        if status & 0x20 != 0 {
            match self.mouse_cycle {
                0 => {
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

                    self.mouse_x = (self.mouse_x + dx).clamp(0, (screen_width - 1).max(0));
                    self.mouse_y = (self.mouse_y - dy).clamp(0, (screen_height - 1).max(0));

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

            if !ext && (code == 0x2A || code == 0x36) {
                self.shift_down = !release;
                return;
            }
            if !ext && code == 0x3A && !release {
                self.caps_lock = !self.caps_lock;
                return;
            }

            if !release {
                let upper = self.shift_down ^ self.caps_lock;
                let shift = self.shift_down;
                let is_tr_f = KEYBOARD_LAYOUT.load(Ordering::Relaxed) == 1;

                if !ext {
                    if let Some(ch) = Self::map_letter(code, upper, is_tr_f) {
                        self.last_key = Some(KeyCode::Char(ch));
                        return;
                    }
                }

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

    pub fn poll_state(
        &mut self,
        screen_width: i32,
        screen_height: i32,
    ) -> (MouseState, Option<KeyCode>) {
        let mut left_clicked = false;
        let mut right_clicked = false;
        self.last_key = None;

        // 1. Kesme (IRQ1/IRQ12) halka tamponundaki olayları tüket
        while let Some((status, data)) = pop_input_event() {
            self.process_byte(
                status,
                data,
                screen_width,
                screen_height,
                &mut left_clicked,
                &mut right_clicked,
            );
        }

        // 2. i8042 mevcutsa tamponda kalan veriyi (en fazla 32 okumayla sınırlı) al
        if I8042_PRESENT.load(Ordering::Relaxed) {
            for _ in 0..32 {
                let status = unsafe { inb(0x64) };
                if status == 0xFF || (status & 0x01) == 0 {
                    break;
                }
                let data = unsafe { inb(0x60) };
                self.process_byte(
                    status,
                    data,
                    screen_width,
                    screen_height,
                    &mut left_clicked,
                    &mut right_clicked,
                );
            }
        }

        // 3. USB xHCI / HID kuyruğundaki olayları birleştir
        while let Some(usb_ev) = pop_usb_hid_event() {
            match usb_ev {
                UsbHidEvent::Key(k) => self.last_key = Some(k),
                UsbHidEvent::MouseDelta { dx, dy, buttons } => {
                    self.mouse_x = (self.mouse_x + dx as i32).clamp(0, (screen_width - 1).max(0));
                    self.mouse_y = (self.mouse_y + dy as i32).clamp(0, (screen_height - 1).max(0));
                    let l_down = (buttons & 1) != 0;
                    if l_down && !self.mouse_left_down {
                        left_clicked = true;
                    }
                    self.mouse_left_down = l_down;
                    let r_down = (buttons & 2) != 0;
                    if r_down && !self.mouse_right_down {
                        right_clicked = true;
                    }
                    self.mouse_right_down = r_down;
                }
                UsbHidEvent::MouseAbsolute {
                    x_norm,
                    y_norm,
                    buttons,
                } => {
                    self.mouse_x =
                        ((x_norm as i64 * screen_width.max(1) as i64) / 32768) as i32;
                    self.mouse_y =
                        ((y_norm as i64 * screen_height.max(1) as i64) / 32768) as i32;
                    let l_down = (buttons & 1) != 0;
                    if l_down && !self.mouse_left_down {
                        left_clicked = true;
                    }
                    self.mouse_left_down = l_down;
                    let r_down = (buttons & 2) != 0;
                    if r_down && !self.mouse_right_down {
                        right_clicked = true;
                    }
                    self.mouse_right_down = r_down;
                }
            }
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
        "universal-hid-input"
    }

    fn poll(&mut self, screen_width: i32, screen_height: i32) -> (MouseState, Option<KeyCode>) {
        self.poll_state(screen_width, screen_height)
    }
}
