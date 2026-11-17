use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const QUEUE_LIMIT: usize = 24;
pub const PER_APP_LIMIT: usize = 6;
pub const MAX_APPS: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    pub fn from_u32(value: u32) -> Level {
        match value {
            1 => Level::Success,
            2 => Level::Warning,
            3 => Level::Error,
            _ => Level::Info,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    Ask,
    Allowed,
    Denied,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub app_id: String,
    pub title: String,
    pub body: String,
    pub level: Level,
}

struct State {
    queue: Vec<Notification>,
    permissions: Vec<(String, Permission)>,
}

struct Slot {
    lock: AtomicBool,
    value: UnsafeCell<State>,
}

unsafe impl Sync for Slot {}

static SLOT: Slot = Slot {
    lock: AtomicBool::new(false),
    value: UnsafeCell::new(State { queue: Vec::new(), permissions: Vec::new() }),
};

fn with<R>(run: impl FnOnce(&mut State) -> R) -> R {
    while SLOT.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let result = run(unsafe { &mut *SLOT.value.get() });
    SLOT.lock.store(false, Ordering::Release);
    result
}

pub fn permission(app_id: &str) -> Permission {
    with(|state| {
        state
            .permissions
            .iter()
            .find(|(id, _)| id == app_id)
            .map(|(_, permission)| *permission)
            .unwrap_or(Permission::Ask)
    })
}

pub fn set_permission(app_id: &str, permission: Permission) {
    with(|state| {
        if let Some(entry) = state.permissions.iter_mut().find(|(id, _)| id == app_id) {
            entry.1 = permission;
            return;
        }
        if state.permissions.len() < MAX_APPS && permission != Permission::Ask {
            state.permissions.push((String::from(app_id), permission));
        }
    })
}

pub fn forget_permission(app_id: &str) {
    with(|state| state.permissions.retain(|(id, _)| id != app_id))
}

pub fn permissions() -> Vec<(String, Permission)> {
    with(|state| state.permissions.clone())
}

pub fn post(notification: Notification) -> bool {
    with(|state| {
        if state.queue.len() >= QUEUE_LIMIT {
            return false;
        }
        if state.queue.iter().filter(|queued| queued.app_id == notification.app_id).count() >= PER_APP_LIMIT {
            return false;
        }
        state.queue.push(notification);
        true
    })
}

pub fn drain() -> Vec<Notification> {
    with(|state| core::mem::take(&mut state.queue))
}

pub fn pending() -> usize {
    with(|state| state.queue.len())
}
