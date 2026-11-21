//! ARMv8-A QEMU VirtIO-net MMIO Driver, Real DHCP Client, and Wi-Fi Adapter Registry
//! (Features 28, 29, 32, 33).
//!
//! - Drives real VirtIO-net MMIO RX/TX queues against QEMU `-netdev user,id=net0`.
//! - Performs real DHCP Discover -> Offer -> Request -> Ack to acquire IPv4/GW/DNS.
//! - Provides honest 802.11 Wi-Fi radio detection & scan infrastructure:
//!   when no 802.11 `WifiAdapter` is registered (e.g., QEMU `virtio-net`),
//!   Wi-Fi state is explicitly `HardwareUnavailable` with zero fabricated SSIDs.

use core::arch::asm;
use core::fmt::Write;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use nc_hal::net::{
    NetError, WifiAdapter, WifiAdapterState, WifiScanEntry, MAX_WIFI_SCAN_RESULTS,
};

const MAGIC: u32 = 0x7472_6976;
const DEVICE_ID_NET: u32 = 1;

const REG_MAGIC: usize = 0x000;
const REG_VERSION: usize = 0x004;
const REG_DEVICE_ID: usize = 0x008;
const REG_DEVICE_FEATURES: usize = 0x010;
const REG_DEVICE_FEATURES_SEL: usize = 0x014;
const REG_DRIVER_FEATURES: usize = 0x020;
const REG_DRIVER_FEATURES_SEL: usize = 0x024;
const REG_GUEST_PAGE_SIZE: usize = 0x028;
const REG_QUEUE_SEL: usize = 0x030;
const REG_QUEUE_NUM_MAX: usize = 0x034;
const REG_QUEUE_NUM: usize = 0x038;
const REG_QUEUE_ALIGN: usize = 0x03C;
const REG_QUEUE_PFN: usize = 0x040;
const REG_QUEUE_READY: usize = 0x044;
const REG_QUEUE_NOTIFY: usize = 0x050;
const REG_INTERRUPT_STATUS: usize = 0x060;
const REG_INTERRUPT_ACK: usize = 0x064;
const REG_STATUS: usize = 0x070;
const REG_QUEUE_DESC_LOW: usize = 0x080;
const REG_QUEUE_DESC_HIGH: usize = 0x084;
const REG_QUEUE_AVAIL_LOW: usize = 0x090;
const REG_QUEUE_AVAIL_HIGH: usize = 0x094;
const REG_QUEUE_USED_LOW: usize = 0x0A0;
const REG_QUEUE_USED_HIGH: usize = 0x0A4;
const REG_CONFIG: usize = 0x100;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

const FEAT_MAC: u32 = 1 << 5;
const FEAT_STATUS: u32 = 1 << 16;

const QUEUE_SIZE: usize = 16;
const RING_BYTES: usize = 8192;
const USED_OFFSET: usize = 4096;
const DESC_F_WRITE: u16 = 2;
const BUF_SIZE: usize = 2048;

#[repr(C, align(4096))]
struct NetQueueMem {
    ring: [u8; RING_BYTES],
}

#[repr(C, align(4096))]
struct PacketPool {
    bufs: [[u8; BUF_SIZE]; QUEUE_SIZE],
}

static mut RX_QUEUE_MEM: NetQueueMem = NetQueueMem { ring: [0; RING_BYTES] };
static mut TX_QUEUE_MEM: NetQueueMem = NetQueueMem { ring: [0; RING_BYTES] };
static mut RX_POOL: PacketPool = PacketPool { bufs: [[0; BUF_SIZE]; QUEUE_SIZE] };
static mut TX_POOL: PacketPool = PacketPool { bufs: [[0; BUF_SIZE]; QUEUE_SIZE] };

#[derive(Clone, Copy, Debug)]
pub struct NetStatus {
    pub present: bool,
    pub link_up: bool,
    pub mac: [u8; 6],
    pub dhcp_bound: bool,
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub lease_secs: u32,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub wifi_radio_present: bool,
    pub wifi_state: WifiAdapterState,
    pub wifi_scan_count: usize,
}

