// crates/nc-net/src/lib.rs başlangıcı:
#![no_std]
extern crate alloc;

pub mod dhcp;
pub mod dns;
pub mod http;
pub mod rtl8168;
pub mod tcp;
pub mod tls;
pub mod tls13;
pub mod tls_engine;
pub mod tls_verify;
pub mod wireless;

use alloc::vec;
use alloc::vec::Vec;
use nc_driver::dma::{DmaBuffer, DmaMask};
use nc_driver::pci::{enumerate_pci_bus, PciEndpoint};

pub use dhcp::{DhcpAction, DhcpClient, DhcpState};
pub use dns::{encode_dns_query, parse_dns_response};
pub use http::{HttpState, TcpHttpClient};
pub use tcp::{
    build_ethernet_frame, build_ipv4_packet, build_tcp_segment, TcpSocket, TcpState, TCP_ACK,
    TCP_FIN, TCP_PSH, TCP_SYN,
};
pub use tls::{build_tls_client_hello, parse_tls_record};

pub const E1000_VENDOR_ID: u16 = 0x8086;
pub const E1000_DEVICE_ID: u16 = 0x100E;
pub const REALTEK_VENDOR_ID: u16 = 0x10EC;

pub fn build_udp_packet(
    _src_ip: [u8; 4],
    _dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(8 + payload.len());
    pkt.extend_from_slice(&src_port.to_be_bytes());
    pkt.extend_from_slice(&dst_port.to_be_bytes());
    let len = (8 + payload.len()) as u16;
    pkt.extend_from_slice(&len.to_be_bytes());
    pkt.extend_from_slice(&[0, 0]);
    pkt.extend_from_slice(payload);
    pkt
}

