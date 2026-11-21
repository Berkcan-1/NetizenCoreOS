extern crate alloc;

use alloc::vec::Vec;

pub const TLS_RECORD_HANDSHAKE: u8 = 22;
pub const TLS_RECORD_APPLICATION_DATA: u8 = 23;

pub fn build_tls_client_hello(domain: &str) -> Vec<u8> {
    let mut hs = Vec::new();

    // Client Version: TLS 1.2 (0x0303)
    hs.extend_from_slice(&[0x03, 0x03]);

    // Random (32 bytes)
    for i in 0..32 {
        hs.push((i * 7 + 13) as u8);
    }

    // Session ID length: 0
    hs.push(0);

    // Cipher Suites (e.g. TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256, TLS_RSA_WITH_AES_128_CBC_SHA)
    let ciphers: [u8; 8] = [0x00, 0x04, 0xC0, 0x2F, 0xC0, 0x2B, 0x00, 0x2F];
    hs.extend_from_slice(&(ciphers.len() as u16).to_be_bytes());
    hs.extend_from_slice(&ciphers);

    // Compression Methods: null (1 byte)
    hs.push(1);
    hs.push(0);

    // Extensions
    let mut ext = Vec::new();

    // Extension: Server Name Indication (SNI = 0x0000)
    let mut sni = Vec::new();
    let dom_bytes = domain.as_bytes();
    let sni_list_len = dom_bytes.len() + 3;
    sni.extend_from_slice(&(sni_list_len as u16).to_be_bytes());
    sni.push(0); // NameType: host_name
    sni.extend_from_slice(&(dom_bytes.len() as u16).to_be_bytes());
    sni.extend_from_slice(dom_bytes);

    ext.extend_from_slice(&0x0000u16.to_be_bytes()); // SNI ext type
    ext.extend_from_slice(&(sni.len() as u16).to_be_bytes());
    ext.extend_from_slice(&sni);

    hs.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    hs.extend_from_slice(&ext);

    // Wrap in Handshake Header (Type 1 = ClientHello)
    let mut msg = Vec::new();
    msg.push(1); // ClientHello
    let hs_len = hs.len();
    msg.push((hs_len >> 16) as u8);
    msg.push((hs_len >> 8) as u8);
    msg.push((hs_len & 0xFF) as u8);
    msg.extend_from_slice(&hs);

    // Wrap in TLS Record Header
    let mut record = Vec::new();
    record.push(TLS_RECORD_HANDSHAKE);
    record.extend_from_slice(&[0x03, 0x01]); // TLS 1.0 record version compatibility
    record.extend_from_slice(&(msg.len() as u16).to_be_bytes());
    record.extend_from_slice(&msg);

    record
}

pub fn parse_tls_record(data: &[u8]) -> Option<(u8, Vec<u8>)> {
    if data.len() < 5 {
        return None;
    }
    let content_type = data[0];
    let record_len = u16::from_be_bytes([data[3], data[4]]) as usize;
    if data.len() < 5 + record_len {
        return None;
    }
    Some((content_type, data[5..5 + record_len].to_vec()))
}
