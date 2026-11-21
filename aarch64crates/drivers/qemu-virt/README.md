# qemu-virt

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/drivers/qemu-virt` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-device` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

QEMU `-M virt` makinesi (aarch64). Gerçek donanım yoktur; çekirdeği ve arayüzü denemek için varsayılan cihazdır.

- UART: PL011 `0x09000000`; RTC: PL031 `0x09010000`; sanal blok: virtio-mmio `0x0a000000` (ilk yuva).
- Ekran: `FramebufferSource::Fixed(0x50000000)`, 720x1280, BGRX. Çekirdek bu adresi `nc-arch-aarch64::fwcfg::setup_ramfb` ile
  QEMU'nun `ramfb` aygıtına bildirir; bu yüzden QEMU'ya `-device ramfb` verilir (`build.sh` otomatik ekler). RAM en az 512 MiB olmalıdır.
- PSCI kanalı: `hvc` (QEMU virt varsayılanı).
