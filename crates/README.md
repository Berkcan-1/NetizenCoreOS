# crates

Paylaşılan Rust modülleri, sorumluluğa göre altı katman klasörüne ayrılmıştır. Hepsi `no_std` ve bağımsız kutulardır; kök
`Cargo.toml` bunları çalışma alanı üyesi yapar. Uygulamalar `apps/`, çekirdek `kernel/`, yükleyiciler `bootloader/` altındadır.

| Klasör | Katman | İçerik |
|---|---|---|
| `foundation/` | 1 | Sözleşmeler (`nc-hal`, `nc-abi`, `nc-efra`) ve bağımsız kütüphaneler (`nc-math`, `nc-crypto`, `nc-inflate`, `nc-i18n`) |
| `hardware/` | 2 | `nc-arch`, `nc-driver`, `nc-input`, `nc-storage`, `nc-platform-qemu`, `nc-platform-generic` |
| `services/` | 3 | `nc-mem`, `nc-fs`, `nc-net` |
| `graphics/` | 4 | `nc-gfx`, `nc-font`, `nc-media`, `nc-assets` |
| `ui/` | 5 | `nc-ui`, `nc-app`, `nc-splash`, `nc-setup`, `nc-desktop` |
| `runtime/` | 6 | `nc-caps`, `nc-elf`, `nc-pkg`, `nc-app-sdk`, `nc-app-host`, `nc-fs-facade`, `nc-net-facade`, `nc-sys-facade` |

Kural: bir crate yalnızca **kendi katmanındaki veya altındaki** crate'lere bağlanır (`python3 tools/check/verify_layout.py` denetler).
Crate adları klasör adlarıyla aynıdır. Tam bağımlılık haritası: `docs/architecture/DEPENDENCIES.md`; düzen kuralları: `docs/architecture/LAYOUT.md`.

Yeni crate eklerken: klasörü uygun katmanda oluştur, `Cargo.toml` yaz, kök `Cargo.toml` üye listesine ekle, `README.md` ve
`src/README.md` yaz, `tools/docs/modules.toml`'a özet ekle ve `python3 tools/docs/sync_module_docs.py` çalıştır.
