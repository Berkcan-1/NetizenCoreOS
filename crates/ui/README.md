# crates/ui — Arayüz

Arayüz araç takımı ve ekranlar: `nc-ui` (tema ve çizim yardımcıları), `nc-app` (uygulama sözleşmesi), `nc-splash`, `nc-setup`, `nc-desktop`.

Çekirdek bu katmana yalnızca `kernel/src/main.rs` içinden ve (nc-ui için) `kernel/src/ring3/` içinden temas eder. Genel API'yi değiştirmediğin sürece bu katmanda çalışırken çekirdeğe dokunulmaz.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-app`](nc-app/README.md) | Uygulama sözleşmesi: App trait, Context, giriş olayları, ikonlar ve pencere çerçevesi | nc-crypto, nc-font, nc-gfx, nc-hal, nc-i18n, nc-ui | nc-app-browser, nc-app-files, nc-app-host, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-apps, nc-browser-r3, nc-desktop, nc-files-r3, nc-pkg, nc-settings-r3, nc-system-r3, nc-kernel*, nc-sys-facade* |
| [`nc-desktop`](nc-desktop/README.md) | Masaüstü: duvar kâğıdı, simgeler, görev çubuğu, başlat menüsü ve pencere yönetimi | nc-app, nc-assets, nc-audio, nc-font, nc-fs, nc-gfx, nc-hal, nc-math, nc-media, nc-ui | nc-kernel |
| [`nc-setup`](nc-setup/README.md) | Kurulum sihirbazı ekranları | nc-audio, nc-driver, nc-font, nc-gfx, nc-hal, nc-ui | nc-kernel |
| [`nc-splash`](nc-splash/README.md) | Açılış animasyonu | nc-assets, nc-font, nc-gfx, nc-math, nc-mem, nc-ui | nc-kernel, nc-kernel-aarch64 |
| [`nc-ui`](nc-ui/README.md) | Tema, ortak çizim yardımcıları ve Ring 3 uzak çizim komut katmanı (remote) | nc-font, nc-gfx, nc-math, nc-abi* | nc-app, nc-app-browser, nc-app-files, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-setup, nc-splash |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
