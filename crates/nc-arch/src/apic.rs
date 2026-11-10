// crates/nc-arch/src/apic.rs
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

pub const IA32_APIC_BASE_MSR: u32 = 0x1B;
pub const IA32_TSC_DEADLINE_MSR: u32 = 0x6E0;
pub const DEFAULT_LAPIC_BASE: u64 = 0xFEE0_0000;

const LAPIC_ID: usize         = 0x020;
const LAPIC_EOI: usize        = 0x0B0;
const LAPIC_SVR: usize        = 0x0F0;
const LAPIC_ICR_LOW: usize    = 0x300;
const LAPIC_ICR_HIGH: usize   = 0x310;
const LAPIC_LVT_TIMER: usize  = 0x320;
const LAPIC_TIMER_INIT: usize = 0x380;
const LAPIC_TIMER_DIV: usize  = 0x3E0;

pub const APIC_TIMER_VECTOR: u8 = 0x20; // Vektör 32
pub const APIC_SPURIOUS_VEC: u8 = 0xFF;
pub const MAX_CPUS: usize = 16;

#[repr(C, align(64))]
pub struct PerCpuData {
    pub cpu_id: AtomicU32,
    pub lapic_id: AtomicU32,
    pub online: AtomicBool,
    pub current_pid: AtomicUsize,
    pub ticks: AtomicU32,
}

impl PerCpuData {
    pub const fn new() -> Self {
        Self {
            cpu_id: AtomicU32::new(0),
            lapic_id: AtomicU32::new(0),
            online: AtomicBool::new(false),
            current_pid: AtomicUsize::new(0),
            ticks: AtomicU32::new(0),
        }
    }
}

pub static PER_CPU: [PerCpuData; MAX_CPUS] = [const { PerCpuData::new() }; MAX_CPUS];
pub static DETECTED_CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
static LAPIC_ACTIVE: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn is_lapic_active() -> bool {
    LAPIC_ACTIVE.load(Ordering::Relaxed)
}

#[inline]
unsafe fn lapic_read(offset: usize) -> u32 {
    let ptr = (DEFAULT_LAPIC_BASE as usize + offset) as *const u32;
    core::ptr::read_volatile(ptr)
}

#[inline]
unsafe fn lapic_write(offset: usize, val: u32) {
    let ptr = (DEFAULT_LAPIC_BASE as usize + offset) as *mut u32;
    core::ptr::write_volatile(ptr, val);
}

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
    ((high as u64) << 32) | (low as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nomem, nostack, preserves_flags));
}

pub fn has_apic() -> bool {
    let cpuid = core::arch::x86_64::__cpuid(1);
    (cpuid.edx & (1 << 9)) != 0
}

pub fn has_tsc_deadline() -> bool {
    let cpuid = core::arch::x86_64::__cpuid(1);
    (cpuid.ecx & (1 << 24)) != 0
}

pub unsafe fn init_local_apic() -> bool {
    if !has_apic() {
        return false;
    }

    let base_msr = rdmsr(IA32_APIC_BASE_MSR);
    let phys_base = base_msr & 0xFFFF_F000;
    if phys_base != DEFAULT_LAPIC_BASE {
        wrmsr(IA32_APIC_BASE_MSR, DEFAULT_LAPIC_BASE | (base_msr & 0xFFF) | (1 << 11));
    } else {
        wrmsr(IA32_APIC_BASE_MSR, base_msr | (1 << 11));
    }

    // Spurious Interrupt Vector Register (Bit 8 = APIC Enable)
    lapic_write(LAPIC_SVR, (1 << 8) | (APIC_SPURIOUS_VEC as u32));

    let bsp_lapic_id = lapic_read(LAPIC_ID) >> 24;
    PER_CPU[0].cpu_id.store(0, Ordering::Relaxed);
    PER_CPU[0].lapic_id.store(bsp_lapic_id, Ordering::Relaxed);
    PER_CPU[0].online.store(true, Ordering::Release);

    // Local APIC Timer: Bölücü 16, Periyodik Mod, Vektör 32
    lapic_write(LAPIC_TIMER_DIV, 0x03);
    lapic_write(LAPIC_LVT_TIMER, (1 << 17) | (APIC_TIMER_VECTOR as u32));
    lapic_write(LAPIC_TIMER_INIT, 0x0002_0000);

    LAPIC_ACTIVE.store(true, Ordering::Release);
    true
}

