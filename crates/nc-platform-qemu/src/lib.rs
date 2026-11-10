#![no_std]

use nc_arch::{halt_forever, inw, outb, outw, reboot, serial};
use nc_hal::Platform;

const DEBUGCON_PORT: u16 = 0x00E9;
const DEBUG_EXIT_PORT: u16 = 0x00F4;
const DISPI_INDEX_PORT: u16 = 0x01CE;
const DISPI_DATA_PORT: u16 = 0x01CF;
const DISPI_REGISTER_ID: u16 = 0;
const DISPI_REGISTER_XRES: u16 = 1;
const DISPI_REGISTER_YRES: u16 = 2;
const DISPI_REGISTER_BPP: u16 = 3;
const DISPI_REGISTER_ENABLE: u16 = 4;
const DISPI_ENABLED_LINEAR: u16 = 0x41;

pub struct QemuPlatform;

unsafe fn dispi_read(register: u16) -> u16 {
    outw(DISPI_INDEX_PORT, register);
    inw(DISPI_DATA_PORT)
}

unsafe fn dispi_write(register: u16, value: u16) {
    outw(DISPI_INDEX_PORT, register);
    outw(DISPI_DATA_PORT, value);
}

impl QemuPlatform {
    pub const fn new() -> QemuPlatform {
        QemuPlatform
    }

    pub fn display_adapter_present(&self) -> bool {
        let id = unsafe { dispi_read(DISPI_REGISTER_ID) };
        (0xB0C0..=0xB0CF).contains(&id)
    }

    pub fn set_display_mode(&self, width: u16, height: u16) -> bool {
        if !self.display_adapter_present() {
            return false;
        }
        unsafe {
            dispi_write(DISPI_REGISTER_ENABLE, 0);
            dispi_write(DISPI_REGISTER_XRES, width);
            dispi_write(DISPI_REGISTER_YRES, height);
            dispi_write(DISPI_REGISTER_BPP, 32);
            dispi_write(DISPI_REGISTER_ENABLE, DISPI_ENABLED_LINEAR);
        }
        true
    }
}

impl Platform for QemuPlatform {
    fn name(&self) -> &'static str {
        "qemu"
    }

    fn init(&mut self) {
        serial::init();
    }

    fn write_log(&self, text: &str) {
        for byte in text.bytes() {
            unsafe {
                outb(DEBUGCON_PORT, byte);
            }
            if byte == b'\n' {
                serial::write_byte(b'\r');
            }
            serial::write_byte(byte);
        }
    }

    fn shutdown(&self, code: u8) -> ! {
        unsafe {
            outb(DEBUG_EXIT_PORT, code);
        }
        halt_forever()
    }

    fn reboot(&self) -> ! {
        reboot()
    }
}
