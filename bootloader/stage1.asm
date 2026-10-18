bits 16
org 0x7C00

STAGE2_SEGMENT equ 0x0880
STAGE2_ENTRY equ 0x8800
BOUNCE_SEGMENT equ 0x0980
BOUNCE_BUFFER equ 0x9800
KERNEL_BASE equ 0x00100000

start:
    jmp short entry
    nop
    times 8 - ($ - $$) db 0

boot_info_pvd:    dd 0
boot_info_file:   dd 0
boot_info_length: dd 0
boot_info_csum:   dd 0
boot_info_pad:    times 10 dd 0

entry:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    cld

    mov [boot_drive], dl

    in al, 0x92
    or al, 2
    and al, 0xFE
    out 0x92, al

    sti

    mov ah, 0x41
    mov bx, 0x55AA
    mov dl, [boot_drive]
    int 0x13
    jc disk_fail
    cmp bx, 0xAA55
    jne disk_fail

    mov eax, [boot_info_length]
    add eax, 2047
    shr eax, 11
    sub eax, 3
    jbe disk_fail
    mov [kernel_sectors], ax

    mov eax, [boot_info_file]
    inc eax
    mov [dap_lba], eax
    mov word [dap_count], 2
    mov word [dap_segment], STAGE2_SEGMENT
    mov word [dap_offset], 0
    mov si, dap
    mov dl, [boot_drive]
    mov ah, 0x42
    int 0x13
    jc disk_fail

    add dword [dap_lba], 2
    mov word [dap_count], 1
    mov word [dap_segment], BOUNCE_SEGMENT
    mov dword [target_ptr], KERNEL_BASE

.kernel_loop:
    cmp word [kernel_sectors], 0
    je .done

    mov si, dap
    mov dl, [boot_drive]
    mov ah, 0x42
    int 0x13
    jc disk_fail

    cli
    lgdt [unreal_gdt_desc]
    mov eax, cr0
    or al, 1
    mov cr0, eax
    mov bx, 0x08
    mov ds, bx
    mov es, bx


    mov esi, BOUNCE_BUFFER
    mov edi, [target_ptr]
    mov ecx, 512
    a32 rep movsd
    mov [target_ptr], edi

    and al, 0xFE
    mov cr0, eax
    xor ax, ax
    mov ds, ax
    mov es, ax
    sti

    inc dword [dap_lba]
    dec word [kernel_sectors]
    jmp .kernel_loop

.done:
    mov dl, [boot_drive]
    jmp 0x0000:STAGE2_ENTRY

disk_fail:
    mov si, message_fail
.print:
    lodsb
    test al, al
    jz .halt
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp .print
.halt:
    cli
    hlt
    jmp .halt

boot_drive:     db 0
kernel_sectors: dw 0
target_ptr:     dd KERNEL_BASE

align 4
dap:
    db 0x10
    db 0
dap_count:   dw 1
dap_offset:  dw 0
dap_segment: dw STAGE2_SEGMENT
dap_lba:     dd 0
             dd 0

align 8
unreal_gdt:
    dq 0
    dq 0x00CF92000000FFFF
unreal_gdt_end:

unreal_gdt_desc:
    dw unreal_gdt_end - unreal_gdt - 1
    dd unreal_gdt

message_fail: db 'NetizenCore: ISO okuma hatasi', 0

times 2048 - ($ - $$) db 0
