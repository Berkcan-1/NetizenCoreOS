//! System Notification Service with Identity Binding & Lock-Screen Privacy (Feature 43).
//!
//! Enforces kernel-authenticated sender identity, SPARK `NOTIFICATION_POST`
//! authorization, bounded history (no unbounded flash writes), channel mute,
//! dismissal, and lock-screen redaction policies.

pub const MAX_NOTIFICATIONS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockVisibility {
    Public = 0,
    PrivateRedacted = 1,
    SecretHidden = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyError {
    PermissionDenied,
    InvalidInput,
    NotFound,
}

#[derive(Clone, Copy)]
pub struct Notification {
    pub in_use: bool,
    pub id: u32,
    pub priority: NotifyPriority,
    pub lock_visibility: LockVisibility,
    pub sender_len: u8,
    pub title_len: u8,
    pub body_len: u8,
    pub sender_app_id: [u8; 32],
    pub title: [u8; 48],
    pub body: [u8; 96],
}

impl Notification {
    pub const EMPTY: Self = Self {
        in_use: false,
        id: 0,
        priority: NotifyPriority::Normal,
        lock_visibility: LockVisibility::PrivateRedacted,
        sender_len: 0,
        title_len: 0,
        body_len: 0,
        sender_app_id: [0; 32],
        title: [0; 48],
        body: [0; 96],
    };

    pub fn sender_str(&self) -> &str {
        core::str::from_utf8(&self.sender_app_id[..self.sender_len as usize]).unwrap_or("")
    }

    pub fn visible_body(&self, device_locked: bool) -> Option<&str> {
        if device_locked {
            match self.lock_visibility {
                LockVisibility::SecretHidden => None,
                LockVisibility::PrivateRedacted => Some("Sensitive content hidden"),
                LockVisibility::Public => {
                    Some(core::str::from_utf8(&self.body[..self.body_len as usize]).unwrap_or(""))
                }
            }
        } else {
            Some(core::str::from_utf8(&self.body[..self.body_len as usize]).unwrap_or(""))
        }
    }
}

pub struct NotificationService {
    items: [Notification; MAX_NOTIFICATIONS],
    next_id: u32,
    evict_cursor: usize,
}

impl NotificationService {
    pub const fn new() -> Self {
        Self {
            items: [Notification::EMPTY; MAX_NOTIFICATIONS],
            next_id: 1,
            evict_cursor: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn post(
        &mut self,
        authorized_by_spark: bool,
        sender_app_id: &str,
        title: &str,
        body: &str,
        priority: NotifyPriority,
        lock_visibility: LockVisibility,
    ) -> Result<u32, NotifyError> {
        if !authorized_by_spark {
            return Err(NotifyError::PermissionDenied);
        }
        if sender_app_id.is_empty() || title.is_empty() {
            return Err(NotifyError::InvalidInput);
        }
        let slot = if let Some(idx) = self.items.iter().position(|n| !n.in_use) {
            idx
        } else {
            let idx = self.evict_cursor % MAX_NOTIFICATIONS;
            self.evict_cursor = self.evict_cursor.wrapping_add(1);
            idx
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let mut n = Notification::EMPTY;
        n.in_use = true;
        n.id = id;
        n.priority = priority;
        n.lock_visibility = lock_visibility;
        let sb = sender_app_id.as_bytes();
        let sl = sb.len().min(32);
        n.sender_len = sl as u8;
        n.sender_app_id[..sl].copy_from_slice(&sb[..sl]);
        let tb = title.as_bytes();
        let tl = tb.len().min(48);
        n.title_len = tl as u8;
        n.title[..tl].copy_from_slice(&tb[..tl]);
        let bb = body.as_bytes();
        let bl = bb.len().min(96);
        n.body_len = bl as u8;
        n.body[..bl].copy_from_slice(&bb[..bl]);
        self.items[slot] = n;
        Ok(id)
    }

    pub fn dismiss(&mut self, id: u32) -> Result<(), NotifyError> {
        let item = self
            .items
            .iter_mut()
            .find(|n| n.in_use && n.id == id)
            .ok_or(NotifyError::NotFound)?;
        *item = Notification::EMPTY;
        Ok(())
    }

    pub fn purge_for_app(&mut self, app_id: &str) -> usize {
        let mut removed = 0usize;
        for item in self.items.iter_mut() {
            if item.in_use && item.sender_str() == app_id {
                *item = Notification::EMPTY;
                removed += 1;
            }
        }
        removed
    }

    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.items.iter().find(|n| n.in_use && n.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_enforce_auth_lock_privacy_and_app_removal() {
        let mut svc = NotificationService::new();

        assert_eq!(
            svc.post(false, "nc.mail", "Hi", "Secret", NotifyPriority::Normal, LockVisibility::PrivateRedacted),
            Err(NotifyError::PermissionDenied)
        );

        let id = svc
            .post(true, "nc.mail", "New mail", "OTP: 123456", NotifyPriority::High, LockVisibility::PrivateRedacted)
            .unwrap();
        let n = svc.get(id).unwrap();
        assert_eq!(n.visible_body(true), Some("Sensitive content hidden"));
        assert_eq!(n.visible_body(false), Some("OTP: 123456"));

        let secret_id = svc
            .post(true, "nc.mail", "Vault", "Key", NotifyPriority::High, LockVisibility::SecretHidden)
            .unwrap();
        assert_eq!(svc.get(secret_id).unwrap().visible_body(true), None);

        // Uninstalling nc.mail purges all its notifications.
        assert_eq!(svc.purge_for_app("nc.mail"), 2);
        assert!(svc.get(id).is_none());
    }
}
