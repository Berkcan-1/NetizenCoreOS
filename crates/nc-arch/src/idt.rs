// crates/nc-arch/src/idt.rs
use crate::{inb, outb, serial};
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

pub const KERNEL_DS: u16 = 0x10;
pub const KERNEL_CS: u16 = 0x18;
pub const USER_DS: u16   = 0x20 | 3; // 0x23 (Ring 3 Data Segment, DPL=3)
pub const USER_CS: u16   = 0x28 | 3; // 0x2B (Ring 3 64-bit Code Segment, DPL=3)
pub const TSS_SEL: u16   = 0x30;

const INPUT_BUF_SIZE: usize = 256;
static mut INPUT_BUF: [(u8, u8); INPUT_BUF_SIZE] = [(0, 0); INPUT_BUF_SIZE];
static INPUT_HEAD: AtomicUsize = AtomicUsize::new(0);
static INPUT_TAIL: AtomicUsize = AtomicUsize::new(0);

pub static IRQ_TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
pub static DROPPED_INPUT_EVENTS: AtomicU64 = AtomicU64::new(0);

pub type UserFaultHook = fn(&ExceptionFrame, u64) -> !;
static mut USER_FAULT_HOOK: Option<UserFaultHook> = None;

pub type SyscallDispatcher = fn(&mut SyscallFrame) -> i64;
static mut SYSCALL_DISPATCHER: Option<SyscallDispatcher> = None;

/// Ring 3 oturumu sırasında çekirdeğin geri döneceği güvenli RSP ve CR3 yuvası
static mut KERNEL_RETURN_RSP: u64 = 0;
static mut KERNEL_RETURN_CR3: u64 = 0;

pub fn register_user_fault_hook(hook: UserFaultHook) {
    unsafe {
        USER_FAULT_HOOK = Some(hook);
    }
}

pub fn register_syscall_dispatcher(dispatcher: SyscallDispatcher) {
    unsafe {
        SYSCALL_DISPATCHER = Some(dispatcher);
    }
}

