use alloc::string::String;
use alloc::vec::Vec;
use nc_abi::sys as wire;
use nc_efra::EfraDriverPackage;

use crate::proto::{Reader, Writer};

const MAX_LIST: usize = 32;
const MAX_DEVICES: usize = 16;

pub fn handle(op: u8, arg0: u32, arg1: u32, _arg2: u32, payload: &[u8]) -> (i32, Vec<u8>) {
    match op {
        wire::OP_PCI_NIC => (0, pci_nic()),
        wire::OP_WL_POLL => {
            nc_net::wireless::global().poll();
            (0, wireless_snapshot())
        }
        wire::OP_WL_CMD => wireless_command(arg0, arg1, payload),
        wire::OP_ETH_INFO => (0, ethernet_snapshot()),
        wire::OP_ETH_CMD => ethernet_command(arg0),
        wire::OP_DRV_LIST => (0, driver_snapshot()),
        wire::OP_DRV_REGISTER => match EfraDriverPackage::parse(payload) {
            Some(pkg) => {
                nc_driver::global().register_or_update_package(pkg);
                (0, driver_snapshot())
            }
            None => (wire::ERR_BAD_REQUEST, Vec::new()),
        },
        wire::OP_DRV_UPGRADE => {
            let reg = nc_driver::global();
            let mut w = Writer::new();
            match reg.upgrade_device_driver(arg0 as usize) {
                Some(pkg) => {
                    let raw = pkg.serialize();
                    w.u8(1);
                    w.u16(raw.len().min(u16::MAX as usize) as u16);
                    w.bytes(&raw);
                }
                None => w.u8(0),
            }
            w.bytes(&driver_snapshot());
            (0, w.buf)
        }
        wire::OP_DRV_SCAN => {
            nc_driver::global().optimize_and_scan();
            (0, driver_snapshot())
        }
        wire::OP_SEC_KIND => (0, alloc::vec![nc_app::security::kind()]),
        wire::OP_SEC_VERIFY => (0, alloc::vec![nc_app::security::verify_current(payload) as u8]),
        wire::OP_SEC_SUBMIT => {
            nc_app::security::submit(arg0 as u8, payload.to_vec());
            (0, Vec::new())
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

fn pci_read_32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        nc_arch::outl(0xCF8, addr);
        nc_arch::inl(0xCFC)
    }
}

fn pci_nic() -> Vec<u8> {
    let mut w = Writer::new();
    for bus in 0..=8u8 {
        for dev in 0..32u8 {
            let vd = pci_read_32(bus, dev, 0, 0x00);
            let vendor = (vd & 0xFFFF) as u16;
            let device = ((vd >> 16) & 0xFFFF) as u16;
            if vendor == 0xFFFF || vendor == 0x0000 {
                continue;
            }
            let class_rev = pci_read_32(bus, dev, 0, 0x08);
            let base_class = ((class_rev >> 24) & 0xFF) as u8;
            let sub_class = ((class_rev >> 16) & 0xFF) as u8;
            if (base_class == 0x02 && sub_class == 0x00) || (vendor == 0x8086 && device == 0x100E) {
                let irq = (pci_read_32(bus, dev, 0, 0x3C) & 0xFF) as u8;
                let model = match (vendor, device) {
                    (0x8086, 0x100E) => "Intel 82540EM Gigabit Ethernet (e1000)",
                    (0x8086, 0x100F) => "Intel 82545EM Copper Gigabit",
                    (0x8086, 0x10D3) => "Intel 82574L Gigabit Ethernet",
                    (0x1AF4, 0x1000) => "VirtIO Paravirtualized Network Adapter",
                    (0x10EC, 0x8139) => "Realtek RTL8139 Fast Ethernet",
                    (0x10EC, 0x8168) => "Realtek RTL8111/8168 Gigabit Ethernet",
                    (0x1022, 0x2000) => "AMD PCnet-PCI II (Am79C970A)",
                    _ => "PCI Ethernet Controller",
                };
                w.u8(1);
                w.u8(bus);
                w.u8(dev);
                w.u8(irq);
                w.string(model);
                return w.buf;
            }
        }
    }
    w.u8(0);
    w.u8(0);
    w.u8(0);
    w.u8(0);
    w.string("PCI Ethernet Not Detected");
    w.buf
}

fn wireless_snapshot() -> Vec<u8> {
    let wl = nc_net::wireless::global();
    let mut w = Writer::new();
    let mut flags = 0u8;
    if wl.wifi_enabled {
        flags |= 1;
    }
    if wl.bt_enabled {
        flags |= 2;
    }
    if wl.wifi_scanning {
        flags |= 4;
    }
    if wl.bt_scanning {
        flags |= 8;
    }
    w.u8(flags);
    w.string(&wl.wifi_adapter_name);
    w.string(&wl.bt_adapter_name);
    w.string(&wl.status_message);
    let wifi_count = wl.wifi_networks.len().min(MAX_LIST);
    w.u8(wifi_count as u8);
    for net in wl.wifi_networks.iter().take(wifi_count) {
        w.string(&net.ssid);
        w.u8(net.channel);
        w.string(&net.freq_label);
        w.u8(net.signal_pct);
        w.string(&net.security);
        w.u8(net.connected as u8);
    }
    let bt_count = wl.bt_devices.len().min(MAX_LIST);
    w.u8(bt_count as u8);
    for dev in wl.bt_devices.iter().take(bt_count) {
        w.string(&dev.mac);
        w.string(&dev.name);
        w.string(&dev.kind);
        w.u8(dev.battery);
        w.u8(dev.connected as u8);
    }
    w.buf
}

fn wireless_command(command: u32, value: u32, payload: &[u8]) -> (i32, Vec<u8>) {
    let wl = nc_net::wireless::global();
    match command {
        wire::WL_INIT => wl.send_command("CMD:INIT"),
        wire::WL_WIFI_SCAN => wl.request_wifi_scan(),
        wire::WL_BT_SCAN => wl.request_bt_scan(),
        wire::WL_SET_WIFI => wl.set_wifi_enabled(value != 0),
        wire::WL_SET_BT => wl.set_bt_enabled(value != 0),
        wire::WL_CONNECT => {
            let mut r = Reader::new(payload);
            let (Some(ssid), Some(pass)) = (r.string(), r.string()) else {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            };
            wl.connect_wifi(&ssid, &pass);
        }
        wire::WL_DISCONNECT => {
            let mut r = Reader::new(payload);
            let Some(ssid) = r.string() else {
                return (wire::ERR_BAD_REQUEST, Vec::new());
            };
            wl.disconnect_wifi(&ssid);
        }
        wire::WL_TOGGLE_BT => wl.toggle_bt_device(value as usize),
        _ => return (wire::ERR_BAD_REQUEST, Vec::new()),
    }
    (0, wireless_snapshot())
}

fn dhcp_code(state: &nc_net::DhcpState) -> u8 {
    match state {
        nc_net::DhcpState::Init => 0,
        nc_net::DhcpState::DiscoverSent => 1,
        nc_net::DhcpState::RequestSent => 2,
        nc_net::DhcpState::Bound => 3,
    }
}

fn ethernet_snapshot() -> Vec<u8> {
    let net = nc_net::global();
    let mut w = Writer::new();
    w.u8(net.link_online as u8);
    w.u8(dhcp_code(&net.dhcp.state));
    w.bytes(&net.mac_addr);
    w.bytes(&net.ip_addr);
    w.bytes(&net.gateway_ip);
    w.bytes(&net.dns_ip);
    w.u64(net.rx_packets);
    w.u64(net.tx_packets);
    w.buf
}

fn ethernet_command(command: u32) -> (i32, Vec<u8>) {
    let net = nc_net::global();
    match command {
        wire::ETH_DHCP => {
            if !net.link_online {
                return (0, alloc::vec![0]);
            }
            let pkt = net.dhcp.build_discover(net.mac_addr);
            if let Some(ref mut drv) = net.driver {
                let _ = drv.send_packet(&pkt);
                net.tx_packets += 1;
            }
            (0, alloc::vec![1])
        }
        wire::ETH_ARP => {
            net.resolve_gateway_mac();
            (0, alloc::vec![1])
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

fn driver_snapshot() -> Vec<u8> {
    let reg = nc_driver::global();
    let mut w = Writer::new();
    let label: String = String::from(reg.environment.label());
    w.string(&label);
    let count = reg.devices.len().min(MAX_DEVICES);
    w.u16(reg.devices.len().min(u16::MAX as usize) as u16);
    w.u8(count as u8);
    for dev in reg.devices.iter().take(count) {
        w.string(&dev.hw_name);
        w.string(&dev.slot_label);
        w.string(&dev.driver_file);
        w.u8(dev.is_generic_binding as u8);
    }
    w.buf
}
