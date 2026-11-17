# nc-elf

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-elf` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-kernel-aarch64`, `nc-pkg`, `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ring 3 imajları için ELF64 doğrulayıcı ve yükleme planı üretici. `no_std`, bağımlılığı yoktur; hiçbir şey belleğe yüklemez,
yalnızca `parse` ile güvenli bir `LoadPlan` (`Segment` listesi) üretir veya `ElfError` döner.

- `Policy`: kabul kuralları (yürütülebilir/yazılabilir segmentler, adres aralığı vb.).
- `Segment`: `page_start`, `page_end`, `page_count`, `writable`, `executable`.
- `ElfError::as_str`: hata nedeni.

Çekirdek (`kernel/src/tasking/process.rs`, `kernel/src/ring3/`) ve `nc-pkg` kullanır. Testler gerçek bir ELF kullanır:
`kernel/embedded/ring3-selftest/faz6_app.elf`. Python modeli ve fuzz: `python3 tools/model/elf_abi_model.py`.
