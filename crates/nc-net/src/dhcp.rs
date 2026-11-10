extern crate alloc;
use alloc::vec::Vec;
use crate::tcp::{build_ethernet_frame, build_ipv4_packet};
use crate::build_udp_packet;

pub const DHCP_DISCOVER: u8 = 1;
pub const DHCP_OFFER: u8 = 2;
pub const DHCP_REQUEST: u8 = 3;
pub const DHCP_ACK: u8 = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DhcpState {
    Init,
    DiscoverSent,
    RequestSent,
    Bound,
}

pub enum DhcpAction {
    SendRequest(Vec<u8>),
    Bound {
        ip: [u8; 4],
        gateway: [u8; 4],
        dns: [u8; 4],
    },
}

pub struct DhcpClient {
    pub state: DhcpState,
    pub xid: u32,
    pub offered_ip: [u8; 4],
    pub server_ip: [u8; 4],
}

impl DhcpClient {
    pub fn new() -> Self {
        DhcpClient {
            state: DhcpState::Init,
            xid: 0x3903_F326,
            offered_ip: [0, 0, 0, 0],
            server_ip: [0, 0, 0, 0],
        }
    }

    pub fn build_discover(&mut self, mac: [u8; 6]) -> Vec<u8> {
        self.state = DhcpState::DiscoverSent;
        self.create_dhcp_frame(mac, DHCP_DISCOVER, [0, 0, 0, 0])
    }

    fn create_dhcp_frame(&self, mac: [u8; 6], msg_type: u8, requested_ip: [u8; 4]) -> Vec<u8> {
        let mut dhcp_payload = [0u8; 300];
        dhcp_payload[0] = 1;  // Boot Request
        dhcp_payload[1] = 1;  // Ethernet (10Mb)
        dhcp_payload[2] = 6;  // MAC len
        dhcp_payload[4..8].copy_from_slice(&self.xid.to_be_bytes());
        dhcp_payload[10] = 0x80; // Broadcast flag
        dhcp_payload[28..34].copy_from_slice(&mac);

        // Magic Cookie (RFC 1497)
        dhcp_payload[236..240].copy_from_slice(&[99, 130, 83, 99]);

        // Option 53: DHCP Message Type
        let mut opt_idx = 240;
        dhcp_payload[opt_idx] = 53;
        dhcp_payload[opt_idx + 1] = 1;
        dhcp_payload[opt_idx + 2] = msg_type;
        opt_idx += 3;

        if msg_type == DHCP_REQUEST {
            // Option 50: Requested IP
            dhcp_payload[opt_idx] = 50;
            dhcp_payload[opt_idx + 1] = 4;
            dhcp_payload[opt_idx + 2..opt_idx + 6].copy_from_slice(&requested_ip);
            opt_idx += 6;

            // Option 54: Server Identifier
            dhcp_payload[opt_idx] = 54;
            dhcp_payload[opt_idx + 1] = 4;
            dhcp_payload[opt_idx + 2..opt_idx + 6].copy_from_slice(&self.server_ip);
            opt_idx += 6;
        }

        // Option 255: End
        dhcp_payload[opt_idx] = 255;

        let udp_pkt = build_udp_packet([0, 0, 0, 0], [255, 255, 255, 255], 68, 67, &dhcp_payload[..opt_idx + 1]);
        let ip_pkt = build_ipv4_packet([0, 0, 0, 0], [255, 255, 255, 255], 17, &udp_pkt);
        build_ethernet_frame(mac, [0xFF; 6], 0x0800, &ip_pkt)
    }

    pub fn handle_packet(&mut self, pkt: &[u8], mac: [u8; 6]) -> Option<DhcpAction> {
        if pkt.len() < 282 {
            return None;
        }
        // UDP Port 68 (BootP/DHCP Client) kontrolü
        let dst_port = u16::from_be_bytes([pkt[36], pkt[37]]);
        if dst_port != 68 {
            return None;
        }

        let dhcp_data = &pkt[42..];
        let xid = u32::from_be_bytes([dhcp_data[4], dhcp_data[5], dhcp_data[6], dhcp_data[7]]);
        if xid != self.xid {
            return None;
        }

        let your_ip = [dhcp_data[16], dhcp_data[17], dhcp_data[18], dhcp_data[19]];
        let options = &dhcp_data[240..];

        let mut msg_type = 0u8;
        let mut server_id = [0u8; 4];
        let mut router = [0u8; 4];
        let mut dns = [0u8; 4];

        let mut idx = 0;
        while idx < options.len() {
            let code = options[idx];
            if code == 255 { break; }
            if code == 0 { idx += 1; continue; }
            if idx + 1 >= options.len() { break; }
            let len = options[idx + 1] as usize;
            if idx + 2 + len > options.len() { break; }
            let val = &options[idx + 2..idx + 2 + len];

            match code {
                53 => msg_type = val[0],
                54 if len >= 4 => server_id.copy_from_slice(&val[..4]),
                3 if len >= 4 => router.copy_from_slice(&val[..4]),
                6 if len >= 4 => dns.copy_from_slice(&val[..4]),
                _ => {}
            }
            idx += 2 + len;
        }

        match (self.state, msg_type) {
            (DhcpState::DiscoverSent, DHCP_OFFER) => {
                self.offered_ip = your_ip;
                self.server_ip = server_id;
                self.state = DhcpState::RequestSent;
                let req_frame = self.create_dhcp_frame(mac, DHCP_REQUEST, your_ip);
                Some(DhcpAction::SendRequest(req_frame))
            }
            (DhcpState::RequestSent, DHCP_ACK) => {
                self.state = DhcpState::Bound;
                Some(DhcpAction::Bound {
                    ip: your_ip,
                    gateway: router,
                    dns,
                })
            }
            _ => None,
        }
    }
}