#[derive(Clone, Copy, Debug)]
pub struct PciDeviceInfo {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub bar0: u32,
    pub irq: u8,
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

const E1000_NUM_RX: usize = 64;
const E1000_NUM_TX: usize = 16;

enum NicBackend {
    IntelE1000 {
        rx_ring_dma: DmaBuffer,
        tx_ring_dma: DmaBuffer,
        rx_bufs_dma: Vec<DmaBuffer>,
        tx_bufs_dma: Vec<DmaBuffer>,
    },
    Realtek8168(rtl8168::Rtl8168Driver),
}

pub struct E1000Driver {
    pub pci: PciDeviceInfo,
    pub mmio_base: usize,
    pub mac_addr: [u8; 6],
    pub rx_cur: usize,
    pub tx_cur: usize,
    backend: NicBackend,
}

pub fn scan_pci_e1000() -> Option<PciDeviceInfo> {
    for ep in enumerate_pci_bus() {
        let is_intel = ep.vendor_id == E1000_VENDOR_ID
            && matches!(
                ep.device_id,
                0x100E | 0x100F | 0x1004 | 0x10D3 | 0x1502 | 0x153A | 0x15B7 | 0x15B8
            );
        let is_realtek = ep.vendor_id == REALTEK_VENDOR_ID
            && matches!(ep.device_id, 0x8168 | 0x8169 | 0x8136 | 0x8125);

        if is_intel || is_realtek {
            let mmio = ep
                .bar(0)
                .mmio_base()
                .or_else(|| ep.bar(2).mmio_base())
                .unwrap_or(0) as u32;
            return Some(PciDeviceInfo {
                bus: ep.loc.bus,
                dev: ep.loc.dev,
                func: ep.loc.func,
                vendor_id: ep.vendor_id,
                device_id: ep.device_id,
                bar0: mmio,
                irq: ep.irq_line,
            });
        }
    }
    None
}

fn find_matching_endpoint(pci: PciDeviceInfo) -> Option<PciEndpoint> {
    enumerate_pci_bus().into_iter().find(|ep| {
        ep.loc.bus == pci.bus && ep.loc.dev == pci.dev && ep.loc.func == pci.func
    })
}

impl E1000Driver {
    pub fn new(pci: PciDeviceInfo) -> Self {
        // 1. Eğer kart Realtek RTL8111/8168 ise native Realtek PCIe sürücüsünü başlat
        if pci.vendor_id == REALTEK_VENDOR_ID {
            if let Some(ep) = find_matching_endpoint(pci) {
                if let Some(rtl) = rtl8168::Rtl8168Driver::try_new(ep) {
                    let mac_addr = rtl.mac_addr;
                    let mmio_base = rtl.mmio_base;
                    return E1000Driver {
                        pci,
                        mmio_base,
                        mac_addr,
                        rx_cur: 0,
                        tx_cur: 0,
                        backend: NicBackend::Realtek8168(rtl),
                    };
                }
            }
        }

        // 2. Intel PRO/1000 (E1000 / 82540EM / 82574L / I217 / I219) DMA kurulumu
        let mut ep_opt = find_matching_endpoint(pci);
        if let Some(ref mut ep) = ep_opt {
            ep.enable_resource_decoding(true, false);
        }

        let mmio_base = pci.bar0 as usize;
        let mut mac_addr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

        if mmio_base != 0 {
            let mac_low = unsafe { core::ptr::read_volatile((mmio_base + 0x5400) as *const u32) };
            let mac_high = unsafe { core::ptr::read_volatile((mmio_base + 0x5404) as *const u32) };
            if mac_low != 0 || mac_high != 0 {
                mac_addr[0] = (mac_low & 0xFF) as u8;
                mac_addr[1] = ((mac_low >> 8) & 0xFF) as u8;
                mac_addr[2] = ((mac_low >> 16) & 0xFF) as u8;
                mac_addr[3] = ((mac_low >> 24) & 0xFF) as u8;
                mac_addr[4] = (mac_high & 0xFF) as u8;
                mac_addr[5] = ((mac_high >> 8) & 0xFF) as u8;
            }
        }

        let mut rx_ring_dma = DmaBuffer::allocate(
            E1000_NUM_RX * core::mem::size_of::<RxDesc>(),
            128,
            DmaMask::Bits64,
        )
        .expect("E1000 RX ring DMA hatasi");
        let mut tx_ring_dma = DmaBuffer::allocate(
            E1000_NUM_TX * core::mem::size_of::<TxDesc>(),
            128,
            DmaMask::Bits64,
        )
        .expect("E1000 TX ring DMA hatasi");

        let mut rx_bufs_dma = Vec::with_capacity(E1000_NUM_RX);
        let mut tx_bufs_dma = Vec::with_capacity(E1000_NUM_TX);

        unsafe {
            let rx_descs = rx_ring_dma
                .as_typed_slice_mut::<RxDesc>(E1000_NUM_RX)
                .unwrap();
            for i in 0..E1000_NUM_RX {
                let buf = DmaBuffer::allocate(2048, 64, DmaMask::Bits64).unwrap();
                rx_descs[i] = RxDesc {
                    addr: buf.phys_addr(),
                    length: 0,
                    checksum: 0,
                    status: 0,
                    errors: 0,
                    special: 0,
                };
                rx_bufs_dma.push(buf);
            }

            let tx_descs = tx_ring_dma
                .as_typed_slice_mut::<TxDesc>(E1000_NUM_TX)
                .unwrap();
            for i in 0..E1000_NUM_TX {
                let buf = DmaBuffer::allocate(2048, 64, DmaMask::Bits64).unwrap();
                tx_descs[i] = TxDesc {
                    addr: buf.phys_addr(),
                    length: 0,
                    cso: 0,
                    cmd: 0,
                    status: 1, // DD (Descriptor Done) = 1
                    css: 0,
                    special: 0,
                };
                tx_bufs_dma.push(buf);
            }
        }

        rx_ring_dma.sync_for_device();
        tx_ring_dma.sync_for_device();

        if mmio_base != 0 {
            unsafe {
                // CTRL (0x0000): SLU (Set Link Up - Bit 6) | ASDE (Bit 5)
                let ctrl = core::ptr::read_volatile(mmio_base as *const u32);
                core::ptr::write_volatile(mmio_base as *mut u32, ctrl | (1 << 6) | (1 << 5));
                // IMC (0x00D8): Kesmeleri maskele
                core::ptr::write_volatile((mmio_base + 0x00D8) as *mut u32, 0xFFFF_FFFF);

                // 64-bit fiziksel RX Ring adresini RDBAL (0x2800) ve RDBAH (0x2804) yazmaçlarına yaz
                let rx_phys = rx_ring_dma.phys_addr();
                core::ptr::write_volatile(
                    (mmio_base + 0x2800) as *mut u32,
                    (rx_phys & 0xFFFF_FFFF) as u32,
                );
                core::ptr::write_volatile(
                    (mmio_base + 0x2804) as *mut u32,
                    (rx_phys >> 32) as u32,
                );
                core::ptr::write_volatile(
                    (mmio_base + 0x2808) as *mut u32,
                    (E1000_NUM_RX * 16) as u32,
                );
                core::ptr::write_volatile((mmio_base + 0x2810) as *mut u32, 0);
                core::ptr::write_volatile(
                    (mmio_base + 0x2818) as *mut u32,
                    (E1000_NUM_RX - 1) as u32,
                );
                core::ptr::write_volatile(
                    (mmio_base + 0x0100) as *mut u32,
                    (1 << 1) | (1 << 15) | (1 << 26),
                );

                // 64-bit fiziksel TX Ring adresini TDBAL (0x3800) ve TDBAH (0x3804) yazmaçlarına yaz
                let tx_phys = tx_ring_dma.phys_addr();
                core::ptr::write_volatile(
                    (mmio_base + 0x3800) as *mut u32,
                    (tx_phys & 0xFFFF_FFFF) as u32,
                );
                core::ptr::write_volatile(
                    (mmio_base + 0x3804) as *mut u32,
                    (tx_phys >> 32) as u32,
                );
                core::ptr::write_volatile(
                    (mmio_base + 0x3808) as *mut u32,
                    (E1000_NUM_TX * 16) as u32,
                );
                core::ptr::write_volatile((mmio_base + 0x3810) as *mut u32, 0);
                core::ptr::write_volatile((mmio_base + 0x3818) as *mut u32, 0);
                core::ptr::write_volatile(
                    (mmio_base + 0x0400) as *mut u32,
                    (1 << 1) | (1 << 3) | (0x0F << 4),
                );
            }
            // DMA adresleri doğrulandıktan sonra Bus Mastering açılır
            if let Some(ref mut ep) = ep_opt {
                ep.enable_bus_mastering();
            }
        }

        E1000Driver {
            pci,
            mmio_base,
            mac_addr,
            rx_cur: 0,
            tx_cur: 0,
            backend: NicBackend::IntelE1000 {
                rx_ring_dma,
                tx_ring_dma,
                rx_bufs_dma,
                tx_bufs_dma,
            },
        }
    }

