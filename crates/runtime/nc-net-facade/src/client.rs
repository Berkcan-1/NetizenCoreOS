extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_abi::net as wire;

pub use nc_abi::net::HttpState;

pub struct NetworkStack;

static mut STACK: NetworkStack = NetworkStack;

pub fn global() -> &'static mut NetworkStack {
    unsafe { &mut *core::ptr::addr_of_mut!(STACK) }
}

type Transport = fn(&[u8], &mut [u8]) -> i64;

fn sys_transport(request: &[u8], response: &mut [u8]) -> i64 {
    nc_app_sdk::sys::net_call(request, response)
}

static mut TRANSPORT: Transport = sys_transport;

pub fn set_transport(transport: Transport) {
    unsafe {
        *core::ptr::addr_of_mut!(TRANSPORT) = transport;
    }
}

static mut TLS_NAMES: Vec<&'static str> = Vec::new();

fn intern(text: &str) -> &'static str {
    let names = unsafe { &mut *core::ptr::addr_of_mut!(TLS_NAMES) };
    for known in names.iter() {
        if *known == text {
            return known;
        }
    }
    if names.len() >= 32 {
        return "TLS error";
    }
    let leaked: &'static str = alloc::boxed::Box::leak(String::from(text).into_boxed_str());
    names.push(leaked);
    leaked
}

fn call(op: u8, id: u32, arg0: u32, arg1: u32, payload: &[u8], response_cap: usize) -> Option<Vec<u8>> {
    let mut request = vec![0u8; wire::REQ_HEADER + payload.len()];
    let used = wire::encode_request(&mut request, op, 0, id, arg0, arg1, payload)?;
    request.truncate(used);
    let mut response = vec![0u8; response_cap.max(wire::RESP_HEADER).min(wire::MAX_RESPONSE)];
    let transport = unsafe { *core::ptr::addr_of!(TRANSPORT) };
    let n = transport(&request, &mut response);
    if n < 0 {
        return None;
    }
    let n = n as usize;
    if n > response.len() {
        return None;
    }
    let (status, payload) = wire::parse_response(&response[..n]).ok()?;
    if status != 0 {
        return None;
    }
    Some(payload.to_vec())
}

fn start_payload(method: &str, url: &str, cookie: &str, headers: &[(String, String)], body: &[u8]) -> Option<Vec<u8>> {
    let mut out = vec![0u8; wire::START_FIXED];
    out[0..2].copy_from_slice(&(u16::try_from(method.len()).ok()?).to_le_bytes());
    out[2..4].copy_from_slice(&(u16::try_from(url.len()).ok()?).to_le_bytes());
    out[4..6].copy_from_slice(&(u16::try_from(cookie.len()).ok()?).to_le_bytes());
    out[6..8].copy_from_slice(&(u16::try_from(headers.len()).ok()?).to_le_bytes());
    out[8..12].copy_from_slice(&(u32::try_from(body.len()).ok()?).to_le_bytes());
    out.extend_from_slice(method.as_bytes());
    out.extend_from_slice(url.as_bytes());
    out.extend_from_slice(cookie.as_bytes());
    for (k, v) in headers {
        let mut buf = vec![0u8; 4 + k.len() + v.len()];
        let n = wire::encode_pair(&mut buf, k, v)?;
        out.extend_from_slice(&buf[..n]);
    }
    out.extend_from_slice(body);
    Some(out)
}

pub struct TcpHttpClient {
    pub state: HttpState,
    pub host: String,
    pub path: String,
    pub remote_port: u16,
    pub raw_body: Vec<u8>,
    pub html_body: String,
    pub status_code: u16,
    pub https: bool,
    pub tls_error: Option<&'static str>,
    pub tick_every: u32,
    pub cookie_header: String,
    pub last_headers: Vec<(String, String)>,
    pub set_cookies: Vec<(String, String)>,
    id: u32,
}

impl Default for TcpHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpHttpClient {
    pub fn new() -> Self {
        TcpHttpClient {
            state: HttpState::Idle,
            host: String::new(),
            path: String::new(),
            remote_port: 443,
            raw_body: Vec::new(),
            html_body: String::new(),
            status_code: 0,
            https: true,
            tls_error: None,
            tick_every: 1,
            cookie_header: String::new(),
            last_headers: Vec::new(),
            set_cookies: Vec::new(),
            id: 0,
        }
    }

    fn release(&mut self) {
        if self.id != 0 {
            let _ = call(wire::OP_CLOSE, self.id, 0, 0, &[], wire::RESP_HEADER);
            self.id = 0;
        }
    }

    fn fail(&mut self) {
        self.release();
        self.state = HttpState::Failed;
        self.html_body = String::new();
        self.raw_body = Vec::new();
    }

    pub fn navigate(&mut self, url: &str, net: &mut NetworkStack) {
        self.request("GET", url, &[], &[], net);
    }

