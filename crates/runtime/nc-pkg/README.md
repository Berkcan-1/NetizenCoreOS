# nc-pkg

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-pkg` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-abi`, `nc-app`, `nc-caps`, `nc-elf` |
| **Yalnızca test bağımlılıkları** | `nc-gfx` |
| **Kullananlar** | `nc-app-files`, `nc-apps`, `nc-kernel-aarch64`, `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

NTZC paket biçimi, imza doğrulama ve yetki motoru.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `NtzcPackage` (`parse`, `parse_checked`, `parse_with_keys`, `runnable`, `capability_bits`, `is_system`, `is_trusted_builtin`), `PackageLoader`, `set_package_loader`, `load_packaged_app`, `PackageError` |
| `manifest.rs` | `PackageManifest`, `PackageAbi`, `valid_app_id` |
| `crypto.rs` | `compute_sha256`, `decode_hex` |
| `dysonsphere.rs` | `DysonSphereEngine`: `Capability`, `AuthDecision`, `AppIdentityRecord`, `AuditEvent`; `register_verified_package`, `check_authorization`, `revoke_app`, `init_dysonsphere`, `set_active_caller`, `authorize_active` |
| `tests.rs` | Birim testleri (gerçek ELF ile) |

Güvenilen yayıncı anahtarları `lib.rs` içindeki `TRUSTED_PUBLISHERS_HEX` listesindedir. Paketleri üreten araçlar `sdk/` altındadır
(`sdk/build-ntzc.py`, `sdk/gen-signing-key.py`); biçim ikisinde birebir aynı olmalıdır. Yetki bitleri `nc-caps`'ten gelir.
