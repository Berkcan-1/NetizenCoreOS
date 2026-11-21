#![cfg_attr(not(test), no_std)]

pub mod backlight;
pub mod battery;
pub mod connsys;
pub mod cpufreq;
pub mod display;
pub mod emmc;
pub mod i2c;
pub mod keys;
pub mod mali;
pub mod mmio;
pub mod pwrap;
pub mod scp;
pub mod touch;
pub mod usb;
pub mod wdt;

use nc_device::{
    Arch, DeviceProfile, DisplayProfile, DriverClass, DriverEntry, DriverHost, FramebufferSource,
    HardwareOps, PixelOrder, PsciConduit, Region, UartKind, UiProfile,
};

/// Fallback DRAM region matching `stock/dtb` `/memory`; LK patches the full physical RAM
/// map into the live DTB passed in `x0` at boot.
const RAM: [Region; 1] = [Region {
    base: 0x4000_0000,
    size: 0x3e60_5000,
}];

/// Hardware peripheral table verified directly against X6826C `stock/dtb` + `stock/dtbo`
/// (all GIC INTIDs computed as `SPI + 32`).
const DRIVERS: [DriverEntry; 16] = [
    DriverEntry {
        name: "mtk-uart0",
        class: DriverClass::Other,
        compatible: "mediatek,mt6577-uart",
        regs: Region { base: 0x1100_2000, size: 0x1000 },
        irq: 144, // SPI 0x70 + 32
    },
    DriverEntry {
        name: "mtk-toprgu-wdt",
        class: DriverClass::Power,
        compatible: "mediatek,mt6768-wdt",
        regs: Region { base: 0x1000_7000, size: 0x1000 },
        irq: 188, // SPI 0x9c + 32
    },
    DriverEntry {
        name: "mtk-pwrap-mt6358",
        class: DriverClass::Power,
        compatible: "mediatek,mt6768-pwrap",
        regs: Region { base: 0x1000_d000, size: 0x1000 },
        irq: 226, // SPI 0xc2 + 32
    },
    DriverEntry {
        name: "mtk-i2c5-rt9471",
        class: DriverClass::Power,
        compatible: "richtek,swchg",
        regs: Region { base: 0x1101_6000, size: 0x1000 },
        irq: 179, // SPI 0x93 + 32
    },
    DriverEntry {
        name: "mtk-usb20-musb",
        class: DriverClass::Power,
        compatible: "mediatek,mt6768-usb20",
        regs: Region { base: 0x1120_0000, size: 0x10000 },
        irq: 129, // SPI 0x61 + 32
    },
    DriverEntry {
        name: "mtk-apmixed-cpufreq",
        class: DriverClass::Power,
        compatible: "mediatek,apmixed",
        regs: Region { base: 0x1000_c000, size: 0x1000 },
        irq: 0,
    },
    DriverEntry {
        name: "mtk-disp-ovl0",
        class: DriverClass::Display,
        compatible: "mediatek,disp_ovl0",
        regs: Region { base: 0x1400_b000, size: 0x1000 },
        irq: 262, // SPI 0xe6 + 32
    },
    DriverEntry {
        name: "mtk-disp-pwm0",
        class: DriverClass::Display,
        compatible: "mediatek,disp_pwm",
        regs: Region { base: 0x1100_e000, size: 0x1000 },
        irq: 0,
    },
    DriverEntry {
        name: "mtk-i2c6-lcd-bias",
        class: DriverClass::Display,
        compatible: "mediatek,i2c_lcd_bias",
        regs: Region { base: 0x1100_d000, size: 0x1000 },
        irq: 180, // SPI 0x94 + 32
    },
    DriverEntry {
        name: "mtk-msdc0-emmc",
        class: DriverClass::Storage,
        compatible: "mediatek,mt6768-mmc",
        regs: Region { base: 0x1123_0000, size: 0x10000 },
        irq: 132, // SPI 0x64 + 32
    },
    DriverEntry {
        name: "td4160-spi2-touch",
        class: DriverClass::Touch,
        compatible: "mediatek,cap_touch_2",
        regs: Region { base: 0x1101_2000, size: 0x1000 },
        irq: 177, // SPI 0x91 + 32
    },
    DriverEntry {
        name: "mtk-kpd-keys",
        class: DriverClass::Keys,
        compatible: "mediatek,kp",
        regs: Region { base: 0x1001_0000, size: 0x1000 },
        irq: 105, // SPI 0x49 + 32
    },
    DriverEntry {
        name: "mtk-scp-sensorhub",
        class: DriverClass::Sensor,
        compatible: "mediatek,scp",
        regs: Region { base: 0x1050_0000, size: 0x000d_a000 },
        irq: 237, // SPI 0xcd + 32
    },
    DriverEntry {
        name: "arm-mali-g52",
        class: DriverClass::Display,
        compatible: "mediatek,mali",
        regs: Region { base: 0x1304_0000, size: 0x4000 },
        irq: 306, // SPI 0x112 + 32 (GPU; MMU=307, JOB=308, EVENT=309, PWR=310)
    },
    DriverEntry {
        name: "mtk-consys-mt6631",
        class: DriverClass::Radio,
        compatible: "mediatek,mt6768-consys",
        regs: Region { base: 0x1800_2000, size: 0x000b_0000 },
        irq: 316, // SPI 0x11c + 32
    },
    DriverEntry {
        name: "mtk-btif",
        class: DriverClass::Radio,
        compatible: "mediatek,btif",
        regs: Region { base: 0x1100_c000, size: 0x1000 },
        irq: 165, // SPI 0x85 + 32
    },
];

