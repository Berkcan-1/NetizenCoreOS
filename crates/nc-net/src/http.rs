extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_arch::rand::HardwareRng;

use crate::tcp::{build_ethernet_frame, build_ipv4_packet, build_tcp_segment, TCP_ACK, TCP_FIN, TCP_PSH, TCP_RST, TCP_SYN};
use crate::tls13::Tls13;
use crate::NetworkStack;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HttpState {
    Idle,
    WaitingNetwork,
    ResolvingDns,
    ConnectingSyn,
    Handshaking,
    Connected,
    RequestSent,
    ReceivingData,
    Complete,
    Failed,
}

pub struct TcpHttpClient {
    pub state: HttpState,
    pub host: String,
    pub path: String,
    pub target_ip: [u8; 4],
    pub local_port: u16,
    pub remote_port: u16,
    pub local_seq: u32,
    pub remote_seq: u32,
    pub response_data: Vec<u8>,
    pub raw_body: Vec<u8>,
    pub html_body: String,
    pub status_code: u16,
    pub https: bool,
    dns_tx_id: u16,
    dns_cache: Vec<(String, [u8; 4])>,
    tls: Option<Tls13>,
    request_sent: bool,
    ticks: u32,
    retries: u8,
    redirects: u8,
    port_counter: u16,
    pending: Option<String>,
    poll_calls: u32,
    /// Timeouts and retries advance once every `tick_every` polls.
    /// Set this high when polling in a tight loop so timers still mean roughly real time.
    pub tick_every: u32,
}

impl TcpHttpClient {
    pub fn new() -> Self {
        TcpHttpClient {
            state: HttpState::Idle,
            host: String::new(),
            path: String::new(),
            target_ip: [0, 0, 0, 0],
            local_port: 49152,
            remote_port: 443,
            local_seq: 1000,
            remote_seq: 0,
            response_data: Vec::new(),
            raw_body: Vec::new(),
            html_body: String::new(),
            status_code: 0,
            https: true,
            dns_tx_id: 0x1A2B,
            dns_cache: Vec::new(),
            tls: None,
            request_sent: false,
            ticks: 0,
            retries: 0,
            redirects: 0,
            port_counter: 0,
            pending: None,
            poll_calls: 0,
            tick_every: 1,
        }
    }

    pub fn navigate(&mut self, url: &str, net: &mut NetworkStack) {
        self.redirects = 0;
        self.start(url, net);
    }

    fn start(&mut self, url: &str, net: &mut NetworkStack) {
        let clean = url.trim();
        let lower_url = clean.to_ascii_lowercase();
        let explicit_http = lower_url.starts_with("http://");
        let explicit_https = lower_url.starts_with("https://");

        let without_proto = if explicit_https {
            &clean[8..]
        } else if explicit_http {
            &clean[7..]
        } else {
            clean
        };

        let (authority, path) = if let Some(slash_idx) = without_proto.find('/') {
            (&without_proto[..slash_idx], &without_proto[slash_idx..])
        } else {
            (without_proto, "/")
        };

        let (host_raw, custom_port) = if let Some(colon) = authority.rfind(':') {
            let p_str = &authority[colon + 1..];
            if let Ok(p) = p_str.parse::<u16>() {
                (&authority[..colon], Some(p))
            } else {
                (authority, None)
            }
        } else {
            (authority, None)
        };

        let host_lower = host_raw.trim().to_ascii_lowercase();
        let is_ip = parse_ipv4(&host_lower).is_some();
        let is_https = if explicit_http {
            false
        } else if explicit_https {
            true
        } else {
            !is_ip
        };

        self.host = host_lower;
        self.path = if path.is_empty() { String::from("/") } else { String::from(path) };
        self.https = is_https;
        self.remote_port = custom_port.unwrap_or(if is_https { 443 } else { 80 });
        self.response_data.clear();
        self.raw_body.clear();
        self.html_body.clear();
        self.tls = None;
        self.request_sent = false;
        self.ticks = 0;
        self.retries = 0;
        self.status_code = 0;
        self.port_counter = self.port_counter.wrapping_add(1);
        self.dns_tx_id = self.dns_tx_id.wrapping_add(1);
        self.local_port = 49152 + (self.port_counter % 12000);
        self.local_seq = HardwareRng::next_u64() as u32;
        self.remote_seq = 0;
        net.inbox.clear();

        if !net.is_ready() {
            self.pending = Some(String::from(url));
            self.state = HttpState::WaitingNetwork;
            return;
        }

        self.begin(net);
    }

