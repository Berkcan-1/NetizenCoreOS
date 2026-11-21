# tools

| Klasör | İçerik |
|---|---|
| `build/` | `build.sh`'ın işlev modülleri (derleme, ISO, QEMU, VirtualBox, disk, radyo köprüsü) ve `ring3-cargo.sh` |
| `check/` | `verify_layout.py`: klasör düzeni, katman kuralı ve yol bütünlüğü denetimi |
| `docs/` | `sync_module_docs.py` ve `modules.toml`: README bağımlılık blokları, `DEPENDENCIES.md` ve `MODULES.md` üretimi |
| `gen/` | `gen_r3_strings.py`: `nc-r3-stopwatch` dil tablosu üreticisi |
| `model/` | Derleyici gerektirmeyen Python modelleri (kpt, heap, nc-elf, nc-abi draw); bkz. `model/README.md` |

arm64'e ait build betikleri ve cihaz/Android araçları `aarch64crates/tools/` altındadır (bkz. `aarch64crates/README.md`).

Tüm betikler proje kökünden çalıştırılacak şekilde yazılmıştır (`python3 tools/...`).
