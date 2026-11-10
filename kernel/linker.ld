ENTRY(_start)

SECTIONS
{
    . = 0x100000;
    __kernel_bin_start = .;

    .text : ALIGN(4096)
    {
        __kpt_text_start = .;
        KEEP(*(.text.entry))
        *(.text .text.*)
        . = ALIGN(4096);
        __kpt_text_end = .;
    }

    .rodata : ALIGN(4096)
    {
        __kpt_rodata_start = .;
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
        . = ALIGN(4096);
        __kpt_rodata_end = .;
    }

    .data : ALIGN(4096)
    {
        __kpt_rw_start = .;
        *(.data .data.*)
        *(.got .got.*)
        . = ALIGN(4096);
    }

    . = ALIGN(4096);
    __kernel_bin_end = .;

    .bss : ALIGN(16)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }

    .stack : ALIGN(4096)
    {
        __stack_bottom = .;
        __kpt_stack_start = .;
        KEEP(*(.stack))
        . = ALIGN(4096);
        __stack_top = .;
    }

    . = ALIGN(4096);
    __kernel_image_end = .;
    __kpt_image_end = .;
    ASSERT((__kernel_image_end - __kernel_bin_start) <= 0x700000,
           "kernel runtime image (.text/.data/.bss/.stack) exceeds the 7 MiB reservation (OVMF ACPI NVS starts at 8 MiB) (image must end below the 16 MiB arena/PMM start)")
    /* .text, Stage2 yurutulebilir penceresinde (0..8 MiB) kalmali; buyuyen .rodata/.data bunun disina tasabilir (NX) */
    ASSERT(__kpt_text_end <= 0x800000, "kernel .text must stay inside the 8 MiB Stage2 executable window")
    ASSERT((__kpt_text_start & 0xFFF) == 0 && (__kpt_text_end & 0xFFF) == 0 && (__kpt_rodata_end & 0xFFF) == 0 && (__kpt_stack_start & 0xFFF) == 0,
           "kernel section boundaries must be 4 KiB aligned (kernel page tables, W^X)")

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
