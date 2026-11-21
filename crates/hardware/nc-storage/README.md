# nc-storage

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/hardware/nc-storage` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-arch`, `nc-crypto`, `nc-driver`, `nc-hal` |
| **Kullananlar** | `nc-kernel`, `nc-fs` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Disk erişimi ve kurulum. `no_std`.

| Dosya | İçerik |
|---|---|
| `lib.rs` | ATA PIO aygıtı (`AtaDevice`, `read_sector`, `write_sector`, `flush`), disk bulma (`find_installed_disk`, `find_install_target_disk`, `find_external_usb_disk`), kurulum (`install_to_disk`, `write_initial_locale`, `read_initial_locale`), harici USB exFAT, güvenlik kasası (`seal_security_vault`, `verify_boot_integrity`), disk kilidi (`read_lock_state`, `apply_lock_request`, `record_lock_attempt`) |
| `ahci.rs`, `nvme.rs` | AHCI ve NVMe blok aygıtı |
| `gpt.rs`, `partition.rs` | GPT ve bölüm tabloları |
| `mbr.bin` | Üretilmiş MBR (`bootloader/bios/mbr.asm`'den `build.sh` yazar; `lib.rs` gömer; elle düzenlenmez) |
| `LOCKING.md` | Denetleyici kilitleri ve kesme bağlamı kuralları |

Disk düzeni sabitleri (`ESP_START_LBA`, `SYSTEM_START_LBA`, `SECURITY_START_LBA`, `DATA_START_LBA`, `MIN_INSTALL_SECTORS`) `lib.rs`
başındadır. Disk kilidi kayıt mantığı `nc-crypto::lock`'tadır.
