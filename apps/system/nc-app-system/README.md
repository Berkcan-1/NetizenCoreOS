# nc-app-system

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/system/nc-app-system` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-app`, `nc-font`, `nc-gfx`, `nc-ui` |
| **Kullananlar** | `nc-system-r3`, `nc-app-files`, `nc-apps` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Sistem bilgisi uygulamasi (id nc.system): cekirdek surumu, cozunurluk, platform, calisma suresi. Etkilesimi yoktur. Bagimlilik: nc-app, nc-gfx, nc-font, nc-ui. Yeni uygulama yazarken sablon olarak kullanilabilir (bkz. nc-app/README.md).