#[inline]
pub fn send_eoi() {
    if is_lapic_active() {
        unsafe {
            lapic_write(LAPIC_EOI, 0);
        }
    }
}

#[inline]
pub fn current_lapic_id() -> u32 {
    if is_lapic_active() {
        unsafe { lapic_read(LAPIC_ID) >> 24 }
    } else {
        0
    }
}

pub unsafe fn send_ipi(target_lapic_id: u32, vector: u8, delivery_mode: u32) {
    if !is_lapic_active() {
        return;
    }
    while (lapic_read(LAPIC_ICR_LOW) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }
    lapic_write(LAPIC_ICR_HIGH, target_lapic_id << 24);
    lapic_write(LAPIC_ICR_LOW, (delivery_mode << 8) | (1 << 14) | (vector as u32));
}

pub unsafe fn discover_smp_topology() -> usize {
    let mut found_cpus = 0usize;
    let mut addr = 0x000E_0000usize;
    let mut rsdp_addr: Option<usize> = None;
    while addr < 0x0010_0000 {
        let sig = core::slice::from_raw_parts(addr as *const u8, 8);
        if sig == b"RSD PTR " {
            rsdp_addr = Some(addr);
            break;
        }
        addr += 16;
    }

    if let Some(rsdp) = rsdp_addr {
        let rsdt_phys = core::ptr::read_unaligned((rsdp + 16) as *const u32) as usize;
        if rsdt_phys > 0 && rsdt_phys < 0xFFE0_0000 {
            let total_len = core::ptr::read_unaligned((rsdt_phys + 4) as *const u32) as usize;
            if total_len >= 36 && total_len <= 4096 {
                let entries = (total_len - 36) / 4;
                let table_ptrs = (rsdt_phys + 36) as *const u32;
                for i in 0..entries {
                    let tbl = core::ptr::read_unaligned(table_ptrs.add(i)) as usize;
                    if tbl == 0 || tbl >= 0xFFE0_0000 {
                        continue;
                    }
                    let tbl_sig = core::slice::from_raw_parts(tbl as *const u8, 4);
                    if tbl_sig == b"APIC" {
                        let madt_len = core::ptr::read_unaligned((tbl + 4) as *const u32) as usize;
                        let mut off = 44usize;
                        while off + 2 <= madt_len && off < 4096 {
                            let entry_type = *((tbl + off) as *const u8);
                            let entry_len = *((tbl + off + 1) as *const u8) as usize;
                            if entry_len < 2 {
                                break;
                            }
                            if entry_type == 0 && entry_len >= 8 {
                                let acpi_cpu_id = *((tbl + off + 2) as *const u8) as u32;
                                let lapic_id = *((tbl + off + 3) as *const u8) as u32;
                                let flags = core::ptr::read_unaligned((tbl + off + 4) as *const u32);
                                if (flags & 1) != 0 && found_cpus < MAX_CPUS {
                                    PER_CPU[found_cpus].cpu_id.store(acpi_cpu_id, Ordering::Relaxed);
                                    PER_CPU[found_cpus].lapic_id.store(lapic_id, Ordering::Relaxed);
                                    if found_cpus == 0 {
                                        PER_CPU[found_cpus].online.store(true, Ordering::Release);
                                    }
                                    found_cpus += 1;
                                }
                            }
                            off += entry_len;
                        }
                        break;
                    }
                }
            }
        }
    }

    if found_cpus == 0 {
        found_cpus = 1;
    }
    DETECTED_CPU_COUNT.store(found_cpus, Ordering::Release);
    found_cpus
}
