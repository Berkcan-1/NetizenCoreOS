#![no_std]

use nc_device::{
    Arch, DeviceProfile, DisplayProfile, DriverClass, DriverEntry, FramebufferSource, PixelOrder,
    PsciConduit, Region, UartKind, UiProfile,
};

const DRIVERS: [DriverEntry; 3] = [
    DriverEntry {
        name: "pl011",
        class: DriverClass::Other,
        compatible: "arm,pl011",
        regs: Region { base: 0x0900_0000, size: 0x1000 },
        irq: 33,
    },
    DriverEntry {
        name: "virtio-mmio-block",
        class: DriverClass::Storage,
        compatible: "virtio,mmio",
        regs: Region { base: 0x0a00_0000, size: 0x200 },
        irq: 48,
    },
    DriverEntry {
        name: "pl031",
        class: DriverClass::Other,
        compatible: "arm,pl031",
        regs: Region { base: 0x0901_0000, size: 0x1000 },
        irq: 34,
    },
];

pub const PROFILE: DeviceProfile = DeviceProfile {
    id: "qemu-virt",
    display_name: "QEMU virt (aarch64)",
    soc: "qemu",
    arch: Arch::Armv8a,
    ui: UiProfile::Mobile,
    ram: &[],
    uart: UartKind::Pl011,
    uart_base: 0x0900_0000,
    psci: PsciConduit::Hvc,
    gic_dist: 0x0800_0000,
    gic_redist: 0x080A_0000,
    display: DisplayProfile {
        source: FramebufferSource::Fixed(0x5000_0000),
        width: 540,
        height: 960,
        stride_bytes: 540 * 4,
        order: PixelOrder::Bgrx,
        rotation_degrees: 0,
    },
    drivers: &DRIVERS,
    register: None,
};
