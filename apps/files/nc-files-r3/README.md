# nc-files-r3

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/files/nc-files-r3` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app`, `nc-app-files`, `nc-app-host`, `nc-app-sdk`, `nc-fs-facade` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Dosyalar uygulamasının Ring 3 giriş ikilisi. `src/main.rs` yalnızca `nc_app_host::run` ile uygulamayı (`nc-app-files`) olay-tabanlı Ring 3
modeline bağlar; uygulama mantığı bu crate'te değildir.

- Kendi `[workspace]`'i ve `Cargo.lock`'u vardır; kök workspace üyesi değildir.
- `build.rs` `crates/runtime/nc-app-sdk/app.ld` bağlayıcı betiğini bağlar.
- Derleme: `bash build.sh` (`tools/build/ring3.sh`). Çıktı: `kernel/embedded/ring3-apps/files.elf` ve `.elf.gz`.
