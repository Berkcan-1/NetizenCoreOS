# nc-hal

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/foundation/nc-hal` |
| **Katman** | Temel |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-app`, `nc-audio`, `nc-desktop`, `nc-device`, `nc-driver`, `nc-fs`, `nc-gfx`, `nc-input`, `nc-kernel`, `nc-kernel-aarch64`, `nc-media`, `nc-net`, `nc-platform-generic`, `nc-platform-qemu`, `nc-setup`, `nc-storage`, `nc-uefi` |
| **Testlerde kullananlar** | `nc-apps` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Bootloader ile cekirdek arasindaki sozlesme ve platform arayuzu. Hicbir bagimliligi yoktur; her modul buna
guvenebilir.

## Genel API

- BOOT_INFO_ADDRESS (0x900), BOOT_INFO_MAGIC, BOOT_INFO_VERSION
- FramebufferInfo: address, pitch, width, height, bpp, red/green/blue position ve size
- BootInfo ve BootInfo::read_from_boot_address() (unsafe, magic ve version dogrular)
- trait Platform: name, init, write_log, shutdown
- LogWriter ve log!(&platform, "bicim {}", deger) makrosu

## Kurallar

- BootInfo alan duzeni bootloader/bios/stage2.asm icindeki fill_boot_info ile birebir ayni olmalidir.
- Platform uygulamalari donanima ozel her seyi kendi icinde tutar; ust katmanlar sadece bu arayuzu gorur.
