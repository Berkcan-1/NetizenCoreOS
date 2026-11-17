use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_abi::sys as wire;
use nc_efra::EfraDriverPackage;

use crate::proto::{Reader, Writer};

pub type Transport = fn(u8, u32, u32, u32, &[u8]) -> Option<Vec<u8>>;

static mut TRANSPORT: Option<Transport> = None;

pub fn set_transport(transport: Transport) {
    unsafe {
        *core::ptr::addr_of_mut!(TRANSPORT) = Some(transport);
    }
}

#[cfg(feature = "kernel")]
fn default_transport() -> Option<Transport> {
    fn direct(op: u8, a0: u32, a1: u32, a2: u32, payload: &[u8]) -> Option<Vec<u8>> {
        let (status, out) = crate::service::handle(op, a0, a1, a2, payload);
        if status == 0 {
            Some(out)
        } else {
            None
        }
    }
    Some(direct)
}

#[cfg(all(feature = "client", not(feature = "kernel")))]
fn default_transport() -> Option<Transport> {
    fn syscall(op: u8, a0: u32, a1: u32, a2: u32, payload: &[u8]) -> Option<Vec<u8>> {
        let mut request = vec![0u8; wire::REQ_HEADER + payload.len()];
        let used = wire::encode_request(&mut request, op, a0, a1, a2, payload)?;
        request.truncate(used);
        let mut response = vec![0u8; wire::MAX_RESPONSE];
        let n = nc_app_sdk::sys::sys_call(&request, &mut response);
        if n < 0 || n as usize > response.len() {
            return None;
        }
        let (status, body) = wire::parse_response(&response[..n as usize]).ok()?;
        if status != 0 {
            return None;
        }
        Some(body.to_vec())
    }
    Some(syscall)
}

#[cfg(not(any(feature = "kernel", feature = "client")))]
fn default_transport() -> Option<Transport> {
    None
}

fn call(op: u8, a0: u32, a1: u32, a2: u32, payload: &[u8]) -> Option<Vec<u8>> {
    let custom = unsafe { *core::ptr::addr_of!(TRANSPORT) };
    let transport = custom.or_else(default_transport)?;
    transport(op, a0, a1, a2, payload)
}

#[derive(Clone, Debug)]
pub struct WifiNetworkInfo {
    pub ssid: String,
    pub channel: u8,
    pub freq_label: String,
    pub signal_pct: u8,
    pub security: String,
    pub connected: bool,
}

impl WifiNetworkInfo {
    pub fn bars(&self) -> u8 {
        match self.signal_pct {
            0..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        }
    }

    pub fn is_open(&self) -> bool {
        self.security.is_empty()
            || self.security == "--"
            || self.security.eq_ignore_ascii_case("none")
            || self.security.eq_ignore_ascii_case("acik")
    }
}

#[derive(Clone, Debug)]
pub struct BluetoothDeviceInfo {
    pub mac: String,
    pub name: String,
    pub kind: String,
    pub battery: u8,
    pub connected: bool,
}

pub struct Wireless {
    pub wifi_adapter_name: String,
    pub bt_adapter_name: String,
    pub wifi_enabled: bool,
    pub bt_enabled: bool,
    pub wifi_scanning: bool,
    pub bt_scanning: bool,
    pub wifi_networks: Vec<WifiNetworkInfo>,
    pub bt_devices: Vec<BluetoothDeviceInfo>,
    pub status_message: String,
}

impl Wireless {
    fn empty() -> Wireless {
        Wireless {
            wifi_adapter_name: String::new(),
            bt_adapter_name: String::new(),
            wifi_enabled: false,
            bt_enabled: false,
            wifi_scanning: false,
            bt_scanning: false,
            wifi_networks: Vec::new(),
            bt_devices: Vec::new(),
            status_message: String::new(),
        }
    }

    fn apply(&mut self, bytes: &[u8]) -> Option<()> {
        let mut r = Reader::new(bytes);
        let flags = r.u8()?;
        let wifi_adapter_name = r.string()?;
        let bt_adapter_name = r.string()?;
        let status_message = r.string()?;
        let wifi_count = r.u8()? as usize;
        let mut wifi_networks = Vec::new();
        for _ in 0..wifi_count {
            wifi_networks.push(WifiNetworkInfo {
                ssid: r.string()?,
                channel: r.u8()?,
                freq_label: r.string()?,
                signal_pct: r.u8()?,
                security: r.string()?,
                connected: r.u8()? != 0,
            });
        }
        let bt_count = r.u8()? as usize;
        let mut bt_devices = Vec::new();
        for _ in 0..bt_count {
            bt_devices.push(BluetoothDeviceInfo {
                mac: r.string()?,
                name: r.string()?,
                kind: r.string()?,
                battery: r.u8()?,
                connected: r.u8()? != 0,
            });
        }
        self.wifi_enabled = flags & 1 != 0;
        self.bt_enabled = flags & 2 != 0;
        self.wifi_scanning = flags & 4 != 0;
        self.bt_scanning = flags & 8 != 0;
        self.wifi_adapter_name = wifi_adapter_name;
        self.bt_adapter_name = bt_adapter_name;
        self.status_message = status_message;
        self.wifi_networks = wifi_networks;
        self.bt_devices = bt_devices;
        Some(())
    }

