#![no_std]

use nc_arch::{halt_forever, reboot, serial};
use nc_hal::Platform;

pub struct GenericPlatform;

impl GenericPlatform {
    pub const fn new() -> GenericPlatform {
        GenericPlatform
    }
}

impl Platform for GenericPlatform {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn init(&mut self) {
        serial::init();
    }

    fn write_log(&self, text: &str) {
        if serial::present() {
            serial::write_str(text);
        }
    }

    // crates/nc-platform-generic/src/lib.rs içinde shutdown fonksiyonunu bununla değiştir:
    fn shutdown(&self, _code: u8) -> ! {
        unsafe {
            // VirtualBox ACPI Shutdown
            nc_arch::outw(0x4004, 0x3400);
            // QEMU / Bochs / Eski Yonga Seti ACPI Shutdown
            nc_arch::outw(0x0604, 0x2000);
            nc_arch::outw(0xB004, 0x2000);
        }
        halt_forever()
    }

    fn reboot(&self) -> ! {
        reboot()
    }
}
