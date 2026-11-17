use core::arch::{asm, global_asm};

pub const FRAME_BYTES: usize = 272;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}

impl TrapFrame {
    pub const EMPTY: TrapFrame = TrapFrame { regs: [0; 31], sp: 0, pc: 0, pstate: 0 };

    pub fn from_user(&self) -> bool {
        self.pstate & 0xF == 0
    }
}

pub struct Handlers {
    pub syscall: fn(&mut TrapFrame),
    pub fault: fn(&mut TrapFrame, u64, u64),
    pub irq: fn(&mut TrapFrame, u32),
}

static mut HANDLERS: Option<Handlers> = None;

pub fn register_handlers(handlers: Handlers) {
    unsafe {
        *core::ptr::addr_of_mut!(HANDLERS) = Some(handlers);
    }
}

global_asm!(
    r#"
    .macro nc_vector index
    .balign 128
    mov x0, #\index
    b nc_exception_common
    .endm

    .macro nc_vector_to target
    .balign 128
    b \target
    .endm

    .macro nc_save
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    mrs x21, sp_el0
    mrs x22, elr_el1
    mrs x23, spsr_el1
    str x21, [sp, #248]
    str x22, [sp, #256]
    str x23, [sp, #264]
    .endm

    .macro nc_restore
    ldr x21, [sp, #248]
    ldr x22, [sp, #256]
    ldr x23, [sp, #264]
    msr sp_el0, x21
    msr elr_el1, x22
    msr spsr_el1, x23
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #272
    eret
    .endm

    .section .text.vectors,"ax",@progbits
    .balign 2048
    .global nc_vectors
nc_vectors:
    nc_vector 0
    nc_vector 1
    nc_vector 2
    nc_vector 3
    nc_vector 4
    nc_vector_to nc_el1_irq
    nc_vector 6
    nc_vector 7
    nc_vector_to nc_el0_sync
    nc_vector_to nc_el0_irq
    nc_vector 10
    nc_vector 11
    nc_vector 12
    nc_vector 13
    nc_vector 14
    nc_vector 15

    .balign 4
nc_el0_sync:
    nc_save
    mov x0, sp
    bl nc_el0_sync_entry
    nc_restore

    .balign 4
nc_el0_irq:
    nc_save
    mov x0, sp
    bl nc_irq_entry
    nc_restore

    .balign 4
nc_el1_irq:
    nc_save
    mov x0, sp
    bl nc_irq_entry
    nc_restore

    .balign 4
    .global nc_enter_user
nc_enter_user:
    adrp x1, __stack_top
    add x1, x1, :lo12:__stack_top
    sub x1, x1, #272
    mov x5, x0
    mov x6, x1
    mov x2, #17
7:
    ldp x3, x4, [x5], #16
    stp x3, x4, [x6], #16
    subs x2, x2, #1
    b.ne 7b
    mov sp, x1
    nc_restore

    .balign 4
nc_exception_common:
    adrp x4, nc_exception_stack_top
    add x4, x4, :lo12:nc_exception_stack_top
    mov sp, x4
    mrs x1, esr_el1
    mrs x2, elr_el1
    mrs x3, far_el1
    bl nc_exception_report
1:
    wfe
    b 1b

    .section .bss.nc_exception_stack,"aw",@nobits
    .balign 16
nc_exception_stack:
    .skip 8192
nc_exception_stack_top:
    "#
);

static IN_REPORT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

extern "C" {
    static nc_vectors: u8;
    fn nc_enter_user(frame: *const TrapFrame) -> !;
}

pub fn install() {
    unsafe {
        let address = &nc_vectors as *const u8 as u64;
        asm!("msr vbar_el1, {}", "isb", in(reg) address, options(nomem, nostack));
    }
}

pub unsafe fn enter_user(frame: &TrapFrame) -> ! {
    nc_enter_user(frame as *const TrapFrame)
}

fn read_esr() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn read_far() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, far_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[no_mangle]
extern "C" fn nc_el0_sync_entry(frame: *mut TrapFrame) {
    let frame = unsafe { &mut *frame };
    let esr = read_esr();
    let handlers = unsafe { (*core::ptr::addr_of!(HANDLERS)).as_ref() };
    match handlers {
        Some(handlers) => {
            if (esr >> 26) & 0x3F == 0x15 {
                (handlers.syscall)(frame);
            } else {
                (handlers.fault)(frame, esr, read_far());
            }
        }
        None => nc_exception_report(8, esr, frame.pc, read_far()),
    }
}

#[no_mangle]
extern "C" fn nc_irq_entry(frame: *mut TrapFrame) {
    let frame = unsafe { &mut *frame };
    let intid = crate::gic::acknowledge();
    if intid >= 1020 {
        return;
    }
    if intid == crate::gic::TIMER_INTID {
        crate::timer::rearm();
    }
    let handlers = unsafe { (*core::ptr::addr_of!(HANDLERS)).as_ref() };
    if let Some(handlers) = handlers {
        (handlers.irq)(frame, intid);
    }
    crate::gic::end(intid);
}

fn write_hex(value: u64) {
    let mut buffer = [0u8; 18];
    buffer[0] = b'0';
    buffer[1] = b'x';
    for index in 0..16 {
        let nibble = ((value >> ((15 - index) * 4)) & 0xF) as u8;
        buffer[2 + index] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
    }
    if let Ok(text) = core::str::from_utf8(&buffer) {
        crate::serial::write_str(text);
    }
}

const KIND_NAMES: [&str; 4] = ["sync", "irq", "fiq", "serror"];
const SOURCE_NAMES: [&str; 4] = ["el1t", "el1h", "el0-64", "el0-32"];

#[no_mangle]
pub extern "C" fn nc_exception_report(kind: u64, esr: u64, elr: u64, far: u64) -> ! {
    if IN_REPORT.load(core::sync::atomic::Ordering::Relaxed) {
        crate::halt_forever();
    }
    IN_REPORT.store(true, core::sync::atomic::Ordering::Relaxed);
    crate::serial::write_str("\n*** NetizenCore istisna: ");
    crate::serial::write_str(SOURCE_NAMES[((kind >> 2) & 3) as usize]);
    crate::serial::write_str(" / ");
    crate::serial::write_str(KIND_NAMES[(kind & 3) as usize]);
    crate::serial::write_str("\n esr=");
    write_hex(esr);
    crate::serial::write_str(" ec=");
    write_hex((esr >> 26) & 0x3F);
    crate::serial::write_str("\n elr=");
    write_hex(elr);
    crate::serial::write_str("\n far=");
    write_hex(far);
    crate::serial::write_str("\n");
    crate::halt_forever()
}
