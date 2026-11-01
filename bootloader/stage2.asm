bits 16
org 0x8800

%ifndef PREFERRED_WIDTH
%define PREFERRED_WIDTH 1024
%endif
%ifndef PREFERRED_HEIGHT
%define PREFERRED_HEIGHT 768
%endif
%ifndef FALLBACK_WIDTH
%define FALLBACK_WIDTH 1024
%endif
%ifndef FALLBACK_HEIGHT
%define FALLBACK_HEIGHT 768
%endif
%ifndef KERNEL_SECTORS_512
%define KERNEL_SECTORS_512 4096
%endif

PML4_ADDRESS equ 0x1000
PDPT_ADDRESS equ 0x2000
PDT_ADDRESS equ 0x3000
PAGE_TABLE_PAGES equ 6

VBE_INFO equ 0x0600
MODE_INFO equ 0x0800
BOOT_INFO equ 0x0900
BOOT_INFO_MAGIC equ 0x4942434E
BOOT_INFO_VERSION equ 2
BOOT_INFO_WORDS equ 32
KERNEL_BASE equ 0x100000
E820_TARGET equ 0x0A00

stage2_start:
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    cld

    mov ax, 0x2401
    int 0x15
    in al, 0x92
    or al, 2
    and al, 0xFE
    out 0x92, al

    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x200000
    push eax
    popfd
    pushfd
    pop eax
    push ecx
    popfd
    xor eax, ecx
    jz no_long_mode

    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb no_long_mode

    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29
    jz no_long_mode

    ; Bulgu #5: CPUID 0x80000001:EDX[20] NX destegini kaydet
    test edx, 1 << 20
    setnz [has_nx]

    call detect_memory
    call setup_video
    jc video_failed

    cli
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:protected_entry

detect_memory:
    xor ebx, ebx
    xor bp, bp
    mov di, E820_TARGET
    mov edx, 0x534D4150
.e820_loop:
    mov eax, 0xE820
    mov ecx, 24
    mov dword [es:di + 20], 1
    int 0x15
    jc .e820_done
    cmp eax, 0x534D4150
    jne .e820_done
    test ecx, ecx
    jz .skip_entry
    cmp cl, 20
    jbe .not_extended
    test byte [es:di + 20], 1
    jz .skip_entry
.not_extended:
    mov ecx, [es:di + 8]
    or ecx, [es:di + 12]
    jz .skip_entry
    inc bp
    add di, 24
    cmp bp, 64
    jae .e820_done
.skip_entry:
    test ebx, ebx
    jnz .e820_loop
.e820_done:
    mov [entry_count], bp
    ret

setup_video:
    mov dword [VBE_INFO], 0x32454256
    mov ax, 0x4F00
    mov di, VBE_INFO
    int 0x10
    cmp ax, 0x004F
    jne .fail
    cmp dword [VBE_INFO], 0x41534556
    jne .fail

    mov ax, PREFERRED_WIDTH
    mov bx, PREFERRED_HEIGHT
    call find_mode
    jnc .found

    mov ax, FALLBACK_WIDTH
    mov bx, FALLBACK_HEIGHT
    call find_mode
    jnc .found

    mov ax, 1920
    mov bx, 1080
    call find_mode
    jnc .found

    mov ax, 1366
    mov bx, 768
    call find_mode
    jnc .found

    mov ax, 1280
    mov bx, 800
    call find_mode
    jnc .found

    mov ax, 800
    mov bx, 600
    call find_mode
    jc .fail

.found:
    mov bx, dx
    or bx, 0x4000
    mov ax, 0x4F02
    int 0x10
    cmp ax, 0x004F
    jne .fail

    call fill_boot_info
    clc
    ret
.fail:
    stc
    ret

find_mode:
    mov [want_width], ax
    mov [want_height], bx
    mov si, [VBE_INFO + 14]
    mov ax, [VBE_INFO + 16]
    mov [mode_list_segment], ax
