//! HTTP hizmeti tel biçimi (`SYS_NET_CALL`). Hepsi little-endian; alloc kullanmaz.
//!
//! İstek:  [op u8][flags u8][0 u16][id u32][arg0 u32][arg1 u32] + yük   (başlık 16 bayt)
//! Yanıt:  [status i32][payload_len u32] + yük                           (başlık 8 bayt)
//!   status 0 = başarılı, > 0 = hizmet hatası (`ERR_*`), < 0 = taşıma hatası (errno).
//!
//! TCP/TLS durum makinesi ÇEKİRDEKTE kalır; uygulama yalnızca istek başlatır, ilerletir ve gövdeyi parça parça okur.
//! OP_START yükü: [method_len u16][url_len u16][cookie_len u16][header_count u16][body_len u32]
//!   + method + url + cookie + her başlık için ([name_len u16][value_len u16] ad değer) + gövde.
//!   Yanıt yükü: [id u32].
//! OP_POLL (arg0 = `tick_every`: zamanlayıcıların kaç poll'da bir ilerleyeceği, 1..=MAX_TICK_EVERY; çağrı başına TEK poll) yanıt yükü:
//!   [state u8][https u8][finished u8][tls_len u8][status_code u16][remote_port u16][html_len u32][raw_len u32]
//!   [host_len u16][path_len u16][cookie_count u16][header_count u16] + tls_hatası + host + yol
//!   + (yalnızca finished=1 iken) cookie_count x ([host_len u16][cookie_len u16] host cookie)
//!   + header_count x ([name_len u16][value_len u16] ad değer).
//! OP_READ (arg0 = 0 html / 1 ham, arg1 = ofset) yanıt yükü: ham bayt (en çok MAX_CHUNK).
//! OP_CLOSE: yük yok.

pub const OP_START: u8 = 1;
pub const OP_POLL: u8 = 2;
pub const OP_READ: u8 = 3;
pub const OP_CLOSE: u8 = 4;

pub const REQ_HEADER: usize = 16;
pub const RESP_HEADER: usize = 8;
pub const START_FIXED: usize = 12;
pub const POLL_FIXED: usize = 24;
pub const MAX_CHUNK: usize = 48 * 1024;
pub const MAX_REQUEST: usize = 192 * 1024;
pub const MAX_RESPONSE: usize = 128 * 1024;
pub const MAX_TICK_EVERY: u32 = 100_000;
pub const MAX_SLOTS_PER_APP: usize = 6;

pub const READ_HTML: u32 = 0;
pub const READ_RAW: u32 = 1;

pub const ERR_BAD_REQUEST: i32 = 1;
pub const ERR_NO_SUCH_ID: i32 = 2;
pub const ERR_TOO_MANY: i32 = 3;
pub const ERR_NO_MEMORY: i32 = 4;
pub const ERR_DENIED: i32 = 5;

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

impl HttpState {
    pub fn to_u8(self) -> u8 {
        match self {
            HttpState::Idle => 0,
            HttpState::WaitingNetwork => 1,
            HttpState::ResolvingDns => 2,
            HttpState::ConnectingSyn => 3,
            HttpState::Handshaking => 4,
            HttpState::Connected => 5,
            HttpState::RequestSent => 6,
            HttpState::ReceivingData => 7,
            HttpState::Complete => 8,
            HttpState::Failed => 9,
        }
    }

