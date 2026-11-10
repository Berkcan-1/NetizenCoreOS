#!/usr/bin/env python3
# Faz 6 Ring 3 test uygulamasini uretir: faz6_app.S -> faz6_app.elf (GNU as + ld gerekir).
# Cikti ELF depoya EKLENIR; kernel yalnizca include_bytes! ile kullanir, build.sh'in as/ld'ye ihtiyaci yoktur.
import struct, subprocess, sys, os

def i32(v): return struct.pack('<i', v)
def u32(v): return struct.pack('<I', v)

def rec(op, payload):
    total = 4 + len(payload)
    assert total % 4 == 0 and total < 65536
    return bytes([op, 0]) + struct.pack('<H', total) + payload

def fill_rect(x, y, w, h, c): return rec(1, i32(x)+i32(y)+i32(w)+i32(h)+u32(c))
def panel(x, y, w, h, r, fill, fa, border, ba): return rec(2, i32(x)+i32(y)+i32(w)+i32(h)+i32(r)+u32(fill)+u32(fa)+u32(border)+u32(ba))
def dot(cx, cy, r, c, s): return rec(3, i32(cx)+i32(cy)+i32(r)+u32(c)+u32(s))
def text(x, y, cap, c, s, tracking=0, centered=0):
    b = s.encode(); pad = (-len(b)) % 4
    return rec(4, i32(x)+i32(y)+i32(cap)+u32(c)+struct.pack('<HBB', len(b), tracking, centered)+b+b'\0'*pad)
def button(x, y, w, h, accent, style, hover, label):
    b = label.encode(); pad = (-len(b)) % 4
    return rec(5, i32(x)+i32(y)+i32(w)+i32(h)+u32(accent)+struct.pack('<HBB', len(b), style, hover)+b+b'\0'*pad)

# Golden vektor: nc-abi/src/draw.rs::golden_bytes_are_stable ile BIREBIR ayni olmali.
golden = fill_rect(1,2,3,4,0xAABBCCDD) + text(5,6,7,0x11223344,"Hi")
expected = bytes([0x01,0,0x18,0, 1,0,0,0, 2,0,0,0, 3,0,0,0, 4,0,0,0, 0xDD,0xCC,0xBB,0xAA,
                  0x04,0,0x1C,0, 5,0,0,0, 6,0,0,0, 7,0,0,0, 0x44,0x33,0x22,0x11, 2,0,0,0, ord('H'),ord('i'),0,0])
assert golden == expected, "Python kodlayici ile Rust golden vektoru ayrisiyor"

BLUE, RED, WHITE, GREY, GREEN = 0x0000FF, 0xFF0000, 0xFFFFFF, 0xCCCCCC, 0x00FF00
cmds = (fill_rect(-8, -8, 200, 200, BLUE) + fill_rect(10, 6, 40, 20, RED) +
        panel(0, 30, 30, 20, 4, WHITE, 255, GREY, 255) + dot(50, 45, 5, GREEN, 255) +
        text(4, 2, 12, WHITE, "Hi") + fill_rect(0, 0, 0, 0, 0) +
        button(58, 36, 36, 16, 0x3080F0, 1, 0, "Go") + text(48, 54, 10, WHITE, "Ok", 30, 1))
RECORDS = 8
badcmds = bytes([99, 0, 4, 0])
path = b"/ram/faz6.txt"
payload = b"NetizenCore Ring3"
msg = b"faz6 uygulamasi INIT\n"
hello = b"Hello"
deny_read = b"/data/system/secret"
traversal = b"/ram/../data/system"
ok_ram = b"/ram/x"

def bytes_dir(label, data):
    lines = [f"{label}:"]
    for i in range(0, len(data), 16):
        lines.append("    .byte " + ", ".join(str(b) for b in data[i:i+16]))
    return "\n".join(lines)

