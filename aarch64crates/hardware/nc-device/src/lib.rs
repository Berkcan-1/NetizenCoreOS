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

/// Real hardware battery state reported by a PMIC/fuel-gauge driver.
/// When `available == false`, no battery sensor exists or it failed to respond;
/// UI MUST NOT display a fabricated percentage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryStatus {
    pub available: bool,
    pub percent: u8,
    pub charging: bool,
    pub full: bool,
    pub voltage_mv: u16,
    /// Temperature in tenths of a degree Celsius; `i16::MIN` when unavailable.
    pub temp_deci_c: i16,
}

impl BatteryStatus {
    pub const UNAVAILABLE: BatteryStatus = BatteryStatus {
        available: false,
        percent: 0,
        charging: false,
        full: false,
        voltage_mv: 0,
        temp_deci_c: i16::MIN,
    };
}

/// Optional real hardware control/status hooks supplied by a board driver crate.
#[derive(Clone, Copy)]
pub struct HardwareOps {
    pub battery: Option<fn() -> BatteryStatus>,
    pub set_backlight: Option<fn(u8) -> bool>,
    pub has_hw_backlight: bool,
    pub has_hw_bt: bool,
    pub has_hw_cellular: bool,
    pub has_hw_audio: bool,
    pub has_hw_camera: bool,
}

impl HardwareOps {
    pub const NONE: HardwareOps = HardwareOps {
        battery: None,
        set_backlight: None,
        has_hw_backlight: false,
        has_hw_bt: false,
        has_hw_cellular: false,
        has_hw_audio: false,
        has_hw_camera: false,
    };
}

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
    pub hw_ops: HardwareOps,
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
