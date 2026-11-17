use crate::vm::{self, AddressSpace, PAGE};
use core::sync::atomic::{AtomicU64, Ordering};
use nc_arch::exceptions::TrapFrame;
use nc_arch::mmu;
use nc_pkg::Tier;

pub const MAX_TASKS: usize = 8;
pub const TICK_HZ: u64 = 100;
const STACK_PAGES: u64 = 4;
const KERNEL_RESUME_PSTATE: u64 = 0x3C5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Free,
    Ready,
    Sleeping,
    Parked,
    Dead,
}

struct Task {
    state: State,
    frame: TrapFrame,
    space: Option<AddressSpace>,
    wake: u64,
    name: &'static str,
    name_buf: [u8; 32],
    name_len: u8,
    app_id: [u8; 32],
    app_id_len: u8,
    tier: Tier,
    caps: u32,
    paused: bool,
    pages: u32,
    last_active: u64,
    entry: u64,
    event_pa: u64,
}

impl Task {
    const EMPTY: Task = Task {
        state: State::Free,
        frame: TrapFrame::EMPTY,
        space: None,
        wake: 0,
        name: "",
        name_buf: [0; 32],
        name_len: 0,
        app_id: [0; 32],
        app_id_len: 0,
        tier: Tier::User,
        caps: 0,
        paused: false,
        pages: 0,
        last_active: 0,
        entry: 0,
        event_pa: 0,
    };

    fn display_name(&self) -> &str {
        if !self.name.is_empty() {
            self.name
        } else {
            core::str::from_utf8(&self.name_buf[..self.name_len as usize]).unwrap_or("?")
        }
    }

