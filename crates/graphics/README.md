# crates/graphics — Grafik

Çizim ve görsel kaynaklar: `nc-gfx` (Canvas, framebuffer aktarımı), `nc-font` (vektör yazı tipi), `nc-media` (PNG/JPEG/MP3/WAV çözücüler), `nc-assets` (logo).

Ekrana çizen kod bir Canvas'a çizer; framebuffer'a yalnızca `nc-gfx` yazar.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|
| [`nc-assets`](nc-assets/README.md) | Logo PNG hattı ve açılış sesi (assets → derleme zamanı gömme, ses IMA-ADPCM'e sıkıştırılır) | — | nc-audio, nc-desktop, nc-splash |
| [`nc-audio`](nc-audio/README.md) | Ses motoru: kesmesiz AC'97 halka akışı, 12 sesli mikser, sentezli ses teması ve açılış müziği | nc-assets, nc-hal | nc-desktop, nc-kernel, nc-setup |
| [`nc-font`](nc-font/README.md) | Vektör yazı tipi (kontur verisi + üretim araçları) | nc-math | nc-app, nc-app-browser, nc-app-files, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-desktop, nc-kernel-aarch64, nc-setup, nc-splash, nc-ui |
| [`nc-gfx`](nc-gfx/README.md) | Canvas, bulanıklaştırma, karıştırma ve ekrana aktarım | nc-hal | nc-app, nc-app-browser, nc-app-files, nc-app-host, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-apps, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-settings-r3, nc-setup, nc-splash, nc-ui |
| [`nc-media`](nc-media/README.md) | PNG/JPEG çözücü ve MP3/WAV PCM16 akışı | nc-hal | nc-app-files, nc-desktop, nc-kernel-aarch64 |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
