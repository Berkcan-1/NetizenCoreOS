#![allow(dead_code)]
extern crate alloc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use nc_arch::idt::{switch_context, task_entry_trampoline, TaskContext, IRQ_TIMER_TICKS};

const KERNEL_STACK_SIZE: usize = 32 * 1024;
const STACK_CANARY: u64 = 0xDEAD_BEEF_CAFE_BABE;

// --- DYSONSPHERE FAZ 2: YETKİ BİT MASKELERİ (CAPABILITIES) ---
pub const CAP_NONE: u32                 = 0;
pub const CAP_SETTINGS_READ: u32        = 1 << 0;
pub const CAP_SETTINGS_WRITE_DISP: u32  = 1 << 1;
pub const CAP_SETTINGS_WRITE_LOCALE: u32= 1 << 2;
pub const CAP_SETTINGS_WRITE_NET: u32   = 1 << 3;
pub const CAP_SETTINGS_WRITE_DRV: u32   = 1 << 4;
pub const CAP_FS_USER_READ: u32         = 1 << 5;
pub const CAP_FS_USER_WRITE: u32        = 1 << 6;
pub const CAP_FS_USER_DELETE: u32       = 1 << 7;
pub const CAP_PKG_INSTALL: u32          = 1 << 8;
pub const CAP_NET_CLIENT: u32           = 1 << 9;
pub const CAP_KERNEL_INTERNAL: u32      = 0xFFFF_FFFF;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrustLevel {
    KernelCore,       // Ring-0 çekirdek servisleri
    BuiltinSystemApp, // Gömülü derlenen sistem uygulamaları (nc.settings, nc.files vb.)
    VerifiedApp,      // Ed25519 + SHA256 doğrulanmış NTZ2 paketi
    UnverifiedLegacy, // İmzasız eski v1 paketi (Sıkı kısıtlı sandbox)
}

#[derive(Clone, Debug)]
pub struct SecurityIdentity {
    pub app_id: String,
    pub uid: u32,
    pub trust: TrustLevel,
    pub capabilities: u32,
}

impl SecurityIdentity {
    pub fn kernel_service(name: &str) -> Self {
        Self {
            app_id: String::from(name),
            uid: 0,
            trust: TrustLevel::KernelCore,
            capabilities: CAP_KERNEL_INTERNAL,
        }
    }

    /// Gömülü uygulamalar (Settings, Files, Browser vb.) için uyumluluk ve en az ayrıcalık ataması
    pub fn from_builtin_app_id(app_id: &str) -> Self {
        let caps = match app_id {
            "nc.settings" => {
                CAP_SETTINGS_READ
                    | CAP_SETTINGS_WRITE_DISP
                    | CAP_SETTINGS_WRITE_LOCALE
                    | CAP_SETTINGS_WRITE_NET
                    | CAP_SETTINGS_WRITE_DRV
            }
            "nc.files" => {
                CAP_FS_USER_READ | CAP_FS_USER_WRITE | CAP_FS_USER_DELETE | CAP_PKG_INSTALL
            }
            "nc.browser" => CAP_NET_CLIENT | CAP_FS_USER_READ,
            "nc.system" | "nc.stopwatch" => CAP_SETTINGS_READ,
            _ => CAP_NONE,
        };
        Self {
            app_id: String::from(app_id),
            uid: 1000,
            trust: TrustLevel::BuiltinSystemApp,
            capabilities: caps,
        }
    }
}

static mut GLOBAL_PM: *mut ProcessManager = core::ptr::null_mut();
static IN_SCHEDULER: AtomicBool = AtomicBool::new(false);
/// O an çalışan uygulamanın etkin yetki maskesi (Hızlı donanımsal/atomik kontrol için)
static ACTIVE_APP_CAPS: AtomicU32 = AtomicU32::new(CAP_KERNEL_INTERNAL);

/// Bir uygulama (update/draw) çalıştırılmadan hemen önce masaüstü yöneticisi tarafından çağrılır
pub fn enter_app_context(app_id: &str, is_verified_pkg: bool, custom_caps: u32) {
    let id = if matches!(
        app_id,
        "nc.settings" | "nc.files" | "nc.system" | "nc.stopwatch" | "nc.browser"
    ) {
        SecurityIdentity::from_builtin_app_id(app_id)
    } else if is_verified_pkg {
        // Üçüncü parti doğrulanmış paketler asla sistem/sürücü yazma yetkisi alamaz
        let safe_caps = custom_caps & (CAP_SETTINGS_READ | CAP_FS_USER_READ | CAP_FS_USER_WRITE | CAP_NET_CLIENT);
        SecurityIdentity {
            app_id: String::from(app_id),
            uid: 1001,
            trust: TrustLevel::VerifiedApp,
            capabilities: safe_caps,
        }
    } else {
        // İmzasız v1 paketler: Varsayılan Ret (Default Deny) - hiçbir kritik yetki verilmez
        SecurityIdentity {
            app_id: String::from(app_id),
            uid: 2000,
            trust: TrustLevel::UnverifiedLegacy,
            capabilities: CAP_NONE,
        }
    };
    ACTIVE_APP_CAPS.store(id.capabilities, Ordering::Release);
}

/// Uygulama çalışması bitip kontrol çekirdeğe/masaüstüne döndüğünde çağrılır
pub fn exit_app_context() {
    ACTIVE_APP_CAPS.store(CAP_KERNEL_INTERNAL, Ordering::Release);
}

/// DysonSphere Sınır Kontrolü: Aktif bağlamın istenen yetkiye sahip olup olmadığını denetler
#[inline]
pub fn current_context_has_cap(required_cap: u32) -> bool {
    (ACTIVE_APP_CAPS.load(Ordering::Acquire) & required_cap) == required_cap
}

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
    pub identity: SecurityIdentity,
    pub state: ProcessState,
    pub ticks: u64,
    pub rsp: u64,
    stack: Vec<u8>,
}

impl ProcessControlBlock {
    #[inline]
    pub fn check_canary(&self) -> bool {
        if self.stack.len() < 8 {
            return true;
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

extern "C" fn net_poller_thread() -> ! {
    loop {
        nc_net::global().poll();
        sleep_ticks(10);
    }
}

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
            identity: SecurityIdentity::kernel_service("nc.kernel.compositor"),
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

        unsafe {
            core::ptr::write_unaligned(stack.as_mut_ptr() as *mut u64, STACK_CANARY);
        }

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
            identity: SecurityIdentity::kernel_service(name),
            state: ProcessState::Ready,
            ticks: 0,
            rsp: ctx_ptr as u64,
            stack,
        });
        pid
    }

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

        // Görev değiştiğinde aktif yetki bağlamını da o görevin kimliğine güncelle
        ACTIVE_APP_CAPS.store(mgr.processes[next].identity.capabilities, Ordering::Release);

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