    fn begin(&mut self, net: &mut NetworkStack) {
        self.ticks = 0;
        self.retries = 0;
        if let Some(ip) = parse_ipv4(&self.host) {
            self.target_ip = ip;
            self.state = HttpState::ConnectingSyn;
            self.send_syn(net);
        } else if let Some((_, cached_ip)) = self.dns_cache.iter().find(|(h, _)| h == &self.host) {
            self.target_ip = *cached_ip;
            self.state = HttpState::ConnectingSyn;
            self.send_syn(net);
        } else {
            self.state = HttpState::ResolvingDns;
            self.send_dns_query(net);
        }
    }

    fn destination_mac(&self, net: &NetworkStack) -> [u8; 6] {
        if net.gateway_mac != [0xFF; 6] && net.gateway_mac != [0; 6] {
            net.gateway_mac
        } else {
            [0x52, 0x55, 0x0A, 0x00, 0x02, 0x02]
        }
    }

    fn send_frame(&mut self, net: &mut NetworkStack, dst_ip: [u8; 4], proto: u8, payload: &[u8]) {
        let ip_pkt = build_ipv4_packet(net.ip_addr, dst_ip, proto, payload);
        let frame = build_ethernet_frame(net.mac_addr, self.destination_mac(net), 0x0800, &ip_pkt);
        if let Some(ref mut drv) = net.driver {
            let _ = drv.send_packet(&frame);
        }
    }

    fn send_dns_query(&mut self, net: &mut NetworkStack) {
        let query = crate::dns::encode_dns_query(&self.host, self.dns_tx_id);
        let udp_pkt = crate::build_udp_packet(net.ip_addr, net.dns_ip, 5353, 53, &query);
        let dns_ip = net.dns_ip;
        self.send_frame(net, dns_ip, 17, &udp_pkt);
    }

    fn send_syn(&mut self, net: &mut NetworkStack) {
        let seg = build_tcp_segment(net.ip_addr, self.target_ip, self.local_port, self.remote_port, self.local_seq, 0, TCP_SYN, &[]);
        let target = self.target_ip;
        self.send_frame(net, target, 6, &seg);
    }

    fn send_ack(&mut self, net: &mut NetworkStack, flags: u8, payload: &[u8]) {
        let seg = build_tcp_segment(net.ip_addr, self.target_ip, self.local_port, self.remote_port, self.local_seq, self.remote_seq, flags, payload);
        let target = self.target_ip;
        self.send_frame(net, target, 6, &seg);
    }

    fn send_data(&mut self, net: &mut NetworkStack, data: &[u8]) {
        let mut offset = 0;
        while offset < data.len() {
            let end = (offset + 1400).min(data.len());
            let chunk = &data[offset..end];
            self.send_ack(net, TCP_ACK | TCP_PSH, chunk);
            self.local_seq = self.local_seq.wrapping_add(chunk.len() as u32);
            offset = end;
        }
    }

    pub fn poll(&mut self, net: &mut NetworkStack) {
        for _ in 0..64 {
            net.poll();
        }
        if matches!(self.state, HttpState::Idle | HttpState::Complete | HttpState::Failed) {
            net.inbox.clear();
            return;
        }
        while !net.inbox.is_empty() {
            let pkt = net.inbox.remove(0);
            let old_port = self.local_port;
            self.process_packet(&pkt, net);
            if matches!(self.state, HttpState::Complete | HttpState::Failed) || self.local_port != old_port {
                break;
            }
        }
        if !matches!(self.state, HttpState::Complete | HttpState::Failed) {
            self.poll_calls = self.poll_calls.wrapping_add(1);
            if self.tick_every <= 1 || self.poll_calls % self.tick_every == 0 {
                self.tick(net);
            }
        }
    }

    fn tick(&mut self, net: &mut NetworkStack) {
        self.ticks += 1;
        match self.state {
            HttpState::WaitingNetwork => {
                if net.is_ready() {
                    if let Some(url) = self.pending.take() {
                        self.start(&url, net);
                    }
                } else if self.ticks > 1800 {
                    self.state = HttpState::Failed;
                }
            }
            HttpState::ResolvingDns | HttpState::ConnectingSyn => {
                if self.ticks % 60 == 0 {
                    self.retries += 1;
                    if self.retries > 5 {
                        self.state = HttpState::Failed;
                    } else {
                        if net.gateway_mac == [0xFF; 6] {
                            net.resolve_gateway_mac();
                        }
                        if self.state == HttpState::ResolvingDns {
                            self.send_dns_query(net);
                        } else {
                            self.send_syn(net);
                        }
                    }
                }
            }
            HttpState::ReceivingData => {
                if is_http_response_complete(&self.response_data) || self.ticks > 120 {
                    self.finish(net);
                }
            }
            _ => {
                if self.ticks > 420 {
                    if self.response_data.is_empty() {
                        self.state = HttpState::Failed;
                    } else {
                        self.finish(net);
                    }
                }
            }
        }
    }

