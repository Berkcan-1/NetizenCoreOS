use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.head,"ax",@progbits
    .global _start
_start:
    b primary_entry
    .long 0
    .quad __nc_text_offset
    .quad __nc_image_size
    .quad 0x2
    .quad 0
    .quad 0
    .quad 0
    .long 0x644d5241
    .long 0

    .balign 4
primary_entry:
    mov x19, x0

    mrs x0, CurrentEL
    lsr x0, x0, #2
    cmp x0, #2
    b.ne 2f

    mov x0, #(1 << 31)
    msr hcr_el2, x0
    mrs x0, midr_el1
    msr vpidr_el2, x0
    mrs x0, mpidr_el1
    msr vmpidr_el2, x0
    mov x0, #0x33ff
    msr cptr_el2, x0
    msr hstr_el2, xzr
    mrs x0, cnthctl_el2
    orr x0, x0, #3
    msr cnthctl_el2, x0
    msr cntvoff_el2, xzr
    movz x0, #0x0800
    movk x0, #0x30d0, lsl #16
    msr sctlr_el1, x0
    mov x0, #0x3c5
    msr spsr_el2, x0
    adr x0, 2f
    msr elr_el2, x0
    eret

2:
    msr daifset, #0xf
    movz x0, #0x0800
    movk x0, #0x30d0, lsl #16
    msr sctlr_el1, x0
    mov x0, #(3 << 20)
    msr cpacr_el1, x0
    isb

    adrp x0, __stack_top
    add x0, x0, :lo12:__stack_top
    mov sp, x0

    adrp x0, __bss_start
    add x0, x0, :lo12:__bss_start
    adrp x1, __bss_end
    add x1, x1, :lo12:__bss_end
3:
    cmp x0, x1
    b.hs 4f
    stp xzr, xzr, [x0], #16
    b 3b
4:
    mrs x2, ctr_el0
    ubfx x2, x2, #16, #4
    mov x3, #4
    lsl x3, x3, x2
    adrp x0, __bss_start
    add x0, x0, :lo12:__bss_start
    adrp x1, __kernel_end
    add x1, x1, :lo12:__kernel_end
6:
    dc ivac, x0
    add x0, x0, x3
    cmp x0, x1
    b.lo 6b
    dsb sy
    isb
    mov x0, x19
    bl nc_main
5:
    wfe
    b 5b

    .balign 4
    .global nc_secondary_trampoline
nc_secondary_trampoline:
    // x0 = cpu_id (context_id passed via PSCI CPU_ON)
    mov x19, x0

    mrs x0, CurrentEL
    lsr x0, x0, #2
    cmp x0, #2
    b.ne 12f

    mov x0, #(1 << 31)
    msr hcr_el2, x0
    mrs x0, midr_el1
    msr vpidr_el2, x0
    mrs x0, mpidr_el1
    msr vmpidr_el2, x0
    mov x0, #0x33ff
    msr cptr_el2, x0
    msr hstr_el2, xzr
    mrs x0, cnthctl_el2
    orr x0, x0, #3
    msr cnthctl_el2, x0
    msr cntvoff_el2, xzr
    movz x0, #0x0800
    movk x0, #0x30d0, lsl #16
    msr sctlr_el1, x0
    mov x0, #0x3c5
    msr spsr_el2, x0
    adr x0, 12f
    msr elr_el2, x0
    eret

12:
    msr daifset, #0xf
    movz x0, #0x0800
    movk x0, #0x30d0, lsl #16
    msr sctlr_el1, x0
    mov x0, #(3 << 20)
    msr cpacr_el1, x0
    isb

    // Load per-CPU kernel stack top: SECONDARY_STACKS + (cpu_id + 1) * 16384
    adrp x1, NC_SECONDARY_STACKS
    add x1, x1, :lo12:NC_SECONDARY_STACKS
    add x2, x19, #1
    lsl x2, x2, #14
    add x1, x1, x2
    mov sp, x1

    mov x0, x19
    bl nc_secondary_main
15:
    wfe
    b 15b
    "#
);
