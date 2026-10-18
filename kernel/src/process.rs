// kernel/src/process.rs

extern crate alloc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use nc_arch::idt::{switch_context, task_entry_trampoline, TaskContext, IRQ_TIMER_TICKS};

const KERNEL_STACK_SIZE: usize = 32 * 1024; // Poller gorevleri icin L1/L2 dostu 32 KB yigin
const STACK_CANARY: u64 = 0xDEAD_BEEF_CAFE_BABE;

static mut GLOBAL_PM: *mut ProcessManager = core::ptr::null_mut();
static IN_SCHEDULER: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcessState {
    Ready,
    Running,
    Sleeping(u64),
    #[allow(dead_code)]
    Stopped,
}

pub struct ProcessControlBlock {
    #[allow(dead_code)]
    pub pid: usize,
    pub name: String,
    pub state: ProcessState,
    pub ticks: u64,
    pub rsp: u64,
    stack: Vec<u8>,
}

impl ProcessControlBlock {
    #[inline]
    pub fn check_canary(&self) -> bool {
        if self.stack.len() < 8 {
            return true; // PID 1 ana cekirdek yigini
        }
        let bottom_ptr = self.stack.as_ptr() as *const u64;
        unsafe { core::ptr::read_unaligned(bottom_ptr) == STACK_CANARY }
    }
}

pub struct ProcessManager {
    pub processes: Vec<ProcessControlBlock>,
    pub current: usize,
    next_pid: usize,
}

/// Arka plan Ag Servisi (Her yoklamadan sonra 10 ms uyur, CPU tuketmez)
extern "C" fn net_poller_thread() -> ! {
    loop {
        nc_net::global().poll();
        sleep_ticks(10);
    }
}

/// Arka plan Depolama / Kesme Izleme Servisi (100 ms periyotla uyur)
extern "C" fn storage_monitor_thread() -> ! {
    loop {
        sleep_ticks(100);
    }
}

#[allow(dead_code)]
pub fn yield_now() {
    unsafe {
        if !GLOBAL_PM.is_null() {
            ProcessManager::schedule_raw(GLOBAL_PM);
        }
    }
}

pub fn sleep_ticks(ticks: u64) {
    unsafe {
        if let Some(pm) = GLOBAL_PM.as_mut() {
            let now = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
            let cur = pm.current;
            pm.processes[cur].state = ProcessState::Sleeping(now.wrapping_add(ticks));
            ProcessManager::schedule_raw(GLOBAL_PM);
        }
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        let mut pm = ProcessManager {
            processes: Vec::new(),
            current: 0,
            next_pid: 1,
        };
        pm.processes.push(ProcessControlBlock {
            pid: 1,
            name: String::from("SystemCompositor"),
            state: ProcessState::Running,
            ticks: 0,
            rsp: 0,
            stack: Vec::new(),
        });
        pm.next_pid = 2;
        pm.spawn("NetPollerService", net_poller_thread);
        pm.spawn("StorageMonitor", storage_monitor_thread);
        pm
    }

    pub fn spawn(&mut self, name: &str, entry: extern "C" fn() -> !) -> usize {
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut stack = vec![0u8; KERNEL_STACK_SIZE];

        // PAIR-031: Stack tabanina canary yaz
        unsafe {
            core::ptr::write_unaligned(stack.as_mut_ptr() as *mut u64, STACK_CANARY);
        }

        // PAIR-019: Trampoline uzerinden System V AMD64 ABI uyumlu baslatma
        let stack_top = (stack.as_mut_ptr() as usize + KERNEL_STACK_SIZE) & !0xF;
        let ctx_ptr = (stack_top - core::mem::size_of::<TaskContext>()) as *mut TaskContext;
        unsafe {
            *ctx_ptr = TaskContext {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: entry as *const () as usize as u64,
                rbx: 0,
                rbp: 0,
                rip: task_entry_trampoline as *const () as usize as u64,
            };
        }

        self.processes.push(ProcessControlBlock {
            pid,
            name: String::from(name),
            state: ProcessState::Ready,
            ticks: 0,
            rsp: ctx_ptr as u64,
            stack,
        });
        pid
    }

    /// PAIR-018: Acik `as_mut()` referansi ve atomik kilit ile guvenli zamanlama
    pub unsafe fn schedule_raw(pm: *mut ProcessManager) {
        if IN_SCHEDULER
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let mgr = match pm.as_mut() {
            Some(m) => m,
            None => {
                IN_SCHEDULER.store(false, Ordering::Release);
                return;
            }
        };

        let count = mgr.processes.len();
        if count <= 1 {
            IN_SCHEDULER.store(false, Ordering::Release);
            return;
        }

        let now_ticks = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
        for proc in mgr.processes.iter_mut() {
            if !proc.check_canary() {
                panic!("Kernel gorev yigini tasti (Stack Overflow): {}", proc.name);
            }
            if let ProcessState::Sleeping(wake_tick) = proc.state {
                if now_ticks >= wake_tick {
                    proc.state = ProcessState::Ready;
                }
            }
        }

        let prev = mgr.current;
        mgr.processes[prev].ticks = mgr.processes[prev].ticks.wrapping_add(1);

        let mut next = (prev + 1) % count;
        let mut found = false;
        for _ in 0..count {
            if mgr.processes[next].state == ProcessState::Ready {
                found = true;
                break;
            }
            next = (next + 1) % count;
        }

        if !found || next == prev {
            IN_SCHEDULER.store(false, Ordering::Release);
            return;
        }

        if mgr.processes[prev].state == ProcessState::Running {
            mgr.processes[prev].state = ProcessState::Ready;
        }
        mgr.processes[next].state = ProcessState::Running;
        mgr.current = next;

        let old_rsp = core::ptr::addr_of_mut!(mgr.processes[prev].rsp);
        let new_rsp = mgr.processes[next].rsp;

        IN_SCHEDULER.store(false, Ordering::Release);
        switch_context(old_rsp, new_rsp);
    }

    pub fn poll_all(&mut self) {
        unsafe {
            GLOBAL_PM = self as *mut ProcessManager;
        }

        let now_ticks = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
        let mut ready_count = 0;
        for p in self.processes.iter_mut() {
            if let ProcessState::Sleeping(wake) = p.state {
                if now_ticks >= wake {
                    p.state = ProcessState::Ready;
                }
            }
            if p.state == ProcessState::Ready {
                ready_count += 1;
            }
        }

        let pm_ptr = self as *mut ProcessManager;
        for _ in 0..ready_count {
            unsafe {
                Self::schedule_raw(pm_ptr);
            }
        }
    }
}