    fn command(&mut self, command: u32, value: u32, payload: &[u8]) {
        if let Some(reply) = call(wire::OP_WL_CMD, command, value, 0, payload) {
            let _ = self.apply(&reply);
        }
    }

    pub fn poll(&mut self) {
        if let Some(reply) = call(wire::OP_WL_POLL, 0, 0, 0, &[]) {
            let _ = self.apply(&reply);
        }
    }

    pub fn send_command(&mut self, cmd: &str) {
        if cmd == "CMD:INIT" {
            self.command(wire::WL_INIT, 0, &[]);
        }
    }

    pub fn request_wifi_scan(&mut self) {
        self.command(wire::WL_WIFI_SCAN, 0, &[]);
    }

    pub fn request_bt_scan(&mut self) {
        self.command(wire::WL_BT_SCAN, 0, &[]);
    }

    pub fn set_wifi_enabled(&mut self, enabled: bool) {
        self.command(wire::WL_SET_WIFI, enabled as u32, &[]);
    }

    pub fn set_bt_enabled(&mut self, enabled: bool) {
        self.command(wire::WL_SET_BT, enabled as u32, &[]);
    }

    pub fn connect_wifi(&mut self, ssid: &str, password: &str) {
        let mut w = Writer::new();
        w.string(ssid);
        w.string(password);
        self.command(wire::WL_CONNECT, 0, &w.buf);
    }

    pub fn disconnect_wifi(&mut self, ssid: &str) {
        let mut w = Writer::new();
        w.string(ssid);
        self.command(wire::WL_DISCONNECT, 0, &w.buf);
    }

    pub fn toggle_bt_device(&mut self, idx: usize) {
        self.command(wire::WL_TOGGLE_BT, idx as u32, &[]);
    }
}

static mut WIRELESS: Option<Wireless> = None;

pub mod wireless {
    pub use super::{BluetoothDeviceInfo, WifiNetworkInfo, Wireless};

    pub fn global() -> &'static mut Wireless {
        super::wireless_global()
    }
}

fn wireless_global() -> &'static mut Wireless {
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(WIRELESS) };
    if slot.is_none() {
        let mut fresh = Wireless::empty();
        fresh.poll();
        *slot = Some(fresh);
    }
    slot.as_mut().unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DhcpState {
    Init,
    DiscoverSent,
    RequestSent,
    Bound,
}

pub struct Ether {
    pub link_online: bool,
    pub dhcp_state: DhcpState,
    pub mac_addr: [u8; 6],
    pub ip_addr: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub rx_packets: u64,
    pub tx_packets: u64,
}

impl Ether {
    fn empty() -> Ether {
        Ether {
            link_online: false,
            dhcp_state: DhcpState::Init,
            mac_addr: [0; 6],
            ip_addr: [0; 4],
            gateway_ip: [0; 4],
            dns_ip: [0; 4],
            rx_packets: 0,
            tx_packets: 0,
        }
    }

    pub fn refresh(&mut self) {
        let Some(reply) = call(wire::OP_ETH_INFO, 0, 0, 0, &[]) else {
            return;
        };
        let mut r = Reader::new(&reply);
        let parsed = (|| -> Option<Ether> {
            Some(Ether {
                link_online: r.u8()? != 0,
                dhcp_state: match r.u8()? {
                    0 => DhcpState::Init,
                    1 => DhcpState::DiscoverSent,
                    2 => DhcpState::RequestSent,
                    _ => DhcpState::Bound,
                },
                mac_addr: r.array::<6>()?,
                ip_addr: r.array::<4>()?,
                gateway_ip: r.array::<4>()?,
                dns_ip: r.array::<4>()?,
                rx_packets: r.u64()?,
                tx_packets: r.u64()?,
            })
        })();
        if let Some(fresh) = parsed {
            *self = fresh;
        }
    }

    pub fn dhcp_discover(&mut self) -> bool {
        let ok = call(wire::OP_ETH_CMD, wire::ETH_DHCP, 0, 0, &[]).map_or(false, |r| r.first() == Some(&1));
        self.refresh();
        ok
    }

    pub fn resolve_gateway_mac(&mut self) {
        let _ = call(wire::OP_ETH_CMD, wire::ETH_ARP, 0, 0, &[]);
        self.refresh();
    }
}

static mut ETHER: Option<Ether> = None;

pub fn ether() -> &'static mut Ether {
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(ETHER) };
    if slot.is_none() {
        let mut fresh = Ether::empty();
        fresh.refresh();
        *slot = Some(fresh);
    }
    slot.as_mut().unwrap()
}

