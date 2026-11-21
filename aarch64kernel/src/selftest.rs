use crate::{task, vm};
use core::arch::global_asm;

global_asm!(
    r#"
    .section .rodata.ncuser,"a",@progbits
    .balign 4
.balign 4
nc_user_a_start:
    mov x20, #4
1:
    mov x8, #4
    mov x0, #1
    adr x1, 2f
    mov x2, #16
    svc #0
    mov x8, #3
    mov x0, #30
    svc #0
    subs x20, x20, #1
    b.ne 1b
    mov x8, #1
    mov x0, #0
    svc #0
2:
    .ascii "[A] merhaba EL0\n"

nc_user_a_end:
.balign 4
nc_user_b_start:
    mov x20, #3
1:
    mov x8, #4
    mov x0, #1
    adr x1, 2f
    mov x2, #20
    svc #0
    mov x8, #3
    mov x0, #45
    svc #0
    subs x20, x20, #1
    b.ne 1b
    mov x8, #1
    mov x0, #0
    svc #0
2:
    .ascii "[B] gorev iki canli\n"

nc_user_b_end:
.balign 4
nc_user_fault_start:
    mov x0, #0x40000000
    ldr x1, [x0]
    mov x8, #1
    svc #0
nc_user_fault_end:
.balign 4
nc_user_badptr_start:
    mov x8, #4
    mov x0, #1
    mov x1, #0x1000
    mov x2, #8
    svc #0
    mov x8, #1
    svc #0
nc_user_badptr_end:
.balign 4
nc_user_spin_start:
    mov x1, #0x4000
    lsl x1, x1, #12
1:
    subs x1, x1, #1
    b.ne 1b
    mov x8, #1
    mov x0, #7
    svc #0
nc_user_spin_end:
    "#
);

extern "C" {
    static nc_user_a_start: u8;
    static nc_user_a_end: u8;
    static nc_user_b_start: u8;
    static nc_user_b_end: u8;
    static nc_user_fault_start: u8;
    static nc_user_fault_end: u8;
    static nc_user_badptr_start: u8;
    static nc_user_badptr_end: u8;
    static nc_user_spin_start: u8;
    static nc_user_spin_end: u8;
}

fn blob(start: &'static u8, end: &'static u8) -> &'static [u8] {
    let begin = start as *const u8 as usize;
    let finish = end as *const u8 as usize;
    unsafe { core::slice::from_raw_parts(begin as *const u8, finish - begin) }
}

static mut FREE_BEFORE: usize = usize::MAX;

pub fn run(continuation: fn() -> !) -> ! {
    task::init(continuation);
    unsafe {
        FREE_BEFORE = vm::free_frames();
    }
    let tasks: [(&'static str, &'static [u8]); 5] = unsafe {
        [
            ("test-a", blob(&nc_user_a_start, &nc_user_a_end)),
            ("test-b", blob(&nc_user_b_start, &nc_user_b_end)),
            ("test-hata", blob(&nc_user_fault_start, &nc_user_fault_end)),
            ("test-gecersiz-isaretci", blob(&nc_user_badptr_start, &nc_user_badptr_end)),
            ("test-mesgul-dongu", blob(&nc_user_spin_start, &nc_user_spin_end)),
        ]
    };
    nc_arch::serial::write_str("EL0 oz-sinama basliyor\n");
    for (name, code) in tasks {
        if task::spawn(name, code, 0).is_none() {
            nc_arch::serial::write_str("gorev olusturulamadi: ");
            nc_arch::serial::write_str(name);
            nc_arch::serial::write_str("\n");
        }
    }
    task::start()
}

pub fn report() {
    let before = unsafe { FREE_BEFORE };
    if before != usize::MAX {
        task::free_before_report(before);
    }
}
