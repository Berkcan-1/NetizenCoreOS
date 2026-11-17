/* Ring 3 uygulama imaji (aarch64): nc-elf Policy 0x80_0100_0000 .. 0x80_0800_0000, W^X, sayfa hizali 3 PT_LOAD. */
ENTRY(_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0x0000008001000000;
    .text : ALIGN(4096) { *(.text._start) *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) *(.data.rel.ro .data.rel.ro.*) } :rodata
    . = ALIGN(4096);
    .data : ALIGN(4096) { *(.got .got.plt) *(.data .data.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.note*) *(.comment*) *(.eh_frame*) *(.eh_frame_hdr) }
}
