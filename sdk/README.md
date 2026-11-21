# sdk

Üçüncü taraf Ring 3 uygulamaları için araçlar.

| Yol | Görev |
|---|---|
| `build.sh` | Uygulamayı derler ve imzalı NTZ2 paketi üretir: `sdk/build.sh <uygulama-dizini> [--seed-file YOL]`; çıktı `build/apps/<kimlik>.ntzc` |
| `build-ntzc.py` | Manifest + ELF'ten imzalı `.ntzc` paketi üretir (biçim `nc-pkg` ile birebir aynı olmalı) |
| `gen-signing-key.py` | Yayıncı anahtar çifti üretir; özel anahtar `sdk/keys/publisher.seed` (git'e girmez), açık anahtar `crates/runtime/nc-pkg/src/lib.rs` içindeki `TRUSTED_PUBLISHERS_HEX`'e eklenir |
| `ring3-sample/` | Şablon uygulama (`Cargo.toml`, `build.rs`, `manifest.toml`, `src/main.rs`); `nc-abi` ve `nc-app-sdk` kullanır |
| `legacy-ncbm/` | Eski `.ncbm` paket aracı; derleme dışı |

Kullanım: `sdk/ring3-sample` klasörünü kopyala, `manifest.toml` ve `Cargo.toml`'u düzenle, `bash sdk/build.sh <klasör>` çalıştır.
Derleme `tools/build/ring3-cargo.sh` ile yapılır. Yerleşik uygulamalar bu klasörün konusu değildir (`apps/`).

## Uygulama sesi ve bildirimler

İki yetki gerekir; `manifest.toml` içinde `capabilities = audio.play, notification.post` olarak bildirilir.

```rust
use nc_app_sdk::audio::{self, Sound, Stream};
use nc_app_sdk::notify::{self, Level, NotifyError};

// Sistem ses temasından kısa bir ses (Click, Open, Close, Notify, Success, Warning, Error, Tick, Connect, Disconnect)
let _ = audio::play(Sound::Success);

// Kendi PCM'in: s16le, 8000–48000 Hz, mono ya da stereo. write() bloklamaz; tampon (~0,5 sn) doluysa
// az kare kabul eder, kalanını sonraki olayda yeniden gönder. status().free_frames kadar yazmak yeterli.
let stream = Stream::open(22_050, 1)?;
let frames_taken = stream.write(&pcm)?;
stream.close(false)?; // false: kalan sesi çalmayı bitir, true: at

// Bildirim: izin kullanıcıdadır. İlk bildirimde masaüstü "izin ver / verme" sorar.
match notify::post("Başlık", "Gövde", Level::Info) {
    Err(NotifyError::Denied) => { /* kullanıcı reddetti: bir daha deneme */ }
    _ => {}
}
```

* Ses her zaman ana ses düzeyine (Ayarlar, ses tuşları, sessiz) tabidir. Açılış/kapanış/kilit sesleri uygulamalara kapalıdır.
* Uygulama başına en çok 2, toplamda 4 akış. Uygulama kapanınca ya da çökünce akışları çekirdek kapatır.
* Başlık ≤ 64, gövde ≤ 256 bayt. Bekleyen bildirim sayısı uygulama başına 6 ile sınırlıdır; fazlası `Limit` döner.
* Ana makinede test için `nc-app-sdk` özelliği `host` sistem çağrılarını kapatır (çağrılar hata döner, uygulama bunları yok saymalıdır).
* Ayrıntı: `crates/foundation/nc-abi/src/shell.rs`, örnek: `ring3-sample/`.


## v8a (ARMv8-A / AArch64) ve serbest paket (NTZ3)

Üçüncü taraf bir ARMv8-A Ring 3 uygulaması derleyip `.ntzc` olarak paketlemek için:

```bash
RUSTFLAGS="-C relocation-model=static -C panic=abort" \
  cargo build --release --manifest-path sdk/ring3-sample/Cargo.toml \
  --target aarch64-unknown-none --target-dir build/cargo-sdk-aarch64

python3 sdk/build-ntzc.py sdk/ring3-sample/manifest.toml \
  build/cargo-sdk-aarch64/aarch64-unknown-none/release/nc-ring3-sample \
  build/nc.user.sayac.ntzc --arch armv8a --free
```

`--free` imza anahtarı olmadan da NTZ3 paketi üretir; kurulan uygulama kullanıcı katmanında, Ring 3 (EL0) içinde çalışır ve yetkileri
`nc_caps::USER_ALLOWED` ile sınırlıdır. Çekirdek statik uygulama listesini düzenlemeden `nc.package-installer` (Dosyalar veya harici USB `/storage/usb0` üzerinden) ile kurulabilir ve anında NetizenHome uygulama çekmecesinde görünür.

## Public Ecosystem SDK (`nc-app-sdk`)

`nc-app-sdk` tüm Ring 3 uygulamaları için `#![no_std]` sistem arayüzleri sunar:

| Modül | Görev | Gerekli Yetki |
|---|---|---|
| `nc_app_sdk::settings` | `device_info`, `self_info`, `set_u32`, `set_str`, `sync_data` | `settings.read` / `settings.write.*` |
| `nc_app_sdk::apps` | `query_registry`, `launch`, `stop`, `uninstall`, `clear_data`, `send_intent`, `take_intent` | `process.launch` / `package.remove` |
| `nc_app_sdk::pkg` | `inspect`, `install`, `target_arch`, `abi_version` | `package.install` / `fs.all.read` |
| `nc_app_sdk::security` | `lock_status`, `authenticate`, `lock_now`, `configure_set`, `configure_disable` | `security.manage` |
| `nc_app_sdk::storage` | `usb_info`, `usb_rescan`, `usb_eject` | `fs.user.read` / `fs.all.read` |

## Manifest ve Simge Sistemi

`manifest.toml` içinde `icon` alanı:
- Kayıtlı simgeler: `files`, `settings`, `browser`, `system`, `stopwatch`
- Genel simge veya bilinmeyen/eksik simge (`icon = "generic"` ya da alan yok): otomatik olarak ortak NetizenCore genel simgesi (`ICON_GENERIC`) kullanılır; hiçbir zaman boş döşeme oluşmaz.

Yetkiler (`capabilities`): `settings.read`, `fs.user.read`, `fs.user.write`, `fs.user.delete`, `network.client`, `audio.play`, `notification.post`. Ayrıcalıklı sistem yetkileri (`package.install`, `package.remove`, `security.manage`, `fs.all.*`, `process.launch`) yalnızca güvenilir sistem paketlerine verilir; kullanıcı paketlerinden çekirdek politikasıyla otomatik düşürülür.

Ayrıntı: `docs/architecture/NTZC_V3_VE_YETKI.md`.
