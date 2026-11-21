# nc-arch-aarch64

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/hardware/nc-arch-aarch64` |
| **Katman** | Donanım |
| **Bağımlılıklar** | — |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

AArch64 (ARMv8-A ve sonrası) mimari katmanı. `nc-arch` cephesi `target_arch = "aarch64"` iken bunu dışa aktarır.

| Modül | Görev |
|---|---|
| `serial` | PL011 (QEMU virt, Raspberry Pi) ve 16550 uyumlu (MediaTek, 8/32 bit yazmaç erişimi) UART; `configure(kind, base, reg_shift)` ile seçilir |
| `time` | `CNTVCT_EL0`/`CNTFRQ_EL0` tabanlı `Clock` (x86'daki PIT+TSC karşılığı) |
| `rand` | `RNDR` (FEAT_RNG) varsa donanım rastgeleliği, yoksa sayaç tohumlu xorshift |
| `rtc` | PL031 saat (varsa); diğer RTC'ler cihaz sürücüsünden gelir |
| `psci` | `SYSTEM_OFF` / `SYSTEM_RESET`; `hvc` veya `smc` kanalı cihaz profilinden seçilir |
| `exceptions` | VBAR_EL1 vektör tablosu; istisnada ESR/ELR/FAR'ı UART'a yazıp durur |
| `mmu` | Kimlik eşlemeli 4 GiB: RAM = Normal WB, framebuffer = Normal NC, geri kalan = Device (2 MiB blok) |

## Kurallar

- Gerçek makinede UART'tan önce hiçbir şey yazılamaz; `serial::configure` ilk çağrıdır.
- MMU açılmadan atomik işlem (kilit) çalışmaz; `mmu::enable` heap'ten önce çağrılmalıdır.
- Çekirdek `+strict-align` ile derlenir (MMU kapalıyken hizasız erişim hata verir).
- Framebuffer Normal-NC eşlenir; yine de çizimden sonra `cache_clean_range` çağrısı zararsızdır.
- 4 GiB üstü RAM henüz eşlenmez.

## Sağlamlaştırma notları

- Açılışta `.bss` ve yığın aralığı `dc ivac` ile geçersiz kılınır (firmware'den kalan bayat önbellek satırları sıfırlanmış belleği ezmesin).
- Yürütülebilir yalnızca çekirdek imajını kapsayan 2 MiB bloklardır; geri kalan RAM, framebuffer ve MMIO `PXN|UXN`. EL0 hiçbir yerde yürütemez.
- İstisna vektörü ayrı 8 KiB acil yığına geçer (yığın taşması bile raporlanır); rapor içi istisna sonsuz döngüye değil `halt`'a gider.
- `AtomicBool::swap`/`compare_exchange` MMU açılmadan önce kullanılmaz (Device bellekte özel erişim hata verebilir); erken kodda yalnızca `load`/`store`.
- Bilinen sınırlar: tüm RAM tek blokta RW (2 MiB blok), çekirdek `.text` yazılabilir; W^X için ince taneli (4 KiB) eşleme Aşama 3'te. `CNTFRQ_EL0` sıfırsa saat varsayılanı yanlış olabilir.
