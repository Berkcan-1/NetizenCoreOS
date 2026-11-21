# nc-fs-policy

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/runtime/nc-fs-policy` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-caps` |
| **Kullananlar** | `nc-kernel-aarch64` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Dosya yolu erişim politikası: hangi uygulama (kimlik + yetki bitleri) hangi yola hangi işlemle erişebilir. `no_std`, tek bağımlılık `nc-caps`.
Çekirdek her VFS isteğinde `check(app_id, caps, yol, işlem)` çağırır; yolu kendisi ayrıştırır (`..`, `.`, boş bileşen, `\`, NUL reddedilir).

| Yol | Okuma | Yazma / silme |
|---|---|---|
| `/` | herkes | yok |
| `/system/**` | `FS_ALL_READ` | hiç kimse (salt-okunur) |
| `/data` | herkes | yok |
| `/data/app/**` | `FS_ALL_READ` | `FS_ALL_WRITE` |
| `/data/appdata/<kimlik>/**` | sahibi (kimlik birebir eşit) ya da `FS_ALL_READ` | sahibi ya da `FS_ALL_WRITE` |
| `/data/media/**` | `FS_USER_READ` ya da `FS_ALL_READ` | `FS_USER_WRITE`/`FS_USER_DELETE` ya da `FS_ALL_WRITE` |
| diğer | yok | yok |

Test: `cargo test -p nc-fs-policy`.