    fn on_established(&mut self, net: &mut NetworkStack) {
        if self.https {
            let mut tls = Tls13::new(&self.host);
            let hello = tls.client_hello();
            self.tls = Some(tls);
            self.state = HttpState::Handshaking;
            self.send_data(net, &hello);
        } else {
            let req = crate::tcp::build_http_get(&self.host, &self.path);
            self.send_data(net, &req);
            self.request_sent = true;
            self.state = HttpState::RequestSent;
        }
    }

    fn on_payload(&mut self, payload: &[u8], net: &mut NetworkStack) {
        const MAX_RESPONSE_BYTES: usize = 3 * 1024 * 1024;
        if !self.https {
            self.state = HttpState::ReceivingData;
            let space = MAX_RESPONSE_BYTES.saturating_sub(self.response_data.len());
            self.response_data.extend_from_slice(&payload[..payload.len().min(space)]);
            if is_http_response_complete(&self.response_data) || self.response_data.len() >= MAX_RESPONSE_BYTES {
                self.finish(net);
            }
            return;
        }

        let mut out = Vec::new();
        let (ready, failed, closed, app) = match self.tls.as_mut() {
            Some(tls) => {
                tls.feed(payload, &mut out);
                (tls.is_ready(), tls.is_failed(), tls.closed, core::mem::take(&mut tls.app_in))
            }
            None => return,
        };
        if !out.is_empty() {
            self.send_data(net, &out);
        }
        if failed {
            if !self.response_data.is_empty() {
                self.finish(net);
            } else {
                self.state = HttpState::Failed;
            }
            return;
        }
        if ready && !self.request_sent {
            let req = crate::tcp::build_http_get(&self.host, &self.path);
            let record = match self.tls.as_mut() {
                Some(tls) => tls.encrypt_app(&req),
                None => Vec::new(),
            };
            self.send_data(net, &record);
            self.request_sent = true;
            self.state = HttpState::RequestSent;
        }
        if !app.is_empty() {
            self.state = HttpState::ReceivingData;
            let space = MAX_RESPONSE_BYTES.saturating_sub(self.response_data.len());
            self.response_data.extend_from_slice(&app[..app.len().min(space)]);
        }
        if closed || is_http_response_complete(&self.response_data) || self.response_data.len() >= MAX_RESPONSE_BYTES {
            self.send_ack(net, TCP_ACK | TCP_FIN, &[]);
            self.local_seq = self.local_seq.wrapping_add(1);
            self.finish(net);
        }
    }

