# kernel/embedded

Çekirdeğe `include_bytes!` ile gömülen ikililer. Kaynak kod değildir.

| Klasör | İçerik | Kim üretir |
|---|---|---|
| `ring3-apps/` | `<ad>.elf` ve `<ad>.elf.gz` (files, settings, system, stopwatch, browser) | `bash build.sh` → `tools/build/ring3.sh` (`apps/*/nc-*-r3` crate'lerinden) |
| `ring3-selftest/` | `faz6_app.elf`, kaynakları (`faz6_app.S`, `faz6.ld`) ve üretici `gen.py` | `python3 kernel/embedded/ring3-selftest/gen.py` |

`ring3-apps/` içeriği her derlemede yeniden yazılır. `ring3-selftest/faz6_app.elf` ayrıca `nc-elf` ve `nc-pkg` testlerinde
ve `tools/model/elf_abi_model.py` içinde sabit test verisi olarak kullanılır; dosya adı değişmedi.

Gömme noktaları: `kernel/src/ring3/apps.rs` (ring3-apps) ve `kernel/src/ring3/runtime.rs` (ring3-selftest).
