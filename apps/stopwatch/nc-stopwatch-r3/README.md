# nc-stopwatch-r3

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/stopwatch/nc-stopwatch-r3` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk`, `nc-r3-stopwatch` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Kronometrenin Ring 3 giriş ikilisi. Diğer dört uygulamanın aksine `nc-app-host` kullanmaz: `src/main.rs`, bağımsız
`nc-r3-stopwatch` mantığını `nc_app_sdk::dispatch` ile doğrudan çalıştırır (durum `static` bellekte kalır; çekirdek her olayda
`_start`'a boş yığınla girer).

- Kendi `[workspace]`'i ve `Cargo.lock`'u vardır; kök workspace üyesi değildir.
- `build.rs` `crates/runtime/nc-app-sdk/app.ld` bağlayıcı betiğini bağlar.
- Derleme: `bash build.sh` (`tools/build/ring3.sh`). Çıktı: `kernel/embedded/ring3-apps/stopwatch.elf` ve `.elf.gz`.
- Mantık: `apps/stopwatch/nc-r3-stopwatch`. Ring 0 sürümü (`nc-app-stopwatch`) bu ikiliye bağlı değildir.
