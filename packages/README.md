# packages

Çekirdeğe gömülmeyen, **ayrı `.ntzc` paketi** olarak dağıtılan sistem parçaları. Arayüz profiline göre iki klasör:

| Klasör | İçerik (hedef) |
|---|---|
| `mobile/` | Android usulü Ayarlar, mobil kabuk (başlatıcı, bildirim gölgesi, hızlı ayarlar, ana ekran), dokunmatik klavye |
| `desktop/` | Masaüstü Ayarlar ve kabuk bileşenleri (arm64 PC / x86_64) |

`bash build.sh armv8a android <cihaz>` cihazın `ui=` değerine göre `packages/<ui>/*.ntzc` dosyalarını `system.img` içindeki
`/packages` klasörüne koyar (`build/apps/*.ntzc` de eklenir). Paket derlemek için `sdk/build.sh` kullanılır (`sdk/README.md`).

**Durum:** `packages/mobile/` altında `nc.files.ntzc` (`aarch64crates/apps/files/nc-files-v8a`) ve `nc.settings.ntzc` (`aarch64crates/apps/settings/nc-settings-v8a`) `bash build.sh armv8a ...` sırasında otomatik derlenip paketlenir ve `super.img` `/packages` içine konur.
