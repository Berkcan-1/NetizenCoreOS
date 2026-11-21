// crates/runtime/nc-pkg/src/crypto.rs
//
// Authoritative SHA-256 is owned by the SPARK Ada Security Core via `nc-crypto`.
// No duplicate Rust SHA-256 implementation remains in `nc-pkg`.

pub fn compute_sha256(data: &[u8]) -> [u8; 32] {
    nc_crypto::sha256(data)
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
