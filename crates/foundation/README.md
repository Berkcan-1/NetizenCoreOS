# crates/foundation — Temel

Sözleşmeler ve bağımsız kütüphaneler. Bu katmandaki hiçbir crate başka bir NetizenCore katmanına bağlanmaz (yalnızca `nc-hal`/`nc-crypto` gibi aynı katmandakilere).

- **Sözleşmeler:** `nc-hal` (önyükleme ve platform), `nc-abi` (Ring 3 tel biçimleri), `nc-efra` (sürücü paketi biçimi).
- **Kütüphaneler:** `nc-math`, `nc-crypto`, `nc-inflate`, `nc-i18n`.

Burada yapılan değişiklik tüm üst katmanları etkiler; sözleşme değişikliklerinde `Kullananlar` sütununu kontrol et.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-abi`](nc-abi/README.md) | Ring 3 ↔ çekirdek tel biçimleri: bağlam/olay blokları, çizim komutları, VFS/ağ/sistem istek-yanıt kodlaması | — | nc-app-host, nc-app-sdk, nc-browser-r3, nc-files-r3, nc-files-v8a, nc-home-r3, nc-kernel-aarch64, nc-pkg, nc-r3-stopwatch, nc-settings-r3, nc-settings-v8a, nc-stopwatch-r3, nc-sys-facade, nc-system-r3, nc-fs-facade*, nc-kernel*, nc-net-facade*, nc-ui* |
| [`nc-crypto`](nc-crypto/README.md) | SHA-256, HMAC, PBKDF2, sabit zamanlı karşılaştırma ve disk kilidi kayıtları | — | nc-app, nc-storage |
| [`nc-efra`](nc-efra/README.md) | EFRA sürücü paketi biçimi (manifest, sağlama toplamı, ayrıştırma/serileştirme) | — | nc-app-settings, nc-driver, nc-sys-facade |
| [`nc-hal`](nc-hal/README.md) | Önyükleme sözleşmesi (BootInfo), Platform ve aygıt arayüzleri, log! makrosu | — | nc-app, nc-audio, nc-desktop, nc-dev-x6826c, nc-device, nc-driver, nc-fs, nc-gfx, nc-input, nc-kernel, nc-kernel-aarch64, nc-media, nc-net, nc-platform-generic, nc-platform-qemu, nc-setup, nc-storage, nc-uefi |
| [`nc-i18n`](nc-i18n/README.md) | 11 dilli çeviri tabloları (langs/*.lang) ve derleme zamanı üretimi | — | nc-app |
| [`nc-inflate`](nc-inflate/README.md) | DEFLATE/gzip açıcı (gömülü .elf.gz yüklerini açar) | — | nc-kernel* |
| [`nc-math`](nc-math/README.md) | Tam sayı matematiği ve yumuşatma eğrileri | — | nc-desktop, nc-font, nc-kernel, nc-kernel-aarch64, nc-splash, nc-ui |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
