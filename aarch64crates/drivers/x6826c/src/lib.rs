#![no_std]

use nc_device::{
    Arch, DeviceProfile, DisplayProfile, DriverClass, DriverEntry, FramebufferSource, PixelOrder,
    PsciConduit, Region, UartKind, UiProfile,
};

const DRIVERS: [DriverEntry; 2] = [
    DriverEntry {
        name: "mtk-uart0",
        class: DriverClass::Other,
        compatible: "mediatek,mt6577-uart",
        regs: Region { base: 0x1100_2000, size: 0x1000 },
        irq: 0,
    },
    DriverEntry {
        name: "mtk-msdc0-emmc",
        class: DriverClass::Storage,
        compatible: "mediatek,mt6768-mmc",
        regs: Region { base: 0x1123_0000, size: 0x1000 },
        irq: 0,
    },
];

pub const PROFILE: DeviceProfile = DeviceProfile {
    id: "x6826c",
    display_name: "Infinix Hot 20 (X6826C)",
    soc: "mt6768",
    arch: Arch::Armv8a,
    ui: UiProfile::Mobile,
    ram: &[],
    uart: UartKind::Ns16550Dw32 { reg_shift: 2 },
    uart_base: 0x1100_2000,
    psci: PsciConduit::Smc,
    gic_dist: 0,
    gic_redist: 0,
    display: DisplayProfile {
        source: FramebufferSource::MtkVideoLfb,
        width: 720,
        height: 1612,
        stride_bytes: 720 * 4,
        order: PixelOrder::Rgbx,
        rotation_degrees: 0,
    },
    drivers: &DRIVERS,
    register: None,
};