#[derive(Clone, Debug)]
pub struct Device {
    pub hw_name: String,
    pub slot_label: String,
    pub driver_file: String,
    pub is_generic_binding: bool,
}

pub struct Drivers {
    pub environment_label: String,
    pub devices: Vec<Device>,
}

impl Drivers {
    fn empty() -> Drivers {
        Drivers { environment_label: String::new(), devices: Vec::new() }
    }

    fn apply(&mut self, bytes: &[u8]) -> Option<()> {
        let mut r = Reader::new(bytes);
        let label = r.string()?;
        let _total = r.u16()?;
        let count = r.u8()? as usize;
        let mut devices = Vec::new();
        for _ in 0..count {
            devices.push(Device {
                hw_name: r.string()?,
                slot_label: r.string()?,
                driver_file: r.string()?,
                is_generic_binding: r.u8()? != 0,
            });
        }
        self.environment_label = label;
        self.devices = devices;
        Some(())
    }

    pub fn refresh(&mut self) {
        if let Some(reply) = call(wire::OP_DRV_LIST, 0, 0, 0, &[]) {
            let _ = self.apply(&reply);
        }
    }

    pub fn register_package(&mut self, pkg: &EfraDriverPackage) {
        if let Some(reply) = call(wire::OP_DRV_REGISTER, 0, 0, 0, &pkg.serialize()) {
            let _ = self.apply(&reply);
        }
    }

    pub fn upgrade(&mut self, index: usize) -> Option<EfraDriverPackage> {
        let reply = call(wire::OP_DRV_UPGRADE, index as u32, 0, 0, &[])?;
        let (&found, rest) = reply.split_first()?;
        if found == 0 {
            let _ = self.apply(rest);
            return None;
        }
        if rest.len() < 2 {
            return None;
        }
        let size = u16::from_le_bytes([rest[0], rest[1]]) as usize;
        let body = &rest[2..];
        if size > body.len() {
            return None;
        }
        let pkg = EfraDriverPackage::parse(&body[..size]);
        let _ = self.apply(&body[size..]);
        pkg
    }

    pub fn optimize_and_scan(&mut self) {
        if let Some(reply) = call(wire::OP_DRV_SCAN, 0, 0, 0, &[]) {
            let _ = self.apply(&reply);
        }
    }
}

static mut DRIVERS: Option<Drivers> = None;

pub fn drivers() -> &'static mut Drivers {
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(DRIVERS) };
    if slot.is_none() {
        let mut fresh = Drivers::empty();
        fresh.refresh();
        *slot = Some(fresh);
    }
    slot.as_mut().unwrap()
}

pub struct PciNic {
    pub found: bool,
    pub bus: u8,
    pub dev: u8,
    pub irq: u8,
    pub model_name: &'static str,
}

static mut MODEL_NAMES: Vec<&'static str> = Vec::new();

fn intern(text: &str) -> &'static str {
    let names = unsafe { &mut *core::ptr::addr_of_mut!(MODEL_NAMES) };
    for known in names.iter() {
        if *known == text {
            return known;
        }
    }
    if names.len() >= 16 {
        return "PCI Ethernet Controller";
    }
    let leaked: &'static str = Box::leak(String::from(text).into_boxed_str());
    names.push(leaked);
    leaked
}

pub fn probe_pci_nic() -> PciNic {
    let fallback = PciNic { found: false, bus: 0, dev: 0, irq: 0, model_name: "PCI Ethernet Not Detected" };
    let Some(reply) = call(wire::OP_PCI_NIC, 0, 0, 0, &[]) else {
        return fallback;
    };
    let mut r = Reader::new(&reply);
    let parsed = (|| -> Option<PciNic> {
        Some(PciNic {
            found: r.u8()? != 0,
            bus: r.u8()?,
            dev: r.u8()?,
            irq: r.u8()?,
            model_name: intern(&r.string()?),
        })
    })();
    parsed.unwrap_or(fallback)
}

pub mod sec {
    use super::*;

    pub fn kind() -> u8 {
        call(wire::OP_SEC_KIND, 0, 0, 0, &[]).and_then(|r| r.first().copied()).unwrap_or(0)
    }

    pub fn verify_current(secret: &[u8]) -> bool {
        call(wire::OP_SEC_VERIFY, 0, 0, 0, secret).map_or(false, |r| r.first() == Some(&1))
    }

    pub fn submit(kind: u8, secret: Vec<u8>) {
        let _ = call(wire::OP_SEC_SUBMIT, kind as u32, 0, 0, &secret);
        let mut secret = secret;
        for byte in secret.iter_mut() {
            unsafe { core::ptr::write_volatile(byte, 0) };
        }
    }
}

pub fn push_settings(encoded: &[u8]) -> bool {
    call(wire::OP_SETTINGS_PUSH, 0, 0, 0, encoded).is_some()
}
