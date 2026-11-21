# nc-apps

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/registry/nc-apps` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-app`, `nc-app-files`, `nc-arch`, `nc-fs`, `nc-gfx`, `nc-pkg` |
| **İsteğe bağlı bağımlılıklar** | `nc-app-browser`, `nc-app-settings`, `nc-app-stopwatch`, `nc-app-system` |
| **Yalnızca test bağımlılıkları** | `nc-desktop`, `nc-hal` |
| **Kullananlar** | `nc-kernel` |
| **Özellikler (features)** | `default`, `ring0-fallback` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Cekirdege dahil edilen yerlesik uygulamalarin listesi. Kernel sadece bu crate'e baglanir; hangi uygulamalarin
gomuldugunu burasi belirler.

## Genel API

- builtin() -> Vec<Box<dyn App>>: Ayarlar, Sistem, Kronometre (bu sirayla). Sira, simge ve baslat menusu sirasidir.

## Uygulama eklemek / cikarmak

lib.rs icindeki apps.push satirini ekle veya sil, Cargo.toml'daki bagimliligi buna gore guncelle. Baska hicbir yeri
degistirmek gerekmez (nc-desktop uygulamalari adiyla bilmez).

Calisma aninda uygulama eklemek icin (ileride paket yoneticisi) DesktopState::install_app kullanilir.
