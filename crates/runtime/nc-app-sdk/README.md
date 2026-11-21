# nc-app-sdk

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-app-sdk` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-abi` |
| **Kullananlar** | `nc-app-host`, `nc-brand`, `nc-browser-r3`, `nc-files-r3`, `nc-files-v8a`, `nc-home-r3`, `nc-mobile-ui`, `nc-r3-stopwatch`, `nc-settings-r3`, `nc-settings-v8a`, `nc-stopwatch-r3`, `nc-system-r3`, `nc-fs-facade`, `nc-net-facade`, `nc-sys-facade` |
| **Özellikler (features)** | `host` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ring 3 uygulamaları için SDK. `no_std`. Üçüncü taraf uygulamalar (`sdk/ring3-sample`) ve yerleşik Ring 3 ikilileri bunu kullanır.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `RingApp` trait'i, `Pointer`, `read_event`, `dispatch`, `run`, `run_with` |
| `sys.rs` | Syscall sarmalayıcıları: `syscall3`, `exit`, `log`, `gui_draw`, `text_width`, `gui_backdrop`, `gui_present`, dosya işlemleri (`file_open/read/write/close`), `pkg_install`, `vfs_call` |
| `gfx.rs` | `Gfx`: komut tamponuna çizim (`fill_rect`, `panel`, `dot`, `text`, `text_centered`, `button`) |
| `line.rs` | Metin/sayı biçimlendirme yardımcısı |
| `app.ld` | Ring 3 ELF bağlayıcı betiği (her giriş crate'inin `build.rs`'i bunu bağlar). aarch64 karşılığı `aarch64crates/runtime/app-aarch64.ld` dosyasına taşındı |

Çizim komutlarının tel biçimi `nc-abi`'de, syscall sözleşmesi `kernel/SYSCALL_ABI.md` içindedir.