    fn app_id_str(&self) -> &str {
        core::str::from_utf8(&self.app_id[..self.app_id_len as usize]).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
pub struct TaskSummary {
    pub pages: u32,
}

struct Scheduler {
    tasks: [Task; MAX_TASKS],
    current: usize,
    continuation: Option<fn() -> !>,
}

static mut SCHEDULER: Scheduler = Scheduler { tasks: [Task::EMPTY; MAX_TASKS], current: 0, continuation: None };
static TICKS: AtomicU64 = AtomicU64::new(0);

fn scheduler() -> &'static mut Scheduler {
    unsafe { &mut *core::ptr::addr_of_mut!(SCHEDULER) }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn init(continuation: fn() -> !) {
    scheduler().continuation = Some(continuation);
}

fn log(text: &str) {
    nc_arch::serial::write_str(text);
}

fn log_number(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    if value == 0 {
        digits[0] = b'0';
        count = 1;
    }
    while value > 0 {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    while count > 0 {
        count -= 1;
        nc_arch::serial::write_byte(digits[count]);
    }
}

pub fn spawn(name: &'static str, code: &[u8], argument: u64) -> Option<usize> {
    let scheduler = scheduler();
    let slot = scheduler.tasks.iter().position(|task| task.state == State::Free || task.state == State::Dead)?;
    let mut space = AddressSpace::new()?;
    let pages = (code.len() as u64 + PAGE - 1) / PAGE;
    let mut ok = pages > 0;
    for index in 0..pages {
        let address = nc_abi::USER_IMAGE_MIN + index * PAGE;
        match space.map_new(address, false, true) {
            Some(physical) => {
                let start = (index * PAGE) as usize;
                let end = code.len().min(start + PAGE as usize);
                unsafe {
                    core::ptr::copy_nonoverlapping(code.as_ptr().add(start), physical as *mut u8, end - start);
                }
                nc_arch::sync_icache_range(physical as usize, PAGE as usize);
            }
            None => {
                ok = false;
                break;
            }
        }
    }
    if ok {
        for index in 0..STACK_PAGES {
            let address = nc_abi::USER_APP_STACK_TOP - (index + 1) * PAGE;
            if space.map_new(address, true, false).is_none() {
                ok = false;
                break;
            }
        }
    }
    if !ok {
        space.destroy();
        return None;
    }
    let mut frame = TrapFrame::EMPTY;
    frame.regs[0] = argument;
    frame.pc = nc_abi::USER_IMAGE_MIN;
    frame.sp = nc_abi::USER_APP_STACK_TOP;
    frame.pstate = 0;
    let mut task = Task::EMPTY;
    task.state = State::Ready;
    task.frame = frame;
    task.space = Some(space);
    task.name = name;
    task.pages = (pages + STACK_PAGES) as u32;
    task.entry = nc_abi::USER_IMAGE_MIN;
    scheduler.tasks[slot] = task;
    Some(slot)
}

fn live_count(scheduler: &Scheduler) -> usize {
    scheduler.tasks.iter().filter(|task| task.state != State::Free && task.state != State::Dead).count()
}

fn wake_sleepers(scheduler: &mut Scheduler) {
    let now = ticks();
    for task in scheduler.tasks.iter_mut() {
        if task.state == State::Sleeping && task.wake <= now {
            task.state = State::Ready;
        }
    }
}

fn pick_next(scheduler: &Scheduler) -> Option<usize> {
    for step in 1..=MAX_TASKS {
        let index = (scheduler.current + step) % MAX_TASKS;
        if scheduler.tasks[index].state == State::Ready {
            return Some(index);
        }
    }
    None
}

fn idle_wait() {
    nc_arch::irq_enable();
    nc_arch::wait_for_interrupt();
    nc_arch::irq_disable();
}

fn reschedule(frame: &mut TrapFrame, save_current: bool) {
    let scheduler = scheduler();
    let current = scheduler.current;
    if save_current && (scheduler.tasks[current].state == State::Ready || scheduler.tasks[current].state == State::Sleeping) {
        scheduler.tasks[current].frame = *frame;
    }
    loop {
        wake_sleepers(scheduler);
        if let Some(next) = pick_next(scheduler) {
            scheduler.current = next;
            *frame = scheduler.tasks[next].frame;
            if let Some(space) = &scheduler.tasks[next].space {
                unsafe {
                    mmu::switch_root(space.root());
                }
            }
            return;
        }
        if live_count(scheduler) == 0 {
            unsafe {
                mmu::switch_root(mmu::kernel_root());
            }
            if let Some(continuation) = scheduler.continuation {
                *frame = TrapFrame::EMPTY;
                frame.pc = continuation as usize as u64;
                frame.pstate = KERNEL_RESUME_PSTATE;
                return;
            }
            nc_arch::halt_forever();
        }
        idle_wait();
    }
}

pub fn start() -> ! {
    let scheduler = scheduler();
    wake_sleepers(scheduler);
    scheduler.current = MAX_TASKS - 1;
    match pick_next(scheduler) {
        Some(next) => {
            scheduler.current = next;
            if let Some(space) = &scheduler.tasks[next].space {
                unsafe {
                    mmu::switch_root(space.root());
                }
            }
            unsafe { nc_arch::exceptions::enter_user(&scheduler.tasks[next].frame) }
        }
        None => match scheduler.continuation {
            Some(continuation) => continuation(),
            None => nc_arch::halt_forever(),
        },
    }
}

fn destroy_slot_in(scheduler: &mut Scheduler, slot: usize) -> bool {
    if slot >= MAX_TASKS {
        return false;
    }
    let was_live = scheduler.tasks[slot].state != State::Free && scheduler.tasks[slot].state != State::Dead;
    if let Some(space) = scheduler.tasks[slot].space.take() {
        unsafe {
            mmu::switch_root(mmu::kernel_root());
        }
        space.destroy();
    }
    scheduler.tasks[slot] = Task::EMPTY;
    scheduler.tasks[slot].state = State::Dead;
    was_live
}

fn kill_current(scheduler: &mut Scheduler) {
    let current = scheduler.current;
    destroy_slot_in(scheduler, current);
}

pub fn kill_slot(slot: usize) -> bool {
    let scheduler = scheduler();
    if slot == scheduler.current && scheduler.tasks[slot].state == State::Ready {
        return false;
    }
    let ok = destroy_slot_in(scheduler, slot);
    if ok && slot < MAX_TASKS {
        if let Some(space) = &scheduler.tasks[scheduler.current].space {
            unsafe {
                mmu::switch_root(space.root());
            }
        }
    }
    ok
}

pub fn exit_current(frame: &mut TrapFrame, code: i64) {
    let scheduler = scheduler();
    let current = scheduler.current;
    log("[gorev] ");
    log(scheduler.tasks[current].display_name());
    log(" bitti, cikis kodu ");
    if code < 0 {
        log("-");
    }
    log_number(code.unsigned_abs());
    log("\n");
    kill_current(scheduler);
    crate::host::on_task_terminated(current);
    reschedule(frame, false);
}

pub fn yield_now(frame: &mut TrapFrame) {
    reschedule(frame, true);
}

pub fn sleep_current(frame: &mut TrapFrame, ticks_to_sleep: u64) {
    let scheduler = scheduler();
    let current = scheduler.current;
    scheduler.tasks[current].state = State::Sleeping;
    scheduler.tasks[current].wake = ticks().saturating_add(ticks_to_sleep.min(1 << 32));
    reschedule(frame, true);
}

pub fn with_current_space<R>(action: impl FnOnce(&AddressSpace) -> R) -> Option<R> {
    let scheduler = scheduler();
    match &scheduler.tasks[scheduler.current].space {
        Some(space) => Some(action(space)),
        None => None,
    }
}

pub fn current_slot() -> usize {
    scheduler().current
}

pub fn current_caps() -> u32 {
    let scheduler = scheduler();
    scheduler.tasks[scheduler.current].caps
}

pub fn current_app_id(out: &mut [u8; 32]) -> usize {
    let scheduler = scheduler();
    let task = &scheduler.tasks[scheduler.current];
    let len = (task.app_id_len as usize).min(32);
    out[..len].copy_from_slice(&task.app_id[..len]);
    len
}

pub fn on_fault(frame: &mut TrapFrame, esr: u64, far: u64) {
    let scheduler = scheduler();
    let current = scheduler.current;
    log("[gorev] ");
    log(scheduler.tasks[current].display_name());
    log(" hata verdi: ec=");
    log_number((esr >> 26) & 0x3F);
    log(" far=");
    log_number(far);
    log(" -> sonlandirildi\n");
    kill_current(scheduler);
    crate::host::on_task_terminated(current);
    reschedule(frame, false);
}

pub fn on_irq(frame: &mut TrapFrame, intid: u32) {
    if intid != nc_arch::gic::TIMER_INTID {
        return;
    }
    TICKS.fetch_add(1, Ordering::Relaxed);
    crate::input::poll();
    crate::host::tick(ticks());
    if frame.from_user() {
        reschedule(frame, true);
    }
}

pub fn free_before_report(before: usize) {
    log("pmm: bosta ");
    log_number(vm::free_frames() as u64);
    log(" kare (baslangic ");
    log_number(before as u64);
    log(")\n");
}

const HOME_STACK_PAGES: u64 = 16;

fn copy_short(dst: &mut [u8; 32], text: &str) -> u8 {
    let bytes = text.as_bytes();
    let len = bytes.len().min(32);
    dst[..len].copy_from_slice(&bytes[..len]);
    len as u8
}

pub fn evict_oldest_paused(protected_slot: usize) -> Option<usize> {
    let scheduler = scheduler();
    let mut victim: Option<(usize, u64)> = None;
    for (index, task) in scheduler.tasks.iter().enumerate() {
        if index == protected_slot || index == scheduler.current {
            continue;
        }
        if task.state == State::Parked && task.paused {
            match victim {
                None => victim = Some((index, task.last_active)),
                Some((_, oldest)) if task.last_active < oldest => victim = Some((index, task.last_active)),
                _ => {}
            }
        }
    }
    if let Some((slot, _)) = victim {
        log("[yasam] bellek/yuva acmak icin arka plan uygulamasi sonlandirildi: ");
        log(scheduler.tasks[slot].display_name());
        log("\n");
        kill_slot(slot);
        Some(slot)
    } else {
        None
    }
}

pub fn spawn_elf(name: &'static str, elf: &[u8]) -> Result<usize, &'static str> {
    spawn_app("nc.home", name, elf, Tier::System, nc_caps::KERNEL_INTERNAL)
}

pub fn spawn_app(app_id: &str, display_name: &str, elf: &[u8], tier: Tier, caps: u32) -> Result<usize, &'static str> {
    let policy = nc_elf::Policy {
        min_vaddr: nc_abi::USER_IMAGE_MIN,
        max_vaddr: nc_abi::USER_IMAGE_MAX,
        max_pages: nc_abi::ELF_MAX_PAGES,
    };
    let plan = nc_elf::parse(elf, &policy).map_err(|error| error.as_str())?;
    let scheduler = scheduler();
    let slot = scheduler
        .tasks
        .iter()
        .position(|task| task.state == State::Free || task.state == State::Dead)
        .ok_or("gorev tablosu dolu")?;
    let mut space = AddressSpace::new().ok_or("bellek yok")?;
    let mut failed = false;
    let mut mapped_pages = 0u32;
    'segments: for segment in plan.segments() {
        let start = segment.page_start();
        for index in 0..segment.page_count() {
            let address = start + index * PAGE;
            let physical = match space.map_new(address, segment.writable(), segment.executable()) {
                Some(physical) => {
                    mapped_pages += 1;
                    physical
                }
                None => {
                    failed = true;
                    break 'segments;
                }
            };
            let file_begin = segment.vaddr;
            let file_end = segment.vaddr + segment.file_size;
            let low = address.max(file_begin);
            let high = (address + PAGE).min(file_end);
            if high > low {
                let source = (segment.file_offset + (low - segment.vaddr)) as usize;
                let length = (high - low) as usize;
                if source + length > elf.len() {
                    failed = true;
                    break 'segments;
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(elf.as_ptr().add(source), (physical + (low - address)) as *mut u8, length);
                }
            }
            if segment.executable() {
                nc_arch::sync_icache_range(physical as usize, PAGE as usize);
            }
        }
    }
    let mut event_pa = 0u64;
    if !failed {
        match space.map_new(nc_abi::USER_EVENT_PAGE, false, false) {
            Some(physical) => {
                mapped_pages += 1;
                event_pa = physical;
            }
            None => failed = true,
        }
    }
    if !failed {
        for index in 0..HOME_STACK_PAGES {
            if space.map_new(nc_abi::USER_APP_STACK_TOP - (index + 1) * PAGE, true, false).is_none() {
                failed = true;
                break;
            }
            mapped_pages += 1;
        }
    }
    if failed {
        space.destroy();
        return Err("yukleme basarisiz (bellek)");
    }
    let mut frame = TrapFrame::EMPTY;
    frame.pc = plan.entry;
    frame.sp = nc_abi::USER_APP_STACK_TOP;
    let mut task = Task::EMPTY;
    task.state = State::Parked;
    task.frame = frame;
    task.space = Some(space);
    task.name_len = copy_short(&mut task.name_buf, display_name);
    task.app_id_len = copy_short(&mut task.app_id, app_id);
    task.tier = tier;
    task.caps = caps;
    task.paused = false;
    task.pages = mapped_pages;
    task.last_active = ticks();
    task.entry = plan.entry;
    task.event_pa = event_pa;
    scheduler.tasks[slot] = task;
    if let Some(current_space) = &scheduler.tasks[scheduler.current].space {
        unsafe {
            mmu::switch_root(current_space.root());
        }
    }
    Ok(slot)
}

pub fn find_app(app_id: &str) -> Option<usize> {
    let scheduler = scheduler();
    for (index, task) in scheduler.tasks.iter().enumerate() {
        if (task.state == State::Parked || task.state == State::Ready || task.state == State::Sleeping)
            && task.app_id_str() == app_id
        {
            return Some(index);
        }
    }
    None
}

pub fn is_alive(slot: usize) -> bool {
    if slot >= MAX_TASKS {
        return false;
    }
    let state = scheduler().tasks[slot].state;
    state != State::Free && state != State::Dead
}

pub fn set_paused(slot: usize, paused: bool) {
    if slot < MAX_TASKS {
        let scheduler = scheduler();
        scheduler.tasks[slot].paused = paused;
        scheduler.tasks[slot].last_active = ticks();
    }
}

pub fn slot_info(slot: usize) -> Option<TaskSummary> {
    if slot >= MAX_TASKS {
        return None;
    }
    let task = &scheduler().tasks[slot];
    if task.state == State::Free || task.state == State::Dead {
        return None;
    }
    Some(TaskSummary { pages: task.pages })
}

pub fn deliver(slot: usize, block: &nc_abi::event::EventBlock) -> bool {
    if slot >= MAX_TASKS {
        return false;
    }
    let scheduler = scheduler();
    let task = &mut scheduler.tasks[slot];
    if task.state != State::Parked || task.event_pa == 0 {
        return false;
    }
    let bytes = block.encode();
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), task.event_pa as *mut u8, bytes.len());
    }
    task.frame = TrapFrame::EMPTY;
    task.frame.pc = task.entry;
    task.frame.sp = nc_abi::USER_APP_STACK_TOP;
    task.last_active = ticks();
    task.state = State::Ready;
    true
}

pub fn app_done(frame: &mut TrapFrame, flags: u32) {
    let scheduler = scheduler();
    let current = scheduler.current;
    scheduler.tasks[current].state = State::Parked;
    crate::host::on_app_done(current, flags);
    reschedule(frame, false);
}
