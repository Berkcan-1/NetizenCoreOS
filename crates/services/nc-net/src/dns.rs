extern crate alloc;

use alloc::vec::Vec;

pub fn encode_dns_query(domain: &str, tx_id: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&tx_id.to_be_bytes());
    buf.extend_from_slice(&0x0100u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    let host_only = domain.split(':').next().unwrap_or(domain);
    for part in host_only.trim_matches('.').split('.') {
        let len = part.len() as u8;
        if len > 0 {
            buf.push(len);
            buf.extend_from_slice(part.as_bytes());
        }
    }
    buf.push(0);

    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf
}

fn skip_dns_name(data: &[u8], mut idx: usize) -> Option<usize> {
    let mut jumps = 0usize;
    while idx < data.len() {
        let b = data[idx];
        if b == 0 {
            return Some(idx + 1);
        }
        if (b & 0xC0) == 0xC0 {
            if idx + 1 >= data.len() {
                return None;
            }
            return Some(idx + 2);
        }
        let label_len = (b & 0x3F) as usize;
        idx += 1 + label_len;
        jumps += 1;
        if jumps > 64 {
            return None;
        }
    }
    None
}

pub fn parse_dns_response(data: &[u8]) -> Option<[u8; 4]> {
    if data.len() < 12 {
        return None;
    }

    let qd_count = u16::from_be_bytes([data[4], data[5]]);
    let an_count = u16::from_be_bytes([data[6], data[7]]);
    if an_count == 0 {
        return None;
    }

    let mut idx = 12;
    for _ in 0..qd_count {
        idx = skip_dns_name(data, idx)?;
        if idx + 4 > data.len() {
            return None;
        }
        idx += 4;
    }

    for _ in 0..an_count {
        idx = skip_dns_name(data, idx)?;
        if idx + 10 > data.len() {
            break;
        }
        let r_type = u16::from_be_bytes([data[idx], data[idx + 1]]);
        let r_len = u16::from_be_bytes([data[idx + 8], data[idx + 9]]) as usize;
        idx += 10;

        if r_type == 1 && r_len == 4 && idx + 4 <= data.len() {
            return Some([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]);
        }
        idx += r_len;
    }

    None
}