#[inline]
pub fn push_input_event(status: u8, data: u8) {
    let head = INPUT_HEAD.load(Ordering::Acquire) & (INPUT_BUF_SIZE - 1);
    let next = (head + 1) & (INPUT_BUF_SIZE - 1);
    let tail = INPUT_TAIL.load(Ordering::Acquire) & (INPUT_BUF_SIZE - 1);
    if next != tail {
        unsafe {
            let buf_ptr = core::ptr::addr_of_mut!(INPUT_BUF) as *mut (u8, u8);
            core::ptr::write(buf_ptr.add(head), (status, data));
        }
        INPUT_HEAD.store(next, Ordering::Release);
    } else {
        DROPPED_INPUT_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub fn pop_input_event() -> Option<(u8, u8)> {
    let head = INPUT_HEAD.load(Ordering::Acquire) & (INPUT_BUF_SIZE - 1);
    let tail = INPUT_TAIL.load(Ordering::Acquire) & (INPUT_BUF_SIZE - 1);
    if tail == head {
        return None;
    }
    let safe_idx = tail & (INPUT_BUF_SIZE - 1);
    let item = unsafe {
        let buf_ptr = core::ptr::addr_of!(INPUT_BUF) as *const (u8, u8);
        core::ptr::read(buf_ptr.add(safe_idx))
    };
    let next_tail = (safe_idx + 1) & (INPUT_BUF_SIZE - 1);
    INPUT_TAIL.store(next_tail, Ordering::Release);
    Some(item)
}

const DOUBLE_FAULT_IST_INDEX: u8 = 1;
const IST_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct AlignedStack([u8; IST_STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack([0; IST_STACK_SIZE]);
static mut DEFAULT_RSP0_STACK: AlignedStack = AlignedStack([0; IST_STACK_SIZE]);

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TaskStateSegment {
    reserved_1: u32,
    rsp: [u64; 3],
    reserved_2: u64,
    ist: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    iomap_base: u16,
}

impl TaskStateSegment {
    const EMPTY: Self = TaskStateSegment {
        reserved_1: 0,
        rsp: [0; 3],
        reserved_2: 0,
        ist: [0; 7],
        reserved_3: 0,
        reserved_4: 0,
        iomap_base: core::mem::size_of::<TaskStateSegment>() as u16,
    };
}

static mut TSS: TaskStateSegment = TaskStateSegment::EMPTY;

#[repr(C, align(16))]
struct ArchGdt([u64; 8]);

static mut GDT: ArchGdt = ArchGdt([
    0x0000_0000_0000_0000, // 0x00: Null
    0x00CF_9A00_0000_FFFF, // 0x08: 32-bit Code
    0x00CF_9200_0000_FFFF, // 0x10: 64-bit Kernel Data (DPL=0)
    0x00AF_9A00_0000_FFFF, // 0x18: 64-bit Kernel Code (DPL=0)
    0x00CF_F200_0000_FFFF, // 0x20: 64-bit User Data   (DPL=3 -> Selector 0x23)
    0x00AF_FA00_0000_FFFF, // 0x28: 64-bit User Code   (DPL=3 -> Selector 0x2B)
    0x0000_0000_0000_0000, // 0x30: TSS Low
    0x0000_0000_0000_0000, // 0x38: TSS High
]);

#[inline]
pub fn set_kernel_stack(rsp0: u64) {
    unsafe {
        let aligned = rsp0 & !0xFu64;
        let tss_ptr = core::ptr::addr_of_mut!(TSS);
        core::ptr::write_unaligned(core::ptr::addr_of_mut!((*tss_ptr).rsp[0]), aligned);
    }
}

#[inline]
pub fn get_kernel_stack() -> u64 {
    unsafe {
        let tss_ptr = core::ptr::addr_of!(TSS);
        core::ptr::read_unaligned(core::ptr::addr_of!((*tss_ptr).rsp[0]))
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const EMPTY: Self = IdtEntry {
        offset_low: 0,
        selector: 0,
        ist: 0,
        type_attr: 0,
        offset_mid: 0,
        offset_high: 0,
        zero: 0,
    };

    fn new_with_ist_and_dpl(handler: unsafe extern "C" fn(), ist_index: u8, dpl: u8) -> Self {
        let addr = handler as usize;
        let attr = 0x8E | ((dpl & 0x03) << 5);
        IdtEntry {
            offset_low: (addr & 0xFFFF) as u16,
            selector: KERNEL_CS,
            ist: ist_index & 0x07,
            type_attr: attr,
            offset_mid: ((addr >> 16) & 0xFFFF) as u16,
            offset_high: (addr >> 32) as u32,
            zero: 0,
        }
    }

    #[inline]
    fn new(handler: unsafe extern "C" fn()) -> Self {
        Self::new_with_ist_and_dpl(handler, 0, 0)
    }
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::EMPTY; 256];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExceptionFrame {
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ExceptionFrame {
    #[inline]
    pub const fn is_user_mode(&self) -> bool {
        (self.cs & 3) == 3
    }
}

/// `int 0x80` sırasında çekirdek yığınına kaydedilen tam kullanıcı bağlamı
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SyscallFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub iret_rip: u64,
    pub iret_cs: u64,
    pub iret_rflags: u64,
    pub iret_rsp: u64,
    pub iret_ss: u64,
}

global_asm!(
    r#"
    .macro SAVE_REGS
        push rbp
        push rax
        push rcx
        push rdx
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        mov rbp, rsp
        and rsp, -16
    .endm

    .macro RESTORE_REGS
        mov rsp, rbp
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rax
        pop rbp
    .endm

    .macro ISR_NOERR vec, name
    .global \name
    \name:
        cli
        push 0
        push \vec
        jmp isr_exception_common
    .endm

    .macro ISR_ERR vec, name
    .global \name
    \name:
        cli
        push \vec
        jmp isr_exception_common
    .endm

    isr_exception_common:
        cld
        mov ax, 0x10
        mov ds, ax
        mov es, ax
        mov rdi, rsp
        and rsp, -16
        call rust_exception_handler
    2:  hlt
        jmp 2b

    ISR_NOERR 0,  isr_vec_0
    ISR_NOERR 1,  isr_vec_1
    ISR_NOERR 2,  isr_vec_2
    ISR_NOERR 3,  isr_vec_3
    ISR_NOERR 4,  isr_vec_4
    ISR_NOERR 5,  isr_vec_5
    ISR_NOERR 6,  isr_vec_6
    ISR_NOERR 7,  isr_vec_7
    ISR_ERR   8,  isr_vec_8_df
    ISR_NOERR 9,  isr_vec_9
    ISR_ERR   10, isr_vec_10
    ISR_ERR   11, isr_vec_11
    ISR_ERR   12, isr_vec_12
    ISR_ERR   13, isr_vec_13_gp
    ISR_ERR   14, isr_vec_14_pf
    ISR_NOERR 15, isr_vec_15
    ISR_NOERR 16, isr_vec_16
    ISR_ERR   17, isr_vec_17
    ISR_NOERR 18, isr_vec_18
    ISR_NOERR 19, isr_vec_19
    ISR_NOERR 20, isr_vec_20
    ISR_ERR   21, isr_vec_21
    ISR_NOERR 22, isr_vec_22
    ISR_NOERR 23, isr_vec_23
    ISR_NOERR 24, isr_vec_24
    ISR_NOERR 25, isr_vec_25
    ISR_NOERR 26, isr_vec_26
    ISR_NOERR 27, isr_vec_27
    ISR_NOERR 28, isr_vec_28
    ISR_ERR   29, isr_vec_29
    ISR_ERR   30, isr_vec_30
    ISR_NOERR 31, isr_vec_31

    .global isr_timer_irq
    isr_timer_irq:
        SAVE_REGS
        cld
        call rust_timer_irq_handler
        RESTORE_REGS
        iretq

    .global isr_keyboard_irq
    isr_keyboard_irq:
        SAVE_REGS
        cld
        call rust_keyboard_irq_handler
        RESTORE_REGS
        iretq

    .global isr_mouse_irq
    isr_mouse_irq:
        SAVE_REGS
        cld
        call rust_mouse_irq_handler
        RESTORE_REGS
        iretq

    .global isr_spurious_irq
    isr_spurious_irq:
        SAVE_REGS
        cld
        call rust_spurious_irq_handler
        RESTORE_REGS
        iretq

    // int 0x80 (DPL=3) Sistem Çağrısı Girişi
    .global isr_syscall_80
    isr_syscall_80:
        cli
        cld
        push rax
        push rbx
        push rcx
        push rdx
        push rsi
        push rdi
        push rbp
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        mov ax, 0x10
        mov ds, ax
        mov es, ax
        mov rdi, rsp
        mov rbp, rsp
        and rsp, -16
        call rust_syscall_handler
        mov rsp, rbp
        mov [rsp + 14*8], rax
        mov ax, 0x23
        mov ds, ax
        mov es, ax
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rbx
        pop rax
        iretq

    .global switch_context
    switch_context:
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15
        mov [rdi], rsp
        mov rsp, rsi
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        ret

    .global task_entry_trampoline
    task_entry_trampoline:
        and rsp, -16
        call r12
    4:  hlt
        jmp 4b

    // Ring 3 görev oturumu başlatıcı:
    // rdi = user_rip, rsi = user_rsp, rdx = user_cr3, rcx = kernel_rsp0
    // Çekirdek callee-saved yazmaçlarını ve CR3'ü kaydeder, Ring 3'e iner.
    // Süreç sys_exit / sys_yield / fault yaptığında abort_ring3_to_kernel ile buraya döner.
    .global run_ring3_task_session
    run_ring3_task_session:
        cli
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15
        mov rax, cr3
        mov [rip + {kernel_cr3_sym}], rax
        mov [rip + {kernel_rsp_sym}], rsp

        // TSS.RSP0 zaten çağıran tarafından ayarlandı; kullanıcı CR3'ünü yükle
        mov cr3, rdx

        mov ax, 0x23
        mov ds, ax
        mov es, ax
        xor eax, eax
        mov fs, ax
        mov gs, ax

        push 0x23
        push rsi
        push 0x202
        push 0x2B
        push rdi

        xor rax, rax
        xor rbx, rbx
        xor rcx, rcx
        xor rdx, rdx
        xor rsi, rsi
        xor rdi, rdi
        xor rbp, rbp
        xor r8, r8
        xor r9, r9
        xor r10, r10
        xor r11, r11
        xor r12, r12
        xor r13, r13
        xor r14, r14
        xor r15, r15
        iretq

    // rdi = dönüş kodu (0 = normal exit, >0 = fault/exception, <0 = yield)
    .global abort_ring3_to_kernel
    abort_ring3_to_kernel:
        cli
        mov ax, 0x10
        mov ds, ax
        mov es, ax
        mov ss, ax
        mov rdx, [rip + {kernel_cr3_sym}]
        mov cr3, rdx
        mov rsp, [rip + {kernel_rsp_sym}]
        mov rax, rdi
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        sti
        ret
    "#,
    kernel_rsp_sym = sym KERNEL_RETURN_RSP,
    kernel_cr3_sym = sym KERNEL_RETURN_CR3,
);

extern "C" {
    fn isr_vec_0();
    fn isr_vec_1();
    fn isr_vec_2();
    fn isr_vec_3();
    fn isr_vec_4();
    fn isr_vec_5();
    fn isr_vec_6();
    fn isr_vec_7();
    fn isr_vec_8_df();
    fn isr_vec_9();
    fn isr_vec_10();
    fn isr_vec_11();
    fn isr_vec_12();
    fn isr_vec_13_gp();
    fn isr_vec_14_pf();
    fn isr_vec_15();
    fn isr_vec_16();
    fn isr_vec_17();
    fn isr_vec_18();
    fn isr_vec_19();
    fn isr_vec_20();
    fn isr_vec_21();
    fn isr_vec_22();
    fn isr_vec_23();
    fn isr_vec_24();
    fn isr_vec_25();
    fn isr_vec_26();
    fn isr_vec_27();
    fn isr_vec_28();
    fn isr_vec_29();
    fn isr_vec_30();
    fn isr_vec_31();
    fn isr_timer_irq();
    fn isr_keyboard_irq();
    fn isr_mouse_irq();
    fn isr_spurious_irq();
    fn isr_syscall_80();
    pub fn switch_context(old_rsp: *mut u64, new_rsp: u64);
    pub fn task_entry_trampoline();
    pub fn run_ring3_task_session(user_rip: u64, user_rsp: u64, user_cr3: u64, kernel_rsp0: u64) -> i64;
    pub fn abort_ring3_to_kernel(status_code: i64) -> !;
}

#[no_mangle]
pub extern "C" fn rust_syscall_handler(frame_ptr: *mut SyscallFrame) -> i64 {
    let frame = unsafe { &mut *frame_ptr };
    // Faz 4 Sertleştirmesi: Kullanıcıya dönerken IOPL=0 (bit 12-13 temiz) ve IF=1 (bit 9 açık) zorla
    frame.iret_rflags = (frame.iret_rflags & !0x3000) | 0x0202;
    unsafe {
        if let Some(dispatcher) = SYSCALL_DISPATCHER {
            return dispatcher(frame);
        }
    }
    -38 // ENOSYS
}

#[no_mangle]
pub extern "C" fn rust_exception_handler(frame_ptr: *const ExceptionFrame) -> ! {
    let frame = unsafe { &*frame_ptr };
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }

    if frame.is_user_mode() {
        serial::write_str("\n[RING-3 USER FAULT] VEC=");
        write_hex(frame.vector);
        serial::write_str(" ERR=");
        write_hex(frame.error_code);
        serial::write_str(" RIP=");
        write_hex(frame.rip);
        serial::write_str(" CS=");
        write_hex(frame.cs);
        serial::write_str(" CR2=");
        write_hex(cr2);
        serial::write_str(" -> Yalnizca ilgili kullanici sureci sonlandiriliyor.\n");

        unsafe {
            if let Some(hook) = USER_FAULT_HOOK {
                hook(frame, cr2);
            }
            abort_ring3_to_kernel(1000 + (frame.vector as i64));
        }
    }

    if frame.vector == 14 {
        serial::write_str("\n[KERNEL PAGE FAULT (RING-0)] CR2=");
        write_hex(cr2);
    } else if frame.vector == 8 {
        serial::write_str("\n[DOUBLE FAULT (IST1)] ");
    } else {
        serial::write_str("\n[KERNEL CPU EXCEPTION (RING-0)] VEC=");
        write_hex(frame.vector);
    }
    serial::write_str(" ERR=");
    write_hex(frame.error_code);
    serial::write_str(" RIP=");
    write_hex(frame.rip);
    serial::write_str(" CS=");
    write_hex(frame.cs);
    serial::write_str(" RFLAGS=");
    write_hex(frame.rflags);
    serial::write_str(" RSP=");
    write_hex(frame.rsp);
    serial::write_str("\n");
    crate::halt_forever();
}

#[no_mangle]
pub extern "C" fn rust_timer_irq_handler() {
    IRQ_TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        outb(PIC1_CMD, 0x20);
    }
}

#[no_mangle]
pub extern "C" fn rust_keyboard_irq_handler() {
    unsafe {
        for _ in 0..16 {
            let status = inb(0x64);
            if status & 0x01 == 0 {
                break;
            }
            let data = inb(0x60);
            push_input_event(status, data);
        }
        outb(PIC1_CMD, 0x20);
    }
}

#[no_mangle]
pub extern "C" fn rust_mouse_irq_handler() {
    unsafe {
        for _ in 0..16 {
            let status = inb(0x64);
            if status & 0x01 == 0 {
                break;
            }
            let data = inb(0x60);
            push_input_event(status, data);
        }
        outb(PIC2_CMD, 0x20);
        outb(PIC1_CMD, 0x20);
    }
}

#[no_mangle]
pub extern "C" fn rust_spurious_irq_handler() {
    unsafe {
        outb(PIC1_CMD, 0x20);
    }
}

fn write_hex(mut val: u64) {
    let mut buf = [b'0'; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in (2..18).rev() {
        let nibble = (val & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + (nibble - 10)
        };
        val >>= 4;
    }
    if let Ok(s) = core::str::from_utf8(&buf) {
        serial::write_str(s);
    }
}

unsafe fn init_gdt_and_tss() {
    let df_stack_top = (core::ptr::addr_of!(DOUBLE_FAULT_STACK) as u64) + (IST_STACK_SIZE as u64);
    let rsp0_default_top = (core::ptr::addr_of!(DEFAULT_RSP0_STACK) as u64) + (IST_STACK_SIZE as u64);
    let tss_ptr = core::ptr::addr_of_mut!(TSS);
    (*tss_ptr).rsp[0] = rsp0_default_top;
    (*tss_ptr).ist[(DOUBLE_FAULT_IST_INDEX - 1) as usize] = df_stack_top;
    (*tss_ptr).iomap_base = core::mem::size_of::<TaskStateSegment>() as u16;

    let tss_base = tss_ptr as u64;
    let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;
    let tss_low = (tss_limit & 0xFFFF)
        | ((tss_base & 0xFFFF_FF) << 16)
        | (0x89u64 << 40)
        | (((tss_limit >> 16) & 0xF) << 48)
        | (((tss_base >> 24) & 0xFF) << 56);
    let tss_high = tss_base >> 32;

    let gdt_ptr = core::ptr::addr_of_mut!(GDT);
    (*gdt_ptr).0[6] = tss_low;
    (*gdt_ptr).0[7] = tss_high;

    let gdt_desc = DescriptorTablePointer {
        limit: (core::mem::size_of::<ArchGdt>() - 1) as u16,
        base: gdt_ptr as u64,
    };

    asm!(
        "lgdt [{0}]",
        "push 0x18",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        "mov ax, 0x10",
        "mov ds, ax",
        "mov es, ax",
        "mov ss, ax",
        "mov ax, 0x30",
        "ltr ax",
        in(reg) &gdt_desc,
        out("rax") _,
        options(preserves_flags)
    );
}

unsafe fn remap_and_enable_pic() {
    outb(PIC1_CMD, 0x11);
    outb(PIC2_CMD, 0x11);
    outb(PIC1_DATA, 0x20);
    outb(PIC2_DATA, 0x28);
    outb(PIC1_DATA, 0x04);
    outb(PIC2_DATA, 0x02);
    outb(PIC1_DATA, 0x01);
    outb(PIC2_DATA, 0x01);
    outb(0x43, 0x36);
    outb(0x40, 0xA9);
    outb(0x40, 0x04);
    outb(PIC1_DATA, 0xF8);
    outb(PIC2_DATA, 0xEF);
}

pub fn init() {
    unsafe {
        init_gdt_and_tss();
        let handlers: [unsafe extern "C" fn(); 32] = [
            isr_vec_0,
            isr_vec_1,
            isr_vec_2,
            isr_vec_3,
            isr_vec_4,
            isr_vec_5,
            isr_vec_6,
            isr_vec_7,
            isr_vec_8_df,
            isr_vec_9,
            isr_vec_10,
            isr_vec_11,
            isr_vec_12,
            isr_vec_13_gp,
            isr_vec_14_pf,
            isr_vec_15,
            isr_vec_16,
            isr_vec_17,
            isr_vec_18,
            isr_vec_19,
            isr_vec_20,
            isr_vec_21,
            isr_vec_22,
            isr_vec_23,
            isr_vec_24,
            isr_vec_25,
            isr_vec_26,
            isr_vec_27,
            isr_vec_28,
            isr_vec_29,
            isr_vec_30,
            isr_vec_31,
        ];
        let idt_ptr = core::ptr::addr_of_mut!(IDT) as *mut IdtEntry;
        for i in 0..32 {
            let entry = if i == 8 {
                IdtEntry::new_with_ist_and_dpl(handlers[i], DOUBLE_FAULT_IST_INDEX, 0)
            } else {
                IdtEntry::new(handlers[i])
            };
            core::ptr::write(idt_ptr.add(i), entry);
        }
        for i in 32..48 {
            core::ptr::write(idt_ptr.add(i), IdtEntry::new(isr_spurious_irq));
        }
        core::ptr::write(idt_ptr.add(32), IdtEntry::new(isr_timer_irq));
        core::ptr::write(idt_ptr.add(33), IdtEntry::new(isr_keyboard_irq));
        core::ptr::write(idt_ptr.add(44), IdtEntry::new(isr_mouse_irq));

        // 0x80: Ring 3 Sistem Çağrısı Kapısı (DPL = 3)
        core::ptr::write(
            idt_ptr.add(0x80),
            IdtEntry::new_with_ist_and_dpl(isr_syscall_80, 0, 3),
        );

        let desc = DescriptorTablePointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: idt_ptr as u64,
        };
        asm!("lidt [{}]", in(reg) &desc, options(readonly, nostack, preserves_flags));
        remap_and_enable_pic();
        asm!("sti", options(nomem, nostack));
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct KernelThreadContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64,
}

pub type TaskContext = KernelThreadContext;

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct UserContext {
    pub cr3: u64,
    pub user_rip: u64,
    pub user_rsp: u64,
    pub rflags: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}
