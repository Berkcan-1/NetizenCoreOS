# nc-hal

Bootloader ile cekirdek arasindaki sozlesme ve platform arayuzu. Hicbir bagimliligi yoktur; her modul buna
guvenebilir.

## Genel API

- BOOT_INFO_ADDRESS (0x900), BOOT_INFO_MAGIC, BOOT_INFO_VERSION
- FramebufferInfo: address, pitch, width, height, bpp, red/green/blue position ve size
- BootInfo ve BootInfo::read_from_boot_address() (unsafe, magic ve version dogrular)
- trait Platform: name, init, write_log, shutdown
- LogWriter ve log!(&platform, "bicim {}", deger) makrosu

## Kurallar

- BootInfo alan duzeni bootloader/stage2.asm icindeki fill_boot_info ile birebir ayni olmalidir.
- Platform uygulamalari donanima ozel her seyi kendi icinde tutar; ust katmanlar sadece bu arayuzu gorur.
