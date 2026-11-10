use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub use nc_crypto::lock::{
    secret_is_valid, LockRecord, LockState, KIND_PASSWORD, KIND_PIN, PASSWORD_MAX, PASSWORD_MIN,
    PIN_LENGTH,
};

pub struct LockRequest {
    pub kind: u8,
    pub secret: Vec<u8>,
}

struct Inner {
    state: LockState,
    request: Option<LockRequest>,
}

struct Slot {
    lock: AtomicBool,
    inner: UnsafeCell<Inner>,
}

unsafe impl Sync for Slot {}

static SLOT: Slot = Slot {
    lock: AtomicBool::new(false),
    inner: UnsafeCell::new(Inner { state: LockState::Disabled, request: None }),
};

fn locked<R>(run: impl FnOnce(&mut Inner) -> R) -> R {
    while SLOT
        .lock
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = run(unsafe { &mut *SLOT.inner.get() });
    SLOT.lock.store(false, Ordering::Release);
    result
}

pub fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        unsafe { core::ptr::write_volatile(byte, 0) };
    }
}

pub fn publish(state: LockState) {
    locked(|inner| inner.state = state);
}

pub fn state() -> LockState {
    locked(|inner| inner.state)
}

pub fn kind() -> u8 {
    match state() {
        LockState::Enabled { record, .. } => record.kind,
        _ => 0,
    }
}

pub fn verify_current(secret: &[u8]) -> bool {
    match state() {
        LockState::Enabled { record, .. } => record.verify(secret),
        LockState::Disabled => true,
        LockState::Destroyed => false,
    }
}

pub fn submit(kind: u8, secret: Vec<u8>) {
    locked(|inner| inner.request = Some(LockRequest { kind, secret }));
}

pub fn take_request() -> Option<LockRequest> {
    locked(|inner| inner.request.take())
}
