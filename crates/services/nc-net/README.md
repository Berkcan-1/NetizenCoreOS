# nc-net

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/services/nc-net` |
| **Katman** | Çekirdek hizmetleri |
| **Bağımlılıklar** | `nc-arch`, `nc-driver`, `nc-hal`, `nc-mem` |
| **Kullananlar** | `nc-kernel`, `nc-net-facade`, `nc-sys-facade` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

DHCP, DNS, TCP, TLS 1.3 ve HTTP istemcisi.

## Son degisiklikler

- http.rs: yanit siniri 6 MiB (`MAX_RESPONSE_BYTES`). Onceki '1 MB' ifadesi kodla uyusmuyordu.
- http.rs: parcali govde cozucude parca sonu hesabi saturating_add ile yapilir, bozuk uzunluk panik uretmez.
- http.rs: `start()` artik host'u ilk `/`, `?` veya `#` karakterinde ayirir; `https://host?x=1` dogru cozulur, fragment sunucuya gonderilmez.