    pub fn from_u8(value: u8) -> HttpState {
        match value {
            0 => HttpState::Idle,
            1 => HttpState::WaitingNetwork,
            2 => HttpState::ResolvingDns,
            3 => HttpState::ConnectingSyn,
            4 => HttpState::Handshaking,
            5 => HttpState::Connected,
            6 => HttpState::RequestSent,
            7 => HttpState::ReceivingData,
            8 => HttpState::Complete,
            _ => HttpState::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    BadLength,
    UnknownOp,
    BadUtf8,
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    pub op: u8,
    pub flags: u8,
    pub id: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub payload: &'a [u8],
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn encode_request(out: &mut [u8], op: u8, flags: u8, id: u32, arg0: u32, arg1: u32, payload: &[u8]) -> Option<usize> {
    let total = REQ_HEADER.checked_add(payload.len())?;
    if total > out.len() || payload.len() > MAX_REQUEST {
        return None;
    }
    out[0] = op;
    out[1] = flags;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&id.to_le_bytes());
    out[8..12].copy_from_slice(&arg0.to_le_bytes());
    out[12..16].copy_from_slice(&arg1.to_le_bytes());
    out[REQ_HEADER..total].copy_from_slice(payload);
    Some(total)
}

pub fn parse_request(bytes: &[u8]) -> Result<Header<'_>, WireError> {
    if bytes.len() < REQ_HEADER {
        return Err(WireError::Truncated);
    }
    let op = bytes[0];
    if !(OP_START..=OP_CLOSE).contains(&op) || bytes[2] != 0 || bytes[3] != 0 {
        return Err(WireError::UnknownOp);
    }
    if bytes.len() > REQ_HEADER + MAX_REQUEST {
        return Err(WireError::TooLarge);
    }
    Ok(Header {
        op,
        flags: bytes[1],
        id: u32_at(bytes, 4),
        arg0: u32_at(bytes, 8),
        arg1: u32_at(bytes, 12),
        payload: &bytes[REQ_HEADER..],
    })
}

pub fn encode_response_header(out: &mut [u8], status: i32, payload_len: usize) -> Option<()> {
    if out.len() < RESP_HEADER || payload_len > u32::MAX as usize {
        return None;
    }
    out[0..4].copy_from_slice(&status.to_le_bytes());
    out[4..8].copy_from_slice(&(payload_len as u32).to_le_bytes());
    Some(())
}

pub fn parse_response(bytes: &[u8]) -> Result<(i32, &[u8]), WireError> {
    if bytes.len() < RESP_HEADER {
        return Err(WireError::Truncated);
    }
    let status = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let len = u32_at(bytes, 4) as usize;
    if bytes.len() != RESP_HEADER + len {
        return Err(WireError::BadLength);
    }
    Ok((status, &bytes[RESP_HEADER..]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartRequest<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub cookie: &'a str,
    pub header_count: usize,
    pub header_bytes: &'a [u8],
    pub body: &'a [u8],
}

fn take<'a>(payload: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], WireError> {
    let end = cursor.checked_add(len).ok_or(WireError::BadLength)?;
    if end > payload.len() {
        return Err(WireError::Truncated);
    }
    let slice = &payload[*cursor..end];
    *cursor = end;
    Ok(slice)
}

pub fn parse_start(payload: &[u8]) -> Result<StartRequest<'_>, WireError> {
    if payload.len() < START_FIXED {
        return Err(WireError::Truncated);
    }
    let method_len = u16_at(payload, 0) as usize;
    let url_len = u16_at(payload, 2) as usize;
    let cookie_len = u16_at(payload, 4) as usize;
    let header_count = u16_at(payload, 6) as usize;
    let body_len = u32_at(payload, 8) as usize;
    if method_len > 16 || url_len > 4096 || cookie_len > 16 * 1024 || header_count > 64 {
        return Err(WireError::TooLarge);
    }
    let mut cursor = START_FIXED;
    let method = core::str::from_utf8(take(payload, &mut cursor, method_len)?).map_err(|_| WireError::BadUtf8)?;
    let url = core::str::from_utf8(take(payload, &mut cursor, url_len)?).map_err(|_| WireError::BadUtf8)?;
    let cookie = core::str::from_utf8(take(payload, &mut cursor, cookie_len)?).map_err(|_| WireError::BadUtf8)?;
    let headers_start = cursor;
    let mut seen = 0usize;
    while seen < header_count {
        if cursor + 4 > payload.len() {
            return Err(WireError::Truncated);
        }
        let name_len = u16_at(payload, cursor) as usize;
        let value_len = u16_at(payload, cursor + 2) as usize;
        cursor += 4;
        take(payload, &mut cursor, name_len)?;
        take(payload, &mut cursor, value_len)?;
        seen += 1;
    }
    let header_bytes = &payload[headers_start..cursor];
    let body = take(payload, &mut cursor, body_len)?;
    if cursor != payload.len() {
        return Err(WireError::BadLength);
    }
    Ok(StartRequest { method, url, cookie, header_count, header_bytes, body })
}

pub struct HeaderPairs<'a> {
    rest: &'a [u8],
    remaining: usize,
}