    /// Gerçek donanım PHY/STATUS yazmacından hat durumunu (Link Up) okur
    pub fn is_link_up(&self) -> bool {
        match &self.backend {
            NicBackend::Realtek8168(rtl) => rtl.is_link_up(),
            NicBackend::IntelE1000 { .. } => {
                if self.mmio_base == 0 {
                    return false;
                }
                // E1000 STATUS Register (0x0008), Bit 1 = LU (Link Up)
                let status =
                    unsafe { core::ptr::read_volatile((self.mmio_base + 0x0008) as *const u32) };
                (status & (1 << 1)) != 0
            }
        }
    }

    pub fn send_packet(&mut self, data: &[u8]) -> Result<(), ()> {
        if data.is_empty() || data.len() > 2048 {
            return Err(());
        }
        match &mut self.backend {
            NicBackend::Realtek8168(rtl) => rtl.send_packet(data),
            NicBackend::IntelE1000 {
                tx_ring_dma,
                tx_bufs_dma,
                ..
            } => {
                let idx = self.tx_cur;
                let tx_descs = tx_ring_dma.virt_ptr() as *mut TxDesc;

                unsafe {
                    tx_ring_dma.sync_for_cpu();
                    let prev_status = core::ptr::read_volatile(&(*tx_descs.add(idx)).status);
                    if (prev_status & 1) == 0 {
                        return Err(());
                    }

                    tx_bufs_dma[idx].as_mut_slice()[..data.len()].copy_from_slice(data);
                    tx_bufs_dma[idx].sync_for_device();

                    let desc_ptr = tx_descs.add(idx);
                    (*desc_ptr).length = data.len() as u16;
                    (*desc_ptr).cmd = (1 << 0) | (1 << 1) | (1 << 3); // EOP | IFCS | RS
                    (*desc_ptr).status = 0;
                    tx_ring_dma.sync_for_device();

                    self.tx_cur = (idx + 1) % E1000_NUM_TX;
                    if self.mmio_base != 0 {
                        core::ptr::write_volatile(
                            (self.mmio_base + 0x3818) as *mut u32,
                            self.tx_cur as u32,
                        );
                    }
                }
                Ok(())
            }
        }
    }

