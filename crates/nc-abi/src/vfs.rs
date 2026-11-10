//! VFS hizmeti tel biçimi (`SYS_VFS_CALL`). Hepsi little-endian; alloc kullanmaz.
//!
//! İstek:  [op u8][0 u8][path_len u16][data_len u32][arg0 u64][arg1 u64] + yol (UTF-8) + veri   (başlık 24 bayt)
//! Yanıt:  [status i32][payload_len u32] + yük                                                    (başlık 8 bayt)
//!   status 0 = başarılı, > 0 = `FsError` kodu (aşağıdaki `ERR_*`), < 0 = taşıma hatası (errno).
//!   LIST yükü: [count u32][0 u32] + her giriş: [kind u8][0 u8][name_len u16][0 u32][size u64][id u64] + ad (8'e dolgulu)
//!   STAT yükü: [size u64][is_dir u8][readonly u8][0 u8 * 6]
//!   READ_RANGE yükü: ham bayt; USB_RESCAN/USB_ONLINE: [u8]; USB_LABEL: UTF-8.

pub const OP_LIST: u8 = 1;
pub const OP_STAT: u8 = 2;
/// arg0 = ofset, arg1 = en çok bayt.
pub const OP_READ_RANGE: u8 = 3;
/// arg0 = ofset, arg1 = toplam uzunluk; veri = parça. Son parça (ofset + len == toplam) dosyayı yazar.
pub const OP_WRITE: u8 = 4;
pub const OP_MKDIR: u8 = 5;
pub const OP_REMOVE: u8 = 6;
pub const OP_USB_RESCAN: u8 = 7;
pub const OP_USB_LABEL: u8 = 8;
pub const OP_USB_ONLINE: u8 = 9;

pub const REQ_HEADER: usize = 24;
pub const RESP_HEADER: usize = 8;
pub const MAX_PATH: usize = 256;
pub const MAX_CHUNK: usize = 48 * 1024;
pub const MAX_WRITE_TOTAL: u64 = 16 * 1024 * 1024;
pub const MAX_LIST_ENTRIES: usize = 2048;

pub const KIND_FILE: u8 = 0;
pub const KIND_DIR: u8 = 1;
pub const KIND_UNKNOWN: u8 = 2;

// FsError kodları (nc_fs::FsError sırası + 1).
pub const ERR_DISK: i32 = 1;
pub const ERR_INVALID_SUPERBLOCK: i32 = 2;
pub const ERR_NOT_FOUND: i32 = 3;
pub const ERR_NOT_A_DIRECTORY: i32 = 4;
pub const ERR_NOT_A_FILE: i32 = 5;
pub const ERR_CORRUPTED: i32 = 6;
pub const ERR_ALREADY_EXISTS: i32 = 7;
pub const ERR_READ_ONLY: i32 = 8;
pub const ERR_PERMISSION_DENIED: i32 = 9;
pub const ERR_INVALID_PATH: i32 = 10;
pub const ERR_STORAGE_FULL: i32 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    BadLength,
    UnknownOp,
    BadUtf8,
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub op: u8,
    pub arg0: u64,
    pub arg1: u64,
    pub path: &'a str,
    pub data: &'a [u8],
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3], b[o + 4], b[o + 5], b[o + 6], b[o + 7]])
}