/// Dependency-ordered MT6768 / X6826C platform driver registration:
/// 1. Watchdog disarm (`wdt`)
/// 2. Core buses (`pwrap` -> `i2c`) before PMIC/I2C clients
/// 3. Display handoff & backlight (`display` -> `backlight`)
/// 4. Power input & battery (`usb`/RT9471 -> `battery`/MT6358 -> `cpufreq` read-only)
/// 5. Storage (`emmc`, strictly read-only)
/// 6. Input (`touch` -> `keys`)
/// 7. Co-processors & radios (`scp`, `mali`, `connsys`)
pub fn register(host: &mut dyn DriverHost) {
    wdt::init(host);
    pwrap::init(host);
    i2c::init(host);
    display::init(host);
    backlight::init(host);
    usb::init(host);
    battery::init(host);
    cpufreq::init(host);
    emmc::init(host);
    touch::init(host);
    keys::init(host);
    scp::init(host);
    mali::init(host);
    connsys::init(host);
}

pub const PROFILE: DeviceProfile = DeviceProfile {
    id: "x6826c",
    display_name: "Infinix Hot 20 (X6826C)",
    soc: "mt6768",
    arch: Arch::Armv8a,
    ui: UiProfile::Mobile,
    ram: &RAM,
    uart: UartKind::Ns16550Dw32 { reg_shift: 2 },
    uart_base: 0x1100_2000,
    psci: PsciConduit::Smc,
    gic_dist: 0x0c00_0000,
    gic_redist: 0x0c04_0000,
    display: DisplayProfile {
        source: FramebufferSource::MtkVideoLfb,
        width: display::PANEL_W,
        height: display::PANEL_H,
        stride_bytes: display::LK_FB_PITCH,
        order: PixelOrder::Bgrx,
        rotation_degrees: 0,
    },
    drivers: &DRIVERS,
    register: Some(register),
    hw_ops: HardwareOps {
        battery: Some(battery::read_status),
        set_backlight: Some(backlight::set_brightness),
        has_hw_backlight: true,
        has_hw_bt: false,
        has_hw_cellular: false,
        has_hw_audio: false,
        has_hw_camera: false,
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_matches_verified_x6826c_hardware() {
        assert_eq!(PROFILE.id, "x6826c");
        assert_eq!(PROFILE.soc, "mt6768");
        assert_eq!(PROFILE.gic_dist, 0x0c00_0000);
        assert_eq!(PROFILE.gic_redist, 0x0c04_0000);
        assert_eq!(PROFILE.uart_base, 0x1100_2000);
        assert_eq!(PROFILE.display.width, 720);
        assert_eq!(PROFILE.display.height, 1640);
        assert_eq!(PROFILE.display.stride_bytes, 2944);
        assert!(PROFILE.hw_ops.battery.is_some());
        assert!(PROFILE.hw_ops.set_backlight.is_some());
        assert!(PROFILE.hw_ops.has_hw_backlight);
        assert!(!PROFILE.hw_ops.has_hw_bt);
        assert!(PROFILE.find_driver(DriverClass::Storage).is_some());
        assert!(PROFILE.find_driver(DriverClass::Touch).is_some());
        assert!(PROFILE.find_driver(DriverClass::Keys).is_some());
    }
}
