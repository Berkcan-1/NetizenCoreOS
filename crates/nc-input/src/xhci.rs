// crates/nc-input/src/xhci.rs
use core::ptr::read_volatile;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use nc_driver::pci::enumerate_pci_bus;
use nc_hal::KeyCode;

const HID_QUEUE_SIZE: usize = 64;

#[derive(Clone, Copy)]
pub enum UsbHidEvent {
    Key(KeyCode),
    MouseDelta {
        dx: i16,
        dy: i16,
        buttons: u8,
    },
    MouseAbsolute {
        x_norm: u16, // 0..32767 (QEMU/VBox usb-tablet uyumu)
        y_norm: u16,
        buttons: u8,
    },
}

static mut HID_QUEUE: [UsbHidEvent; HID_QUEUE_SIZE] = [UsbHidEvent::Key(KeyCode::Unknown); HID_QUEUE_SIZE];
static HID_HEAD: AtomicUsize = AtomicUsize::new(0);
static HID_TAIL: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
pub struct XhciControllerStatus {
    pub present: bool,
    pub vendor_id: u16,
    pub device_id: u16,
    pub mmio_base: usize,
    pub max_ports: u8,
    pub connected_ports: u8,
    pub bios_legacy_active: bool,
}

static mut XHCI_STATUS: XhciControllerStatus = XhciControllerStatus {
    present: false,
    vendor_id: 0,
    device_id: 0,
    mmio_base: 0,
    max_ports: 0,
    connected_ports: 0,
    bios_legacy_active: false,
};
static XHCI_PROBED: AtomicBool = AtomicBool::new(false);

pub fn push_usb_hid_event(ev: UsbHidEvent) {
    let head = HID_HEAD.load(Ordering::Acquire) & (HID_QUEUE_SIZE - 1);
    let next = (head + 1) & (HID_QUEUE_SIZE - 1);
    let tail = HID_TAIL.load(Ordering::Acquire) & (HID_QUEUE_SIZE - 1);
    if next != tail {
        unsafe {
            let ptr = core::ptr::addr_of_mut!(HID_QUEUE) as *mut UsbHidEvent;
            core::ptr::write(ptr.add(head), ev);
        }
        HID_HEAD.store(next, Ordering::Release);
    }
}

pub fn pop_usb_hid_event() -> Option<UsbHidEvent> {
    let head = HID_HEAD.load(Ordering::Acquire) & (HID_QUEUE_SIZE - 1);
    let tail = HID_TAIL.load(Ordering::Acquire) & (HID_QUEUE_SIZE - 1);
    if tail == head {
        return None;
    }
    let ev = unsafe {
        let ptr = core::ptr::addr_of!(HID_QUEUE) as *const UsbHidEvent;
        core::ptr::read(ptr.add(tail))
    };
    HID_TAIL.store((tail + 1) & (HID_QUEUE_SIZE - 1), Ordering::Release);
    Some(ev)
}

