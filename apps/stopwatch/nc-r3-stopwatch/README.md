# nc-r3-stopwatch

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/stopwatch/nc-r3-stopwatch` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk` |
| **Yalnızca test bağımlılıkları** | `nc-app`, `nc-app-stopwatch`, `nc-gfx`, `nc-i18n`, `nc-ui` |
| **Kullananlar** | `nc-stopwatch-r3` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Kronometrenin **bağımsız Ring 3 sürümü**: `nc-app-host` ve `nc-app` kullanmadan `nc_app_sdk::RingApp` olarak yazılmıştır
(`StopwatchApp`). Çalışma ikilisi `apps/stopwatch/nc-stopwatch-r3`'tür.

| Dosya | İçerik |
|---|---|
| `src/lib.rs` | `StopwatchApp` |
| `src/strings.rs` | **Üretilmiş** dil tabloları (`for_language`): `python3 tools/gen/gen_r3_strings.py` yazar, elle düzenlenmez |
| `tests/parity.rs` | `nc-i18n` ile eşlik testi: üretilmiş tablo güncel değilse test kırılır |

`nc-app`, `nc-gfx`, `nc-ui` (remote), `nc-i18n` ve `nc-app-stopwatch` yalnızca testlerde (dev-dependency) kullanılır.
