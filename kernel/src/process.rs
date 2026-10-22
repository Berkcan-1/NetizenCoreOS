// kernel/src/process.rs
#![allow(dead_code)]
extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use nc_arch::idt::{
    run_ring3_task_session, set_kernel_stack, switch_context, task_entry_trampoline,
    KernelThreadContext, UserContext, IRQ_TIMER_TICKS,
};
use nc_mem::{alloc_frame, free_frame, AddressSpace, PagePermissions};

use crate::syscall::{
    HandleKind, Ncu1Image, ProcessHandle, STDOUT_HANDLE, STDERR_HANDLE,
    USER_CODE_BASE, USER_DATA_BASE, USER_STACK_PAGE, USER_STACK_TOP,
};

pub unsafe fn active_user_pcb() -> Option<&'static mut ProcessControlBlock> {
    if GLOBAL_PM.is_null() || ACTIVE_USER_PID == 0 {
        return None;
    }
    (*GLOBAL_PM).processes.iter_mut().find(|p| p.pid == ACTIVE_USER_PID)
}

const KERNEL_STACK_SIZE: usize = 32 * 1024;
const STACK_CANARY: u64 = 0xDEAD_BEEF_CAFE_BABE;

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
    KernelCore,
    BuiltinSystemApp,
    VerifiedApp,
    UnverifiedLegacy,
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

pub static mut GLOBAL_PM: *mut ProcessManager = core::ptr::null_mut();
static IN_SCHEDULER: AtomicBool = AtomicBool::new(false);
static ACTIVE_APP_CAPS: AtomicU32 = AtomicU32::new(CAP_KERNEL_INTERNAL);

static mut ACTIVE_USER_PID: usize = 0;
static mut ACTIVE_USER_SPACE: Option<AddressSpace> = None;

pub fn active_user_pid() -> usize {
    unsafe { ACTIVE_USER_PID }
}

pub fn active_user_address_space() -> Option<AddressSpace> {
    unsafe { ACTIVE_USER_SPACE }
}

pub fn save_active_user_context(rip: u64, rsp: u64) {
    unsafe {
        if let Some(pm) = GLOBAL_PM.as_mut() {
            let pid = ACTIVE_USER_PID;
            if let Some(proc) = pm.processes.iter_mut().find(|p| p.pid == pid) {
                if let Some(ref mut ctx) = proc.user_ctx {
                    ctx.user_rip = rip;
                    ctx.user_rsp = rsp;
                }
            }
        }
    }
}

pub fn record_active_user_exit(exit_code: i32) {
    unsafe {
        if let Some(pm) = GLOBAL_PM.as_mut() {
            let pid = ACTIVE_USER_PID;
            if let Some(proc) = pm.processes.iter_mut().find(|p| p.pid == pid) {
                proc.exit_code = Some(exit_code);
                proc.state = ProcessState::Terminated(exit_code);
            }
        }
    }
}

pub fn record_active_user_fault(vec: u8, rip: u64, cr2: u64) {
    unsafe {
        if let Some(pm) = GLOBAL_PM.as_mut() {
            let pid = ACTIVE_USER_PID;
            if let Some(proc) = pm.processes.iter_mut().find(|p| p.pid == pid) {
                proc.fault_vec = Some(vec);
                proc.fault_rip = rip;
                proc.fault_cr2 = cr2;
                proc.exit_code = Some(-(vec as i32));
                proc.state = ProcessState::Terminated(-(vec as i32));
            }
        }
    }
}

pub fn enter_app_context(app_id: &str, is_verified_pkg: bool, custom_caps: u32) {
    let id = if matches!(
        app_id,
        "nc.settings" | "nc.files" | "nc.system" | "nc.stopwatch" | "nc.browser"
    ) {
        SecurityIdentity::from_builtin_app_id(app_id)
    } else if is_verified_pkg {
        let safe_caps = custom_caps
            & (CAP_SETTINGS_READ | CAP_FS_USER_READ | CAP_FS_USER_WRITE | CAP_NET_CLIENT);
        SecurityIdentity {
            app_id: String::from(app_id),
            uid: 1001,
            trust: TrustLevel::VerifiedApp,
            capabilities: safe_caps,
        }
    } else {
        SecurityIdentity {
            app_id: String::from(app_id),
            uid: 2000,
            trust: TrustLevel::UnverifiedLegacy,
            capabilities: CAP_NONE,
        }
    };
    ACTIVE_APP_CAPS.store(id.capabilities, Ordering::Release);
}

