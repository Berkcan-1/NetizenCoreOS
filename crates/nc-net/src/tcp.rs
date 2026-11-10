extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait,
}

pub struct TcpSocket {
    pub id: usize,
    pub src_port: u16,
    pub dst_port: u16,
    pub dst_ip: [u8; 4],
    pub seq_num: u32,
    pub ack_num: u32,
    pub state: TcpState,
    pub rx_buffer: Vec<u8>,
}

impl TcpSocket {
    pub fn new(id: usize, dst_ip: [u8; 4], dst_port: u16, src_port: u16) -> Self {
        TcpSocket {
            id,
            src_port,
            dst_port,
            dst_ip,
            seq_num: 1000,
            ack_num: 0,
            state: TcpState::Closed,
            rx_buffer: Vec::new(),
        }
    }
}

pub fn build_ipv4_packet(
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    protocol: u8,
    payload: &[u8],
) -> Vec<u8> {
    let header_len = 20usize;
    let total_len = (header_len + payload.len()) as u16;
    let mut pkt = Vec::with_capacity(header_len + payload.len());

    pkt.push(0x45);
    pkt.push(0x00);
    pkt.extend_from_slice(&total_len.to_be_bytes());
    pkt.extend_from_slice(&0x1234u16.to_be_bytes());
    pkt.extend_from_slice(&0x4000u16.to_be_bytes());
    pkt.push(64);
    pkt.push(protocol);
    pkt.extend_from_slice(&[0, 0]);
    pkt.extend_from_slice(&src_ip);
    pkt.extend_from_slice(&dst_ip);

    let csum = compute_checksum(&pkt);
    pkt[10] = (csum >> 8) as u8;
    pkt[11] = (csum & 0xFF) as u8;

    pkt.extend_from_slice(payload);
    pkt
}

pub fn build_ethernet_frame(
    src_mac: [u8; 6],
    dst_mac: [u8; 6],
    ethertype: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ethertype.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

pub fn user_agent_for_host(_host: &str) -> &'static str {
    // Google now refuses text-browser identities for search ("enable JavaScript"),
    // so every site gets the same modern identity.
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 NetizenCore/1.0"
}

fn clean_header(s: &str) -> String {
    s.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

pub fn build_http_request(method: &str, host: &str, path: &str, headers: &[(String, String)], body: &[u8], cookie: &str) -> Vec<u8> {
    let has = |n: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    let mut req: Vec<u8> = Vec::new();
    req.extend_from_slice(method.as_bytes());
    req.push(b' ');
    req.extend_from_slice(clean_header(path).as_bytes());
    req.extend_from_slice(b" HTTP/1.1\r\nHost: ");
    req.extend_from_slice(host.as_bytes());
    req.extend_from_slice(b"\r\nUser-Agent: ");
    req.extend_from_slice(user_agent_for_host(host).as_bytes());
    if !has("accept") {
        req.extend_from_slice(b"\r\nAccept: text/html,application/xhtml+xml,text/css,application/json,image/png,image/jpeg,image/bmp,image/svg+xml;q=0.9,*/*;q=0.5");
    }
    if !has("accept-language") {
        req.extend_from_slice(b"\r\nAccept-Language: tr-TR,tr;q=0.9,en;q=0.7");
    }
    req.extend_from_slice(b"\r\nAccept-Encoding: identity\r\nConnection: close");
    if !cookie.is_empty() {
        req.extend_from_slice(b"\r\nCookie: ");
        req.extend_from_slice(clean_header(cookie).as_bytes());
    }
    for (k, v) in headers.iter() {
        let kl = k.to_ascii_lowercase();
        if matches!(kl.as_str(), "host" | "user-agent" | "content-length" | "connection" | "accept-encoding" | "cookie" | "transfer-encoding") {
            continue;
        }
        req.extend_from_slice(b"\r\n");
        req.extend_from_slice(clean_header(k).as_bytes());
        req.extend_from_slice(b": ");
        req.extend_from_slice(clean_header(v).as_bytes());
    }
    if !body.is_empty() || matches!(method, "POST" | "PUT" | "PATCH") {
        req.extend_from_slice(b"\r\nContent-Length: ");
        req.extend_from_slice(alloc::format!("{}", body.len()).as_bytes());
    }
    req.extend_from_slice(b"\r\n\r\n");
    req.extend_from_slice(body);
    req
}

pub fn build_http_get(host: &str, path: &str) -> Vec<u8> {
    build_http_request("GET", host, path, &[], &[], "")
}

pub fn build_tcp_segment(
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let tcp_header_len = 20usize;
    let total_tcp_len = tcp_header_len + payload.len();
    let mut tcp_pkt = Vec::with_capacity(total_tcp_len);

    tcp_pkt.extend_from_slice(&src_port.to_be_bytes());
    tcp_pkt.extend_from_slice(&dst_port.to_be_bytes());
    tcp_pkt.extend_from_slice(&seq.to_be_bytes());
    tcp_pkt.extend_from_slice(&ack.to_be_bytes());
    tcp_pkt.push(0x50);
    tcp_pkt.push(flags);
    tcp_pkt.extend_from_slice(&32768u16.to_be_bytes());
    tcp_pkt.extend_from_slice(&[0, 0]);
    tcp_pkt.extend_from_slice(&[0, 0]);
    tcp_pkt.extend_from_slice(payload);

    let mut pseudo = Vec::new();
    pseudo.extend_from_slice(&src_ip);
    pseudo.extend_from_slice(&dst_ip);
    pseudo.push(0);
    pseudo.push(6);
    pseudo.extend_from_slice(&(total_tcp_len as u16).to_be_bytes());
    pseudo.extend_from_slice(&tcp_pkt);

    let csum = compute_checksum(&pseudo);
    tcp_pkt[16] = (csum >> 8) as u8;
    tcp_pkt[17] = (csum & 0xFF) as u8;

    tcp_pkt
}

fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < data.len() {
        let word = (data[i] as u32) << 8;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}