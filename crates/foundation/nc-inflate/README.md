# nc-inflate

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/foundation/nc-inflate` |
| **Katman** | Temel |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

DEFLATE açıcı. `no_std`, bağımlılığı yoktur. İki giriş noktası: `inflate_raw` (ham DEFLATE) ve `gunzip` (gzip sarmalı).

Çekirdek, gömülü Ring 3 ELF dosyalarını (`kernel/embedded/ring3-apps/*.elf.gz`) çalıştırmadan önce `kernel/src/ring3/apps.rs`
içinde bu crate ile açar.
