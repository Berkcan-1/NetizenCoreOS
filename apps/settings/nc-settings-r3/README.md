# nc-settings-r3

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/settings/nc-settings-r3` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app`, `nc-app-host`, `nc-app-sdk`, `nc-app-settings`, `nc-fs-facade`, `nc-gfx`, `nc-sys-facade` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ayarlar uygulamasının Ring 3 giriş ikilisi. `src/main.rs` yalnızca `nc_app_host::run` ile uygulamayı (`nc-app-settings`) olay-tabanlı Ring 3
modeline bağlar; uygulama mantığı bu crate'te değildir.

- Kendi `[workspace]`'i ve `Cargo.lock`'u vardır; kök workspace üyesi değildir.
- `build.rs` `crates/runtime/nc-app-sdk/app.ld` bağlayıcı betiğini bağlar.
- Derleme: `bash build.sh` (`tools/build/ring3.sh`). Çıktı: `kernel/embedded/ring3-apps/settings.elf` ve `.elf.gz`.