/// PCI veriyolunda xHCI (0C:03:30) denetleyicisini arar, bağlı USB aygıt sayısını
/// ve BIOS SMM/Legacy HID durumunu salt-okunur (mevcut klavyeyi koparmadan) doğrular.
pub fn init_and_probe_xhci() -> XhciControllerStatus {
    if XHCI_PROBED.load(Ordering::Acquire) {
        return unsafe { XHCI_STATUS };
    }

    for mut ep in enumerate_pci_bus() {
        // Class 0x0C (Serial Bus), Subclass 0x03 (USB), ProgIF 0x30 (xHCI)
        if ep.class_code == 0x0C && ep.subclass == 0x03 && ep.prog_if == 0x30 {
            let Some(mmio) = ep.bar(0).mmio_base() else {
                continue;
            };
            if mmio == 0 || mmio >= 0xFFFF_0000 {
                continue;
            }

            ep.enable_resource_decoding(true, false);

            unsafe {
                let cap_len = read_volatile(mmio as *const u8) as usize;
                let hcsparams1 = read_volatile((mmio + 0x04) as *const u32);
                let hccparams1 = read_volatile((mmio + 0x10) as *const u32);

                let max_ports = ((hcsparams1 >> 24) & 0xFF) as u8;
                let op_base = mmio + cap_len;

                let mut connected = 0u8;
                for p in 0..(max_ports.min(32) as usize) {
                    let portsc = read_volatile((op_base + 0x400 + p * 0x10) as *const u32);
                    // Bit 0 = CCS (Current Connect Status)
                    if (portsc & 1) != 0 {
                        connected += 1;
                    }
                }

                // Extended Capabilities Pointer (xECP) üzerinden USB Legacy Support kontrolü
                let xecp_dwords = ((hccparams1 >> 16) & 0xFFFF) as usize;
                let mut bios_owned = false;
                if xecp_dwords != 0 {
                    let leg_reg = read_volatile((mmio + xecp_dwords * 4) as *const u32);
                    if (leg_reg & 0xFF) == 1 && (leg_reg & (1 << 16)) != 0 {
                        bios_owned = true;
                    }
                }

                let st = XhciControllerStatus {
                    present: true,
                    vendor_id: ep.vendor_id,
                    device_id: ep.device_id,
                    mmio_base: mmio,
                    max_ports,
                    connected_ports: connected,
                    bios_legacy_active: bios_owned,
                };
                XHCI_STATUS = st;
                XHCI_PROBED.store(true, Ordering::Release);
                return st;
            }
        }
    }

    XHCI_PROBED.store(true, Ordering::Release);
    unsafe { XHCI_STATUS }
}

/// Standart 8-bayt USB HID Boot Keyboard raporunu (`[modifiers, reserved, k0..k5]`) çözümler
pub fn decode_usb_boot_keyboard_report(report: &[u8; 8], prev_keys: &mut [u8; 6]) {
    let modifiers = report[0];
    let shift = (modifiers & 0x22) != 0; // Left Shift (0x02) | Right Shift (0x20)

    for i in 2..8 {
        let usage = report[i];
        if usage < 4 || prev_keys.contains(&usage) {
            continue;
        }
        let key = match usage {
            0x04..=0x1D => {
                let base = b'a' + (usage - 0x04);
                let ch = if shift { (base - 32) as char } else { base as char };
                KeyCode::Char(ch)
            }
            0x1E => KeyCode::Char(if shift { '!' } else { '1' }),
            0x1F => KeyCode::Char(if shift { '\'' } else { '2' }),
            0x20 => KeyCode::Char(if shift { '^' } else { '3' }),
            0x21 => KeyCode::Char(if shift { '+' } else { '4' }),
            0x22 => KeyCode::Char(if shift { '%' } else { '5' }),
            0x23 => KeyCode::Char(if shift { '&' } else { '6' }),
            0x24 => KeyCode::Char(if shift { '/' } else { '7' }),
            0x25 => KeyCode::Char(if shift { '(' } else { '8' }),
            0x26 => KeyCode::Char(if shift { ')' } else { '9' }),
            0x27 => KeyCode::Char(if shift { '=' } else { '0' }),
            0x28 => KeyCode::Enter,
            0x29 => KeyCode::Escape,
            0x2A => KeyCode::Backspace,
            0x2B => KeyCode::Tab,
            0x2C => KeyCode::Space,
            0x4F => KeyCode::Right,
            0x50 => KeyCode::Left,
            0x51 => KeyCode::Down,
            0x52 => KeyCode::Up,
            _ => KeyCode::Unknown,
        };
        if key != KeyCode::Unknown {
            push_usb_hid_event(UsbHidEvent::Key(key));
        }
    }
    prev_keys.copy_from_slice(&report[2..8]);
}

/// Standart 3 veya 4 baytlık USB HID Boot Mouse raporunu (`[buttons, dx, dy]`) çözümler
pub fn decode_usb_boot_mouse_report(report: &[u8]) {
    if report.len() < 3 {
        return;
    }
    let buttons = report[0] & 0x07;
    let dx = (report[1] as i8) as i16;
    let dy = (report[2] as i8) as i16;
    push_usb_hid_event(UsbHidEvent::MouseDelta { dx, dy, buttons });
}
