# nc-crypto

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/foundation/nc-crypto` |
| **Katman** | Temel |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-app`, `nc-storage` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Çekirdeğin ihtiyaç duyduğu küçük kriptografi araçları. `no_std`.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `Sha256`, `sha256`, `HmacSha256`, `hmac_sha256`, `pbkdf2_hmac_sha256`, `constant_time_eq` |
| `lock.rs` | Disk kilidi: `LockRecord` (türetme/doğrulama), `Marker`, `LockState` ve `evaluate`, `secret_is_valid`, sektör kodlama |
| `tests.rs` | Birim testleri |

Disk kilidi kayıtlarını `nc-storage` sektöre yazar/okur (`read_lock_state`, `apply_lock_request`); bu crate yalnızca
kayıt mantığını ve doğrulamayı sağlar.
