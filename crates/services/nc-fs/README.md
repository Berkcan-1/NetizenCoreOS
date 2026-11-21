# nc-fs

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/services/nc-fs` |
| **Katman** | Çekirdek hizmetleri |
| **Bağımlılıklar** | `nc-hal` |
| **İsteğe bağlı bağımlılıklar** | `nc-storage` |
| **Kullananlar** | `nc-apps`, `nc-desktop`, `nc-kernel`, `nc-kernel-aarch64`, `nc-fs-facade` |
| **Özellikler (features)** | `default`, `storage` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Sanal dosya sistemi (VFS). `no_std`.

| Dosya | İçerik |
|---|---|
| `vfs.rs` | `FileSystem` trait'i; `FileType`, `DirEntry`, `FileStat`, `FsError` |
| `fat32.rs`, `exfat.rs`, `erofs.rs` | Dosya sistemi gerçeklemeleri (`Fat32`, `Exfat`, `Erofs`) |
| `memfs.rs` | Bellek içi dosya sistemi (`MemFs`, `SharedMemFs`) |
| `lib.rs` | `VfsManager`: bölümleri bağlar, USB tarama (`rescan_usb`, `is_usb_online`, `usb_label`), yol bazlı okuma/listeleme (`list_dir_path`, `read_file_path`, `read_file_range_path`), `partition_metrics`; `set_vfs_caller`/`get_vfs_caller` |

Uygulamalar `nc-fs`'e doğrudan değil `crates/runtime/nc-fs-facade` üzerinden erişir; böylece aynı kod Ring 0'da ve Ring 3'te çalışır.

`storage` özelliği (varsayılan açık) `manager.rs` içindeki `VfsManager`'ı ve `nc-storage` bağımlılığını (x86 ATA/AHCI/NVMe, port G/Ç) etkinleştirir. `default-features = false` ile yalnızca taşınabilir parçalar (`vfs`, `fat32`, `exfat`, `erofs`, `memfs`) derlenir; aarch64 çekirdeği böyle kullanır.
