// kernel/src/entry.rs

use core::arch::global_asm;

global_asm!(
    r#"
    .pushsection .text.entry,"ax",@progbits
    .global _start
_start:
    cli
    cld

    lea rsp, [rip + __stack_top]
    xor ebp, ebp

    lea rdi, [rip + __bss_start]
    lea rcx, [rip + __bss_end]
    sub rcx, rdi
    shr rcx, 3
    xor eax, eax
    rep stosq

    mov rax, cr0
    and ax, 0xFFFB
    or ax, 0x0002
    mov cr0, rax
    mov rax, cr4
    or rax, 3 << 9
    mov cr4, rax

    call kernel_main
.Lhalt:
    cli
    hlt
    jmp .Lhalt
    .popsection

    .pushsection .stack,"aw",@nobits
    .balign 16
__stack_bottom:
    .skip 524288
__stack_top:
    .popsection
"#
);