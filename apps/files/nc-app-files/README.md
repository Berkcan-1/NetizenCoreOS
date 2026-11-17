# nc-app-files

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `apps/files/nc-app-files` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-app`, `nc-font`, `nc-fs-facade`, `nc-gfx`, `nc-media`, `nc-pkg`, `nc-ui` |
| **İsteğe bağlı bağımlılıklar** | `nc-app-settings`, `nc-app-stopwatch`, `nc-app-system` |
| **Kullananlar** | `nc-apps`, `nc-files-r3` |
| **Özellikler (features)** | `builtin-instances`, `default`, `kernel-fs` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Dosyalar uygulaması (id `nc.files`). `nc_app::App` uygular (`FilesApp`).

- Dosya gezinme, ses dosyası çalma (`play_audio_file_blocking`), `.ntzc` paketini diske kalıcılaştırma ve kurma
  (`persist_ntzc_to_data_app`, `create_packaged_app`, `GenericInstalledApp`).
- Dosya sistemine `nc-fs-facade` üzerinden erişir; Ring 3'te cephe `client` özelliğiyle derlenir (`apps/files/nc-files-r3`).
- Diğer uygulamaları isteğe bağlı bağımlılık olarak içerebilir (Ring 0 yedeği); Ring 3 girişi bunları `default-features = false` ile kapatır.
