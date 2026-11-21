# nc-arch

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/hardware/nc-arch` |
| **Katman** | Donanım |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-apps`, `nc-driver`, `nc-input`, `nc-kernel`, `nc-kernel-aarch64`, `nc-net`, `nc-platform-generic`, `nc-platform-qemu`, `nc-storage`, `nc-sys-facade` |
| **Özellikler (features)** | `host-sim` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Mimari cephesi. Hedef mimariye göre gerçek uygulamayı yeniden dışa aktarır; üst katmanlar yalnızca `nc_arch::...` yolunu görür.

| Hedef | Uygulama crate'i |
|---|---|
| `x86_64` | `nc-arch-x86_64` (port G/Ç, IDT, APIC, RTC, PIT/TSC) |
| `aarch64` | `nc-arch-aarch64` (PL011/16550 UART, genel zamanlayıcı, istisna vektörleri, MMU, PSCI) |

Ortak sözleşme (her uygulamada aynı adla bulunur): `serial::{init, present, write_byte, write_str}`, `time::{read_tsc, Clock}`,
`rand::HardwareRng`, `rtc::unix_time_now`, `halt_forever`, `wait_for_interrupt`, `reboot`, `enable_cpu_hardening`.
Port G/Ç (`outb`, `inb`, ...), `idt`, `apic` ve `disable_pic` yalnızca x86_64'te vardır; aarch64'te bulunmazlar ve bunları
kullanan kod `#[cfg(target_arch = "x86_64")]` ile ayrılmalıdır.

Yeni mimari eklemek için `docs/architecture/MULTIARCH.md` dosyasına bakın.