impl NetStatus {
    pub const EMPTY: Self = Self {
        present: false,
        link_up: false,
        mac: [0; 6],
        dhcp_bound: false,
        ip: [0; 4],
        netmask: [0; 4],
        gateway: [0; 4],
        dns: [0; 4],
        lease_secs: 0,
        tx_packets: 0,
        rx_packets: 0,
        wifi_radio_present: false,
        wifi_state: WifiAdapterState::HardwareUnavailable,
        wifi_scan_count: 0,
    };
}

struct VirtioNetState {
    base: usize,
    version: u32,
    hdr_len: usize,
    rx_size: u16,
    tx_size: u16,
    rx_avail: u16,
    rx_last_used: u16,
    tx_avail: u16,
    tx_last_used: u16,
    status: NetStatus,
    wifi_adapter: Option<&'static mut dyn WifiAdapter>,
    wifi_scans: [WifiScanEntry; MAX_WIFI_SCAN_RESULTS],
}

static mut NET: VirtioNetState = VirtioNetState {
    base: 0,
    version: 0,
    hdr_len: 12,
    rx_size: 0,
    tx_size: 0,
    rx_avail: 0,
    rx_last_used: 0,
    tx_avail: 0,
    tx_last_used: 0,
    status: NetStatus::EMPTY,
    wifi_adapter: None,
    wifi_scans: [WifiScanEntry::EMPTY; MAX_WIFI_SCAN_RESULTS],
};

struct Serial;
impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

macro_rules! log {
    ($($arg:tt)*) => {{ let _ = writeln!(Serial, $($arg)*); }};
}

#[inline(always)]
fn barrier() {
    unsafe {
        asm!("dmb sy", options(nostack, preserves_flags));
    }
}

unsafe fn rd(base: usize, reg: usize) -> u32 {
    nc_arch::mmio_read32(base + reg)
}

unsafe fn wr(base: usize, reg: usize, val: u32) {
    nc_arch::mmio_write32(base + reg, val);
}

unsafe fn r16(addr: usize) -> u16 {
    read_volatile(addr as *const u16)
}

unsafe fn r32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

unsafe fn w16(addr: usize, val: u16) {
    write_volatile(addr as *mut u16, val);
}

unsafe fn w32(addr: usize, val: u32) {
    write_volatile(addr as *mut u32, val);
}

unsafe fn w64(addr: usize, val: u64) {
    write_volatile(addr as *mut u64, val);
}

unsafe fn setup_queue(base: usize, version: u32, q_idx: u32, mem: usize) -> Option<u16> {
    wr(base, REG_QUEUE_SEL, q_idx);
    barrier();
    let max = rd(base, REG_QUEUE_NUM_MAX) as usize;
    if max < 4 {
        return None;
    }
    let mut size = QUEUE_SIZE;
    while size > max {
        size >>= 1;
    }
    let avail = mem + size * 16;
    w16(avail, 0);
    w16(avail + 2, 0);
    w16(mem + USED_OFFSET, 0);
    w16(mem + USED_OFFSET + 2, 0);
    barrier();
    wr(base, REG_QUEUE_NUM, size as u32);
    if version == 1 {
        wr(base, REG_QUEUE_ALIGN, 4096);
        wr(base, REG_QUEUE_PFN, (mem >> 12) as u32);
        if rd(base, REG_QUEUE_PFN) == 0 {
            return None;
        }
    } else {
        wr(base, REG_QUEUE_DESC_LOW, mem as u32);
        wr(base, REG_QUEUE_DESC_HIGH, (mem as u64 >> 32) as u32);
        wr(base, REG_QUEUE_AVAIL_LOW, avail as u32);
        wr(base, REG_QUEUE_AVAIL_HIGH, (avail as u64 >> 32) as u32);
        let used = mem + USED_OFFSET;
        wr(base, REG_QUEUE_USED_LOW, used as u32);
        wr(base, REG_QUEUE_USED_HIGH, (used as u64 >> 32) as u32);
        wr(base, REG_QUEUE_READY, 1);
        if rd(base, REG_QUEUE_READY) == 0 {
            return None;
        }
    }
    Some(size as u16)
}