.next:
    mov fs, [mode_list_segment]
    mov cx, [fs:si]
    add si, 2
    cmp cx, 0xFFFF
    je .none
    mov [candidate_mode], cx

    push si
    mov ax, 0x4F01
    mov di, MODE_INFO
    int 0x10
    pop si
    cmp ax, 0x004F
    jne .next

    mov ax, [MODE_INFO]
    and ax, 0x0091
    cmp ax, 0x0091
    jne .next

    cmp byte [MODE_INFO + 25], 32
    jne .next
    cmp byte [MODE_INFO + 27], 6
    jne .next

    mov ax, [MODE_INFO + 18]
    cmp ax, [want_width]
    jne .next
    mov ax, [MODE_INFO + 20]
    cmp ax, [want_height]
    jne .next

    mov dx, [candidate_mode]
    clc
    ret
.none:
    stc
    ret

fill_boot_info:
    xor ax, ax
    mov di, BOOT_INFO
    mov cx, BOOT_INFO_WORDS
    rep stosw

    mov dword [BOOT_INFO], BOOT_INFO_MAGIC
    mov dword [BOOT_INFO + 4], BOOT_INFO_VERSION
    mov eax, [MODE_INFO + 40]
    mov [BOOT_INFO + 0x08], eax

    movzx eax, word [MODE_INFO + 16]
    cmp word [VBE_INFO + 4], 0x0300
    jb .store_pitch
    movzx edx, word [MODE_INFO + 50]
    test edx, edx
    jz .store_pitch
    mov eax, edx
.store_pitch:
    mov [BOOT_INFO + 0x10], eax

    movzx eax, word [MODE_INFO + 18]
    mov [BOOT_INFO + 0x14], eax
    movzx eax, word [MODE_INFO + 20]
    mov [BOOT_INFO + 0x18], eax

    mov al, [MODE_INFO + 25]
    mov [BOOT_INFO + 0x1C], al
    mov al, [MODE_INFO + 32]
    mov [BOOT_INFO + 0x1D], al
    mov al, [MODE_INFO + 31]
    mov [BOOT_INFO + 0x1E], al
    mov al, [MODE_INFO + 34]
    mov [BOOT_INFO + 0x1F], al
    mov al, [MODE_INFO + 33]
    mov [BOOT_INFO + 0x20], al
    mov al, [MODE_INFO + 36]
    mov [BOOT_INFO + 0x21], al
    mov al, [MODE_INFO + 35]
    mov [BOOT_INFO + 0x22], al
    mov byte [BOOT_INFO + 0x23], 0
    mov dword [BOOT_INFO + 0x28], E820_TARGET
    mov dword [BOOT_INFO + 0x2C], 0
    movzx eax, word [entry_count]
    mov [BOOT_INFO + 0x30], eax
    ret

print_string:
    lodsb
    test al, al
    jz .done
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp print_string
.done:
    ret

no_long_mode:
    mov si, message_no_long_mode
    jmp fatal
video_failed:
    mov si, message_video_failed
fatal:
    call print_string
.halt:
    cli
    hlt
    jmp .halt

want_width: dw 0
want_height: dw 0
candidate_mode: dw 0
mode_list_segment: dw 0
entry_count: dw 0
has_nx: db 0

message_no_long_mode: db 'NetizenCore: 64 bit islemci gerekli', 0
message_video_failed: db 'NetizenCore: VBE grafik modu bulunamadi', 0

align 8
gdt_start:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
    dq 0x00AF9A000000FFFF
gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1
    dd gdt_start

