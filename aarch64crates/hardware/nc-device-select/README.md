# nc-device-select

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/hardware/nc-device-select` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-dev-rpi4`, `nc-device` |
| **Kullananlar** | `nc-kernel-aarch64` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Derleme sırasında seçilen cihazı çekirdeğe bağlayan **üretilen** crate. `Cargo.toml` ve `src/lib.rs` içeriğini
`aarch64crates/tools/select_device.py` yeniden yazar (`bash build.sh armv8a android x6826c` bunu otomatik çalıştırır).
Elle düzenlemeyin; depoda varsayılan olarak `aarch64crates/drivers/qemu-virt` seçili gelir.

Dışa aktarılanlar: `PROFILE` (`nc_device::DeviceProfile`) ve `SELECTED_ID`.
