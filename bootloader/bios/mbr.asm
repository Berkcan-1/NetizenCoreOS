bits 16
org 0x7C00

mbr_start:
    jmp short mbr_entry
    nop
    db "NETIZEN"

mbr_entry:
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
    int 0x13
    jc disk_fail
    cmp bx, 0xAA55
    jne disk_fail

    mov word [dap_count], 8
    mov word [dap_segment], 0x0880
    mov dword [dap_lba], 2048
    mov si, dap
    mov dl, [boot_drive]
    mov ah, 0x42
    int 0x13
    jc disk_fail

    ; Bulgu #11: Stage2 sonunda (0x97F8) "NCKH" basligi varsa gercek sektor sayisini al
    mov ax, 4096
    cmp dword [0x97F8], 0x4E434B48
    jne .use_sectors
    mov cx, [0x97FC]
    test cx, cx
    jz .use_sectors
    cmp cx, 8192
    ja .use_sectors
    mov ax, cx
.use_sectors:
    add ax, 63
    and ax, 0xFFC0
    mov [sectors_left], ax

    mov dword [dap_lba], 2056
    mov word [dap_segment], 0x1000
    mov dword [target_ptr], 0x00100000

.read_loop:
    cmp word [sectors_left], 0
    jbe .done_reading
    mov word [dap_count], 64
    sub word [sectors_left], 64
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
    mov esi, 0x00010000
    mov edi, [target_ptr]
    mov ecx, 8192
    a32 rep movsd
    mov [target_ptr], edi
    and al, 0xFE
    mov cr0, eax
    xor ax, ax
    mov ds, ax
    mov es, ax
    sti

    add dword [dap_lba], 64
    jmp .read_loop

.done_reading:
    mov dl, [boot_drive]
    jmp 0x0000:0x8800

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

boot_drive:   db 0
sectors_left: dw 0
target_ptr:   dd 0x00100000

align 4
dap:
    db 0x10
    db 0
dap_count:   dw 0
dap_offset:  dw 0x0000
dap_segment: dw 0x0880
dap_lba:     dd 2048
             dd 0

align 8
unreal_gdt:
    dq 0
    dq 0x00CF92000000FFFF
unreal_gdt_end:

unreal_gdt_desc:
    dw unreal_gdt_end - unreal_gdt - 1
    dd unreal_gdt

message_fail: db "NetizenCore: MBR hatasi", 0

times 446 - ($ - $$) db 0
