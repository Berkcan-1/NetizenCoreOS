# nc-app-stopwatch

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/stopwatch/nc-app-stopwatch` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-app`, `nc-font`, `nc-gfx`, `nc-ui` |
| **Kullananlar** | `nc-app-files`, `nc-apps` |
| **Testlerde kullananlar** | `nc-r3-stopwatch` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Kronometre (id nc.stopwatch). START/STOP ve RESET dugmeleri; MM:SS.T gosterir. Etkilesimli uygulama ornegi olarak da kullanilabilir. Bagimlilik: nc-app, nc-gfx, nc-font, nc-ui. Yeni uygulama yazarken sablon olarak kullanilabilir (bkz. nc-app/README.md).