    pub fn request(&mut self, method: &str, url: &str, headers: &[(String, String)], body: &[u8], _net: &mut NetworkStack) {
        self.release();
        self.set_cookies.clear();
        self.last_headers.clear();
        self.raw_body = Vec::new();
        self.html_body = String::new();
        self.status_code = 0;
        self.tls_error = None;
        let Some(payload) = start_payload(method, url, &self.cookie_header, headers, body) else {
            self.fail();
            return;
        };
        match call(wire::OP_START, 0, 0, 0, &payload, wire::RESP_HEADER + 8) {
            Some(reply) if reply.len() >= 4 => {
                self.id = u32::from_le_bytes([reply[0], reply[1], reply[2], reply[3]]);
                self.state = HttpState::WaitingNetwork;
            }
            _ => self.fail(),
        }
    }

    pub fn poll(&mut self, _net: &mut NetworkStack) {
        if matches!(self.state, HttpState::Idle | HttpState::Complete | HttpState::Failed) {
            return;
        }
        if self.id == 0 {
            self.state = HttpState::Failed;
            return;
        }
        let tick = self.tick_every.clamp(1, wire::MAX_TICK_EVERY);
        let Some(reply) = call(wire::OP_POLL, self.id, tick, 0, &[], wire::MAX_RESPONSE) else {
            self.fail();
            return;
        };
        let Ok(fixed) = wire::PollFixed::decode(&reply) else {
            self.fail();
            return;
        };
        let mut cursor = wire::POLL_FIXED;
        let tls_len = fixed.tls_len as usize;
        let host_len = fixed.host_len as usize;
        let path_len = fixed.path_len as usize;
        let strings_end = cursor + tls_len + host_len + path_len;
        if strings_end > reply.len() {
            self.fail();
            return;
        }
        self.tls_error = if tls_len == 0 {
            None
        } else {
            core::str::from_utf8(&reply[cursor..cursor + tls_len]).ok().map(intern)
        };
        cursor += tls_len;
        if let Ok(h) = core::str::from_utf8(&reply[cursor..cursor + host_len]) {
            if self.host != h {
                self.host = String::from(h);
            }
        }
        cursor += host_len;
        if let Ok(p) = core::str::from_utf8(&reply[cursor..cursor + path_len]) {
            if self.path != p {
                self.path = String::from(p);
            }
        }
        cursor += path_len;
        self.https = fixed.https;
        self.remote_port = fixed.remote_port;
        self.status_code = fixed.status_code;

        if !fixed.finished {
            self.state = fixed.state;
            return;
        }

        let mut cookies: Vec<(String, String)> = Vec::new();
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut ok = true;
        for _ in 0..fixed.cookie_count {
            match read_pair(&reply, &mut cursor) {
                Some(pair) => cookies.push(pair),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            for _ in 0..fixed.header_count {
                match read_pair(&reply, &mut cursor) {
                    Some(pair) => headers.push(pair),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if !ok {
            self.fail();
            return;
        }
        self.set_cookies = cookies;
        self.last_headers = headers;

        if fixed.state == HttpState::Complete {
            let (which, total) = if fixed.html_len > 0 { (wire::READ_HTML, fixed.html_len as usize) } else { (wire::READ_RAW, fixed.raw_len as usize) };
            match self.download(which, total) {
                Some(bytes) => {
                    if which == wire::READ_HTML {
                        self.html_body = String::from_utf8(bytes).unwrap_or_default();
                        self.raw_body = Vec::new();
                    } else {
                        self.raw_body = bytes;
                        self.html_body = String::new();
                    }
                }
                None => {
                    self.fail();
                    return;
                }
            }
        }
        self.release();
        self.state = fixed.state;
    }

    fn download(&mut self, which: u32, total: usize) -> Option<Vec<u8>> {
        let mut data: Vec<u8> = Vec::new();
        data.try_reserve_exact(total).ok()?;
        while data.len() < total {
            let chunk = call(wire::OP_READ, self.id, which, data.len() as u32, &[], wire::RESP_HEADER + wire::MAX_CHUNK)?;
            if chunk.is_empty() {
                return None;
            }
            data.extend_from_slice(&chunk);
        }
        data.truncate(total);
        Some(data)
    }
}

impl Drop for TcpHttpClient {
    fn drop(&mut self) {
        self.release();
    }
}

fn read_pair(bytes: &[u8], cursor: &mut usize) -> Option<(String, String)> {
    if *cursor + 4 > bytes.len() {
        return None;
    }
    let a = u16::from_le_bytes([bytes[*cursor], bytes[*cursor + 1]]) as usize;
    let b = u16::from_le_bytes([bytes[*cursor + 2], bytes[*cursor + 3]]) as usize;
    let end = (*cursor + 4).checked_add(a)?.checked_add(b)?;
    if end > bytes.len() {
        return None;
    }
    let first = core::str::from_utf8(&bytes[*cursor + 4..*cursor + 4 + a]).ok()?;
    let second = core::str::from_utf8(&bytes[*cursor + 4 + a..end]).ok()?;
    *cursor = end;
    Some((String::from(first), String::from(second)))
}
