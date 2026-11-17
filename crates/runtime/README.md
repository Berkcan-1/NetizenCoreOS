# crates/runtime — Ring 3 çalışma zamanı

Uygulamaları çekirdek dışında çalıştırmak için gereken her şey: yetkiler (`nc-caps`), ELF doğrulama (`nc-elf`), paket biçimi ve yetki motoru (`nc-pkg`), SDK (`nc-app-sdk`), ana-bilgisayar (`nc-app-host`) ve hizmet cepheleri (`nc-fs-facade`, `nc-net-facade`, `nc-sys-facade`).

Sözleşme: `kernel/SYSCALL_ABI.md`, tel biçimleri: `crates/foundation/nc-abi`.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-app-host`](nc-app-host/README.md) | Mevcut nc_app::App uygulamalarını değiştirmeden Ring 3'te çalıştıran ana-bilgisayar çalışma zamanı | nc-abi, nc-app, nc-app-sdk, nc-gfx | nc-browser-r3, nc-files-r3, nc-settings-r3, nc-system-r3, nc-kernel* |
| [`nc-app-sdk`](nc-app-sdk/README.md) | Ring 3 uygulama SDK'sı: syscall sarmalayıcıları, Gfx, RingApp, bağlayıcı betik (app.ld) | nc-abi | nc-app-host, nc-browser-r3, nc-files-r3, nc-files-v8a, nc-home-r3, nc-r3-stopwatch, nc-settings-r3, nc-settings-v8a, nc-stopwatch-r3, nc-system-r3, nc-fs-facade*, nc-net-facade*, nc-sys-facade* |
| [`nc-caps`](nc-caps/README.md) | Yetki (capability) bitleri için tek doğruluk kaynağı | — | nc-fs-policy, nc-kernel, nc-kernel-aarch64, nc-pkg |
| [`nc-elf`](nc-elf/README.md) | ELF64 doğrulayıcı ve yükleme planı üretici (Ring 3 imajları) | — | nc-kernel-aarch64, nc-pkg, nc-kernel* |
| [`nc-fs-facade`](nc-fs-facade/README.md) | nc-fs cephesi: kernel özelliği gerçek VFS'i, client özelliği Ring 3 VFS istemcisini verir | nc-abi*, nc-app-sdk*, nc-fs* | nc-app-files, nc-app-settings, nc-files-r3, nc-kernel, nc-settings-r3 |
| [`nc-net-facade`](nc-net-facade/README.md) | nc-net cephesi: kernel özelliği gerçek ağ yığınını, client özelliği Ring 3 istemcisini verir | nc-abi*, nc-app-sdk*, nc-net* | nc-app-browser, nc-browser-r3 |
| [`nc-pkg`](nc-pkg/README.md) | NTZC paket biçimi, imza doğrulama, manifest ve DysonSphere yetki motoru | nc-abi, nc-app, nc-caps, nc-elf | nc-app-files, nc-apps, nc-kernel-aarch64, nc-kernel* |
| [`nc-sys-facade`](nc-sys-facade/README.md) | Sistem/ayar hizmeti cephesi: Wi-Fi/Bluetooth yansıması ve çekirdek hizmet tarafı | nc-abi, nc-efra, nc-app*, nc-app-sdk*, nc-arch*, nc-driver*, nc-net* | nc-app-settings, nc-settings-r3, nc-kernel* |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
