# nc-efra

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/foundation/nc-efra` |
| **Katman** | Temel |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-app-settings`, `nc-driver`, `nc-sys-facade` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

EFRA sürücü paketi biçimi. `no_std`, bağımlılığı yoktur.

- `EfraClass`, `HardwareEnvironment`: paketin hedeflediği aygıt sınıfı ve donanım ortamı.
- `EfraManifest`, `EfraDriverPackage`: paket yapısı; `serialize`, `serialize_padded_512` ve `parse`, `compute_checksum`, `is_generic`.
- `EfraCompiler::compile_in_os`: paketi sistem içinde derleme yolu.

`nc-driver` paketleri kayıt defterine işler (`register_or_update_package`, `rebind_all`); Ayarlar uygulaması ve
`nc-sys-facade` sürücü güncellemesi için kullanır.
