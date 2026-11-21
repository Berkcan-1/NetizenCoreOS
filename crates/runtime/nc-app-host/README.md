# nc-app-host

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-app-host` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-abi`, `nc-app`, `nc-app-sdk`, `nc-gfx` |
| **Kullananlar** | `nc-browser-r3`, `nc-files-r3`, `nc-settings-r3`, `nc-system-r3`, `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Mevcut `nc_app::App` uygulamalarını **değiştirmeden** Ring 3'te çalıştıran ana-bilgisayar çalışma zamanı. `no_std` + `alloc`.

Çekirdek her olayda uygulamayı baştan çağırır (olay-tabanlı model, bkz. `nc-abi`); durum bu crate'in `static`'inde ve uygulamanın
heap'inde kalır. Çizim, pencere boyutunda bir piksel tamponuna yapılır: `SYS_GUI_BACKDROP` altta kalan masaüstünü tampona
kopyalar, uygulama çizer, `SYS_GUI_PRESENT` tamponu pencereye kırparak basar. Uygulama pencereyi (0,0)-(w,h) olarak görür.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `run(&HEAP, factory)`, `AppFactory`, `Host`, `HostIo`/`SysIo`, bağlam ve girdi dönüşümleri (`block_from_context`, `settings_from_block`, `input_from_event`, `encode_key`/`decode_key`) |
| `heap.rs` | Uygulamanın küçük heap'i: `Heap`, `GlobalHeap` |
| `tests.rs` | Birim testleri |

Kullanımı için `apps/files/nc-files-r3/src/main.rs` örnektir.
