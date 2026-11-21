//! ARMv8-A Symmetric Multiprocessing (SMP) — Feature 1.
//!
//! Brings up secondary CPUs via PSCI `CPU_ON`, initializes per-core kernel
//! stacks, exception vectors, MMU W^X page tables, GICv3 redistributors,
//! and provides cross-core IPIs and Inner-Shareable TLB shootdowns.

use core::arch::asm;
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub const MAX_CPUS: usize = 8;
const SECONDARY_STACK_SIZE: usize = 16384; // 16 KiB per core (1 << 14)

#[repr(C, align(16))]
pub struct CpuStacks(pub [[u8; SECONDARY_STACK_SIZE]; MAX_CPUS]);

#[no_mangle]
pub static mut NC_SECONDARY_STACKS: CpuStacks = CpuStacks([[0u8; SECONDARY_STACK_SIZE]; MAX_CPUS]);

extern "C" {
    fn nc_secondary_trampoline();
}

static ONLINE_MASK: AtomicU32 = AtomicU32::new(1); // CPU 0 is always online first
static ONLINE_COUNT: AtomicUsize = AtomicUsize::new(1);
static IPI_RECEIVED: [AtomicU64; MAX_CPUS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

struct Serial;
impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

pub fn current_cpu_id() -> usize {
    let mpidr: u64;
    unsafe {
        asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack, preserves_flags));
    }
    ((mpidr & 0xFF) as usize).min(MAX_CPUS - 1)
}

pub fn online_count() -> usize {
    ONLINE_COUNT.load(Ordering::Acquire)
}

pub fn online_mask() -> u32 {
    ONLINE_MASK.load(Ordering::Acquire)
}

pub fn on_ipi(cpu_id: usize, intid: u32) {
    if cpu_id < MAX_CPUS {
        IPI_RECEIVED[cpu_id].fetch_add(1, Ordering::Relaxed);
    }
    if intid == nc_arch::gic::IPI_TLB_SHOOTDOWN_SGI {
        nc_arch::mmu::tlb_invalidate_all_is();
    }
}

pub fn send_resched_ipi(target_mask: u16) {
    if online_count() > 1 {
        nc_arch::gic::send_sgi(nc_arch::gic::IPI_RESCHED_SGI, target_mask);
    }
}

pub fn tlb_shootdown_va(va: u64) {
    nc_arch::mmu::tlb_invalidate_va_is(va);
}

#[no_mangle]
pub extern "C" fn nc_secondary_main(cpu_id: usize) -> ! {
    if cpu_id == 0 || cpu_id >= MAX_CPUS {
        nc_arch::halt_forever();
    }
    unsafe {
        nc_arch::mmu::enable_secondary();
        nc_arch::exceptions::install();
    }
    let _ = nc_arch::gic::init_secondary(cpu_id);
    ONLINE_MASK.fetch_or(1u32 << cpu_id, Ordering::Release);
    ONLINE_COUNT.fetch_add(1, Ordering::Release);
    nc_arch::irq_enable();
    loop {
        nc_arch::wait_for_interrupt();
    }
}

/// Probe PSCI and bring up secondary CPUs `1..MAX_CPUS` without deadlocking
/// if firmware reports `PSCI_INVALID_PARAMETERS` or `PSCI_NOT_PRESENT`.
pub fn bringup() -> usize {
    let ver = nc_arch::psci::version();
    if ver.is_none() {
        let _ = writeln!(Serial, "smp: 1 CPU online (psci yok)");
        return 1;
    }
    let (maj, min) = ver.unwrap();
    let entry = nc_secondary_trampoline as *const () as usize as u64;

    for cpu_id in 1..MAX_CPUS {
        let rc = nc_arch::psci::cpu_on(cpu_id as u64, entry, cpu_id as u64);
        if rc != nc_arch::psci::PSCI_SUCCESS {
            break;
        }
        // Bounded wait for secondary CPU to complete MMU + GIC init
        let mut spins = 0u32;
        while (ONLINE_MASK.load(Ordering::Acquire) & (1u32 << cpu_id)) == 0 && spins < 5_000_000 {
            core::hint::spin_loop();
            spins += 1;
        }
    }

    let count = online_count();
    let _ = writeln!(Serial, "smp: {} CPU online (psci v{}.{}, mask={:#x})", count, maj, min, online_mask());
    count
}
