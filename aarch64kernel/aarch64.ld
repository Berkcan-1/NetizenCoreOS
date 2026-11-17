ENTRY(_start)

SECTIONS
{
    . = KERNEL_BASE;
    __image_base = .;

    .text : ALIGN(4096)
    {
        __text_start = .;
        KEEP(*(.text.head))
        *(.text .text.*)
        . = ALIGN(4096);
        __text_end = .;
    }

    .rodata : ALIGN(4096)
    {
        __rodata_start = .;
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
        . = ALIGN(4096);
        __rodata_end = .;
    }

    .data : ALIGN(4096)
    {
        __data_start = .;
        *(.data .data.*)
        *(.got .got.*)
        . = ALIGN(4096);
    }

    __image_end = .;

    .bss (NOLOAD) : ALIGN(4096)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4096);
        __bss_end = .;
    }

    .stack (NOLOAD) : ALIGN(4096)
    {
        __stack_bottom = .;
        . += 0x80000;
        __stack_top = .;
    }

    __kernel_end = .;
    __nc_image_size = __image_end - __image_base;
    __nc_text_offset = KERNEL_BASE & 0x1fffff;
    ASSERT((KERNEL_BASE & 0xfff) == 0, "KERNEL_BASE 4 KiB hizali olmali")
    ASSERT((__kernel_end - __image_base) < 0x1000000, "cekirdek 16 MiB'i asti")

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
