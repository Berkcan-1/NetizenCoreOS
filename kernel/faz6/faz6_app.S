# Otomatik uretildi (gen.py). Duzenleme icin gen.py'yi degistirin.
# NetizenCore Ring 3 ABI v1 test uygulamasi (olay-tabanli). Secici: EventBlock.flags
#   0 normal | 1 ud2 | 2 salt-okunur olay sayfasina yaz | 3 kotu cizim akislari | 4 kotu dosya cagrilari
#   5 sonsuz dongu (watchdog) | 6 sistem cagrisi yagdirma (watchdog)
.intel_syntax noprefix
.globl _start

.section .text
_start:
    movabs rbx, 0x8000F00000
    inc DWORD PTR [rip + counter]
    mov r12d, DWORD PTR [rbx + 12]
    cmp r12d, 1
    je .Lud2
    cmp r12d, 2
    je .Lwrite_ro
    cmp r12d, 3
    je .Lbad_draw
    cmp r12d, 4
    je .Lbad_file
    cmp r12d, 5
    je .Lhang
    cmp r12d, 6
    je .Lspam
    mov eax, DWORD PTR [rbx + 8]
    test eax, eax
    je .Linit
    cmp eax, 2
    je .Ldraw
    cmp eax, 3
    je .Lclose
    mov r13d, 1
    jmp .Ldone

.Lclose:
    mov r13d, 2
    jmp .Ldone

.Ldraw:
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    lea rsi, [rip + cmds]
    mov edx, 224
    int 0x80
    mov r13d, 1
    cmp rax, 8
    je .Ldone
    or r13d, 0x80000000
    jmp .Ldone

.Linit:
    xor r13d, r13d
    mov eax, 4
    mov edi, 1
    lea rsi, [rip + msg]
    mov edx, 21
    int 0x80
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    lea rsi, [rip + cmds]
    mov edx, 224
    int 0x80
    cmp rax, -11
    jne .Lfail
    or r13d, 0x400
    mov eax, 21
    lea rdi, [rip + hello]
    mov esi, 5
    mov edx, 16
    int 0x80
    test rax, rax
    jle .Lfail
    or r13d, 0x200
    mov eax, 17
    lea rdi, [rip + path]
    mov esi, 13
    mov edx, 2
    int 0x80
    test rax, rax
    js .Lfail
    mov r14, rax
    mov eax, 19
    mov rdi, r14
    lea rsi, [rip + payload]
    mov edx, 17
    int 0x80
    cmp rax, 17
    jne .Lfail
    mov eax, 22
    mov rdi, r14
    int 0x80
    test rax, rax
    jne .Lfail
    mov eax, 17
    lea rdi, [rip + path]
    mov esi, 13
    mov edx, 1
    int 0x80
    test rax, rax
    js .Lfail
    mov r14, rax
    mov eax, 18
    mov rdi, r14
    lea rsi, [rip + rdbuf]
    mov edx, 64
    int 0x80
    cmp rax, 17
    jne .Lfail
    mov eax, 22
    mov rdi, r14
    int 0x80
    test rax, rax
    jne .Lfail
    lea rsi, [rip + payload]
    lea rdi, [rip + rdbuf]
    mov ecx, 17
    repe cmpsb
    jne .Lfail
    or r13d, 0x100
    or r13d, 1
    jmp .Ldone

.Lbad_draw:
    xor r13d, r13d
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    lea rsi, [rip + badcmds]
    mov edx, 4
    int 0x80
    cmp rax, -22
    jne .Lfail
    or r13d, 0x1000
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    lea rsi, [rip + cmds]
    mov edx, 0x100000
    int 0x80
    cmp rax, -22
    jne .Lfail
    or r13d, 0x2000
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    movabs rsi, 0xFFFF800000000000
    mov edx, 24
    int 0x80
    cmp rax, -14
    jne .Lfail
    or r13d, 0x4000
    mov eax, 20
    mov edi, 99
    lea rsi, [rip + cmds]
    mov edx, 224
    int 0x80
    cmp rax, -9
    jne .Lfail
    or r13d, 0x8000
    or r13d, 1
    jmp .Ldone