pub fn exit_app_context() {
    ACTIVE_APP_CAPS.store(CAP_KERNEL_INTERNAL, Ordering::Release);
}

#[inline]
pub fn current_context_has_cap(required_cap: u32) -> bool {
    (ACTIVE_APP_CAPS.load(Ordering::Acquire) & required_cap) == required_cap
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcessState {
    Ready,
    Running,
    Sleeping(u64),
    Terminated(i32),
    Stopped,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExecutionMode {
    KernelThread,
    UserProcess,
}

pub struct ProcessControlBlock {
    pub pid: usize,
    pub name: String,
    pub mode: ExecutionMode,
    pub identity: SecurityIdentity,
    pub state: ProcessState,
    pub address_space: AddressSpace,
    pub user_ctx: Option<UserContext>,
    pub handles: Vec<ProcessHandle>,
    pub next_handle_id: u64,
    pub owned_frames: Vec<u64>,
    pub exit_code: Option<i32>,
    pub fault_vec: Option<u8>,
    pub fault_rip: u64,
    pub fault_cr2: u64,
    pub ticks: u64,
    pub rsp: u64,
    pub kernel_stack_top: u64,
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
        let kernel_space = AddressSpace::current();
        let mut pm = ProcessManager {
            processes: Vec::new(),
            current: 0,
            next_pid: 1,
        };
        pm.processes.push(ProcessControlBlock {
            pid: 1,
            name: String::from("SystemCompositor"),
            mode: ExecutionMode::KernelThread,
            identity: SecurityIdentity::kernel_service("nc.kernel.compositor"),
            state: ProcessState::Running,
            address_space: kernel_space,
            user_ctx: None,
            handles: Vec::new(),
            next_handle_id: 1,
            owned_frames: Vec::new(),
            exit_code: None,
            fault_vec: None,
            fault_rip: 0,
            fault_cr2: 0,
            ticks: 0,
            rsp: 0,
            kernel_stack_top: 0,
            stack: Vec::new(),
        });
        pm.next_pid = 2;
        pm
    }

    pub fn init_system_services(&mut self) {
        self.spawn("NetPollerService", net_poller_thread);
        self.spawn("StorageMonitor", storage_monitor_thread);
    }

    pub fn spawn(&mut self, name: &str, entry: extern "C" fn() -> !) -> usize {
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut stack = vec![0u8; KERNEL_STACK_SIZE];
        unsafe {
            core::ptr::write_unaligned(stack.as_mut_ptr() as *mut u64, STACK_CANARY);
        }
        let stack_top = (stack.as_mut_ptr() as usize + KERNEL_STACK_SIZE) & !0xF;
        let ctx_ptr =
            (stack_top - core::mem::size_of::<KernelThreadContext>()) as *mut KernelThreadContext;
        unsafe {
            *ctx_ptr = KernelThreadContext {
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
            mode: ExecutionMode::KernelThread,
            identity: SecurityIdentity::kernel_service(name),
            state: ProcessState::Ready,
            address_space: AddressSpace::current(),
            user_ctx: None,
            handles: Vec::new(),
            next_handle_id: 1,
            owned_frames: Vec::new(),
            exit_code: None,
            fault_vec: None,
            fault_rip: 0,
            fault_cr2: 0,
            ticks: 0,
            rsp: ctx_ptr as u64,
            kernel_stack_top: stack_top as u64,
            stack,
        });
        pid
    }

    pub fn spawn_user_ncu1(
        &mut self,
        name: &str,
        identity: SecurityIdentity,
        raw_image: &[u8],
    ) -> Result<usize, &'static str> {
        let parsed = Ncu1Image::parse_and_verify(raw_image)?;
        let mut user_space =
            AddressSpace::create_user_space().map_err(|_| "User CR3 olusturulamadi")?;
        let mut owned_frames = Vec::new();
        owned_frames.push(user_space.pml4_phys);

        let Some(code_frame) = alloc_frame() else {
            Self::cleanup_frames(&owned_frames);
            return Err("Kod sayfasi icin PMM frame yetersiz");
        };
        owned_frames.push(code_frame);
        unsafe {
            core::ptr::copy_nonoverlapping(
                parsed.code.as_ptr(),
                code_frame as *mut u8,
                parsed.code.len(),
            );
        }
        if user_space
            .map_4k(USER_CODE_BASE, code_frame, PagePermissions::USER_RX)
            .is_err()
        {
            Self::cleanup_frames(&owned_frames);
            return Err("Kod sayfasi map_4k basarisiz");
        }

        let Some(data_frame) = alloc_frame() else {
            Self::cleanup_frames(&owned_frames);
            return Err("Veri sayfasi icin PMM frame yetersiz");
        };
        owned_frames.push(data_frame);
        if !parsed.rodata.is_empty() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    parsed.rodata.as_ptr(),
                    data_frame as *mut u8,
                    parsed.rodata.len(),
                );
            }
        }
        if user_space
            .map_4k(USER_DATA_BASE, data_frame, PagePermissions::USER_RO)
            .is_err()
        {
            Self::cleanup_frames(&owned_frames);
            return Err("Veri sayfasi map_4k basarisiz");
        }

        let Some(stack_frame) = alloc_frame() else {
            Self::cleanup_frames(&owned_frames);
            return Err("Stack sayfasi icin PMM frame yetersiz");
        };
        owned_frames.push(stack_frame);
        if user_space
            .map_4k(USER_STACK_PAGE, stack_frame, PagePermissions::USER_RW)
            .is_err()
        {
            Self::cleanup_frames(&owned_frames);
            return Err("Stack sayfasi map_4k basarisiz");
        }

        Self::collect_user_table_frames(user_space.pml4_phys, &mut owned_frames);

        let mut kstack = vec![0u8; KERNEL_STACK_SIZE];
        unsafe {
            core::ptr::write_unaligned(kstack.as_mut_ptr() as *mut u64, STACK_CANARY);
        }
        let kstack_top = ((kstack.as_mut_ptr() as usize + KERNEL_STACK_SIZE) & !0xF) as u64;

        let pid = self.next_pid;
        self.next_pid += 1;

        let uctx = UserContext {
            cr3: user_space.pml4_phys,
            user_rip: USER_CODE_BASE + (parsed.entry_offset as u64),
            user_rsp: USER_STACK_TOP - 16,
            rflags: 0x202,
            ..Default::default()
        };

        let mut default_handles = Vec::new();
        default_handles.push(ProcessHandle {
            id: STDOUT_HANDLE,
            kind: HandleKind::ConsoleOut,
            required_cap: CAP_NONE,
        });
        default_handles.push(ProcessHandle {
            id: STDERR_HANDLE,
            kind: HandleKind::ConsoleErr,
            required_cap: CAP_NONE,
        });

        self.processes.push(ProcessControlBlock {
            pid,
            name: String::from(name),
            mode: ExecutionMode::UserProcess,
            identity,
            state: ProcessState::Ready,
            address_space: user_space,
            user_ctx: Some(uctx),
            handles: default_handles,
            next_handle_id: 3,
            owned_frames,
            exit_code: None,
            fault_vec: None,
            fault_rip: 0,
            fault_cr2: 0,
            ticks: 0,
            rsp: kstack_top,
            kernel_stack_top: kstack_top,
            stack: kstack,
        });

        Ok(pid)
    }

    unsafe fn collect_table(table_phys: u64, level: u8, out: &mut Vec<u64>) {
        if table_phys == 0 || level == 0 {
            return;
        }
        let ptr = table_phys as *const u64;
        let start_idx = if level == 4 { 1 } else { 0 };
        let end_idx = if level == 4 { 256 } else { 512 };
        for i in start_idx..end_idx {
            let entry = core::ptr::read_volatile(ptr.add(i));
            if (entry & 1) != 0 && (entry & (1 << 7)) == 0 {
                let child = entry & 0x000F_FFFF_FFFF_F000;
                if !out.contains(&child) {
                    out.push(child);
                }
                if level > 2 {
                    Self::collect_table(child, level - 1, out);
                }
            }
        }
    }

    fn collect_user_table_frames(pml4_phys: u64, out: &mut Vec<u64>) {
        unsafe {
            Self::collect_table(pml4_phys, 4, out);
        }
    }

    fn cleanup_frames(frames: &[u64]) {
        for &f in frames {
            unsafe {
                free_frame(f);
            }
        }
    }

    pub fn process_exit_code(&self, pid: usize) -> Option<i32> {
        self.processes.iter().find(|p| p.pid == pid).and_then(|p| p.exit_code)
    }

    pub fn process_fault_vec(&self, pid: usize) -> Option<u8> {
        self.processes.iter().find(|p| p.pid == pid).and_then(|p| p.fault_vec)
    }

    pub fn reap_terminated_user_processes(&mut self) -> usize {
        let mut reaped = 0usize;
        let mut i = 0;
        while i < self.processes.len() {
            if self.processes[i].mode == ExecutionMode::UserProcess
                && matches!(self.processes[i].state, ProcessState::Terminated(_))
            {
                let proc = self.processes.remove(i);
                Self::cleanup_frames(&proc.owned_frames);
                if self.current >= self.processes.len() && !self.processes.is_empty() {
                    self.current = 0;
                }
                reaped += 1;
            } else {
                i += 1;
            }
        }
        reaped
    }

    pub unsafe fn schedule_raw(pm: *mut ProcessManager) {
        GLOBAL_PM = pm;
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
                if now_ticks.wrapping_sub(wake_tick) < (u64::MAX / 2) {
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

        if !found {
            IN_SCHEDULER.store(false, Ordering::Release);
            return;
        }

        // Eğer çalışan çekirdek iş parçacığı kendisiyle eşleştiyse tekrar geçiş yapma
        if next == prev && mgr.processes[prev].state == ProcessState::Running {
            IN_SCHEDULER.store(false, Ordering::Release);
            return;
        }

        // Hedef süreç Ring 3 Kullanıcı Süreci ise
        if mgr.processes[next].mode == ExecutionMode::UserProcess {
            let Some(uctx) = mgr.processes[next].user_ctx else {
                IN_SCHEDULER.store(false, Ordering::Release);
                return;
            };
            mgr.processes[next].state = ProcessState::Running;
            mgr.current = next;
            ACTIVE_USER_PID = mgr.processes[next].pid;
            ACTIVE_USER_SPACE = Some(mgr.processes[next].address_space);
            ACTIVE_APP_CAPS.store(mgr.processes[next].identity.capabilities, Ordering::Release);
            set_kernel_stack(mgr.processes[next].kernel_stack_top);

            IN_SCHEDULER.store(false, Ordering::Release);
            let status = run_ring3_task_session(
                uctx.user_rip,
                uctx.user_rsp,
                uctx.cr3,
                mgr.processes[next].kernel_stack_top,
            );

            ACTIVE_USER_PID = 0;
            ACTIVE_USER_SPACE = None;
            ACTIVE_APP_CAPS.store(CAP_KERNEL_INTERNAL, Ordering::Release);

            if status == 0 {
                if matches!(mgr.processes[next].state, ProcessState::Running) {
                    mgr.processes[next].state = ProcessState::Terminated(0);
                    if mgr.processes[next].exit_code.is_none() {
                        mgr.processes[next].exit_code = Some(0);
                    }
                }
            } else if status >= 1000 {
                let vec = (status - 1000) as u8;
                mgr.processes[next].fault_vec = Some(vec);
                mgr.processes[next].exit_code = Some(-(vec as i32));
                mgr.processes[next].state = ProcessState::Terminated(-(vec as i32));
            } else if status == -1 {
                if matches!(mgr.processes[next].state, ProcessState::Running) {
                    mgr.processes[next].state = ProcessState::Ready;
                }
            } else if status <= -10 {
                let sleep_t = ((-status) - 10) as u64;
                let now = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
                mgr.processes[next].state = ProcessState::Sleeping(now.wrapping_add(sleep_t));
            }
            return;
        }

        // Ring 0 Çekirdek İş Parçacığı
        if mgr.processes[prev].state == ProcessState::Running {
            mgr.processes[prev].state = ProcessState::Ready;
        }
        mgr.processes[next].state = ProcessState::Running;
        mgr.current = next;

        if mgr.processes[next].kernel_stack_top != 0 {
            set_kernel_stack(mgr.processes[next].kernel_stack_top);
        }

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
                if now_ticks.wrapping_sub(wake) < (u64::MAX / 2) {
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
