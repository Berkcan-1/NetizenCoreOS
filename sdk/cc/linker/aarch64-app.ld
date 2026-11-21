/* NetizenCore Standalone AArch64 EL0 Application Linker Script (C/C++ SDK) */
ENTRY(_start)

SECTIONS
{
    . = 0x40000000;

    .text : ALIGN(4096)
    {
        *(.text.start)
        *(.text .text.*)
    }

    .rodata : ALIGN(4096)
    {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4096)
    {
        *(.data .data.*)
    }

    .bss (NOLOAD) : ALIGN(4096)
    {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
