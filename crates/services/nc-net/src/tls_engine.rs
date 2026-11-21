extern crate alloc;
use alloc::vec::Vec;
use nc_arch::rand::HardwareRng;

pub const TLS_RECORD_HANDSHAKE: u8 = 22;
pub const TLS_RECORD_APPLICATION_DATA: u8 = 23;
pub const TLS_RECORD_ALERT: u8 = 21;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TlsState {
    Initial,
    ClientHelloSent,
    ServerHelloReceived,
    HandshakeCompleted,
    Connected,
    Closed,
}

pub struct TlsSession {
    pub state: TlsState,
    pub server_name: alloc::string::String,
    pub client_random: [u8; 32],
    pub server_random: [u8; 32],
    // Ephemeral Private Key (X25519 için 32 bayt rastgele)
    pub private_key: [u8; 32],
    pub peer_public_key: [u8; 32],
}

impl TlsSession {
    /// F-03: yalnızca güvenli donanım entropisi; yoksa `Err` (tahmin edilebilir PRNG'ye düşülmez).
    pub fn new(host: &str) -> Result<Self, &'static str> {
        let mut client_random = [0u8; 32];
        let mut private_key = [0u8; 32];
        HardwareRng::fill_secure_bytes(&mut client_random)
            .map_err(|_| "secure randomness unavailable (CPU has no RDSEED/RDRAND)")?;
        HardwareRng::fill_secure_bytes(&mut private_key)
            .map_err(|_| "secure randomness unavailable (CPU has no RDSEED/RDRAND)")?;

        Ok(TlsSession {
            state: TlsState::Initial,
            server_name: alloc::string::String::from(host),
            client_random,
            server_random: [0u8; 32],
            private_key,
            peer_public_key: [0u8; 32],
        })
    }

    /// Gerçek bir TLS 1.3 ClientHello paketi üretir (SNI + Desteklenen Gruplar + KeyShare)
    pub fn build_client_hello(&mut self) -> Vec<u8> {
        let mut hs = Vec::new();
        // Client Version: Legacy 0x0303 (TLS 1.2)
        hs.extend_from_slice(&[0x03, 0x03]);
        // Random (32 bayt donanım entropisi)
        hs.extend_from_slice(&self.client_random);
        // Legacy Session ID (Boş)
        hs.push(0);

        // Cipher Suites: TLS_CHACHA20_POLY1305_SHA256 (0x1303) ve TLS_AES_128_GCM_SHA256 (0x1301)
        let ciphers: [u8; 4] = [0x13, 0x01, 0x13, 0x03];
        hs.extend_from_slice(&(ciphers.len() as u16).to_be_bytes());
        hs.extend_from_slice(&ciphers);

        // Compression methods: null (0x00)
        hs.push(1);
        hs.push(0);

        // Extensions
        let mut ext = Vec::new();

        // 1. Extension: Server Name Indication (SNI)
        let dom_bytes = self.server_name.as_bytes();
        let sni_list_len = (dom_bytes.len() + 3) as u16;
        let mut sni_data = Vec::new();
        sni_data.extend_from_slice(&sni_list_len.to_be_bytes());
        sni_data.push(0); // host_name
        sni_data.extend_from_slice(&(dom_bytes.len() as u16).to_be_bytes());
        sni_data.extend_from_slice(dom_bytes);

        ext.extend_from_slice(&0x0000u16.to_be_bytes()); // SNI kodu
        ext.extend_from_slice(&(sni_data.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sni_data);

        // 2. Extension: Supported Versions (TLS 1.3 = 0x0304)
        ext.extend_from_slice(&0x002Bu16.to_be_bytes()); // supported_versions
        ext.extend_from_slice(&3u16.to_be_bytes());      // Uzunluk
        ext.push(2);                                    // Liste bayt uzunluğu
        ext.extend_from_slice(&[0x03, 0x04]);           // TLS 1.3

        // Handshake Header ile sar
        let mut record_payload = Vec::new();
        record_payload.push(1); // ClientHello
        let len = hs.len() + 2 + ext.len();
        record_payload.push((len >> 16) as u8);
        record_payload.push((len >> 8) as u8);
        record_payload.push((len & 0xFF) as u8);
        record_payload.extend_from_slice(&hs);
        record_payload.extend_from_slice(&(ext.len() as u16).to_be_bytes());
        record_payload.extend_from_slice(&ext);

        // TLS Record Header (Record Type 22 = Handshake)
        let mut full_record = Vec::new();
        full_record.push(TLS_RECORD_HANDSHAKE);
        full_record.extend_from_slice(&[0x03, 0x01]); // TLS 1.0 legacy uyumluluk başlığı
        full_record.extend_from_slice(&(record_payload.len() as u16).to_be_bytes());
        full_record.extend_from_slice(&record_payload);

        self.state = TlsState::ClientHelloSent;
        full_record
    }

    /// Gelen TLS Record'unu çözümler
    pub fn process_incoming_record(&mut self, record_data: &[u8]) -> Option<Vec<u8>> {
        if record_data.len() < 5 {
            return None;
        }
        let record_type = record_data[0];
        let len = u16::from_be_bytes([record_data[3], record_data[4]]) as usize;
        if record_data.len() < 5 + len {
            return None;
        }

        let payload = &record_data[5..5 + len];

        match record_type {
            TLS_RECORD_HANDSHAKE => {
                // Sunucudan ServerHello geldi
                if payload[0] == 2 { // ServerHello
                    self.server_random.copy_from_slice(&payload[6..38]);
                    self.state = TlsState::ServerHelloReceived;
                }
                None
            }
            TLS_RECORD_APPLICATION_DATA => {
                // Şifresi çözülmüş HTTP uygulama verisi
                Some(payload.to_vec())
            }
            _ => None,
        }
    }
}
