#![no_std]

use nc_hal::{BlockDevice, InputDevice};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Arm64,
    Armv8a,
    Armv9a,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiProfile {
    Desktop,
    Mobile,
}

#[derive(Clone, Copy)]
pub struct Region {
    pub base: u64,
    pub size: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UartKind {
    None,
    Pl011,
    Ns16550 { reg_shift: u8 },
    Ns16550Dw32 { reg_shift: u8 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PsciConduit {
    None,
    Hvc,
    Smc,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PixelOrder {
    Bgrx,
    Rgbx,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FramebufferSource {
    None,
    DtbSimpleFramebuffer,
    MtkVideoLfb,
    Fixed(u64),
}

#[derive(Clone, Copy)]
pub struct DisplayProfile {
    pub source: FramebufferSource,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub order: PixelOrder,
    pub rotation_degrees: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DriverClass {
    Storage,
    Touch,
    Display,
    Keys,
    Power,
    Radio,
    Audio,
    Sensor,
    Other,
}

#[derive(Clone, Copy)]
pub struct DriverEntry {
    pub name: &'static str,
    pub class: DriverClass,
    pub compatible: &'static str,
    pub regs: Region,
    pub irq: u32,
}

pub trait DriverHost {
    fn add_block_device(&mut self, device: &'static mut dyn BlockDevice);
    fn add_input_device(&mut self, device: &'static mut dyn InputDevice);
    fn log(&mut self, text: &str);
}

pub type RegisterFn = fn(&mut dyn DriverHost);

pub struct DeviceProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub soc: &'static str,
    pub arch: Arch,
    pub ui: UiProfile,
    pub ram: &'static [Region],
    pub uart: UartKind,
    pub uart_base: u64,
    pub psci: PsciConduit,
    pub gic_dist: u64,
    pub gic_redist: u64,
    pub display: DisplayProfile,
    pub drivers: &'static [DriverEntry],
    pub register: Option<RegisterFn>,
}

impl DeviceProfile {
    pub fn find_driver(&self, class: DriverClass) -> Option<&DriverEntry> {
        for entry in self.drivers {
            if entry.class == class {
                return Some(entry);
            }
        }
        None
    }

    pub fn register_all(&self, host: &mut dyn DriverHost) {
        if let Some(register) = self.register {
            register(host);
        }
    }
}

pub const NO_DISPLAY: DisplayProfile = DisplayProfile {
    source: FramebufferSource::None,
    width: 0,
    height: 0,
    stride_bytes: 0,
    order: PixelOrder::Bgrx,
    rotation_degrees: 0,
};
