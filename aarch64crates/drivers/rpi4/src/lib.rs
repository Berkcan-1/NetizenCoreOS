#![no_std]

use nc_device::{
    Arch, DeviceProfile, DisplayProfile, DriverClass, DriverEntry, FramebufferSource, HardwareOps,
    PixelOrder, PsciConduit, Region, UartKind, UiProfile,
};

const DRIVERS: [DriverEntry; 2] = [
    DriverEntry {
        name: "pl011-uart0",
        class: DriverClass::Other,
        compatible: "arm,pl011",
        regs: Region { base: 0xFE20_1000, size: 0x200 },
        irq: 0,
    },
    DriverEntry {
        name: "emmc2-sd",
        class: DriverClass::Storage,
        compatible: "brcm,bcm2711-emmc2",
        regs: Region { base: 0xFE34_0000, size: 0x100 },
        irq: 0,
    },
];

pub const PROFILE: DeviceProfile = DeviceProfile {
    id: "rpi4",
    display_name: "Raspberry Pi 4 (BCM2711)",
    soc: "bcm2711",
    arch: Arch::Arm64,
    ui: UiProfile::Desktop,
    ram: &[],
    uart: UartKind::Pl011,
    uart_base: 0xFE20_1000,
    psci: PsciConduit::Smc,
    gic_dist: 0,
    gic_redist: 0,
    display: DisplayProfile {
        source: FramebufferSource::DtbSimpleFramebuffer,
        width: 1280,
        height: 720,
        stride_bytes: 1280 * 4,
        order: PixelOrder::Bgrx,
        rotation_degrees: 0,
    },
    drivers: &DRIVERS,
    register: None,
    hw_ops: HardwareOps::NONE,
};
