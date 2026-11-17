# crates/hardware — Donanım

x86_64 mimarisi, aygıt sürücüleri, giriş, depolama ve platform uygulamaları. Donanıma özel kod yalnızca burada olur.

Aygıtın kendisi burada, onu kullanan hizmet `crates/services/`'tedir (disk: `nc-storage`, dosya sistemi: `nc-fs`).

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-arch`](nc-arch/README.md) | Mimari cephesi: hedefe göre nc-arch-x86_64 veya nc-arch-aarch64'ü yeniden dışa aktarır | — | nc-apps, nc-driver, nc-input, nc-kernel, nc-kernel-aarch64, nc-net, nc-platform-generic, nc-platform-qemu, nc-storage, nc-sys-facade* |
| [`nc-arch-x86_64`](nc-arch-x86_64/README.md) | x86_64 mimari katmanı: portlar, IDT, APIC, RTC, seri port, zaman, rastgele sayı | — | — |
| [`nc-driver`](nc-driver/README.md) | PCI taraması, DMA, PS/2, AC'97 ve EFRA tabanlı sürücü kayıt defteri | nc-arch, nc-efra, nc-hal | nc-audio, nc-input, nc-kernel, nc-net, nc-setup, nc-storage, nc-sys-facade* |
| [`nc-input`](nc-input/README.md) | Klavye/fare girişi: PS/2 ve xHCI USB HID olay kuyruğu | nc-arch, nc-driver, nc-hal | nc-kernel |
| [`nc-platform-generic`](nc-platform-generic/README.md) | Genel PC (VirtualBox ve çoğu gerçek makine) için asgari Platform uygulaması | nc-arch, nc-hal | nc-kernel* |
| [`nc-platform-qemu`](nc-platform-qemu/README.md) | QEMU sanal donanımına özel Platform uygulaması | nc-arch, nc-hal | nc-kernel* |
| [`nc-storage`](nc-storage/README.md) | Blok aygıtları (ATA/AHCI/NVMe), bölüm tabloları, kurulum yazıcısı ve disk kilidi | nc-arch, nc-crypto, nc-driver, nc-hal | nc-kernel, nc-fs* |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
