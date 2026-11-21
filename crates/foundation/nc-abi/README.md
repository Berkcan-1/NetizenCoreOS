# nc-abi

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/foundation/nc-abi` |
| **Katman** | Temel |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-app-host`, `nc-app-sdk`, `nc-browser-r3`, `nc-files-r3`, `nc-files-v8a`, `nc-home-r3`, `nc-kernel-aarch64`, `nc-pkg`, `nc-r3-stopwatch`, `nc-settings-r3`, `nc-settings-v8a`, `nc-stopwatch-r3`, `nc-sys-facade`, `nc-system-r3`, `nc-fs-facade`, `nc-kernel`, `nc-net-facade`, `nc-ui` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ring 3 uygulamaları ile çekirdek arasındaki **tel biçimleri**: yalnızca veri yapıları ve kodlayıcı/ayrıştırıcılar, mantık yok.
Hem çekirdek hem Ring 3 tarafı aynı crate'i kullandığı için iki taraf birbirinden sapamaz. `no_std`, bağımlılığı yoktur.

## Modüller (`src/`)

| Dosya | İçerik |
|---|---|
| `ctx.rs` | `ContextBlock`: uygulamaya verilen bağlam (platform adı dahil), `encode`/`decode` |
| `event.rs` | `EventBlock`: çekirdekten uygulamaya olay bloğu |
| `key.rs` | Tuş kodu ↔ karakter dönüşümü |
| `draw.rs` | Çizim komut akışı: `Cmd`, `CmdWriter`, `CmdReader`, `validate`, `Summary`, `CmdError` |
| `vfs.rs` | Dosya sistemi hizmeti istek/yanıt kodlaması (`Request`, `DirItem`, `DirEntries`, `WireError`) |
| `net.rs` | Ağ hizmeti istek/yanıt kodlaması (HTTP durumu, başlıklar, `PollFixed`, `StartRequest`) |
| `sys.rs` | Sistem/ayar hizmeti istek/yanıt kodlaması |
| `lib.rs` | `heap_for` ve ortak sabitler |

## Notlar

- Çizim komut ayrıştırıcısı güvenilmeyen girdiyle karşılaşır; `python3 tools/model/draw_model.py` bu doğrulayıcının Python modelini
  ve fuzz testini çalıştırır.
- Bir biçimi değiştirirsen Ring 3 tarafını (`crates/runtime/nc-app-sdk`, `nc-app-host`, cepheler) ve çekirdek tarafını
  (`kernel/src/tasking`, `kernel/src/ring3`) birlikte güncelle. Syscall sözleşmesi: `kernel/SYSCALL_ABI.md`.