bits 32
protected_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x7C00

    ; IA32_PAT (MSR 0x277): PA0=WB(06), PA1=WC(01), PA2=UC-(07), PA3=UC(00)
    mov ecx, 0x277
    mov eax, 0x00070106
    mov edx, 0x00070106
    wrmsr

    mov edi, PML4_ADDRESS
    xor eax, eax
    mov ecx, PAGE_TABLE_PAGES * 4096 / 4
    rep stosd

    mov dword [PML4_ADDRESS], PDPT_ADDRESS | 0x03
    mov dword [PDPT_ADDRESS], PDT_ADDRESS | 0x03
    mov dword [PDPT_ADDRESS + 8], (PDT_ADDRESS + 0x1000) | 0x03
    mov dword [PDPT_ADDRESS + 16], (PDT_ADDRESS + 0x2000) | 0x03
    mov dword [PDPT_ADDRESS + 24], (PDT_ADDRESS + 0x3000) | 0x03

    ; Bulgu #3 & #5: NX varsa 4 MiB uzerindeki veri/arena/heap sayfalarina NX (Bit 63) uygula
    xor ebx, ebx
    cmp byte [has_nx], 0
    je .no_nx_hi
    mov ebx, 0x80000000
.no_nx_hi:

    mov edi, PDT_ADDRESS
    xor eax, eax
    mov ecx, 2048
.fill_pdt:
    mov edx, 0x83               ; 0-3 GB: Write-Back (PA0)
    cmp ecx, 512
    ja .store_entry
    mov edx, 0x93               ; 3-4 GB varsayilan: Uncacheable (MMIO)
.store_entry:
    or edx, eax
    mov [edi], edx
    ; Ilk iki 2 MiB sayfa (0..4 MiB) cekirdek kodunu icerir, geri kalan 2046 sayfaya NX koy
    cmp ecx, 2046
    ja .skip_nx_page
    mov [edi + 4], ebx
.skip_nx_page:
    add eax, 0x200000
    add edi, 8
    loop .fill_pdt

    ; Framebuffer fiziksel adresini Write-Combining (0x8B -> PA1=WC) + NX yap
    mov eax, [BOOT_INFO + 0x08]
    shr eax, 21                 ; 2 MB sayfa indeksi
    cmp eax, 2048
    jae .skip_fb_wc
    mov ecx, 2048
    sub ecx, eax
    cmp ecx, 16
    jbe .fb_count_ok
    mov ecx, 16                 ; En fazla 16 * 2 MB = 32 MB VRAM penceresi
.fb_count_ok:
    lea edi, [PDT_ADDRESS + eax * 8]
    mov eax, [BOOT_INFO + 0x08]
    and eax, 0xFFE00000
.fb_wc_loop:
    mov edx, eax
    or edx, 0x8B                ; Present | RW | PWT (PA1 = Write-Combining) | PS
    mov [edi], edx
    mov [edi + 4], ebx          ; Framebuffer uzerinde kod calistirilamaz (NX)
    add eax, 0x200000
    add edi, 8
    loop .fb_wc_loop
.skip_fb_wc:

    mov eax, cr4
    or eax, 1 << 5              ; PAE
    mov cr4, eax

    mov eax, PML4_ADDRESS
    mov cr3, eax

    ; Bulgu #5: EFER.LME (bit 8) ve yalnizca CPU destekliyorsa EFER.NXE (bit 11)
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    cmp byte [has_nx], 0
    je .write_efer
    or eax, (1 << 11)
.write_efer:
    wrmsr

    mov eax, cr0
    or eax, (1 << 31) | (1 << 16) ; PG + WP (Write Protect)
    mov cr0, eax

    jmp 0x18:long_entry

bits 64
long_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    mov rsp, 0x90000
    mov rax, KERNEL_BASE
    jmp rax

; Bulgu #11: MBR'nin gercek cekirdek boyutunu okumasi icin Stage2 sonuna (0x97F8) gomulu baslik
times 4088 - ($ - $$) db 0
stage2_kernel_header:
    dd 0x4E434B48               ; Magic: "NCKH" (0x97F8)
    dw KERNEL_SECTORS_512       ; Gercek cekirdek 512-bayt sektor sayisi (0x97FC)
    dw 0x0001                   ; Baslik versiyonu (0x97FE)
    