unsafe fn post_rx_desc(s: &mut VirtioNetState, desc_idx: u16) {
    let q = addr_of_mut!(RX_QUEUE_MEM.ring) as usize;
    let buf_addr = addr_of_mut!(RX_POOL.bufs[desc_idx as usize]) as usize as u64;
    let d = q + (desc_idx as usize) * 16;
    w64(d, buf_addr);
    w32(d + 8, BUF_SIZE as u32);
    w16(d + 12, DESC_F_WRITE);
    w16(d + 14, 0);

    let avail = q + (s.rx_size as usize) * 16;
    let slot = (s.rx_avail % s.rx_size) as usize;
    w16(avail + 4 + slot * 2, desc_idx);
    barrier();
    s.rx_avail = s.rx_avail.wrapping_add(1);
    w16(avail + 2, s.rx_avail);
    barrier();
}

/// Initialize the VirtIO-net MMIO device at `addr` and run real DHCP against QEMU SLIRP.
pub fn init_virtio_net(addr: usize) -> bool {
    unsafe {
        if rd(addr, REG_MAGIC) != MAGIC || rd(addr, REG_DEVICE_ID) != DEVICE_ID_NET {
            return false;
        }
        let version = rd(addr, REG_VERSION);
        if version != 1 && version != 2 {
            return false;
        }
        wr(addr, REG_STATUS, 0);
        barrier();
        wr(addr, REG_STATUS, STATUS_ACK);
        wr(addr, REG_STATUS, STATUS_ACK | STATUS_DRIVER);

        wr(addr, REG_DEVICE_FEATURES_SEL, 0);
        let low = rd(addr, REG_DEVICE_FEATURES);
        let accepted = low & (FEAT_MAC | FEAT_STATUS);
        if version == 2 {
            wr(addr, REG_DEVICE_FEATURES_SEL, 1);
            if rd(addr, REG_DEVICE_FEATURES) & 1 == 0 {
                return false;
            }
            wr(addr, REG_DRIVER_FEATURES_SEL, 0);
            wr(addr, REG_DRIVER_FEATURES, accepted);
            wr(addr, REG_DRIVER_FEATURES_SEL, 1);
            wr(addr, REG_DRIVER_FEATURES, 1);
            wr(addr, REG_STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
            if rd(addr, REG_STATUS) & STATUS_FEATURES_OK == 0 {
                return false;
            }
        } else {
            wr(addr, REG_DRIVER_FEATURES_SEL, 0);
            wr(addr, REG_DRIVER_FEATURES, accepted);
            wr(addr, REG_GUEST_PAGE_SIZE, 4096);
        }

        let rx_mem = addr_of_mut!(RX_QUEUE_MEM.ring) as usize;
        let tx_mem = addr_of_mut!(TX_QUEUE_MEM.ring) as usize;
        let Some(rx_size) = setup_queue(addr, version, 0, rx_mem) else {
            return false;
        };
        let Some(tx_size) = setup_queue(addr, version, 1, tx_mem) else {
            return false;
        };

        let s = &mut *addr_of_mut!(NET);
        s.base = addr;
        s.version = version;
        s.hdr_len = if version == 2 { 12 } else { 10 };
        s.rx_size = rx_size;
        s.tx_size = tx_size;
        s.rx_avail = 0;
        s.rx_last_used = 0;
        s.tx_avail = 0;
        s.tx_last_used = 0;
        s.status.present = true;

        for i in 0..6 {
            s.status.mac[i] = nc_arch::mmio_read8(addr + REG_CONFIG + i);
        }
        let link_status = nc_arch::mmio_read8(addr + REG_CONFIG + 6);
        s.status.link_up = (accepted & FEAT_STATUS == 0) || (link_status & 1 != 0);

        for i in 0..rx_size {
            post_rx_desc(s, i);
        }

        let final_status = if version == 2 {
            STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
        } else {
            STATUS_ACK | STATUS_DRIVER | STATUS_DRIVER_OK
        };
        wr(addr, REG_STATUS, final_status);
        barrier();
        wr(addr, REG_QUEUE_NOTIFY, 0);

        log!(
            "[ag] {:#x}: virtio-net v{} MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} (link={})",
            addr,
            version,
            s.status.mac[0],
            s.status.mac[1],
            s.status.mac[2],
            s.status.mac[3],
            s.status.mac[4],
            s.status.mac[5],
            s.status.link_up
        );

        run_dhcp_handshake();
        true
    }
}

pub fn send_frame(frame: &[u8]) -> Result<(), NetError> {
    unsafe {
        let s = &mut *addr_of_mut!(NET);
        if !s.status.present || s.base == 0 {
            return Err(NetError::HardwareUnavailable);
        }
        let total = s.hdr_len + frame.len();
        if total > BUF_SIZE {
            return Err(NetError::BufferTooSmall);
        }
        let idx = (s.tx_avail % s.tx_size) as usize;
        let buf = &mut (*addr_of_mut!(TX_POOL.bufs))[idx];
        for b in &mut buf[..s.hdr_len] {
            *b = 0;
        }
        buf[s.hdr_len..total].copy_from_slice(frame);

        let q = addr_of_mut!(TX_QUEUE_MEM.ring) as usize;
        let d = q + idx * 16;
        w64(d, buf.as_ptr() as u64);
        w32(d + 8, total as u32);
        w16(d + 12, 0);
        w16(d + 14, 0);

        let avail = q + (s.tx_size as usize) * 16;
        w16(avail + 4 + idx * 2, idx as u16);
        barrier();
        s.tx_avail = s.tx_avail.wrapping_add(1);
        w16(avail + 2, s.tx_avail);
        barrier();
        wr(s.base, REG_QUEUE_NOTIFY, 1);

        // Wait briefly for TX completion
        let used = q + USED_OFFSET;
        let mut spins = 0u32;
        while r16(used + 2) == s.tx_last_used && spins < 2_000_000 {
            core::hint::spin_loop();
            spins += 1;
        }
        if r16(used + 2) != s.tx_last_used {
            s.tx_last_used = r16(used + 2);
            let isr = rd(s.base, REG_INTERRUPT_STATUS);
            if isr != 0 {
                wr(s.base, REG_INTERRUPT_ACK, isr);
            }
            s.status.tx_packets += 1;
            Ok(())
        } else {
            Err(NetError::Timeout)
        }
    }
}

pub fn recv_frame(out: &mut [u8]) -> Option<usize> {
    unsafe {
        let s = &mut *addr_of_mut!(NET);
        if !s.status.present || s.base == 0 {
            return None;
        }
        let q = addr_of_mut!(RX_QUEUE_MEM.ring) as usize;
        let used = q + USED_OFFSET;
        barrier();
        let used_idx = r16(used + 2);
        if used_idx == s.rx_last_used {
            return None;
        }
        let elem = used + 4 + ((s.rx_last_used % s.rx_size) as usize) * 8;
        let desc_id = r32(elem) as u16;
        let total_len = r32(elem + 4) as usize;
        s.rx_last_used = s.rx_last_used.wrapping_add(1);

        let isr = rd(s.base, REG_INTERRUPT_STATUS);
        if isr != 0 {
            wr(s.base, REG_INTERRUPT_ACK, isr);
        }

        let mut copied = None;
        if total_len > s.hdr_len && (desc_id as usize) < QUEUE_SIZE {
            let payload_len = total_len - s.hdr_len;
            if payload_len <= out.len() && total_len <= BUF_SIZE {
                let buf = &(*addr_of_mut!(RX_POOL.bufs))[desc_id as usize];
                out[..payload_len].copy_from_slice(&buf[s.hdr_len..total_len]);
                s.status.rx_packets += 1;
                copied = Some(payload_len);
            }
        }
        post_rx_desc(s, desc_id);
        wr(s.base, REG_QUEUE_NOTIFY, 0);
        copied
    }
}

fn ipv4_checksum(hdr: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum += u16::from_be_bytes([hdr[i], hdr[i + 1]]) as u32;
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn build_dhcp_frame(mac: [u8; 6], xid: u32, msg_type: u8, req_ip: [u8; 4], srv_ip: [u8; 4], out: &mut [u8]) -> usize {
    let mut dhcp = [0u8; 300];
    dhcp[0] = 1; // BOOTREQUEST
    dhcp[1] = 1; // Ethernet
    dhcp[2] = 6; // MAC len
    dhcp[4..8].copy_from_slice(&xid.to_be_bytes());
    dhcp[10] = 0x80; // Broadcast
    dhcp[28..34].copy_from_slice(&mac);
    dhcp[236..240].copy_from_slice(&[99, 130, 83, 99]); // Magic cookie

    let mut o = 240;
    dhcp[o] = 53;
    dhcp[o + 1] = 1;
    dhcp[o + 2] = msg_type;
    o += 3;

    if msg_type == 3 {
        dhcp[o] = 50;
        dhcp[o + 1] = 4;
        dhcp[o + 2..o + 6].copy_from_slice(&req_ip);
        o += 6;
        dhcp[o] = 54;
        dhcp[o + 1] = 4;
        dhcp[o + 2..o + 6].copy_from_slice(&srv_ip);
        o += 6;
    }
    // Option 55: Parameter Request List (Subnet Mask, Router, DNS)
    dhcp[o] = 55;
    dhcp[o + 1] = 3;
    dhcp[o + 2] = 1;
    dhcp[o + 3] = 3;
    dhcp[o + 4] = 6;
    o += 5;
    dhcp[o] = 255;
    o += 1;

    let udp_len = (8 + o) as u16;
    let ip_len = (20 + udp_len) as u16;

    // Ethernet header (14 bytes)
    out[0..6].copy_from_slice(&[0xFF; 6]);
    out[6..12].copy_from_slice(&mac);
    out[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

    // IPv4 header (20 bytes)
    let ip = &mut out[14..34];
    ip.fill(0);
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&ip_len.to_be_bytes());
    ip[4..6].copy_from_slice(&0x1234u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17; // UDP
    ip[12..16].copy_from_slice(&[0, 0, 0, 0]);
    ip[16..20].copy_from_slice(&[255, 255, 255, 255]);
    let csum = ipv4_checksum(ip);
    ip[10..12].copy_from_slice(&csum.to_be_bytes());

    // UDP header (8 bytes)
    out[34..36].copy_from_slice(&68u16.to_be_bytes());
    out[36..38].copy_from_slice(&67u16.to_be_bytes());
    out[38..40].copy_from_slice(&udp_len.to_be_bytes());
    out[40..42].copy_from_slice(&0u16.to_be_bytes());

    out[42..42 + o].copy_from_slice(&dhcp[..o]);
    42 + o
}

struct ParsedDhcp {
    msg_type: u8,
    yiaddr: [u8; 4],
    server_id: [u8; 4],
    netmask: [u8; 4],
    gateway: [u8; 4],
    dns: [u8; 4],
    lease_secs: u32,
}

fn parse_dhcp_reply(frame: &[u8], expected_xid: u32) -> Option<ParsedDhcp> {
    if frame.len() < 42 + 240 {
        return None;
    }
    if u16::from_be_bytes([frame[12], frame[13]]) != 0x0800 {
        return None;
    }
    if frame[23] != 17 {
        return None;
    }
    let ihl = ((frame[14] & 0x0F) as usize) * 4;
    if ihl < 20 || frame.len() < 14 + ihl + 8 + 240 {
        return None;
    }
    let udp = 14 + ihl;
    let dst_port = u16::from_be_bytes([frame[udp + 2], frame[udp + 3]]);
    if dst_port != 68 {
        return None;
    }
    let bootp = udp + 8;
    if frame[bootp] != 2 {
        return None; // Not BOOTREPLY
    }
    let xid = u32::from_be_bytes([
        frame[bootp + 4],
        frame[bootp + 5],
        frame[bootp + 6],
        frame[bootp + 7],
    ]);
    if xid != expected_xid {
        return None;
    }
    if frame[bootp + 236..bootp + 240] != [99, 130, 83, 99] {
        return None;
    }
    let mut res = ParsedDhcp {
        msg_type: 0,
        yiaddr: [
            frame[bootp + 16],
            frame[bootp + 17],
            frame[bootp + 18],
            frame[bootp + 19],
        ],
        server_id: [0; 4],
        netmask: [255, 255, 255, 0],
        gateway: [0; 4],
        dns: [0; 4],
        lease_secs: 0,
    };
    let mut i = bootp + 240;
    while i < frame.len() {
        let opt = frame[i];
        if opt == 255 {
            break;
        }
        if opt == 0 {
            i += 1;
            continue;
        }
        if i + 1 >= frame.len() {
            break;
        }
        let len = frame[i + 1] as usize;
        if i + 2 + len > frame.len() {
            break;
        }
        let val = &frame[i + 2..i + 2 + len];
        match opt {
            53 if len >= 1 => res.msg_type = val[0],
            1 if len >= 4 => res.netmask.copy_from_slice(&val[0..4]),
            3 if len >= 4 => res.gateway.copy_from_slice(&val[0..4]),
            6 if len >= 4 => res.dns.copy_from_slice(&val[0..4]),
            51 if len >= 4 => res.lease_secs = u32::from_be_bytes([val[0], val[1], val[2], val[3]]),
            54 if len >= 4 => res.server_id.copy_from_slice(&val[0..4]),
            _ => {}
        }
        i += 2 + len;
    }
    if res.msg_type != 0 { Some(res) } else { None }
}

/// Perform real DHCP Discover -> Offer -> Request -> Ack over `virtio-net`.
pub fn run_dhcp_handshake() -> bool {
    let mac = unsafe { (*addr_of_mut!(NET)).status.mac };
    let xid = (nc_arch::rand::HardwareRng::next_u64() as u32) | 1;
    let mut tx_buf = [0u8; 512];
    let mut rx_buf = [0u8; 1600];

    let len = build_dhcp_frame(mac, xid, 1, [0; 4], [0; 4], &mut tx_buf);
    if send_frame(&tx_buf[..len]).is_err() {
        return false;
    }

    // Wait for DHCP Offer (bounded polling, fast on QEMU SLIRP)
    let mut offer = None;
    for _ in 0..800_000u32 {
        if let Some(n) = recv_frame(&mut rx_buf) {
            if let Some(p) = parse_dhcp_reply(&rx_buf[..n], xid) {
                if p.msg_type == 2 {
                    offer = Some(p);
                    break;
                }
            }
        }
        core::hint::spin_loop();
    }
    let Some(off) = offer else {
        log!("[ag] dhcp: yanit yok (zaman asimi)");
        return false;
    };

    let req_len = build_dhcp_frame(mac, xid, 3, off.yiaddr, off.server_id, &mut tx_buf);
    if send_frame(&tx_buf[..req_len]).is_err() {
        return false;
    }

    for _ in 0..800_000u32 {
        if let Some(n) = recv_frame(&mut rx_buf) {
            if let Some(ack) = parse_dhcp_reply(&rx_buf[..n], xid) {
                if ack.msg_type == 5 {
                    unsafe {
                        let s = &mut *addr_of_mut!(NET);
                        s.status.dhcp_bound = true;
                        s.status.ip = ack.yiaddr;
                        s.status.netmask = ack.netmask;
                        s.status.gateway = ack.gateway;
                        s.status.dns = ack.dns;
                        s.status.lease_secs = ack.lease_secs;
                    }
                    log!(
                        "[ag] dhcp: ip={}.{}.{}.{} gw={}.{}.{}.{} dns={}.{}.{}.{} (lease={}s)",
                        ack.yiaddr[0], ack.yiaddr[1], ack.yiaddr[2], ack.yiaddr[3],
                        ack.gateway[0], ack.gateway[1], ack.gateway[2], ack.gateway[3],
                        ack.dns[0], ack.dns[1], ack.dns[2], ack.dns[3],
                        ack.lease_secs
                    );
                    return true;
                }
            }
        }
        core::hint::spin_loop();
    }
    log!("[ag] dhcp: ACK alinamadi");
    false
}

/// Register a real hardware 802.11 Wi-Fi adapter (e.g., future X6826C CONNSYS driver).
#[allow(dead_code)]
pub fn register_wifi_adapter(adapter: &'static mut dyn WifiAdapter) {
    unsafe {
        let s = &mut *addr_of_mut!(NET);
        s.status.wifi_radio_present = true;
        s.status.wifi_state = adapter.state();
        s.wifi_adapter = Some(adapter);
    }
}

/// Trigger a real Wi-Fi network scan if an 802.11 adapter is registered.
/// On QEMU without an 802.11 radio, returns `Err(NetError::HardwareUnavailable)`.
pub fn wifi_scan() -> Result<&'static [WifiScanEntry], NetError> {
    unsafe {
        let s = &mut *addr_of_mut!(NET);
        let Some(adapter) = s.wifi_adapter.as_mut() else {
            s.status.wifi_radio_present = false;
            s.status.wifi_state = WifiAdapterState::HardwareUnavailable;
            s.status.wifi_scan_count = 0;
            return Err(NetError::HardwareUnavailable);
        };
        let count = adapter.scan(&mut s.wifi_scans)?;
        let clamped = count.min(MAX_WIFI_SCAN_RESULTS);
        s.status.wifi_scan_count = clamped;
        s.status.wifi_state = adapter.state();
        let scans_ptr = core::ptr::addr_of!(NET.wifi_scans).cast::<WifiScanEntry>();
        Ok(core::slice::from_raw_parts(scans_ptr, clamped))
    }
}

pub fn cached_wifi_scans() -> &'static [WifiScanEntry] {
    unsafe {
        let s = &*core::ptr::addr_of!(NET);
        let count = s.status.wifi_scan_count.min(MAX_WIFI_SCAN_RESULTS);
        &s.wifi_scans[..count]
    }
}

pub fn status() -> NetStatus {
    unsafe { (*core::ptr::addr_of!(NET)).status }
}

/// Resolve an IPv4 address to an Ethernet MAC via ARP over `virtio-net`.
#[allow(dead_code)]
pub fn resolve_arp(target_ip: [u8; 4]) -> Option<[u8; 6]> {
    let st = status();
    if !st.present || !st.dhcp_bound {
        return None;
    }
    let mut pkt = [0u8; 42];
    pkt[0..6].copy_from_slice(&[0xFF; 6]);
    pkt[6..12].copy_from_slice(&st.mac);
    pkt[12..14].copy_from_slice(&0x0806u16.to_be_bytes()); // ARP
    pkt[14..16].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
    pkt[16..18].copy_from_slice(&0x0800u16.to_be_bytes()); // IPv4
    pkt[18] = 6;
    pkt[19] = 4;
    pkt[20..22].copy_from_slice(&1u16.to_be_bytes()); // Request
    pkt[22..28].copy_from_slice(&st.mac);
    pkt[28..32].copy_from_slice(&st.ip);
    pkt[38..42].copy_from_slice(&target_ip);
    if send_frame(&pkt).is_err() {
        return None;
    }
    let mut rx = [0u8; 256];
    for _ in 0..400_000u32 {
        if let Some(n) = recv_frame(&mut rx) {
            if n >= 42
                && u16::from_be_bytes([rx[12], rx[13]]) == 0x0806
                && u16::from_be_bytes([rx[20], rx[21]]) == 2
                && rx[28..32] == target_ip
            {
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&rx[22..28]);
                return Some(mac);
            }
        }
        core::hint::spin_loop();
    }
    None
}