impl<'a> HeaderPairs<'a> {
    pub fn new(bytes: &'a [u8], count: usize) -> Self {
        Self { rest: bytes, remaining: count }
    }
}

impl<'a> Iterator for HeaderPairs<'a> {
    type Item = Result<(&'a str, &'a str), WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let b = self.rest;
        if b.len() < 4 {
            self.remaining = 0;
            return Some(Err(WireError::Truncated));
        }
        let name_len = u16_at(b, 0) as usize;
        let value_len = u16_at(b, 2) as usize;
        let end = 4 + name_len + value_len;
        if end > b.len() {
            self.remaining = 0;
            return Some(Err(WireError::Truncated));
        }
        let name = core::str::from_utf8(&b[4..4 + name_len]);
        let value = core::str::from_utf8(&b[4 + name_len..end]);
        self.rest = &b[end..];
        match (name, value) {
            (Ok(n), Ok(v)) => Some(Ok((n, v))),
            _ => {
                self.remaining = 0;
                Some(Err(WireError::BadUtf8))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollFixed {
    pub state: HttpState,
    pub https: bool,
    pub finished: bool,
    pub tls_len: u8,
    pub status_code: u16,
    pub remote_port: u16,
    pub html_len: u32,
    pub raw_len: u32,
    pub host_len: u16,
    pub path_len: u16,
    pub cookie_count: u16,
    pub header_count: u16,
}

impl PollFixed {
    pub fn encode(&self, out: &mut [u8]) -> Option<()> {
        if out.len() < POLL_FIXED {
            return None;
        }
        out[0] = self.state.to_u8();
        out[1] = self.https as u8;
        out[2] = self.finished as u8;
        out[3] = self.tls_len;
        out[4..6].copy_from_slice(&self.status_code.to_le_bytes());
        out[6..8].copy_from_slice(&self.remote_port.to_le_bytes());
        out[8..12].copy_from_slice(&self.html_len.to_le_bytes());
        out[12..16].copy_from_slice(&self.raw_len.to_le_bytes());
        out[16..18].copy_from_slice(&self.host_len.to_le_bytes());
        out[18..20].copy_from_slice(&self.path_len.to_le_bytes());
        out[20..22].copy_from_slice(&self.cookie_count.to_le_bytes());
        out[22..24].copy_from_slice(&self.header_count.to_le_bytes());
        Some(())
    }

    pub fn decode(bytes: &[u8]) -> Result<PollFixed, WireError> {
        if bytes.len() < POLL_FIXED {
            return Err(WireError::Truncated);
        }
        Ok(PollFixed {
            state: HttpState::from_u8(bytes[0]),
            https: bytes[1] != 0,
            finished: bytes[2] != 0,
            tls_len: bytes[3],
            status_code: u16_at(bytes, 4),
            remote_port: u16_at(bytes, 6),
            html_len: u32_at(bytes, 8),
            raw_len: u32_at(bytes, 12),
            host_len: u16_at(bytes, 16),
            path_len: u16_at(bytes, 18),
            cookie_count: u16_at(bytes, 20),
            header_count: u16_at(bytes, 22),
        })
    }
}

/// `[a_len u16][b_len u16] a b` çiftini yazar; yazılan bayt sayısını döndürür.
pub fn encode_pair(out: &mut [u8], a: &str, b: &str) -> Option<usize> {
    if a.len() > u16::MAX as usize || b.len() > u16::MAX as usize {
        return None;
    }
    let total = 4 + a.len() + b.len();
    if total > out.len() {
        return None;
    }
    out[0..2].copy_from_slice(&(a.len() as u16).to_le_bytes());
    out[2..4].copy_from_slice(&(b.len() as u16).to_le_bytes());
    out[4..4 + a.len()].copy_from_slice(a.as_bytes());
    out[4 + a.len()..total].copy_from_slice(b.as_bytes());
    Some(total)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn start_payload(method: &str, url: &str, cookie: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; START_FIXED];
        p[0..2].copy_from_slice(&(method.len() as u16).to_le_bytes());
        p[2..4].copy_from_slice(&(url.len() as u16).to_le_bytes());
        p[4..6].copy_from_slice(&(cookie.len() as u16).to_le_bytes());
        p[6..8].copy_from_slice(&(headers.len() as u16).to_le_bytes());
        p[8..12].copy_from_slice(&(body.len() as u32).to_le_bytes());
        p.extend_from_slice(method.as_bytes());
        p.extend_from_slice(url.as_bytes());
        p.extend_from_slice(cookie.as_bytes());
        for (k, v) in headers {
            let mut buf = vec![0u8; 4 + k.len() + v.len()];
            let n = encode_pair(&mut buf, k, v).unwrap();
            p.extend_from_slice(&buf[..n]);
        }
        p.extend_from_slice(body);
        p
    }

    #[test]
    fn request_round_trip() {
        let mut out = vec![0u8; 64];
        let n = encode_request(&mut out, OP_POLL, 0, 7, 3, 9, &[1, 2, 3]).unwrap();
        let h = parse_request(&out[..n]).unwrap();
        assert_eq!((h.op, h.id, h.arg0, h.arg1, h.payload), (OP_POLL, 7, 3, 9, &[1u8, 2, 3][..]));
    }

    #[test]
    fn bad_op_is_rejected() {
        let mut out = vec![0u8; 32];
        let n = encode_request(&mut out, OP_POLL, 0, 1, 0, 0, &[]).unwrap();
        out[0] = 0;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
        out[0] = 99;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
    }

    #[test]
    fn start_round_trip() {
        let p = start_payload("POST", "https://a.b/c?d=1", "k=v", &[("Accept", "*/*"), ("X-A", "1")], b"body");
        let s = parse_start(&p).unwrap();
        assert_eq!((s.method, s.url, s.cookie, s.body), ("POST", "https://a.b/c?d=1", "k=v", &b"body"[..]));
        let pairs: Vec<_> = HeaderPairs::new(s.header_bytes, s.header_count).map(|r| r.unwrap()).collect();
        assert_eq!(pairs, vec![("Accept", "*/*"), ("X-A", "1")]);
    }

    #[test]
    fn start_rejects_truncated_and_trailing() {
        let p = start_payload("GET", "http://x/", "", &[], b"");
        assert!(parse_start(&p[..p.len() - 1]).is_err());
        let mut longer = p.clone();
        longer.push(0);
        assert_eq!(parse_start(&longer), Err(WireError::BadLength));
    }

    #[test]
    fn poll_fixed_round_trip() {
        let f = PollFixed {
            state: HttpState::ReceivingData,
            https: true,
            finished: false,
            tls_len: 3,
            status_code: 200,
            remote_port: 443,
            html_len: 1234,
            raw_len: 5678,
            host_len: 11,
            path_len: 5,
            cookie_count: 2,
            header_count: 9,
        };
        let mut buf = [0u8; POLL_FIXED];
        f.encode(&mut buf).unwrap();
        assert_eq!(PollFixed::decode(&buf).unwrap(), f);
    }

    #[test]
    fn state_codes_round_trip() {
        for code in 0u8..=9 {
            assert_eq!(HttpState::from_u8(code).to_u8(), code);
        }
    }
}
