# bootloader/uefi

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `bootloader/uefi` |
| **Katman** | Önyükleyici |
| **Bağımlılıklar** | `nc-hal` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

`nc-uefi`: UEFI yükleyici (`x86_64-unknown-uefi` hedefi). `src/main.rs` UEFI tablolarını (`EfiSystemTable`, `EfiBootServices`,
GOP) kullanarak çerçeve tamponunu ve bellek haritasını alır, `nc-hal::BootInfo` yapısını doldurur ve çekirdeğe atlar.

- Yalnızca `nc-hal`'e bağlanır (bootloader yalnızca foundation katmanına bağlanabilir).
- Çekirdek ve stage2 ikilisi derleme sırasında ortam değişkenleriyle verilir (`NC_KERNEL_BIN`, `NC_STAGE2_BIN`); `tools/build/boot.sh`
  (`build_uefi_optional`) bunu üretir.
- Kök workspace üyesidir.