    fn process_packet(&mut self, pkt: &[u8], net: &mut NetworkStack) {
        if pkt.len() < 34 || u16::from_be_bytes([pkt[12], pkt[13]]) != 0x0800 {
            return;
        }
        let ip_len = u16::from_be_bytes([pkt[16], pkt[17]]) as usize;
        let end = (14 + ip_len).min(pkt.len());
        if end < 34 {
            return;
        }
        let proto = pkt[23];
        if proto == 17 {
            if self.state != HttpState::ResolvingDns || end < 42 {
                return;
            }
            if u16::from_be_bytes([pkt[36], pkt[37]]) != 5353 {
                return;
            }
            let dns_payload = &pkt[42..end];
            if dns_payload.len() >= 2 && u16::from_be_bytes([dns_payload[0], dns_payload[1]]) == self.dns_tx_id {
                if let Some(ip) = crate::dns::parse_dns_response(dns_payload) {
                    if self.dns_cache.len() >= 16 {
                        self.dns_cache.remove(0);
                    }
                    self.dns_cache.push((self.host.clone(), ip));
                    self.target_ip = ip;
                    self.state = HttpState::ConnectingSyn;
                    self.ticks = 0;
                    self.retries = 0;
                    self.send_syn(net);
                }
            }
            return;
        }

        if proto != 6 || end < 54 {
            return;
        }
        let src_port = u16::from_be_bytes([pkt[34], pkt[35]]);
        let dst_port = u16::from_be_bytes([pkt[36], pkt[37]]);
        if src_port != self.remote_port || dst_port != self.local_port {
            return;
        }

        let seq_in = u32::from_be_bytes([pkt[38], pkt[39], pkt[40], pkt[41]]);
        let flags = pkt[47];
        let payload_start = 34 + ((pkt[46] >> 4) as usize) * 4;
        let payload: &[u8] = if end > payload_start { &pkt[payload_start..end] } else { &[] };

        if flags & TCP_RST != 0 {
            if !self.response_data.is_empty() {
                self.finish(net);
            } else {
                self.state = HttpState::Failed;
            }
            return;
        }

        if self.state == HttpState::ConnectingSyn {
            if flags & (TCP_SYN | TCP_ACK) == (TCP_SYN | TCP_ACK) {
                self.remote_seq = seq_in.wrapping_add(1);
                self.local_seq = self.local_seq.wrapping_add(1);
                self.ticks = 0;
                self.retries = 0;
                self.on_established(net);
            }
            return;
        }

        if matches!(
            self.state,
            HttpState::Idle | HttpState::Complete | HttpState::Failed | HttpState::ResolvingDns | HttpState::WaitingNetwork
        ) {
            return;
        }

        let initial_port = self.local_port;
        if !payload.is_empty() {
            if seq_in != self.remote_seq {
                self.send_ack(net, TCP_ACK, &[]);
                return;
            }
            self.remote_seq = seq_in.wrapping_add(payload.len() as u32);
            self.ticks = 0;
            self.send_ack(net, TCP_ACK, &[]);
            self.on_payload(payload, net);
            if matches!(self.state, HttpState::Complete | HttpState::Failed) || self.local_port != initial_port {
                return;
            }
        }

        if flags & TCP_FIN != 0 && seq_in.wrapping_add(payload.len() as u32) == self.remote_seq {
            self.remote_seq = self.remote_seq.wrapping_add(1);
            self.send_ack(net, TCP_ACK | TCP_FIN, &[]);
            self.local_seq = self.local_seq.wrapping_add(1);
            self.finish(net);
        }
    }

    fn resolve_location(&self, location: &str) -> String {
        let scheme = if self.https { "https://" } else { "http://" };
        if location.starts_with("http://") || location.starts_with("https://") {
            String::from(location)
        } else if let Some(rest) = location.strip_prefix("//") {
            alloc::format!("{}{}", scheme, rest)
        } else if location.starts_with('/') {
            alloc::format!("{}{}{}", scheme, self.host, location)
        } else {
            alloc::format!("{}{}/{}", scheme, self.host, location)
        }
    }

    fn finish(&mut self, net: &mut NetworkStack) {
        let (status, raw_bytes, html_str) = parse_http_payload_bytes(&self.response_data);
        self.status_code = status;
        if matches!(status, 301 | 302 | 303 | 307 | 308) && self.redirects < 6 {
            if let Some(location) = header_value(&self.response_data, "location") {
                let url = self.resolve_location(&location);
                let next_redirects = self.redirects + 1;
                self.start(&url, net);
                self.redirects = next_redirects;
                return;
            }
        }
        self.raw_body = raw_bytes;
        self.html_body = html_str;
        self.state = HttpState::Complete;
    }

    pub fn decode_chunked_body(&self, data: &[u8]) -> Vec<u8> {
        decode_chunked_body(data)
    }
}

fn is_http_response_complete(raw: &[u8]) -> bool {
    let Some(header_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let header_bytes = &raw[..header_end];
    let body_bytes = &raw[header_end + 4..];
    let Ok(header_str) = core::str::from_utf8(header_bytes) else {
        return false;
    };
    for line in header_str.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if key.eq_ignore_ascii_case("content-length") {
                if let Ok(expected_len) = val.parse::<usize>() {
                    return body_bytes.len() >= expected_len;
                }
            } else if key.eq_ignore_ascii_case("transfer-encoding") && val.to_ascii_lowercase().contains("chunked") {
                return body_bytes.windows(5).any(|w| w == b"0\r\n\r\n");
            }
        }
    }
    false
}

pub fn header_value(raw: &[u8], name: &str) -> Option<String> {
    let header_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = core::str::from_utf8(&raw[..header_end]).ok()?;
    for line in text.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                return Some(String::from(value.trim()));
            }
        }
    }
    None
}

