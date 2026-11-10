# nc-hal/src

## lib.rs

- Sabitler: BOOT_INFO_ADDRESS, BOOT_INFO_MAGIC (0x4942434E), BOOT_INFO_VERSION (1).
- FramebufferInfo (repr C, 32 bayt): address u64, pitch u32, width u32, height u32, bpp u8 ve alti kanal alani u8, reserved.
- BootInfo (repr C, 40 bayt): magic, version, framebuffer.
- BootInfo::read_from_boot_address: 0x900'den okur; magic ve version eslesmezse None.
- Platform trait'i: name -> &'static str, init(&mut self), write_log(&self, &str), shutdown(&self, u8) -> !.
- LogWriter: Platform'u core::fmt::Write'a cevirir.
- log! makrosu: writeln! ile satir yazar; $crate::LogWriter kullanir.
