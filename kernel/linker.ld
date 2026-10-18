ENTRY(_start)

SECTIONS
{
    . = 0x100000;
    __kernel_bin_start = .;

    .text : ALIGN(16)
    {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
    }

    .data : ALIGN(16)
    {
        *(.data .data.*)
        *(.got .got.*)
    }

    . = ALIGN(16);
    __kernel_bin_end = .;

    .bss : ALIGN(16)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }

    .stack : ALIGN(16)
    {
        __stack_bottom = .;
        KEEP(*(.stack))
        . = ALIGN(16);
        __stack_top = .;
    }

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
