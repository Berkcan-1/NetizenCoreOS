# NTZ3 (serbest .ntzc) ve yetki katmanları

## Katmanlar

| Katman | Kimler | Yetki | Nasıl girilir |
|---|---|---|---|
| Sistem | Ayarlar, Dosyalar, Paket Kurucu | tam manifest yetkisi (`nc_caps::SYSTEM_ONLY` dahil) | platform anahtarıyla imza + salt-okunur sistem bölümünden gelme + doğrulanmış imza |
| Kullanıcı | sonradan kurulan her şey | en çok `nc_caps::USER_ALLOWED` | hiçbir şart yok: imzasız NTZ3 de kurulur |

İkisi de her zaman Ring 3'te (aarch64'te EL0) çalışır. Ring 0'da yalnızca çekirdek ve sürücüler vardır.

Kullanıcı katmanı `PACKAGE_INSTALL`, `PACKAGE_REMOVE`, `PROCESS_LAUNCH`, `FS_ALL_*`, `FS_GRANT_*`, `SETTINGS_WRITE_*`, `SYSTEM_AUDIT_READ`
yetkilerini ALAMAZ; manifest isterse fazlası sessizce düşer (`NtzcPackage::denied_caps`) ve kurulum ekranı bunu listeler.
Konum, `/data/app` altına kurulu paketin sistem katmanına çıkmasını da engeller (`InstallSource::UserData` her zaman Kullanıcı).

## Biçim

NTZ2 ile aynı 64 baytlık başlık; sihir `NTZ3`, sürüm 3. Manifestte `target_arch=aarch64-unknown-none` (v8a) ya da `x86_64-unknown-none`.
`sig=` satırı İSTEĞE BAĞLIdır: varsa Ed25519 doğrulanır (yanlışsa `BadSignature`), yoksa paket imzasızdır. Yük SHA-256'sı her
durumda doğrulanır. İptal listesi (`REVOKED_PUBLISHERS_HEX`) imzalı serbest paketlerde de geçerlidir. Aynı `id` için güncelleme,
yalnızca aynı `publisher_key_id` ile (imzalıysa) ya da kullanıcının onayıyla yapılmalıdır (kurucu uygulamada uygulanacak).

Üretim: `python3 sdk/build-ntzc.py manifest.toml app.elf app.ntzc --arch armv8a --free [--seed-file anahtar]`

## Dosya erişim politikası (`aarch64crates/runtime/nc-fs-policy`, testli; çekirdeğe bağlanması sırada)

| Yol | Kullanıcı katmanı | Sistem katmanı |
|---|---|---|
| `/data/appdata/<kendi-kimliği>/` | okuma + yazma (yetki gerekmez) | `FS_ALL_*` |
| `/data/media/` (ortak depolama) | `FS_USER_READ/WRITE/DELETE` | aynı |
| `/data/appdata/<başkası>/`, `/data/app/`, `/system/` | erişemez | `FS_ALL_READ` (yazma `FS_ALL_WRITE`; `/system` her zaman salt-okunur) |

## Durum

Yazıldı ve burada doğrulandı: `sdk/build-ntzc.py` (NTZ3, `--arch`, `--free`; oluşan paketler Python'da ayrıştırılıp hash ve imza
denetlendi).
Yazıldı ama DERLENMEDİ (bu ortamda cargo yok): `nc-caps`, `nc-pkg` (NTZ3 ayrıştırma, `tier`, `granted_caps`, `runnable_free`, testler).

## Sıradaki işler (sırayla)

1. Yazılabilir `/data`: virtio-blk yazma + exFAT `userdata.img` (FAT32 salt-okunur kalır; `nc-fs` yalnızca exFAT ve memfs'te yazar).
2. aarch64 çekirdeğinde süreç kimliği (`app_id`, katman, yetki bitleri) ve `SYS_FILE_*`, `SYS_VFS_CALL`, `SYS_PKG_INSTALL`, yeni
   `SYS_APP_LAUNCH`; yol politikası yukarıdaki tabloya göre.
3. Çok pencereli compositor (şu an tek uygulama yuvası: NetizenHome).
4. Ayarlar ve Dosyalar'ın aarch64 EL0'a derlenmesi (`tools/build/ring3.sh` aarch64 hedefi), Paket Kurucu uygulaması.
5. Sistem paketlerinin platform anahtarıyla imzalanıp `super.img` `/packages` içine konması; `build.sh armv8a android <cihaz>` akışına bağlanması.
