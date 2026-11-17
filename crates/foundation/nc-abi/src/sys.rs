//! Sistem hizmeti tel biçimi (`SYS_SYS_CALL`): yalnızca Ayarlar için. Hepsi little-endian; alloc kullanmaz.
//!
//! İstek:  [op u8][0 u8;3][arg0 u32][arg1 u32][arg2 u32] + yük   (başlık 16 bayt)
//! Yanıt:  [status i32][payload_len u32] + yük                   (başlık 8 bayt)
//!   status 0 = başarılı, > 0 = hizmet hatası (`ERR_*`), < 0 = taşıma hatası (errno).

pub const OP_PCI_NIC: u8 = 1;
pub const OP_WL_POLL: u8 = 2;
pub const OP_WL_CMD: u8 = 3;
pub const OP_ETH_INFO: u8 = 4;
pub const OP_ETH_CMD: u8 = 5;
pub const OP_DRV_LIST: u8 = 6;
pub const OP_DRV_REGISTER: u8 = 7;
pub const OP_DRV_UPGRADE: u8 = 8;
pub const OP_DRV_SCAN: u8 = 9;
pub const OP_SETTINGS_PUSH: u8 = 10;
pub const OP_SEC_KIND: u8 = 11;
pub const OP_SEC_VERIFY: u8 = 12;
pub const OP_SEC_SUBMIT: u8 = 13;
/// Gercek donanim, depolama, bellek, paket ve EL0 gorev durumunu dondurur (anahtar=deger satirlari).
pub const OP_DEVICE_INFO: u8 = 14;
/// Mobil/aarch64 ayarlarini gunceller ve `/data/appdata/nc.settings/settings.cfg` dosyasina kalici yazar.
pub const OP_MOBILE_SET: u8 = 15;
pub const OP_LAST: u8 = OP_MOBILE_SET;

pub const MOBILE_SET_ACCENT: u32 = 1;
pub const MOBILE_SET_WALLPAPER: u32 = 2;
pub const MOBILE_SET_TZ: u32 = 3;
pub const MOBILE_SET_LANG: u32 = 4;
pub const MOBILE_SYNC_DATA: u32 = 5;
pub const MOBILE_SET_WIFI: u32 = 6;
pub const MOBILE_SET_BT: u32 = 7;
pub const MOBILE_SET_BRIGHTNESS: u32 = 8;

pub const WL_INIT: u32 = 1;
pub const WL_WIFI_SCAN: u32 = 2;
pub const WL_BT_SCAN: u32 = 3;
pub const WL_SET_WIFI: u32 = 4;
pub const WL_SET_BT: u32 = 5;
pub const WL_CONNECT: u32 = 6;
pub const WL_DISCONNECT: u32 = 7;
pub const WL_TOGGLE_BT: u32 = 8;

pub const ETH_DHCP: u32 = 1;
pub const ETH_ARP: u32 = 2;

pub const REQ_HEADER: usize = 16;
pub const RESP_HEADER: usize = 8;
pub const MAX_REQUEST: usize = 8 * 1024;
pub const MAX_RESPONSE: usize = 32 * 1024;

pub const ERR_BAD_REQUEST: i32 = 1;
pub const ERR_DENIED: i32 = 2;
pub const ERR_FAILED: i32 = 3;
pub const ERR_NO_MEMORY: i32 = 4;

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

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;

    #[test]
    fn request_round_trip() {
        let mut out = vec![0u8; 64];
        let n = encode_request(&mut out, OP_WL_CMD, 6, 1, 2, b"abc").unwrap();
        let r = parse_request(&out[..n]).unwrap();
        assert_eq!((r.op, r.arg0, r.arg1, r.arg2, r.payload), (OP_WL_CMD, 6, 1, 2, &b"abc"[..]));
    }

    #[test]
    fn rejects_unknown_ops() {
        let mut out = vec![0u8; 32];
        let n = encode_request(&mut out, OP_PCI_NIC, 0, 0, 0, &[]).unwrap();
        out[0] = 0;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
        out[0] = OP_LAST + 1;
        assert_eq!(parse_request(&out[..n]), Err(WireError::UnknownOp));
    }

    #[test]
    fn response_round_trip() {
        let mut out = vec![0u8; RESP_HEADER + 3];
        encode_response_header(&mut out, 0, 3).unwrap();
        out[RESP_HEADER..].copy_from_slice(b"xyz");
        assert_eq!(parse_response(&out).unwrap(), (0, &b"xyz"[..]));
        assert_eq!(parse_response(&out[..out.len() - 1]), Err(WireError::BadLength));
    }
}
