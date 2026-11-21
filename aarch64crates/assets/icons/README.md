# Custom Mobile Application Icons (`aarch64crates/assets/icons/`)

Drop PNG icon artwork into this directory using the exact filenames below.
Until a custom PNG is provided, `nc-mobile-assets` falls back safely to the
generic NetizenCore icon (`ICON_GENERIC = 6`) without failing the build.

| Icon ID (`nc_abi::draw`) | Manifest `icon` key | Expected PNG filename | Application |
|---|---|---|---|
| `1` (`ICON_FILES`) | `files` | `ic_files.png` | Files (`nc.files`) |
| `2` (`ICON_SETTINGS`) | `settings` | `ic_settings.png` | Settings (`nc.settings`) |
| `3` (`ICON_BROWSER`) | `browser` | `ic_browser.png` | Browser (`nc.browser`) |
| `4` (`ICON_SYSTEM`) | `system` | `ic_system.png` | System (`nc.system`) |
| `5` (`ICON_STOPWATCH`) | `stopwatch` | `ic_stopwatch.png` | Stopwatch (`nc.stopwatch`) |
| `6` (`ICON_GENERIC`) | `generic` | `ic_generic.png` | Generic / fallback |
| `7` (`ICON_KEYBOARD`) | `keyboard` | `ic_keyboard.png` | Keyboard (`nc.keyboard`) |
| `8` (`ICON_GALLERY`) | `gallery` | `ic_gallery.png` | Gallery (`nc.gallery`) |
| `9` (`ICON_MESSAGES`) | `messages` | `ic_messages.png` | Messages (`nc.messages`) |
| `10` (`ICON_PHONE`) | `phone` | `ic_phone.png` | Phone (`nc.phone`) |
| `11` (`ICON_CONTACTS`) | `contacts` | `ic_contacts.png` | Contacts (`nc.contacts`) |
| `12` (`ICON_CAMERA`) | `camera` | `ic_camera.png` | Camera (`nc.camera`) |

Existing icon IDs `1..=7` are part of the Ring 3 drawing ABI contract and must never be renumbered.
