// crates/nc-pkg/src/crypto.rs
//
// F-11: SHA-256 artık elle yazılmış değil; denetlenmiş RustCrypto `sha2` kullanılır.
// Genel API (`compute_sha256`, `decode_hex`) değişmedi.
use sha2::{Digest, Sha256};

pub fn compute_sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

pub fn decode_hex(hex: &str, out: &mut [u8]) -> Result<(), ()> {
    if hex.len() != out.len() * 2 {
        return Err(());
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(())
}

fn hex_nibble(b: u8) -> Result<u8, ()> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(()),
    }
}
