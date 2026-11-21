//! Structured crash logging & panic diagnostics (Feature 49).
//!
//! Zero-allocation, bounded, checksummed crash records captured during kernel
//! panic or fatal exception handling. Never records PINs, passwords, keys, or
//! user file contents.

use crate::task;
use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const CRASH_MAGIC: u32 = 0x5352_434E; // "NCRS"
pub const CRASH_VERSION: u16 = 1;
pub const CRASH_MSG_MAX: usize = 160;
pub const CRASH_RING_LEN: usize = 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CrashRecord {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
    pub ticks: u64,
    pub cpu_id: u64,
    pub current_el: u64,
    pub esr_el1: u64,
    pub far_el1: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
    pub sp: u64,
    pub app_id_len: u8,
    pub stack_ok: u8,
    pub msg_len: u16,
    pub checksum: u32,
    pub app_id: [u8; 32],
    pub msg: [u8; CRASH_MSG_MAX],
}

impl CrashRecord {
    pub const EMPTY: Self = Self {
        magic: 0,
        version: 0,
        flags: 0,
        ticks: 0,
        cpu_id: 0,
        current_el: 0,
        esr_el1: 0,
        far_el1: 0,
        elr_el1: 0,
        spsr_el1: 0,
        sp: 0,
        app_id_len: 0,
        stack_ok: 0,
        msg_len: 0,
        checksum: 0,
        app_id: [0; 32],
        msg: [0; CRASH_MSG_MAX],
    };

    fn compute_crc32(&self) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        let mut feed = |bytes: &[u8]| {
            for &b in bytes {
                crc ^= b as u32;
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }
        };
        feed(&self.magic.to_le_bytes());
        feed(&self.version.to_le_bytes());
        feed(&self.flags.to_le_bytes());
        feed(&self.ticks.to_le_bytes());
        feed(&self.cpu_id.to_le_bytes());
        feed(&self.current_el.to_le_bytes());
        feed(&self.esr_el1.to_le_bytes());
        feed(&self.far_el1.to_le_bytes());
        feed(&self.elr_el1.to_le_bytes());
        feed(&self.spsr_el1.to_le_bytes());
        feed(&self.sp.to_le_bytes());
        feed(&[self.app_id_len, self.stack_ok]);
        feed(&self.msg_len.to_le_bytes());
        feed(&self.app_id);
        feed(&self.msg);
        !crc
    }

    pub fn seal(&mut self) {
        self.magic = CRASH_MAGIC;
        self.version = CRASH_VERSION;
        self.checksum = self.compute_crc32();
    }

    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        self.magic == CRASH_MAGIC
            && self.version == CRASH_VERSION
            && self.checksum == self.compute_crc32()
    }
}

static mut CRASH_RING: [CrashRecord; CRASH_RING_LEN] = [CrashRecord::EMPTY; CRASH_RING_LEN];
static CRASH_HEAD: AtomicUsize = AtomicUsize::new(0);

struct FixedBuf<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> Write for FixedBuf<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let avail = self.buf.len().saturating_sub(self.len);
        let take = bytes.len().min(avail);
        self.buf[self.len..self.len + take].copy_from_slice(&bytes[..take]);
        self.len += take;
        Ok(())
    }
}

struct SerialOut;
impl Write for SerialOut {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

#[inline]
fn read_sys_regs() -> (u64, u64, u64, u64, u64, u64, u64) {
    let mpidr: u64;
    let cur_el: u64;
    let esr: u64;
    let far: u64;
    let elr: u64;
    let spsr: u64;
    let sp: u64;
    unsafe {
        asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, CurrentEL", out(reg) cur_el, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, elr_el1", out(reg) elr, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, spsr_el1", out(reg) spsr, options(nomem, nostack, preserves_flags));
        asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    (mpidr & 0xFF, (cur_el >> 2) & 0x3, esr, far, elr, spsr, sp)
}

pub fn record_and_dump_panic(info: &PanicInfo, stack_ok: bool) {
    let (cpu_id, current_el, esr_el1, far_el1, elr_el1, spsr_el1, sp) = read_sys_regs();
    let mut rec = CrashRecord::EMPTY;
    rec.ticks = task::ticks();
    rec.cpu_id = cpu_id;
    rec.current_el = current_el;
    rec.esr_el1 = esr_el1;
    rec.far_el1 = far_el1;
    rec.elr_el1 = elr_el1;
    rec.spsr_el1 = spsr_el1;
    rec.sp = sp;
    rec.stack_ok = if stack_ok { 1 } else { 0 };
    rec.app_id_len = task::current_app_id(&mut rec.app_id).min(32) as u8;

    {
        let mut writer = FixedBuf { buf: &mut rec.msg, len: 0 };
        let _ = write!(writer, "{}", info);
        rec.msg_len = writer.len as u16;
    }
    rec.seal();

    let idx = CRASH_HEAD.fetch_add(1, Ordering::Relaxed) % CRASH_RING_LEN;
    unsafe {
        let slot = core::ptr::addr_of_mut!(CRASH_RING).cast::<CrashRecord>().add(idx);
        core::ptr::write_volatile(slot, rec);
    }

    let app_str = core::str::from_utf8(&rec.app_id[..rec.app_id_len as usize]).unwrap_or("<kernel>");
    let msg_str = core::str::from_utf8(&rec.msg[..rec.msg_len as usize]).unwrap_or("<non-utf8>");
    let ec = (rec.esr_el1 >> 26) & 0x3F;
    let _ = writeln!(SerialOut, "\n*** CEKIRDEK PANIGI [NCRS v{} crc={:#010x}] ***", rec.version, rec.checksum);
    let _ = writeln!(SerialOut, "  cpu={} el={} ticks={} stack_ok={} app={}", rec.cpu_id, rec.current_el, rec.ticks, stack_ok, app_str);
    let _ = writeln!(SerialOut, "  esr={:#018x} (ec={:#04x}) far={:#018x}", rec.esr_el1, ec, rec.far_el1);
    let _ = writeln!(SerialOut, "  elr={:#018x} spsr={:#018x} sp={:#018x}", rec.elr_el1, rec.spsr_el1, rec.sp);
    let _ = writeln!(SerialOut, "  msg: {}", msg_str);
}
