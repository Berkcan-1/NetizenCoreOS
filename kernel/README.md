# kernel

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `kernel` |
| **Katman** | Çekirdek |
| **Bağımlılıklar** | `nc-apps`, `nc-arch`, `nc-audio`, `nc-caps`, `nc-desktop`, `nc-driver`, `nc-fs`, `nc-fs-facade`, `nc-gfx`, `nc-hal`, `nc-input`, `nc-math`, `nc-mem`, `nc-net`, `nc-setup`, `nc-splash`, `nc-storage`, `nc-ui` |
| **İsteğe bağlı bağımlılıklar** | `nc-abi`, `nc-app`, `nc-app-host`, `nc-elf`, `nc-inflate`, `nc-pkg`, `nc-platform-generic`, `nc-platform-qemu`, `nc-sys-facade` |
| **Kullananlar** | — |
| **Özellikler (features)** | `audio-selftest`, `default`, `generic`, `kernel-paging`, `qemu`, `ring0-fallback`, `ring3-apps` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Çekirdek kutusu (`nc-kernel`, ikili adı `kernel`). Açılış, süreçler, syscall, IPC ve Ring 3 çalışma zamanı burada;
arayüz, sürücü, dosya sistemi ve ağ mantığı `crates/` ve `apps/` altındaki modüllerdedir.

## Klasörler

| Yol | İçerik |
|---|---|
| `src/main.rs` | `kernel_main` (açılış akışı, splash/kurulum/masaüstü döngüleri), panik ekranı, RAM sınırı tespiti |
| `src/boot/` | `entry.rs` (_start), `platform.rs` (profil seçimi), `prepare.rs` (aygıt/hazırlık), `devices.rs` (DeviceManager) |
| `src/tasking/` | `process.rs` (süreçler), `syscall.rs` (syscall işleyicileri), `ipc.rs` (uç noktalar ve mesajlar) |
| `src/ring3/` | `runtime.rs` (Ring 3 çalışma zamanı), `apps.rs` (gömülü ELF'lerin kaydı ve paket yükleyici), `net.rs` (ağ hizmeti) |
| `embedded/ring3-apps/` | Ring 3 ELF ve .gz dosyaları (`build.sh` üretir, çekirdeğe gömülür) |
| `embedded/ring3-selftest/` | Ring 3 kendini-sınama uygulaması ve üreticisi |
| `linker.ld`, `build.rs` | Bağlayıcı betik ve onu bağlayan build.rs |
| `SYSCALL_ABI.md` | Syscall sözleşmesi |

Modül adları (`crate::process`, `crate::ring3`, `crate::ring3_app`, `crate::ring3_net`, `crate::syscall`, `crate::ipc`, ...) eskisiyle
aynıdır: `main.rs` içindeki `mod` bildirimleri dosyaları `#[path = "..."]` ile alt klasörlere bağlar. Böylece dosya içi
`use crate::...` satırları değişmedi.

## Dokunma yüzeyi (hangi dosya hangi crate'lere bağlı)

| Dosya | Kullandığı crate'ler |
|---|---|
| `src/main.rs` | nc-apps, nc-arch, nc-desktop, nc-driver, nc-gfx, nc-hal, nc-input, nc-math, nc-mem, nc-setup, nc-splash, nc-storage, nc-ui |
| `src/boot/devices.rs` | nc-hal |
| `src/boot/platform.rs` | nc-platform-generic, nc-platform-qemu |
| `src/boot/prepare.rs` | nc-hal, nc-input, nc-net, nc-storage |
| `src/tasking/ipc.rs` | nc-mem |
| `src/tasking/process.rs` | nc-abi, nc-arch, nc-caps, nc-elf, nc-gfx, nc-mem, nc-net |
| `src/tasking/syscall.rs` | nc-arch, nc-mem |
| `src/ring3/apps.rs` | nc-abi, nc-app, nc-app-host, nc-apps, nc-arch, nc-caps, nc-elf, nc-gfx, nc-inflate, nc-pkg, nc-ui |
| `src/ring3/net.rs` | nc-abi, nc-net |
| `src/ring3/runtime.rs` | nc-abi, nc-app, nc-app-host, nc-arch, nc-fs, nc-gfx, nc-mem, nc-pkg, nc-sys-facade, nc-ui |

Sonuçlar:

- `nc-desktop`, `nc-setup` ve `nc-splash` yalnızca `src/main.rs`'ten kullanılır. İçlerinde düzenleme yaparken (genel API'yi
  değiştirmeden) çekirdeğe dokunulmaz.
- `nc-ui` hem `main.rs`'te hem de Ring 3 uzak çizimi için `src/ring3/` içinde kullanılır.
- `src/main.rs` içindeki `kernel_main` splash, kurulum ve masaüstü döngülerini hâlâ birlikte barındıran büyük bir işlevdir; arayüz
  döngülerini ayrı bir crate'e çıkarmak ileride çekirdek bağını daha da azaltır (derleyiciyle doğrulanarak yapılmalıdır).

## Özellikler (features)

| Özellik | Anlamı |
|---|---|
| `qemu` / `generic` | Platform profili (tam biri seçilmeli) |
| `ring3-apps` (varsayılan) | Ring 3 çalışma zamanı ve gömülü ELF'ler |
| `ring0-fallback` | `nc-apps`'in Ring 0 uygulama gerçeklemeleri |
| `kernel-paging` | Çekirdek sayfa tabloları (`nc-mem::kpt`) |
