# nc-caps

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/runtime/nc-caps` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-fs-policy`, `nc-kernel`, `nc-kernel-aarch64`, `nc-pkg` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Yetki (capability) bitleri için **tek doğruluk kaynağı**. `no_std`, bağımlılığı yoktur; yalnızca `u32` sabitleri içerir.

| Sabit | Bit |
|---|---|
| `SETTINGS_READ` | 0 |
| `SETTINGS_WRITE_DISPLAY` / `_LOCALE` / `_KEYBOARD` / `_NETWORK` / `_DRIVERS` | 1 / 2 / 3 / 4 / 5 |
| `FS_USER_READ` / `FS_USER_WRITE` / `FS_USER_DELETE` | 6 / 7 / 8 |
| `FS_GRANT_READ` / `FS_GRANT_WRITE` | 9 / 10 |
| `PACKAGE_INSTALL` | 11 |
| `NETWORK_CLIENT` | 12 |
| `SYSTEM_AUDIT_READ` | 13 |
| `IPC_ENDPOINT` | 14 |
| `KERNEL_INTERNAL` | tüm bitler: yalnızca çekirdek içi kimlik; üçüncü taraf bunu alamaz |

İki yetki aynı biti paylaşırsa veya tek bit değilse derleme hata verir. Çekirdek (`kernel/src/tasking/process.rs`) ve
`nc-pkg` (`Capability`) aynı değerleri buradan türetir; yeni yetki eklerken yalnızca bu dosyaya yeni bit ekle.
