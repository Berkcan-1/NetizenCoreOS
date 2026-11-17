# rpi4 — Raspberry Pi 4 Model B

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/drivers/rpi4` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-device` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

**Durum:** iskelet. UART0 (PL011) ve EMMC2 adresleri BCM2711 "ana" adres modundadır (`0xFE000000` tabanı) ve doğrulanmamıştır.
Pi'de firmware (`start4.elf`) çekirdeği `kernel8.img` olarak `0x80000` adresine yükler; `kernel_base=0x00080000` bu yüzdendir.
Ekran firmware'in `config.txt` ile kurduğu framebuffer'dır (`simple-framebuffer`, DTB'den okunur). Gerçek çıktı için SD karta
`kernel8.img`, `start4.elf`, `fixup4.dat`, `bcm2711-rpi-4-b.dtb` ve `config.txt` (`arm_64bit=1`, `enable_uart=1`) gerekir.