    pub fn poll_rx(&mut self) -> Option<Vec<u8>> {
        match &mut self.backend {
            NicBackend::Realtek8168(rtl) => rtl.poll_rx(),
            NicBackend::IntelE1000 {
                rx_ring_dma,
                rx_bufs_dma,
                ..
            } => {
                let idx = self.rx_cur;
                let rx_descs = rx_ring_dma.virt_ptr() as *mut RxDesc;

                unsafe {
                    rx_ring_dma.sync_for_cpu();
                    let status = core::ptr::read_volatile(&(*rx_descs.add(idx)).status);
                    if (status & 1) != 0 {
                        let len = (core::ptr::read_volatile(&(*rx_descs.add(idx)).length) as usize)
                            .min(2048);
                        rx_bufs_dma[idx].sync_for_cpu();
                        let packet = rx_bufs_dma[idx].as_slice()[..len].to_vec();

                        (*rx_descs.add(idx)).status = 0;
                        rx_ring_dma.sync_for_device();

                        self.rx_cur = (idx + 1) % E1000_NUM_RX;
                        if self.mmio_base != 0 {
                            core::ptr::write_volatile(
                                (self.mmio_base + 0x2818) as *mut u32,
                                idx as u32,
                            );
                        }
                        Some(packet)
                    } else {
                        None
                    }
                }
            }
        }
    }
}

pub struct NetworkStack {
    pub driver: Option<E1000Driver>,
    pub ip_addr: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub gateway_mac: [u8; 6],
    pub dns_ip: [u8; 4],
    pub mac_addr: [u8; 6],
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub link_online: bool,
    pub dhcp: DhcpClient,
    pub inbox: Vec<Vec<u8>>,
    pub dhcp_wait: u32,
}

impl NetworkStack {
    pub fn new() -> Self {
        let mut driver = None;
        let mut mac_addr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        let mut link_online = false;

        if let Some(pci) = scan_pci_e1000() {
            let drv = E1000Driver::new(pci);
            mac_addr = drv.mac_addr;
            driver = Some(drv);
            link_online = true;
        }

        let mut dhcp = DhcpClient::new();
        let initial_tx = 0u64;

        let mut pending_discover: Option<Vec<u8>> = None;
        if link_online {
            pending_discover = Some(dhcp.build_discover(mac_addr));
        }

        let mut stack = NetworkStack {
            driver,
            ip_addr: [0, 0, 0, 0],
            gateway_ip: [0, 0, 0, 0],
            gateway_mac: [0xFF; 6],
            dns_ip: [1, 1, 1, 1],
            mac_addr,
            rx_packets: 0,
            tx_packets: initial_tx,
            link_online,
            dhcp,
            inbox: Vec::new(),
            dhcp_wait: 0,
        };

        if let Some(pkt) = pending_discover {
            if let Some(ref mut drv) = stack.driver {
                let _ = drv.send_packet(&pkt);
                stack.tx_packets += 1;
            }
        }

        stack
    }

