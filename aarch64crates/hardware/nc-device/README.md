# nc-device

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/hardware/nc-device` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-hal` |
| **Kullananlar** | `nc-dev-qemu-virt`, `nc-dev-rpi4`, `nc-dev-x6826c`, `nc-device-select`, `nc-kernel-aarch64` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Cihaz profili sözleşmesi. `aarch64crates/drivers/<cihaz>/` klasörlerindeki her cihaz crate'i bu türlerle bir `PROFILE` sabiti tanımlar;
çekirdek seçilen cihazın profilini `nc-device-select` üzerinden okur.

## Genel API

- `DeviceProfile`: kimlik, SoC, mimari (`Arch`), arayüz profili (`UiProfile`), RAM bölgeleri, UART türü/adresi, PSCI kanalı,
  ekran (`DisplayProfile`), sürücü listesi (`DriverEntry`) ve isteğe bağlı `register` işlevi.
- `DriverHost`: cihaz sürücülerinin çekirdeğe blok/giriş aygıtı eklediği arayüz.
- `UartKind`, `PsciConduit`, `FramebufferSource`, `DriverClass`: sabit tablolar.

## Kurallar

- RAM bölgesi boş bırakılırsa çekirdek DTB'deki `/memory` düğümünü kullanır.
- `register` içinde yalnızca `'static` aygıt nesneleri eklenir (heap gerekmez).
- Cihaza özel bilgi (adresler, IRQ'lar) yalnızca `aarch64crates/drivers/` altında durur; çekirdeğe cihaz adı yazılmaz.
