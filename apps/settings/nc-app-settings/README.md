# nc-app-settings

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/settings/nc-app-settings` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-app`, `nc-efra`, `nc-font`, `nc-fs-facade`, `nc-gfx`, `nc-sys-facade`, `nc-ui` |
| **Kullananlar** | `nc-settings-r3`, `nc-app-files`, `nc-apps` |
| **Özellikler (features)** | `default`, `kernel-sys` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ayarlar uygulamasi (id nc.settings). Iki sayfa: PERSONALIZE (duvar kagidi ve vurgu rengi secimi) ve DISPLAY (cozunurluk, salt okunur). Bagimlilik: nc-app, nc-gfx, nc-font, nc-ui. Yeni uygulama yazarken sablon olarak kullanilabilir (bkz. nc-app/README.md).
