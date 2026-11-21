//! EL0 DysonSphere IPC Wire Protocol & Fail-Closed Kernel Reference Monitor (Feature 13).
//!
//! Defines the bounded request/response wire format between the kernel reference
//! monitor and an isolated EL0 DysonSphere security server, plus a bounded
//! policy-generation-tracked authorization cache that ALWAYS fails closed on
//! server offline, timeout, ABI mismatch, or malformed reply.

pub const DS_IPC_MAGIC: u32 = 0x5344_434E; // "NCDS"
pub const DS_IPC_ABI_VERSION: u16 = 1;
pub const DS_REQ_SIZE: usize = 64;
pub const DS_RESP_SIZE: usize = 32;
pub const DS_CACHE_SLOTS: usize = 32;

pub const DS_DECISION_ALLOW: u32 = 0;
pub const DS_DECISION_DENY_POLICY_LOCKED: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DsAuthRequest {
    pub abi_version: u16,
    pub flags: u16,
    pub request_id: u32,
    pub policy_gen: u32,
    pub caller_slot: u32,
    pub requested_cap: u64,
    pub caller_id_len: u8,
    pub caller_id: [u8; 32],
}

impl DsAuthRequest {
    pub fn encode(&self) -> [u8; DS_REQ_SIZE] {
        let mut out = [0u8; DS_REQ_SIZE];
        out[0..4].copy_from_slice(&DS_IPC_MAGIC.to_le_bytes());
        out[4..6].copy_from_slice(&self.abi_version.to_le_bytes());
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..12].copy_from_slice(&self.request_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.policy_gen.to_le_bytes());
        out[16..20].copy_from_slice(&self.caller_slot.to_le_bytes());
        out[20..24].copy_from_slice(&0u32.to_le_bytes());
        out[24..32].copy_from_slice(&self.requested_cap.to_le_bytes());
        // Byte 31 is part of requested_cap; store id_len in flags high byte if needed, or pack cleanly:
        // Wait: 32 + 32 = 64! Put caller_id_len at offset 20 (reserved slot).
        out[20] = self.caller_id_len.min(32);
        out[32..64].copy_from_slice(&self.caller_id);
        out
    }

    pub fn decode(raw: &[u8]) -> Option<Self> {
        if raw.len() != DS_REQ_SIZE {
            return None;
        }
        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let abi_version = u16::from_le_bytes([raw[4], raw[5]]);
        if magic != DS_IPC_MAGIC || abi_version != DS_IPC_ABI_VERSION {
            return None;
        }
        let caller_id_len = raw[20];
        if caller_id_len == 0 || caller_id_len > 32 {
            return None;
        }
        let mut caller_id = [0u8; 32];
        caller_id.copy_from_slice(&raw[32..64]);
        Some(Self {
            abi_version,
            flags: u16::from_le_bytes([raw[6], raw[7]]),
            request_id: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            policy_gen: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
            caller_slot: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
            requested_cap: u64::from_le_bytes([
                raw[24], raw[25], raw[26], raw[27], raw[28], raw[29], raw[30], raw[31],
            ]),
            caller_id_len,
            caller_id,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DsAuthResponse {
    pub abi_version: u16,
    pub request_id: u32,
    pub policy_gen: u32,
    pub caller_slot: u32,
    pub requested_cap: u64,
    pub decision: u32,
}

impl DsAuthResponse {
    pub fn encode(&self) -> [u8; DS_RESP_SIZE] {
        let mut out = [0u8; DS_RESP_SIZE];
        out[0..4].copy_from_slice(&DS_IPC_MAGIC.to_le_bytes());
        out[4..6].copy_from_slice(&self.abi_version.to_le_bytes());
        out[8..12].copy_from_slice(&self.request_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.policy_gen.to_le_bytes());
        out[16..20].copy_from_slice(&self.caller_slot.to_le_bytes());
        out[20..24].copy_from_slice(&self.decision.to_le_bytes());
        out[24..32].copy_from_slice(&self.requested_cap.to_le_bytes());
        out
    }

    pub fn decode(raw: &[u8]) -> Option<Self> {
        if raw.len() != DS_RESP_SIZE {
            return None;
        }
        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let abi_version = u16::from_le_bytes([raw[4], raw[5]]);
        if magic != DS_IPC_MAGIC || abi_version != DS_IPC_ABI_VERSION {
            return None;
        }
        Some(Self {
            abi_version,
            request_id: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            policy_gen: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
            caller_slot: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
            decision: u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]),
            requested_cap: u64::from_le_bytes([
                raw[24], raw[25], raw[26], raw[27], raw[28], raw[29], raw[30], raw[31],
            ]),
        })
    }
}

#[derive(Clone, Copy)]
struct CacheEntry {
    valid: bool,
    policy_gen: u32,
    caller_slot: u32,
    cap: u64,
    decision: u32,
}

impl CacheEntry {
    const EMPTY: Self = Self {
        valid: false,
        policy_gen: 0,
        caller_slot: 0,
        cap: 0,
        decision: DS_DECISION_DENY_POLICY_LOCKED,
    };
}

/// Kernel-resident fail-closed reference monitor cache for EL0 DysonSphere.
pub struct ReferenceMonitor {
    service_online: bool,
    policy_gen: u32,
    next_cursor: usize,
    cache: [CacheEntry; DS_CACHE_SLOTS],
}

impl ReferenceMonitor {
    pub const fn new() -> Self {
        Self {
            service_online: false,
            policy_gen: 1,
            next_cursor: 0,
            cache: [CacheEntry::EMPTY; DS_CACHE_SLOTS],
        }
    }

    pub fn set_service_online(&mut self, online: bool) {
        self.service_online = online;
        if !online {
            self.invalidate_all();
        }
    }

    pub fn bump_policy_generation(&mut self) {
        self.policy_gen = self.policy_gen.wrapping_add(1).max(1);
        self.invalidate_all();
    }

    pub fn invalidate_all(&mut self) {
        self.cache = [CacheEntry::EMPTY; DS_CACHE_SLOTS];
    }

    pub fn lookup_cached(&self, caller_slot: u32, cap: u64) -> Option<u32> {
        if !self.service_online {
            return Some(DS_DECISION_DENY_POLICY_LOCKED);
        }
        for e in self.cache.iter() {
            if e.valid && e.policy_gen == self.policy_gen && e.caller_slot == caller_slot && e.cap == cap {
                return Some(e.decision);
            }
        }
        None
    }

    /// Validate an EL0 DysonSphere reply against the kernel's original request.
    /// Always fails closed (`DS_DECISION_DENY_POLICY_LOCKED`) on any mismatch,
    /// ABI error, stale policy generation, or offline service.
    pub fn validate_and_cache(&mut self, req: &DsAuthRequest, raw_resp: Option<&[u8]>) -> u32 {
        if !self.service_online {
            return DS_DECISION_DENY_POLICY_LOCKED;
        }
        let Some(bytes) = raw_resp else {
            return DS_DECISION_DENY_POLICY_LOCKED;
        };
        let Some(resp) = DsAuthResponse::decode(bytes) else {
            return DS_DECISION_DENY_POLICY_LOCKED;
        };
        if resp.request_id != req.request_id
            || resp.policy_gen != self.policy_gen
            || resp.caller_slot != req.caller_slot
            || resp.requested_cap != req.requested_cap
        {
            return DS_DECISION_DENY_POLICY_LOCKED;
        }
        let slot = self.next_cursor % DS_CACHE_SLOTS;
        self.next_cursor = self.next_cursor.wrapping_add(1);
        self.cache[slot] = CacheEntry {
            valid: true,
            policy_gen: self.policy_gen,
            caller_slot: req.caller_slot,
            cap: req.requested_cap,
            decision: resp.decision,
        };
        resp.decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dysonsphere_ipc_fails_closed_on_crash_spoof_or_revocation() {
        let mut mon = ReferenceMonitor::new();
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(b"nc.files");
        let req = DsAuthRequest {
            abi_version: DS_IPC_ABI_VERSION,
            flags: 0,
            request_id: 77,
            policy_gen: 1,
            caller_slot: 3,
            requested_cap: 1 << 2,
            caller_id_len: 8,
            caller_id: id,
        };

        // Wire roundtrip.
        assert_eq!(DsAuthRequest::decode(&req.encode()), Some(req));

        // 1. Offline service ALWAYS fails closed.
        assert_eq!(mon.lookup_cached(3, 1 << 2), Some(DS_DECISION_DENY_POLICY_LOCKED));

        // 2. Online service valid reply is accepted & cached.
        mon.set_service_online(true);
        let resp = DsAuthResponse {
            abi_version: DS_IPC_ABI_VERSION,
            request_id: 77,
            policy_gen: 1,
            caller_slot: 3,
            requested_cap: 1 << 2,
            decision: DS_DECISION_ALLOW,
        };
        assert_eq!(mon.validate_and_cache(&req, Some(&resp.encode())), DS_DECISION_ALLOW);
        assert_eq!(mon.lookup_cached(3, 1 << 2), Some(DS_DECISION_ALLOW));

        // 3. Policy revocation invalidates cache immediately.
        mon.bump_policy_generation();
        assert_eq!(mon.lookup_cached(3, 1 << 2), None);

        // 4. Stale policy_gen or mismatched request_id fails closed.
        assert_eq!(mon.validate_and_cache(&req, Some(&resp.encode())), DS_DECISION_DENY_POLICY_LOCKED);

        // 5. Crash / timeout (None reply) fails closed.
        assert_eq!(mon.validate_and_cache(&req, None), DS_DECISION_DENY_POLICY_LOCKED);
    }
}
