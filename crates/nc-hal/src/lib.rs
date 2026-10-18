#![no_std]

use core::fmt;

pub const BOOT_INFO_ADDRESS: usize = 0x0900;
pub const BOOT_INFO_MAGIC: u32 = 0x4942_434E;
pub const BOOT_INFO_VERSION: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub region_type: u32,
    pub acpi_extended: u32,
}

impl MemoryRegion {
    pub fn is_usable(&self) -> bool {
        self.region_type == 1
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub address: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub red_position: u8,
    pub red_size: u8,
    pub green_position: u8,
    pub green_size: u8,
    pub blue_position: u8,
    pub blue_size: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub framebuffer: FramebufferInfo,
    pub memory_map_addr: u64,
    pub memory_map_count: u32,
}

impl BootInfo {
    pub unsafe fn read_from_boot_address() -> Option<BootInfo> {
        let info = core::ptr::read(BOOT_INFO_ADDRESS as *const BootInfo);
        if info.magic == BOOT_INFO_MAGIC && info.version == BOOT_INFO_VERSION {
            Some(info)
        } else {
            None
        }
    }

    pub fn memory_regions(&self) -> &[MemoryRegion] {
        if self.memory_map_addr == 0 || self.memory_map_count == 0 {
            return &[];
        }
        unsafe {
            core::slice::from_raw_parts(
                self.memory_map_addr as *const MemoryRegion,
                self.memory_map_count as usize,
            )
        }
    }

    pub fn total_usable_memory(&self) -> u64 {
        let mut total = 0u64;
        for region in self.memory_regions() {
            if region.is_usable() {
                total = total.saturating_add(region.length);
            }
        }
        total
    }
}

pub trait Platform {
    fn name(&self) -> &'static str;
    fn init(&mut self);
    fn write_log(&self, text: &str);
    fn shutdown(&self, code: u8) -> !;
    fn reboot(&self) -> !;
}

pub struct LogWriter<'a, P: Platform>(pub &'a P);

impl<'a, P: Platform> fmt::Write for LogWriter<'a, P> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.write_log(text);
        Ok(())
    }
}

#[macro_export]
macro_rules! log {
    ($platform:expr, $($argument:tt)*) => {{
        use core::fmt::Write as _;
        let mut writer = $crate::LogWriter($platform);
        let _ = writeln!(writer, $($argument)*);
    }};
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    HardDisk,
    OpticalDisc,
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy)]
pub struct BlockDeviceInfo {
    pub channel: u8,
    pub model: [u8; 40],
    pub model_len: usize,
    pub block_count: u64,
    pub block_size: u32,
    pub device_type: DeviceType,
}

impl BlockDeviceInfo {
    pub const EMPTY: BlockDeviceInfo = BlockDeviceInfo {
        channel: 0xFF,
        model: [0; 40],
        model_len: 0,
        block_count: 0,
        block_size: 512,
        device_type: DeviceType::HardDisk,
    };

    pub fn model_str(&self) -> &str {
        let len = self.model_len.min(self.model.len());
        if let Ok(s) = core::str::from_utf8(&self.model[..len]) {
            s
        } else {
            "UNKNOWN DEVICE"
        }
    }

    // M-09: Tasmalara karsi checked aritmetik
    pub fn size_mb(&self) -> u64 {
        self.block_count
            .checked_mul(self.block_size as u64)
            .map(|bytes| bytes / (1024 * 1024))
            .unwrap_or(0)
    }

    pub fn type_label(&self) -> &'static str {
        match self.device_type {
            DeviceType::OpticalDisc => "ROM",
            DeviceType::HardDisk => "HDD",
            _ => "DEV",
        }
    }
}

pub trait BlockDevice {
    fn info(&self) -> BlockDeviceInfo;
    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()>;
    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyCode {
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    Char(char),
    Unknown,
}

#[derive(Clone, Copy)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left_down: bool,
    pub left_clicked: bool,
    pub right_down: bool,
    pub right_clicked: bool,
}

pub trait InputDevice {
    fn name(&self) -> &'static str;
    fn poll(&mut self, width: i32, height: i32) -> (MouseState, Option<KeyCode>);
}