    pub fn resolve_gateway_mac(&mut self) {
        if self.gateway_ip == [0, 0, 0, 0] {
            return;
        }
        let mut arp_pkt = [0u8; 42];
        arp_pkt[0..6].copy_from_slice(&[0xFF; 6]);
        arp_pkt[6..12].copy_from_slice(&self.mac_addr);
        arp_pkt[12..14].copy_from_slice(&[0x08, 0x06]);
        arp_pkt[14..16].copy_from_slice(&[0x00, 0x01]);
        arp_pkt[16..18].copy_from_slice(&[0x08, 0x00]);
        arp_pkt[18] = 6;
        arp_pkt[19] = 4;
        arp_pkt[20..22].copy_from_slice(&[0x00, 0x01]);
        arp_pkt[22..28].copy_from_slice(&self.mac_addr);
        arp_pkt[28..32].copy_from_slice(&self.ip_addr);
        arp_pkt[32..38].copy_from_slice(&[0x00; 6]);
        arp_pkt[38..42].copy_from_slice(&self.gateway_ip);

        if let Some(ref mut drv) = self.driver {
            let _ = drv.send_packet(&arp_pkt);
            self.tx_packets += 1;
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ip_addr != [0, 0, 0, 0]
    }

    fn retry_dhcp(&mut self) {
        if !self.link_online || self.dhcp.state == DhcpState::Bound {
            return;
        }
        self.dhcp_wait += 1;
        if self.dhcp_wait >= 300 {
            self.dhcp_wait = 0;
            let pkt = self.dhcp.build_discover(self.mac_addr);
            if let Some(ref mut drv) = self.driver {
                let _ = drv.send_packet(&pkt);
                self.tx_packets += 1;
            }
        }
    }

    pub fn poll(&mut self) {
        wireless::global().poll();
        self.retry_dhcp();
        let rx_opt = if let Some(ref mut drv) = self.driver {
            drv.poll_rx()
        } else {
            None
        };

        let rx = match rx_opt {
            Some(pkt) => pkt,
            None => return,
        };

        self.rx_packets += 1;
        if rx.len() < 14 {
            return;
        }

        let eth_type = u16::from_be_bytes([rx[12], rx[13]]);

        if eth_type == 0x0806 && rx.len() >= 42 {
            let opcode = u16::from_be_bytes([rx[20], rx[21]]);
            if opcode == 1 {
                let target_ip = [rx[38], rx[39], rx[40], rx[41]];
                if self.ip_addr != [0, 0, 0, 0] && target_ip == self.ip_addr {
                    let mut arp_reply = vec![0u8; 42];
                    arp_reply[0..6].copy_from_slice(&rx[6..12]);
                    arp_reply[6..12].copy_from_slice(&self.mac_addr);
                    arp_reply[12..14].copy_from_slice(&[0x08, 0x06]);
                    arp_reply[14..16].copy_from_slice(&[0x00, 0x01]);
                    arp_reply[16..18].copy_from_slice(&[0x08, 0x00]);
                    arp_reply[18] = 6;
                    arp_reply[19] = 4;
                    arp_reply[20..22].copy_from_slice(&[0x00, 0x02]);
                    arp_reply[22..28].copy_from_slice(&self.mac_addr);
                    arp_reply[28..32].copy_from_slice(&self.ip_addr);
                    arp_reply[32..38].copy_from_slice(&rx[22..28]);
                    arp_reply[38..42].copy_from_slice(&rx[28..32]);

                    if let Some(ref mut drv) = self.driver {
                        let _ = drv.send_packet(&arp_reply);
                        self.tx_packets += 1;
                    }
                }
            } else if opcode == 2 {
                let sender_ip = [rx[28], rx[29], rx[30], rx[31]];
                if sender_ip == self.gateway_ip && self.gateway_ip != [0, 0, 0, 0] {
                    self.gateway_mac.copy_from_slice(&rx[22..28]);
                }
            }
            return;
        }

        if eth_type == 0x0800 && rx.len() >= 34 {
            let proto = rx[23];

            if proto == 17 && self.dhcp.state != DhcpState::Bound {
                if let Some(action) = self.dhcp.handle_packet(&rx, self.mac_addr) {
                    match action {
                        DhcpAction::SendRequest(req_pkt) => {
                            if let Some(ref mut drv) = self.driver {
                                let _ = drv.send_packet(&req_pkt);
                                self.tx_packets += 1;
                            }
                        }
                        DhcpAction::Bound { ip, gateway, dns } => {
                            self.ip_addr = ip;
                            if gateway != [0, 0, 0, 0] {
                                self.gateway_ip = gateway;
                            }
                            if dns != [0, 0, 0, 0] {
                                self.dns_ip = dns;
                            }
                            self.resolve_gateway_mac();
                        }
                    }
                    return;
                }
            }

            if proto == 1 && rx.len() >= 42 {
                let icmp_type = rx[34];
                if icmp_type == 8 {
                    let mut ping_reply = rx.clone();
                    ping_reply[0..6].copy_from_slice(&rx[6..12]);
                    ping_reply[6..12].copy_from_slice(&self.mac_addr);
                    ping_reply[26..30].copy_from_slice(&rx[30..34]);
                    ping_reply[30..34].copy_from_slice(&rx[26..30]);
                    ping_reply[34] = 0;
                    ping_reply[36..38].copy_from_slice(&[0x00, 0x00]);
                    if let Some(ref mut drv) = self.driver {
                        let _ = drv.send_packet(&ping_reply);
                        self.tx_packets += 1;
                    }
                }
            }
            if proto == 6 || proto == 17 {
                if self.inbox.len() >= 64 {
                    self.inbox.remove(0);
                }
                self.inbox.push(rx);
            }
        }
    }
}

static mut GLOBAL_STACK: Option<NetworkStack> = None;
static mut STACK_MAGIC: u64 = 0;

pub fn global() -> &'static mut NetworkStack {
    unsafe {
        let slot = core::ptr::addr_of_mut!(GLOBAL_STACK);
        let magic = core::ptr::addr_of_mut!(STACK_MAGIC);
        if core::ptr::read_volatile(magic) != 0x4E45_545F_494E_4954 {
            core::ptr::write(slot, None);
            core::ptr::write(slot, Some(NetworkStack::new()));
            core::ptr::write(magic, 0x4E45_545F_494E_4954);
        }
        (*slot).as_mut().unwrap()
    }
}
