# crates/services — Çekirdek hizmetleri

Bellek yönetimi (`nc-mem`), sanal dosya sistemi (`nc-fs`) ve ağ yığını (`nc-net`). Donanım katmanının üzerinde, arayüzden bağımsızdır.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-fs`](nc-fs/README.md) | Sanal dosya sistemi (VFS): FAT32, exFAT, EROFS, bellek içi FS ve USB bölümleri | nc-hal, nc-storage* | nc-apps, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-fs-facade* |
| [`nc-mem`](nc-mem/README.md) | Bellek yönetimi: arena/PMM, heap, sayfalama ve çekirdek sayfa tabloları (kpt) | — | nc-kernel, nc-kernel-aarch64, nc-net, nc-splash |
| [`nc-net`](nc-net/README.md) | Ağ yığını: NIC sürücüsü, DHCP, DNS, TCP, TLS 1.3, HTTP istemcisi, Wi-Fi/Bluetooth durumu | nc-arch, nc-driver, nc-hal, nc-mem | nc-kernel, nc-net-facade*, nc-sys-facade* |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