fn skip_dns_name(msg: &[u8], mut offset: usize) -> Option<usize> {
    let mut jumps = 0usize;
    while offset < msg.len() {
        let len = msg[offset];
        if len == 0 {
            return Some(offset + 1);
        }
        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= msg.len() || jumps >= 16 {
                return None;
            }
            return Some(offset + 2);
        }
        offset = offset.checked_add(1 + (len as usize))?;
        jumps += 1;
        if jumps > 64 {
            return None;
        }
    }
    None
}

/// Perform a real RFC 1035 UDP DNS A-record lookup using the DHCP-acquired DNS server.
#[allow(dead_code)]
pub fn resolve_dns_a(hostname: &str) -> Option<[u8; 4]> {
    let st = status();
    if !st.present || !st.dhcp_bound || st.dns == [0; 4] || hostname.is_empty() || hostname.len() > 200 {
        return None;
    }
    let dst_mac = resolve_arp(st.dns).or_else(|| resolve_arp(st.gateway))?;
    let xid = (nc_arch::rand::HardwareRng::next_u64() as u16) | 1;

    let mut dns = [0u8; 300];
    dns[0..2].copy_from_slice(&xid.to_be_bytes());
    dns[2..4].copy_from_slice(&0x0100u16.to_be_bytes()); // Standard query, RD=1
    dns[4..6].copy_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    let mut pos = 12usize;
    for label in hostname.split('.') {
        let b = label.as_bytes();
        if b.is_empty() || b.len() > 63 || pos + 1 + b.len() + 5 > dns.len() {
            return None;
        }
        dns[pos] = b.len() as u8;
        pos += 1;
        dns[pos..pos + b.len()].copy_from_slice(b);
        pos += b.len();
    }
    dns[pos] = 0;
    pos += 1;
    dns[pos..pos + 2].copy_from_slice(&1u16.to_be_bytes()); // QTYPE = A
    dns[pos + 2..pos + 4].copy_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
    pos += 4;

    let udp_len = (8 + pos) as u16;
    let ip_len = (20 + udp_len) as u16;
    let mut tx = [0u8; 512];
    tx[0..6].copy_from_slice(&dst_mac);
    tx[6..12].copy_from_slice(&st.mac);
    tx[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    let ip = &mut tx[14..34];
    ip.fill(0);
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&ip_len.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17;
    ip[12..16].copy_from_slice(&st.ip);
    ip[16..20].copy_from_slice(&st.dns);
    let csum = ipv4_checksum(ip);
    ip[10..12].copy_from_slice(&csum.to_be_bytes());
    let src_port: u16 = 49152 + (xid & 0x3FFF);
    tx[34..36].copy_from_slice(&src_port.to_be_bytes());
    tx[36..38].copy_from_slice(&53u16.to_be_bytes());
    tx[38..40].copy_from_slice(&udp_len.to_be_bytes());
    tx[40..42].copy_from_slice(&0u16.to_be_bytes());
    tx[42..42 + pos].copy_from_slice(&dns[..pos]);

    if send_frame(&tx[..42 + pos]).is_err() {
        return None;
    }

    let mut rx = [0u8; 1600];
    for _ in 0..800_000u32 {
        if let Some(n) = recv_frame(&mut rx) {
            if n < 54 || u16::from_be_bytes([rx[12], rx[13]]) != 0x0800 || rx[23] != 17 {
                continue;
            }
            let ihl = ((rx[14] & 0x0F) as usize) * 4;
            if ihl < 20 || n < 14 + ihl + 8 + 12 {
                continue;
            }
            let udp = 14 + ihl;
            if u16::from_be_bytes([rx[udp + 2], rx[udp + 3]]) != src_port {
                continue;
            }
            let msg = &rx[udp + 8..n];
            if u16::from_be_bytes([msg[0], msg[1]]) != xid {
                continue;
            }
            let qd = u16::from_be_bytes([msg[4], msg[5]]);
            let an = u16::from_be_bytes([msg[6], msg[7]]);
            let mut off = 12usize;
            for _ in 0..qd {
                off = skip_dns_name(msg, off)?;
                off = off.checked_add(4)?;
            }
            for _ in 0..an {
                off = skip_dns_name(msg, off)?;
                if off + 10 > msg.len() {
                    return None;
                }
                let rtype = u16::from_be_bytes([msg[off], msg[off + 1]]);
                let rdlen = u16::from_be_bytes([msg[off + 8], msg[off + 9]]) as usize;
                off += 10;
                if off + rdlen > msg.len() {
                    return None;
                }
                if rtype == 1 && rdlen == 4 {
                    return Some([msg[off], msg[off + 1], msg[off + 2], msg[off + 3]]);
                }
                off += rdlen;
            }
        }
        core::hint::spin_loop();
    }
    None
}
