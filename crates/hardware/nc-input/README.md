# nc-input

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/hardware/nc-input` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-arch`, `nc-driver`, `nc-hal` |
| **Kullananlar** | `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Klavye ve fare girişi. `no_std`.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `Ps2Input` (`nc_hal::InputDevice` uygulaması), `init`, `poll_state`, `set_keyboard_layout` |
| `xhci.rs` | xHCI USB HID: `init_and_probe_xhci`, `UsbHidEvent` kuyruğu (`push_usb_hid_event`, `pop_usb_hid_event`), `decode_usb_boot_keyboard_report`, `decode_usb_boot_mouse_report`, `XhciControllerStatus` |

Çekirdek `kernel/src/boot/prepare.rs` ve `kernel/src/main.rs` içinde başlatır; masaüstü ve kurulum ekranı olayları bu kuyruktan okur.