.Lbad_file:
    xor r13d, r13d
    mov eax, 17
    lea rdi, [rip + deny_read]
    mov esi, 19
    mov edx, 1
    int 0x80
    cmp rax, -13
    jne .Lfail
    or r13d, 0x100
    mov eax, 17
    lea rdi, [rip + traversal]
    mov esi, 19
    mov edx, 1
    int 0x80
    cmp rax, -22
    jne .Lfail
    or r13d, 0x200
    mov eax, 17
    lea rdi, [rip + ok_ram]
    mov esi, 6
    mov edx, 7
    int 0x80
    cmp rax, -22
    jne .Lfail
    or r13d, 0x400
    mov eax, 18
    mov edi, 77
    lea rsi, [rip + rdbuf]
    mov edx, 8
    int 0x80
    cmp rax, -9
    jne .Lfail
    or r13d, 0x800
    or r13d, 1
    jmp .Ldone

.Lfail:
    or r13d, 0x80000000
.Ldone:
    mov eax, DWORD PTR [rip + counter]
    and eax, 0xFF
    shl eax, 16
    or r13d, eax
    mov eax, 16
    mov edi, r13d
    int 0x80
    ud2

.Lud2:
    ud2

.Lwrite_ro:
    mov DWORD PTR [rbx], 0
    ud2

.Lhang:
    jmp .Lhang

.Lspam:
    mov eax, 5
    int 0x80
    jmp .Lspam

.section .rodata
cmds:
    .byte 1, 0, 24, 0, 248, 255, 255, 255, 248, 255, 255, 255, 200, 0, 0, 0
    .byte 200, 0, 0, 0, 255, 0, 0, 0, 1, 0, 24, 0, 10, 0, 0, 0
    .byte 6, 0, 0, 0, 40, 0, 0, 0, 20, 0, 0, 0, 0, 0, 255, 0
    .byte 2, 0, 40, 0, 0, 0, 0, 0, 30, 0, 0, 0, 30, 0, 0, 0
    .byte 20, 0, 0, 0, 4, 0, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0
    .byte 204, 204, 204, 0, 255, 0, 0, 0, 3, 0, 24, 0, 50, 0, 0, 0
    .byte 45, 0, 0, 0, 5, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0
    .byte 4, 0, 28, 0, 4, 0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0
    .byte 255, 255, 255, 0, 2, 0, 0, 0, 72, 105, 0, 0, 1, 0, 24, 0
    .byte 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
    .byte 0, 0, 0, 0, 5, 0, 32, 0, 58, 0, 0, 0, 36, 0, 0, 0
    .byte 36, 0, 0, 0, 16, 0, 0, 0, 240, 128, 48, 0, 2, 0, 1, 0
    .byte 71, 111, 0, 0, 4, 0, 28, 0, 48, 0, 0, 0, 54, 0, 0, 0
    .byte 10, 0, 0, 0, 255, 255, 255, 0, 2, 0, 30, 1, 79, 107, 0, 0
badcmds:
    .byte 99, 0, 4, 0
path:
    .byte 47, 114, 97, 109, 47, 102, 97, 122, 54, 46, 116, 120, 116
payload:
    .byte 78, 101, 116, 105, 122, 101, 110, 67, 111, 114, 101, 32, 82, 105, 110, 103
    .byte 51
msg:
    .byte 102, 97, 122, 54, 32, 117, 121, 103, 117, 108, 97, 109, 97, 115, 105, 32
    .byte 73, 78, 73, 84, 10
hello:
    .byte 72, 101, 108, 108, 111
deny_read:
    .byte 47, 100, 97, 116, 97, 47, 115, 121, 115, 116, 101, 109, 47, 115, 101, 99
    .byte 114, 101, 116
traversal:
    .byte 47, 114, 97, 109, 47, 46, 46, 47, 100, 97, 116, 97, 47, 115, 121, 115
    .byte 116, 101, 109
ok_ram:
    .byte 47, 114, 97, 109, 47, 120

.section .data
.balign 4
counter:
    .long 0

.section .bss
.balign 16
rdbuf:
    .skip 64
