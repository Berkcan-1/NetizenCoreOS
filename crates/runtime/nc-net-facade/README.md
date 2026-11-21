# nc-net-facade

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-net-facade` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | — |
| **İsteğe bağlı bağımlılıklar** | `nc-abi`, `nc-app-sdk`, `nc-net` |
| **Kullananlar** | `nc-app-browser`, `nc-browser-r3` |
| **Özellikler (features)** | `client`, `default`, `kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

`nc-net` cephesi: tarayıcı gibi uygulamalar ağ yığınına bu crate üzerinden erişir.

`kernel` özelliği `nc_net::{global, HttpState, NetworkStack, TcpHttpClient}` yeniden dışa aktarır; `client` özelliği Ring 3 istemcisini (`src/client.rs`: `NetworkStack`, `TcpHttpClient`, `set_transport`) sağlar.

`kernel` ve `client` özellikleri **birlikte açılamaz** (derleme hatası). Uygulama crate'leri cepheyi `default-features = false` ile tanımlar (Cargo'da `nc-fs = { package = "nc-fs-facade", ... }` gibi
yeniden adlandırarak), böylece uygulama kodu aynı kalır; `kernel` özelliğini çekirdek (`kernel/Cargo.toml`), `client` özelliğini
Ring 3 giriş crate'leri (`apps/*/nc-*-r3`) açar. Tel biçimi `nc-abi`'dedir.
