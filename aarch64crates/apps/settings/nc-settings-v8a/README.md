# nc-settings-v8a

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/apps/settings/nc-settings-v8a` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

AArch64 (ARMv8-A, EL0) için gerçek donanım, depolama, yaşam döngüsü ve kalıcı `/data` ayarlarına bağlı Ayarlar uygulaması (`nc.settings`).
Sahte Wi-Fi, Bluetooth, pil veya ses denetimi içermez; yalnızca gerçek çekirdek durumunu (`OP_DEVICE_INFO`) gösterir ve değişiklikleri `/data/appdata/nc.settings/settings.cfg` dosyasına kalıcı yazar (`OP_MOBILE_SET`).
