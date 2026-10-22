#![no_std]

use core::arch::asm;

pub mod idt;
pub mod rand;
pub mod serial;
pub mod time;

pub unsafe fn disable_pic() {
    outb(0x21, 0xFF);
    outb(0xA1, 0xFF);
}

#[inline]
pub fn wait_for_interrupt() {
    unsafe {
        let rflags: u64;
        asm!("pushfq; pop {}", out(reg) rflags, options(nomem, preserves_flags));
        if rflags & 0x200 != 0 {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        } else {
            core::hint::spin_loop();
        }
    }
}

pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

pub unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

pub unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

pub fn halt_forever() -> ! {
    loop {
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

pub fn reboot() -> ! {
    unsafe {
        outb(0x0CF9, 0x02);
        outb(0x0CF9, 0x06);
        let mut temp: u8 = 0x02;
        let mut spins = 0u32;
        while temp & 0x02 != 0 && spins < 100_000 {
            temp = inb(0x64);
            spins += 1;
        }
        outb(0x64, 0xFE);
    }
    loop {
        core::hint::spin_loop();
    }
}

pub fn enable_cpu_hardening() -> (bool, bool) {
    let mut smep_active = false;
    unsafe {
        // 1. CR0.WP (Bit 16) kesin olarak aç
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        cr0 |= 1 << 16;
        asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack, preserves_flags));

        // 2. CPUID.(EAX=7, ECX=0):EBX[bit 7] ile SMEP desteğini kontrol et
        let max_leaf = core::arch::x86_64::__cpuid(0).eax;
        if max_leaf >= 7 {
            let cpuid7 = core::arch::x86_64::__cpuid_count(7, 0);
            if (cpuid7.ebx & (1 << 7)) != 0 {
                let mut cr4: u64;
                asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
                cr4 |= 1 << 20; // CR4.SMEP
                asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));
                smep_active = true;
            }
        }
    }
    (true, smep_active)
}
