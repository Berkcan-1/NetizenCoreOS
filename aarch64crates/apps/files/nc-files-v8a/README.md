# nc-files-v8a

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/apps/files/nc-files-v8a` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk`, `nc-brand` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

AArch64 (ARMv8-A, EL0) için yalın ve gerçek VFS tabanlı Dosyalar uygulaması (`nc.files`).
`/data` (exFAT, yazılabilir) ve `/system` (FAT32, salt-okunur) bölümlerinde gezinir, dosya okur, `/data` altında klasör ve metin dosyası oluşturur/düzenler/siler ve `.ntzc` paketlerini inceler/kurar.