fn decode_text_with_charset(bytes: &[u8], headers_lower: &str) -> String {
    let explicit_latin5 = headers_lower.contains("iso-8859-9")
        || headers_lower.contains("windows-1254")
        || headers_lower.contains("iso-8859-1");
    if !explicit_latin5 {
        if let Ok(valid_utf8) = core::str::from_utf8(bytes) {
            return String::from(valid_utf8);
        }
    }
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        if !explicit_latin5 {
            if let Ok(s) = core::str::from_utf8(&bytes[i..(i + 4).min(bytes.len())]) {
                if let Some(ch) = s.chars().next() {
                    out.push(ch);
                    i += ch.len_utf8();
                    continue;
                }
            } else if let Ok(s) = core::str::from_utf8(&bytes[i..(i + 3).min(bytes.len())]) {
                if let Some(ch) = s.chars().next() {
                    out.push(ch);
                    i += ch.len_utf8();
                    continue;
                }
            } else if let Ok(s) = core::str::from_utf8(&bytes[i..(i + 2).min(bytes.len())]) {
                if let Some(ch) = s.chars().next() {
                    out.push(ch);
                    i += ch.len_utf8();
                    continue;
                }
            }
        }
        let mapped = match b {
            0xC7 => 'C',
            0xE7 => 'c',
            0xD0 => 'G',
            0xF0 => 'g',
            0xDD => 'I',
            0xFD => 'i',
            0xD6 => 'O',
            0xF6 => 'o',
            0xDE => 'S',
            0xFE => 's',
            0xDC => 'U',
            0xFC => 'u',
            0xA0 => ' ',
            0xA9 => 'C',
            0xAE => 'R',
            0x93 | 0x94 => '"',
            0x91 | 0x92 => '\'',
            0x96 | 0x97 => '-',
            other => other as char,
        };
        out.push(mapped);
        i += 1;
    }
    out
}

pub fn parse_http_payload_bytes(raw_response: &[u8]) -> (u16, Vec<u8>, String) {
    let header_end = match raw_response.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(pos) => pos,
        None => {
            let s = decode_text_with_charset(raw_response, "");
            return (200, raw_response.to_vec(), s);
        }
    };
    let header_str = core::str::from_utf8(&raw_response[..header_end]).unwrap_or("");
    let body_bytes = &raw_response[header_end + 4..];

    let status_code = header_str
        .lines()
        .next()
        .and_then(|line| {
            let mut parts = line.split_whitespace();
            parts.next()?;
            parts.next()?.parse::<u16>().ok()
        })
        .unwrap_or(0);

    let lower_headers = header_str.to_ascii_lowercase();
    let is_chunked = lower_headers.contains("transfer-encoding: chunked");
    let clean_body = if is_chunked {
        decode_chunked_body(body_bytes)
    } else {
        body_bytes.to_vec()
    };

    let html_string = decode_text_with_charset(&clean_body, &lower_headers);
    (status_code, clean_body, html_string)
}

pub fn parse_http_payload(raw_response: &[u8]) -> (u16, String) {
    let (status, _, text) = parse_http_payload_bytes(raw_response);
    (status, text)
}

pub fn decode_chunked_body(mut data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        while data.starts_with(b"\r\n") {
            data = &data[2..];
        }
        let crlf = match data.windows(2).position(|w| w == b"\r\n") {
            Some(pos) => pos,
            None => break,
        };
        let hex_line = core::str::from_utf8(&data[..crlf]).unwrap_or("0");
        let hex_only = hex_line.split(';').next().unwrap_or("0").trim();
        let chunk_size = usize::from_str_radix(hex_only, 16).unwrap_or(0);
        if chunk_size == 0 {
            break;
        }
        let chunk_start = crlf + 2;
        let chunk_end = chunk_start.saturating_add(chunk_size);
        if chunk_end > data.len() {
            if chunk_start < data.len() {
                result.extend_from_slice(&data[chunk_start..]);
            }
            break;
        }
        result.extend_from_slice(&data[chunk_start..chunk_end]);
        data = if chunk_end + 2 <= data.len() {
            &data[chunk_end + 2..]
        } else {
            &[]
        };
    }
    result
}

pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut parts = [0u8; 4];
    let mut count = 0;
    for segment in s.split('.') {
        if count >= 4 {
            return None;
        }
        parts[count] = segment.parse::<u8>().ok()?;
        count += 1;
    }
    if count == 4 {
        Some(parts)
    } else {
        None
    }
}
