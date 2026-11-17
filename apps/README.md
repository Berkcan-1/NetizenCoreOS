# apps

Uygulamalar. Her uygulama kendi klasöründe; bir uygulamayla çalışırken yalnızca o klasörü vermek yeter.

| Klasör | Uygulama mantığı (kütüphane) | Ring 3 giriş ikilisi | Not |
|---|---|---|---|
| `files/` | `nc-app-files` (id `nc.files`) | `nc-files-r3` | Gezinme, ses çalma, .ntzc kurma |
| `settings/` | `nc-app-settings` (id `nc.settings`) | `nc-settings-r3` | |
| `system/` | `nc-app-system` (id `nc.system`) | `nc-system-r3` | Yeni uygulama için en yalın şablon |
| `stopwatch/` | `nc-app-stopwatch` (id `nc.stopwatch`) | `nc-stopwatch-r3` | Ring 3 mantığı ayrı: `nc-r3-stopwatch` (`nc-app-host` kullanılmaz) |
| `browser/` | `nc-app-browser` (id `nc.browser`) | `nc-browser-r3` | `nc-js`: JavaScript yorumlayıcısı |
| `registry/` | `nc-apps` | — | Çekirdeğe gömülen yerleşik uygulama listesi |

## İki tür crate

- **Mantık crate'i** (`nc-app-*`): `nc_app::App` sözleşmesini uygular, kök workspace üyesidir ve çekirdekte (Ring 0) ya da Ring 3'te çalışabilir.
- **Ring 3 giriş crate'i** (`nc-*-r3`): `no_std`/`no_main` ELF ikilisi; Dosyalar, Ayarlar, Sistem ve Tarayıcı `nc-app-host` ile
  mantık crate'ini Ring 3'e bağlar, Kronometre ise `nc-r3-stopwatch` + `nc_app_sdk::dispatch` kullanır. Kendi
  `[workspace]`'i ve `Cargo.lock`'u vardır (kök workspace üyesi değildir); `bash build.sh` bunları
  `tools/build/ring3.sh` ile derleyip `kernel/embedded/ring3-apps/` içine yazar.

## Yeni uygulama eklemek

1. `apps/<ad>/nc-app-<ad>/` oluştur (şablon: `apps/system/nc-app-system`), kök `Cargo.toml` üye listesine ekle.
2. Ring 3 için `apps/<ad>/nc-<ad>-r3/` oluştur (şablon: `apps/files/nc-files-r3`) ve `tools/build/ring3.sh` içindeki
   uygulama listesine `<ad>:nc-<ad>-r3` ekle; çekirdekte `kernel/src/ring3/apps.rs` kaydı gerekir.
3. `apps/registry/nc-apps`'e uygulamayı ekle.
4. `tools/docs/modules.toml`'a özet yaz, `python3 tools/docs/sync_module_docs.py` ve `python3 tools/check/verify_layout.py` çalıştır.

Üçüncü taraf uygulamalar bu klasörde değil, `sdk/` ile paketlenir.

Bu klasör x86_64 uygulamalarını taşır. arm64 uygulamaları (`nc-home-r3`, `nc-files-v8a`, `nc-settings-v8a`) `aarch64crates/apps/` altındadır.
