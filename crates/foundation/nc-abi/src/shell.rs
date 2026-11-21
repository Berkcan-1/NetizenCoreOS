//! Kabuk hizmeti tel biçimi (`SYS_SHELL_CALL`): uygulama sesi ve bildirimler. Hepsi little-endian; alloc kullanmaz.
//!
//! İstek:  [op u8][0 u8;3][arg0 u32][arg1 u32][arg2 u32] + yük   (başlık 16 bayt)
//! Yanıt:  [status i32][payload_len u32] + yük                   (başlık 8 bayt)
//!   status 0 = başarılı, > 0 = hizmet hatası (`ERR_*`), < 0 = taşıma hatası (errno).
//!
//! Sistem sesi:  OP_SOUND_PLAY (arg0 = `SOUND_*`). Yetki: `audio.play`.
//! Ses akışı:    OP_STREAM_OPEN (arg0 = örnekleme hızı 8000..=48000, arg1 = kanal 1|2) yanıt yükü [id u32].
//!               OP_STREAM_WRITE (arg0 = id, yük = PCM s16le, kanallar araya girmiş) yanıt yükü [kabul edilen kare u32].
//!               Bloklamaz: tampon doluysa kabul edilen kare sayısı yazılandan az olur; kalanı uygulama sonra yollar.
//!               OP_STREAM_STATUS (arg0 = id) yanıt yükü [kuyruktaki kare u32][boş kare u32].
//!               OP_STREAM_VOLUME (arg0 = id, arg1 = 0..=100). OP_STREAM_CLOSE (arg0 = id, arg1 = 1 ise kalan sesi at).
//! Bildirim:     OP_NOTIFY_POST (arg0 = `LEVEL_*`, yük = [title_len u16][body_len u16] başlık gövde) yetki: `notification.post`.
//!               OP_NOTIFY_STATE yanıt yükü [durum u8] (`PERM_*`).
//!               Bildirim izni kullanıcıdadır: ilk bildirimde masaüstü sorar; reddedilmişse OP_NOTIFY_POST `ERR_DENIED` döner.

pub const OP_SOUND_PLAY: u8 = 1;
pub const OP_STREAM_OPEN: u8 = 2;
pub const OP_STREAM_WRITE: u8 = 3;
pub const OP_STREAM_STATUS: u8 = 4;
pub const OP_STREAM_VOLUME: u8 = 5;
pub const OP_STREAM_CLOSE: u8 = 6;
pub const OP_NOTIFY_POST: u8 = 7;
pub const OP_NOTIFY_STATE: u8 = 8;
pub const OP_LAST: u8 = OP_NOTIFY_STATE;

pub const SOUND_CLICK: u32 = 1;
pub const SOUND_OPEN: u32 = 2;
pub const SOUND_CLOSE: u32 = 3;
pub const SOUND_NOTIFY: u32 = 4;
pub const SOUND_SUCCESS: u32 = 5;
pub const SOUND_WARNING: u32 = 6;
pub const SOUND_ERROR: u32 = 7;
pub const SOUND_TICK: u32 = 8;
pub const SOUND_CONNECT: u32 = 9;
pub const SOUND_DISCONNECT: u32 = 10;
pub const SOUND_LAST: u32 = SOUND_DISCONNECT;

pub const LEVEL_INFO: u32 = 0;
pub const LEVEL_SUCCESS: u32 = 1;
pub const LEVEL_WARNING: u32 = 2;
pub const LEVEL_ERROR: u32 = 3;

pub const PERM_ASK: u8 = 0;
pub const PERM_ALLOWED: u8 = 1;
pub const PERM_DENIED: u8 = 2;

pub const REQ_HEADER: usize = 16;
pub const RESP_HEADER: usize = 8;
pub const MAX_PCM_WRITE: usize = 16 * 1024;
pub const MAX_REQUEST: usize = MAX_PCM_WRITE + 8;
pub const MAX_RESPONSE: usize = 64;
pub const MAX_TITLE: usize = 64;
pub const MAX_BODY: usize = 256;
pub const MAX_STREAMS_PER_APP: usize = 2;
pub const MIN_RATE: u32 = 8_000;
pub const MAX_RATE: u32 = 48_000;

