//! Portable Network Device, Wi-Fi Adapter, and Per-Application Network Firewall HAL
//! (Features 28, 29, 32, 33).
//!
//! Strictly enforces honest hardware reporting: when no 802.11 Wi-Fi radio is
//! present (e.g. QEMU `virtio-net`), Wi-Fi operations return
//! `Err(NetError::HardwareUnavailable)` and `WifiAdapterState::HardwareUnavailable`.

pub const MAX_WIFI_SCAN_RESULTS: usize = 16;
pub const MAX_FIREWALL_APPS: usize = 64;

pub const NET_CAP_INTERNET: u32 = 1 << 0;
pub const NET_CAP_LOCAL: u32 = 1 << 1;
pub const NET_CAP_DNS: u32 = 1 << 2;
pub const NET_CAP_LISTEN: u32 = 1 << 3;
pub const NET_CAP_BACKGROUND: u32 = 1 << 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    HardwareUnavailable,
    LinkDown,
    BufferTooSmall,
    QueueFull,
    PermissionDenied,
    Timeout,
    InvalidFrame,
}

/// Architecture-independent Ethernet/packet network device abstraction.
pub trait NetDevice {
    fn mac_address(&self) -> [u8; 6];
    fn link_up(&self) -> bool;
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), NetError>;
    fn recv_frame(&mut self, out: &mut [u8]) -> Option<usize>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiSecurity {
    Open = 0,
    Wpa2Psk = 1,
    Wpa3Sae = 2,
    Enterprise = 3,
    Unknown = 255,
}

impl WifiSecurity {
    pub const fn as_str(self) -> &'static str {
        match self {
            WifiSecurity::Open => "OPEN",
            WifiSecurity::Wpa2Psk => "WPA2",
            WifiSecurity::Wpa3Sae => "WPA3",
            WifiSecurity::Enterprise => "EAP",
            WifiSecurity::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiAdapterState {
    HardwareUnavailable = 0,
    Disabled = 1,
    Disconnected = 2,
    Scanning = 3,
    Connected = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WifiScanEntry {
    pub ssid: [u8; 32],
    pub ssid_len: u8,
    pub bssid: [u8; 6],
    pub rssi_dbm: i8,
    pub channel: u8,
    pub security: WifiSecurity,
}

impl WifiScanEntry {
    pub const EMPTY: Self = Self {
        ssid: [0; 32],
        ssid_len: 0,
        bssid: [0; 6],
        rssi_dbm: -100,
        channel: 0,
        security: WifiSecurity::Unknown,
    };

    pub fn ssid_str(&self) -> &str {
        let len = (self.ssid_len as usize).min(32);
        core::str::from_utf8(&self.ssid[..len]).unwrap_or("<non-utf8>")
    }
}

/// Hardware-backed 802.11 Wi-Fi radio adapter abstraction.
/// Implementations MUST NOT fabricate SSIDs or fake connections when hardware is absent.
pub trait WifiAdapter {
    fn state(&self) -> WifiAdapterState;
    fn scan(&mut self, out: &mut [WifiScanEntry]) -> Result<usize, NetError>;
    fn disconnect(&mut self) -> Result<(), NetError>;
}

/// Default-deny per-application network firewall bound to kernel-authenticated app IDs (Feature 33).
#[derive(Clone, Copy)]
struct FirewallRule {
    in_use: bool,
    app_len: u8,
    app_id: [u8; 32],
    allowed_caps: u32,
    background_allowed: bool,
}

impl FirewallRule {
    const EMPTY: Self = Self {
        in_use: false,
        app_len: 0,
        app_id: [0; 32],
        allowed_caps: 0,
        background_allowed: false,
    };

    fn matches(&self, app_id: &str) -> bool {
        let b = app_id.as_bytes();
        self.app_len as usize == b.len() && &self.app_id[..b.len()] == b
    }
}

pub struct PerAppNetFirewall {
    rules: [FirewallRule; MAX_FIREWALL_APPS],
    denied_count: u64,
}

impl PerAppNetFirewall {
    pub const fn new() -> Self {
        Self {
            rules: [FirewallRule::EMPTY; MAX_FIREWALL_APPS],
            denied_count: 0,
        }
    }

    pub fn set_policy(
        &mut self,
        app_id: &str,
        allowed_caps: u32,
        background_allowed: bool,
    ) -> Result<(), NetError> {
        let b = app_id.as_bytes();
        if b.is_empty() || b.len() > 32 {
            return Err(NetError::InvalidFrame);
        }
        if let Some(r) = self.rules.iter_mut().find(|r| r.in_use && r.matches(app_id)) {
            r.allowed_caps = allowed_caps;
            r.background_allowed = background_allowed;
            return Ok(());
        }
        let slot = self.rules.iter_mut().find(|r| !r.in_use).ok_or(NetError::QueueFull)?;
        *slot = FirewallRule::EMPTY;
        slot.in_use = true;
        slot.app_len = b.len() as u8;
        slot.app_id[..b.len()].copy_from_slice(b);
        slot.allowed_caps = allowed_caps;
        slot.background_allowed = background_allowed;
        Ok(())
    }

    pub fn revoke(&mut self, app_id: &str) {
        for r in self.rules.iter_mut() {
            if r.in_use && r.matches(app_id) {
                r.allowed_caps = 0;
                r.background_allowed = false;
            }
        }
    }

    /// Authorize a network operation for kernel-authenticated `caller_app_id`.
    /// Default-deny: unknown apps, revoked apps, or background ops without
    /// `background_allowed` are rejected and counted.
    pub fn authorize(&mut self, caller_app_id: &str, req_cap: u32, is_background: bool) -> bool {
        if let Some(r) = self.rules.iter().find(|r| r.in_use && r.matches(caller_app_id)) {
            let cap_ok = (r.allowed_caps & req_cap) == req_cap && req_cap != 0;
            let bg_ok = !is_background || r.background_allowed;
            if cap_ok && bg_ok {
                return true;
            }
        }
        self.denied_count = self.denied_count.saturating_add(1);
        false
    }

    pub fn denied_count(&self) -> u64 {
        self.denied_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firewall_is_default_deny_and_enforces_background_and_revocation() {
        let mut fw = PerAppNetFirewall::new();

        // Unknown app denied by default.
        assert!(!fw.authorize("nc.unknown", NET_CAP_INTERNET, false));
        assert_eq!(fw.denied_count(), 1);

        // Grant foreground internet + DNS to browser, deny background.
        fw.set_policy("nc.browser", NET_CAP_INTERNET | NET_CAP_DNS, false).unwrap();
        assert!(fw.authorize("nc.browser", NET_CAP_INTERNET, false));
        assert!(fw.authorize("nc.browser", NET_CAP_DNS, false));
        assert!(!fw.authorize("nc.browser", NET_CAP_LISTEN, false));
        assert!(!fw.authorize("nc.browser", NET_CAP_INTERNET, true));

        // Revocation denies immediately.
        fw.revoke("nc.browser");
        assert!(!fw.authorize("nc.browser", NET_CAP_INTERNET, false));
    }
}
