# nc-fs-facade

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-fs-facade` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | — |
| **İsteğe bağlı bağımlılıklar** | `nc-abi`, `nc-app-sdk`, `nc-fs` |
| **Kullananlar** | `nc-app-files`, `nc-app-settings`, `nc-files-r3`, `nc-kernel`, `nc-settings-r3` |
| **Özellikler (features)** | `client`, `default`, `kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

`nc-fs` cephesi: uygulamalar `VfsManager` ve eşlerini bu crate üzerinden kullanır.

`kernel` özelliği gerçek `nc-fs`'i yeniden dışa aktarır; `client` özelliği Ring 3 VFS hizmeti istemcisini (`src/client.rs`: `VfsManager`, `FileType`, `DirEntry`, `FileStat`, `FsError`, `set_transport`) sağlar.

`kernel` ve `client` özellikleri **birlikte açılamaz** (derleme hatası). Uygulama crate'leri cepheyi `default-features = false` ile tanımlar (Cargo'da `nc-fs = { package = "nc-fs-facade", ... }` gibi
yeniden adlandırarak), böylece uygulama kodu aynı kalır; `kernel` özelliğini çekirdek (`kernel/Cargo.toml`), `client` özelliğini
Ring 3 giriş crate'leri (`apps/*/nc-*-r3`) açar. Tel biçimi `nc-abi`'dedir.