pub const ERR_BAD_REQUEST: i32 = 1;
pub const ERR_DENIED: i32 = 2;
pub const ERR_FAILED: i32 = 3;
pub const ERR_NO_MEMORY: i32 = 4;
pub const ERR_LIMIT: i32 = 5;
pub const ERR_NO_STREAM: i32 = 6;
pub const ERR_NO_DEVICE: i32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    BadLength,
    UnknownOp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub op: u8,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
    pub payload: &'a [u8],
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn encode_request(out: &mut [u8], op: u8, arg0: u32, arg1: u32, arg2: u32, payload: &[u8]) -> Option<usize> {
    let total = REQ_HEADER.checked_add(payload.len())?;
    if total > out.len() || payload.len() > MAX_REQUEST {
        return None;
    }
    out[0] = op;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&arg0.to_le_bytes());
    out[8..12].copy_from_slice(&arg1.to_le_bytes());
    out[12..16].copy_from_slice(&arg2.to_le_bytes());
    out[REQ_HEADER..total].copy_from_slice(payload);
    Some(total)
}

pub fn parse_request(bytes: &[u8]) -> Result<Request<'_>, WireError> {
    if bytes.len() < REQ_HEADER {
        return Err(WireError::Truncated);
    }
    if bytes[0] == 0 || bytes[0] > OP_LAST || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0 {
        return Err(WireError::UnknownOp);
    }
    if bytes.len() > REQ_HEADER + MAX_REQUEST {
        return Err(WireError::BadLength);
    }
    Ok(Request {
        op: bytes[0],
        arg0: u32_at(bytes, 4),
        arg1: u32_at(bytes, 8),
        arg2: u32_at(bytes, 12),
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

pub fn encode_notify_payload(out: &mut [u8], title: &str, body: &str) -> Option<usize> {
    let (t, b) = (title.as_bytes(), body.as_bytes());
    if t.len() > MAX_TITLE || b.len() > MAX_BODY || out.len() < 4 + t.len() + b.len() {
        return None;
    }
    out[0..2].copy_from_slice(&(t.len() as u16).to_le_bytes());
    out[2..4].copy_from_slice(&(b.len() as u16).to_le_bytes());
    out[4..4 + t.len()].copy_from_slice(t);
    out[4 + t.len()..4 + t.len() + b.len()].copy_from_slice(b);
    Some(4 + t.len() + b.len())
}

pub fn parse_notify_payload(payload: &[u8]) -> Option<(&str, &str)> {
    if payload.len() < 4 {
        return None;
    }
    let tl = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let bl = u16::from_le_bytes([payload[2], payload[3]]) as usize;
    if tl == 0 || tl > MAX_TITLE || bl > MAX_BODY || payload.len() != 4 + tl + bl {
        return None;
    }
    let title = core::str::from_utf8(&payload[4..4 + tl]).ok()?;
    let body = core::str::from_utf8(&payload[4 + tl..]).ok()?;
    Some((title, body))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;

    #[test]
    fn request_round_trip() {
        let mut out = vec![0u8; 64];
        let n = encode_request(&mut out, OP_STREAM_OPEN, 44_100, 2, 0, b"").unwrap();
        let r = parse_request(&out[..n]).unwrap();
        assert_eq!((r.op, r.arg0, r.arg1), (OP_STREAM_OPEN, 44_100, 2));
    }

    #[test]
    fn rejects_unknown_ops_and_oversize() {
        let mut out = vec![0u8; 32];
        let n = encode_request(&mut out, OP_SOUND_PLAY, 1, 0, 0, &[]).unwrap();
        out[0] = 0;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
        out[0] = OP_LAST + 1;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
        let big = vec![0u8; REQ_HEADER + MAX_REQUEST + 1];
        let mut big = big;
        big[0] = OP_STREAM_WRITE;
        assert_eq!(parse_request(&big), Err(WireError::BadLength));
    }

    #[test]
    fn notify_payload_round_trip_and_limits() {
        let mut buf = vec![0u8; 400];
        let n = encode_notify_payload(&mut buf, "Başlık", "gövde metni").unwrap();
        assert_eq!(parse_notify_payload(&buf[..n]), Some(("Başlık", "gövde metni")));
        assert!(parse_notify_payload(&buf[..n - 1]).is_none());
        assert!(encode_notify_payload(&mut buf, &"x".repeat(MAX_TITLE + 1), "").is_none());
        let n = encode_notify_payload(&mut buf, "t", "").unwrap();
        assert_eq!(parse_notify_payload(&buf[..n]), Some(("t", "")));
        buf[0] = 0;
        buf[1] = 0;
        assert!(parse_notify_payload(&buf[..4]).is_none());
    }

    #[test]
    fn response_round_trip() {
        let mut out = vec![0u8; 16];
        encode_response_header(&mut out, ERR_DENIED, 4).unwrap();
        assert_eq!(parse_response(&out[..12]), Ok((ERR_DENIED, &out[8..12])));
        assert_eq!(parse_response(&out[..11]), Err(WireError::BadLength));
    }
}
