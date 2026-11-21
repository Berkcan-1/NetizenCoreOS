//! Bounded Address-Space-Scoped Futex Wait Queue (Feature 6).
//!
//! Ensures threads within the same process (matching `space_id`) can wait/wake
//! on aligned user-space 32-bit futex words, while strictly isolating different
//! processes from waking or observing each other's futexes.

pub const MAX_FUTEX_WAITERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutexError {
    InvalidAddress,
    QueueFull,
    ValueMismatch,
}

#[derive(Clone, Copy)]
struct Waiter {
    in_use: bool,
    space_id: u64,
    uaddr: u64,
    thread_id: u32,
    deadline_tick: u64, // 0 = no timeout
}

impl Waiter {
    const EMPTY: Self = Self {
        in_use: false,
        space_id: 0,
        uaddr: 0,
        thread_id: 0,
        deadline_tick: 0,
    };
}

pub struct FutexTable {
    waiters: [Waiter; MAX_FUTEX_WAITERS],
}

impl FutexTable {
    pub const fn new() -> Self {
        Self {
            waiters: [Waiter::EMPTY; MAX_FUTEX_WAITERS],
        }
    }

    pub fn enqueue(
        &mut self,
        space_id: u64,
        uaddr: u64,
        current_val: u32,
        expected_val: u32,
        thread_id: u32,
        deadline_tick: u64,
    ) -> Result<(), FutexError> {
        if uaddr == 0 || (uaddr & 3) != 0 {
            return Err(FutexError::InvalidAddress);
        }
        if current_val != expected_val {
            return Err(FutexError::ValueMismatch);
        }
        let slot = self
            .waiters
            .iter_mut()
            .find(|w| !w.in_use)
            .ok_or(FutexError::QueueFull)?;
        *slot = Waiter {
            in_use: true,
            space_id,
            uaddr,
            thread_id,
            deadline_tick,
        };
        Ok(())
    }

    /// Wake up to `max_wake` waiters matching `(space_id, uaddr)`.
    /// Writes woken thread IDs into `out` and returns the count woken.
    pub fn wake(&mut self, space_id: u64, uaddr: u64, max_wake: usize, out: &mut [u32]) -> usize {
        if uaddr == 0 || (uaddr & 3) != 0 || max_wake == 0 {
            return 0;
        }
        let limit = max_wake.min(out.len());
        let mut woken = 0usize;
        for w in self.waiters.iter_mut() {
            if woken >= limit {
                break;
            }
            if w.in_use && w.space_id == space_id && w.uaddr == uaddr {
                out[woken] = w.thread_id;
                *w = Waiter::EMPTY;
                woken += 1;
            }
        }
        woken
    }

    /// Expire timed-out waiters at `now_tick` and return woken thread IDs.
    pub fn expire_timeouts(&mut self, now_tick: u64, out: &mut [u32]) -> usize {
        let mut expired = 0usize;
        for w in self.waiters.iter_mut() {
            if expired >= out.len() {
                break;
            }
            if w.in_use && w.deadline_tick != 0 && now_tick >= w.deadline_tick {
                out[expired] = w.thread_id;
                *w = Waiter::EMPTY;
                expired += 1;
            }
        }
        expired
    }

    /// Remove all waiters belonging to a terminated thread or process.
    pub fn remove_thread(&mut self, thread_id: u32) {
        for w in self.waiters.iter_mut() {
            if w.in_use && w.thread_id == thread_id {
                *w = Waiter::EMPTY;
            }
        }
    }

    pub fn remove_space(&mut self, space_id: u64) {
        for w in self.waiters.iter_mut() {
            if w.in_use && w.space_id == space_id {
                *w = Waiter::EMPTY;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn futex_wait_wake_timeout_and_cross_process_isolation() {
        let mut ft = FutexTable::new();
        let addr = 0x80_0100_0004u64;

        // Unaligned address rejected.
        assert_eq!(ft.enqueue(1, addr + 1, 0, 0, 101, 0), Err(FutexError::InvalidAddress));
        // Value mismatch rejected.
        assert_eq!(ft.enqueue(1, addr, 1, 0, 101, 0), Err(FutexError::ValueMismatch));

        // Two threads in space 1 wait on `addr`; one with timeout at tick 50.
        ft.enqueue(1, addr, 0, 0, 101, 0).unwrap();
        ft.enqueue(1, addr, 0, 0, 102, 50).unwrap();

        // Different process (space 2) CANNOT wake space 1's waiters!
        let mut woken = [0u32; 4];
        assert_eq!(ft.wake(2, addr, 2, &mut woken), 0);

        // Space 1 wakes 1 waiter (thread 101).
        assert_eq!(ft.wake(1, addr, 1, &mut woken), 1);
        assert_eq!(woken[0], 101);

        // Thread 102 times out at tick 50.
        assert_eq!(ft.expire_timeouts(49, &mut woken), 0);
        assert_eq!(ft.expire_timeouts(50, &mut woken), 1);
        assert_eq!(woken[0], 102);
    }
}