pub fn encode_request(out: &mut [u8], op: u8, arg0: u64, arg1: u64, path: &str, data: &[u8]) -> Option<usize> {
    let total = REQ_HEADER.checked_add(path.len())?.checked_add(data.len())?;
    if path.len() > MAX_PATH || data.len() > MAX_CHUNK || total > out.len() {
        return None;
    }
    out[0] = op;
    out[1] = 0;
    out[2..4].copy_from_slice(&(path.len() as u16).to_le_bytes());
    out[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
    out[8..16].copy_from_slice(&arg0.to_le_bytes());
    out[16..24].copy_from_slice(&arg1.to_le_bytes());
    out[REQ_HEADER..REQ_HEADER + path.len()].copy_from_slice(path.as_bytes());
    out[REQ_HEADER + path.len()..total].copy_from_slice(data);
    Some(total)
}

pub fn parse_request(bytes: &[u8]) -> Result<Request<'_>, WireError> {
    if bytes.len() < REQ_HEADER {
        return Err(WireError::Truncated);
    }
    let op = bytes[0];
    if !(OP_LIST..=OP_USB_ONLINE).contains(&op) || bytes[1] != 0 {
        return Err(WireError::UnknownOp);
    }
    let path_len = u16_at(bytes, 2) as usize;
    let data_len = u32_at(bytes, 4) as usize;
    if path_len > MAX_PATH || data_len > MAX_CHUNK {
        return Err(WireError::TooLarge);
    }
    if bytes.len() != REQ_HEADER + path_len + data_len {
        return Err(WireError::BadLength);
    }
    let path = core::str::from_utf8(&bytes[REQ_HEADER..REQ_HEADER + path_len]).map_err(|_| WireError::BadUtf8)?;
    Ok(Request {
        op,
        arg0: u64_at(bytes, 8),
        arg1: u64_at(bytes, 16),
        path,
        data: &bytes[REQ_HEADER + path_len..],
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

/// Dönüş: (status, yük).
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

fn padded8(n: usize) -> usize {
    (n + 7) & !7
}

/// Bir LIST girişini yazar; yazılan bayt sayısını döndürür.
pub fn encode_dir_entry(out: &mut [u8], kind: u8, size: u64, id: u64, name: &str) -> Option<usize> {
    if name.len() > u16::MAX as usize {
        return None;
    }
    let total = 24 + padded8(name.len());
    if total > out.len() {
        return None;
    }
    out[0] = kind;
    out[1] = 0;
    out[2..4].copy_from_slice(&(name.len() as u16).to_le_bytes());
    out[4..8].copy_from_slice(&0u32.to_le_bytes());
    out[8..16].copy_from_slice(&size.to_le_bytes());
    out[16..24].copy_from_slice(&id.to_le_bytes());
    out[24..24 + name.len()].copy_from_slice(name.as_bytes());
    for b in &mut out[24 + name.len()..total] {
        *b = 0;
    }
    Some(total)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirItem<'a> {
    pub kind: u8,
    pub size: u64,
    pub id: u64,
    pub name: &'a str,
}

/// LIST yükünü doğrulayarak ayrıştırır (sayı, uzunluklar, UTF-8, dolgu).
pub struct DirEntries<'a> {
    rest: &'a [u8],
    remaining: u32,
}

impl<'a> DirEntries<'a> {
    pub fn new(payload: &'a [u8]) -> Result<Self, WireError> {
        if payload.len() < 8 {
            return Err(WireError::Truncated);
        }
        let count = u32_at(payload, 0);
        if count as usize > MAX_LIST_ENTRIES {
            return Err(WireError::TooLarge);
        }
        Ok(Self { rest: &payload[8..], remaining: count })
    }
}

impl<'a> Iterator for DirEntries<'a> {
    type Item = Result<DirItem<'a>, WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let b = self.rest;
        if b.len() < 24 {
            self.remaining = 0;
            return Some(Err(WireError::Truncated));
        }
        let name_len = u16_at(b, 2) as usize;
        let total = 24 + padded8(name_len);
        if b.len() < total || b[1] != 0 || u32_at(b, 4) != 0 || b[24 + name_len..total].iter().any(|&x| x != 0) {
            self.remaining = 0;
            return Some(Err(WireError::BadLength));
        }
        let Ok(name) = core::str::from_utf8(&b[24..24 + name_len]) else {
            self.remaining = 0;
            return Some(Err(WireError::BadUtf8));
        };
        let item = DirItem { kind: b[0].min(KIND_UNKNOWN), size: u64_at(b, 8), id: u64_at(b, 16), name };
        self.rest = &b[total..];
        Some(Ok(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_and_limits() {
        let mut buf = [0u8; 512];
        let n = encode_request(&mut buf, OP_WRITE, 4096, 9000, "/data/documents/a.txt", b"merhaba").unwrap();
        let r = parse_request(&buf[..n]).unwrap();
        assert_eq!((r.op, r.arg0, r.arg1, r.path, r.data), (OP_WRITE, 4096, 9000, "/data/documents/a.txt", &b"merhaba"[..]));
        assert!(encode_request(&mut buf, OP_LIST, 0, 0, &"x".repeat(257), b"").is_none());
        assert!(encode_request(&mut buf[..30], OP_LIST, 0, 0, "/data/documents", b"").is_none());
    }

    #[test]
    fn request_parser_rejects_malformed() {
        let mut buf = [0u8; 128];
        let n = encode_request(&mut buf, OP_STAT, 0, 0, "/ram/x", b"").unwrap();
        for cut in 0..n {
            assert!(parse_request(&buf[..cut]).is_err(), "kesik istek kabul edildi: {}", cut);
        }
        let mut b = buf;
        b[0] = 0;
        assert_eq!(parse_request(&b[..n]), Err(WireError::UnknownOp));
        b[0] = 10;
        assert_eq!(parse_request(&b[..n]), Err(WireError::UnknownOp));
        let mut b = buf;
        b[1] = 1;
        assert_eq!(parse_request(&b[..n]), Err(WireError::UnknownOp));
        let mut b = buf;
        b[2] = 0xFF;
        b[3] = 0xFF;
        assert_eq!(parse_request(&b[..n]), Err(WireError::TooLarge));
        let mut b = buf;
        b[4] = 5;
        assert_eq!(parse_request(&b[..n]), Err(WireError::BadLength));
        let mut b = buf;
        b[REQ_HEADER] = 0xFF;
        assert_eq!(parse_request(&b[..n]), Err(WireError::BadUtf8));
        assert_eq!(parse_request(&buf[..n + 1]), Err(WireError::BadLength));
    }

    #[test]
    fn response_roundtrip() {
        let mut buf = [0u8; 32];
        encode_response_header(&mut buf, ERR_NOT_FOUND, 3).unwrap();
        buf[8..11].copy_from_slice(b"abc");
        assert_eq!(parse_response(&buf[..11]), Ok((ERR_NOT_FOUND, &b"abc"[..])));
        assert_eq!(parse_response(&buf[..10]), Err(WireError::BadLength));
        assert_eq!(parse_response(&buf[..7]), Err(WireError::Truncated));
    }

    #[test]
    fn directory_listing_roundtrip_and_strictness() {
        let mut payload = [0u8; 512];
        payload[0..4].copy_from_slice(&3u32.to_le_bytes());
        let mut at = 8;
        for (kind, size, id, name) in [(KIND_DIR, 0u64, 1u64, "Belgeler"), (KIND_FILE, 1234, 2, "not.txt"), (KIND_FILE, 5, 3, "ş")] {
            at += encode_dir_entry(&mut payload[at..], kind, size, id, name).unwrap();
        }
        let items: [DirItem; 3] = {
            let mut it = DirEntries::new(&payload[..at]).unwrap();
            [it.next().unwrap().unwrap(), it.next().unwrap().unwrap(), it.next().unwrap().unwrap()]
        };
        assert_eq!(items[0], DirItem { kind: KIND_DIR, size: 0, id: 1, name: "Belgeler" });
        assert_eq!(items[2].name, "ş");
        assert!(DirEntries::new(&payload[..at]).unwrap().skip(3).next().is_none());

        // sayı gerçek girişlerden fazlaysa hata (taşma yok)
        let mut p = payload;
        p[0..4].copy_from_slice(&4u32.to_le_bytes());
        assert!(DirEntries::new(&p[..at]).unwrap().any(|e| e.is_err()));
        // aşırı sayı
        p[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(DirEntries::new(&p[..at]).err(), Some(WireError::TooLarge));
        // kesik giriş
        for cut in 8..at {
            let ok = DirEntries::new(&payload[..cut]).unwrap().all(|e| e.is_ok());
            if cut != at {
                assert!(!ok || cut == 8 + 0, "kesik liste sessizce kabul edildi: {}", cut);
            }
        }
    }

    #[test]
    fn dir_entry_mutation_fuzz_never_panics() {
        let mut payload = [0u8; 256];
        payload[0..4].copy_from_slice(&2u32.to_le_bytes());
        let mut at = 8;
        at += encode_dir_entry(&mut payload[at..], KIND_FILE, 7, 9, "a.txt").unwrap();
        at += encode_dir_entry(&mut payload[at..], KIND_DIR, 0, 10, "klasor").unwrap();
        let mut state: u64 = 0x1234_5678_9ABC_DEF1;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..30_000 {
            let mut b = payload;
            for _ in 0..(1 + next() % 3) {
                b[(next() as usize) % at] = next() as u8;
            }
            let len = if next() % 4 == 0 { (next() as usize) % (at + 1) } else { at };
            if let Ok(entries) = DirEntries::new(&b[..len]) {
                for e in entries {
                    if let Ok(item) = e {
                        assert!(item.kind <= KIND_UNKNOWN);
                    }
                }
            }
            let _ = parse_request(&b[..len]);
            let _ = parse_response(&b[..len]);
        }
    }
}