S = f"""# Otomatik uretildi (gen.py). Duzenleme icin gen.py'yi degistirin.
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
    mov edx, {len(cmds)}
    int 0x80
    mov r13d, 1
    cmp rax, {RECORDS}
    je .Ldone
    or r13d, 0x80000000
    jmp .Ldone

.Linit:
    xor r13d, r13d
    mov eax, 4
    mov edi, 1
    lea rsi, [rip + msg]
    mov edx, {len(msg)}
    int 0x80
    mov eax, 20
    mov rdi, QWORD PTR [rbx + 16]
    lea rsi, [rip + cmds]
    mov edx, {len(cmds)}
    int 0x80
    cmp rax, -11
    jne .Lfail
    or r13d, 0x400
    mov eax, 21
    lea rdi, [rip + hello]
    mov esi, {len(hello)}
    mov edx, 16
    int 0x80
    test rax, rax
    jle .Lfail
    or r13d, 0x200
    mov eax, 17
    lea rdi, [rip + path]
    mov esi, {len(path)}
    mov edx, 2
    int 0x80
    test rax, rax
    js .Lfail
    mov r14, rax
    mov eax, 19
    mov rdi, r14
    lea rsi, [rip + payload]
    mov edx, {len(payload)}
    int 0x80
    cmp rax, {len(payload)}
    jne .Lfail
    mov eax, 22
    mov rdi, r14
    int 0x80
    test rax, rax
    jne .Lfail
    mov eax, 17
    lea rdi, [rip + path]
    mov esi, {len(path)}
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
    cmp rax, {len(payload)}
    jne .Lfail
    mov eax, 22
    mov rdi, r14
    int 0x80
    test rax, rax
    jne .Lfail
    lea rsi, [rip + payload]
    lea rdi, [rip + rdbuf]
    mov ecx, {len(payload)}
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
    mov edx, {len(badcmds)}
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
    mov edx, {len(cmds)}
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
    mov esi, {len(deny_read)}
    mov edx, 1
    int 0x80
    cmp rax, -13
    jne .Lfail
    or r13d, 0x100
    mov eax, 17
    lea rdi, [rip + traversal]
    mov esi, {len(traversal)}
    mov edx, 1
    int 0x80
    cmp rax, -22
    jne .Lfail
    or r13d, 0x200
    mov eax, 17
    lea rdi, [rip + ok_ram]
    mov esi, {len(ok_ram)}
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
{bytes_dir("cmds", cmds)}
{bytes_dir("badcmds", badcmds)}
{bytes_dir("path", path)}
{bytes_dir("payload", payload)}
{bytes_dir("msg", msg)}
{bytes_dir("hello", hello)}
{bytes_dir("deny_read", deny_read)}
{bytes_dir("traversal", traversal)}
{bytes_dir("ok_ram", ok_ram)}

.section .data
.balign 4
counter:
    .long 0

.section .bss
.balign 16
rdbuf:
    .skip 64
"""

LD = """ENTRY(_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0x0000008001000000;
    .text : ALIGN(4096) { *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) } :rodata
    . = ALIGN(4096);
    .data : ALIGN(4096) { *(.data .data.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.note*) *(.comment*) *(.eh_frame*) }
}
"""

here = os.path.dirname(os.path.abspath(__file__))
open(os.path.join(here, "faz6_app.S"), "w").write(S)
open(os.path.join(here, "faz6.ld"), "w").write(LD)
subprocess.check_call(["as", "--64", "-o", os.path.join(here, "faz6_app.o"), os.path.join(here, "faz6_app.S")])
subprocess.check_call(["ld", "-static", "-nostdlib", "-z", "max-page-size=4096", "-z", "noexecstack",
                       "-T", os.path.join(here, "faz6.ld"), "-o", os.path.join(here, "faz6_app.elf"),
                       os.path.join(here, "faz6_app.o")])
os.remove(os.path.join(here, "faz6_app.o"))
print("cmds_len=%d records=%d" % (len(cmds), RECORDS))
