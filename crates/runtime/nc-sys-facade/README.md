# nc-sys-facade

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-sys-facade` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-abi`, `nc-efra` |
| **İsteğe bağlı bağımlılıklar** | `nc-app`, `nc-app-sdk`, `nc-arch`, `nc-driver`, `nc-net` |
| **Kullananlar** | `nc-app-settings`, `nc-settings-r3`, `nc-kernel` |
| **Özellikler (features)** | `client`, `default`, `kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Sistem/ayar hizmeti cephesi (Wi-Fi, Bluetooth, sürücü güncelleme gibi çekirdek ayarlarına Ring 3'ten erişim).

| Dosya | İçerik |
|---|---|
| `mirror.rs` | İstemci tarafı yansıma: `Transport`, `set_transport`, `Wireless`, `WifiNetworkInfo`, `BluetoothDeviceInfo`, tarama/açma-kapama/bağlanma istekleri |
| `proto.rs` | Tel biçimi için `Writer` / `Reader` |
| `service.rs` | Çekirdek tarafı: `handle` (yalnızca `kernel` özelliği) |

`kernel` ve `client` özellikleri birlikte açılamaz. Çekirdek tarafını `kernel/src/ring3/runtime.rs` kullanır; istemci tarafını
Ayarlar uygulaması (`apps/settings/nc-app-settings`). Biçimler `nc-abi`'nin `sys.rs` dosyasıyla uyumlu kalmalıdır.
