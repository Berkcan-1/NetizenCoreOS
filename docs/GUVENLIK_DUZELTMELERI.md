# Güvenlik düzeltmeleri (2026-11-05) — durum raporu

> **DERLENMEDİ, ÇALIŞTIRILMADI.** Bu değişiklikler `cargo`'suz bir ortamda yazıldı. Hiçbir Rust
> kodu derlenmedi; Rust testleri koşturulmadı. Yalnızca Python tarafı (SDK imzalayıcı, i18n
> denetimi) ve doğrulama algoritmasının Python modeli gerçekten çalıştırıldı.
> İlk iş: `cargo test -p nc-pkg -p nc-caps -p nc-arch -p nc-net` ve `cargo build`.

## Yapılanlar

| Bulgu | Durum | Özet |
|---|---|---|
| F-01 | Yapıldı (geçici güvenlik ağı) | Yerel (NCBM) kod yolu `unsafe-native-modules` özelliğinin arkasında, **varsayılan kapalı**; açık olsa bile yalnızca imzası doğrulanmış paketler. İmzasız paket listeye girmez, kurulmaz, diske yazılmaz; ret nedeni seri günlüğe yazılır. **Kalıcı çözüm (Ring 3) yapılmadı.** |
| F-02 | Yapıldı | `ed25519-dalek` ile gerçek doğrulama (`verify_strict`). İmza, `sig=` satırından önceki ham baytlar üzerinde; manifest yalnızca imzalı kısımdan okunur; bağlayıcı hash imzalı manifestteki; iptal listesi var. |
| F-02 (ek bulgu) | Yapıldı | `TRUSTED_PUBLISHERS` içindeki anahtar **32 sıfır baytlık seed'in** açık anahtarıydı (özel anahtarı herkes bilir). Liste boşaltıldı; testi var. |
| F-02 (ek bulgu) | Yapıldı | NTZC v1 başlığındaki `uid=1000` ile "Sistem Uygulaması" rozeti taklit edilebiliyordu; v1'de `uid/permissions` artık yok sayılır. Uygulama kimliği sıkı doğrulanır (yol atlatma önlemi). |
| F-03 | Yapıldı | TLS gizli değerleri yalnızca `fill_secure_bytes`; yoksa `Tls13::new` hata döner, HTTPS başlamaz. Entropi havuzu / virtio-rng **eklenmedi**. |
| F-04 | Yapıldı | `nc-arch/src/rtc.rs` (CMOS RTC) + TLS'e bağlandı; saat yoksa doğrulama **başarısız** olur (sessiz atlama yok). |
| F-05 | Yapıldı | `nc-caps` tek kaynak; kernel ve nc-pkg aynı bitleri kullanır, çakışma derleme hatasıdır. Kaba `CAP_*` modelinin `CSpace`'e taşınması yapılmadı. |
| F-10 | Belgelendi | PIN kilidi disk koruması değildir (kod yorumu). |
| F-11 | Kısmen | `nc-pkg` SHA-256'sı artık RustCrypto `sha2`. `nc-crypto`'daki elle yazılmış SHA-256/HMAC/PBKDF2 duruyor (PIN kayıtlarını bozmamak için). |
| F-12 | Yapıldı | TCP ISN önce güvenli entropiyle; yoksa eski best-effort. |

## Yapılmayanlar (bilerek)

F-06 (uygulamaları Ring 3'e taşıma), F-07 (SMAP, KASLR, IOMMU, NX/W^X, secure boot), F-08 (syscall ABI),
F-09 (SMP / `static mut`), F-13 (belge senkronu), F-14 (işlevsel eksikler). Bunlar çok dosyalı mimari
işlerdir ve derleyici/QEMU olmadan güvenle yazılamaz.

## Davranış değişiklikleri (dikkat!)

1. **SDK uygulamaları (Takvim, örnek uygulama) artık çalışmaz** — tamamı imzasız v1 üretiyordu.
   Geliştirme için: `python3 sdk/gen-signing-key.py` → çıkan açık anahtarı
   `crates/nc-pkg/src/lib.rs` içindeki `TRUSTED_PUBLISHERS_HEX` listesine ekle → çekirdeği
   `--features nc-app-files/unsafe-native-modules` ile derle → `bash sdk/build.sh calendar`.
   `host_usb/calendar.zip` aslında eski imzasız bir `.ntzc`; yeniden üretilmeli.
2. `nc.calculator` demo paketi (imzasız) artık uygulama olarak görünmez; Dosyalar'da açılınca
   "imzalanmamış" uyarısı çıkar.
3. **HTTPS**, RDRAND/RDSEED olmayan CPU'da (ör. `qemu -cpu qemu64`) ve RTC okunamadığında hata verir.
   `build.sh` KVM'de `-cpu host` kullanıyor (sorun yok); KVM'siz çalıştırmada `-cpu max` ver.
4. Yerleşik uygulamalar (Dosyalar, Tarayıcı, Ayarlar, Sistem, Kronometre) artık diskteki imzasız
   `*.ntzc` dosyalarına bağlı değil, doğrudan koddan oluşturulur (sıra aynı).
5. `Cargo.lock` güncellenmeli (`ed25519-dalek` eklendi): ağ açıkken `cargo build` yeter.
   `--locked` kullanıyorsan önce `cargo update -p nc-pkg` / `cargo generate-lockfile` çalıştır.
6. Çekirdekteki yetki bit değerleri değişti (nc-pkg düzenine geçti). Bitleri sabit kodlayan
   harici kod yoktu (grep ile kontrol edildi).

## Doğrulama listesi

```
python3 crates/nc-i18n/tools/check_lang.py      # çalıştırıldı: 297 anahtar, 0 sorun
cargo test -p nc-pkg -p nc-caps -p nc-arch -p nc-net
cargo build --release                           # çekirdek
```
QEMU: seri günlükte RTC tarihi; imzasız `.ntzc` → `[pkg] reddedildi`; `-cpu qemu64` → TLS hatası.
Hâlâ yapılmamış olan F-01 doğrulaması: `.bss` gerçekten yürütülebilir miydi (QEMU'da NCBM testi).

## QEMU 2 GB "Faz 1: Kernel code mapping bulunamadi" (2026-11-05, ikinci tur)

**Teşhis (kod okumasından; henüz çalıştırılmadı):** UEFI yolunda çekirdek, OVMF'nin kurduğu sayfa
tablosuyla çalışıyor. `translate_with_flags` 1 GiB yaprak sayfaları (PDPTE.PS=1) tanımıyordu. RAM 512 MB
iken OVMF'nin tüm ayırmaları ilk GiB'ın içinde kaldığından ilk GiB parçalanıyor (2 MiB/4 KiB), 2 GB'de
ayırmalar ikinci GiB'a düştüğü için ilk GiB tek bir 1 GiB sayfa kalıyor ve `0x100000` çevrilemiyordu.
**Düzeltme:** `translate_with_flags` 1 GiB sayfaları çözüyor (+ ana makine testi), ayrıca başarısızlıkta
`[Faz 1 tani] CR3=.. PML4E=.. PDPTE=.. PDE=.. PTE=..` satırı seri porta yazılıyor.

**Bilinen açık risk (düzeltilmedi):** `nc-uefi`, `EfiBootServicesData` bölgelerini çekirdeğe "kullanılabilir"
olarak veriyor, oysa çekirdek OVMF'nin sayfa tablolarıyla çalışmaya devam ediyor; OVMF'nin tabloları bu
bölgelerde olabilir ve PMM/heap bunları ezebilir. Kalıcı çözüm: çekirdeğin kendi sayfa tablolarını kurması
(Ring 3 / NX / SMAP için de ön koşul).

## Güncelleme (Ring 3, Adım 1a)

"Bilinen açık risk" (OVMF tabloları / `EfiBootServicesData`) için çözüm yazıldı: çekirdeğin kendi sayfa
tabloları, `kernel-paging` özelliğiyle (varsayılan KAPALI). Ayrıntı, test prosedürü ve sınırlar:
`docs/RING3_YOL_HARITASI.md`. **Derlenmedi/çalıştırılmadı**; yalnızca algoritmanın Python modeli koştu.

## Güncelleme: F-01 KAPANDI (zorunlu Ring 3)

Üçüncü taraf kodu Ring 0'da çalıştıran NCBM yolu **tamamen silindi** (`EXEC_SLOTS`, `NativeModuleApp`, host API,
`unsafe-native-modules` özelliği). Yüklenen uygulamalar yalnızca şu koşulların HEPSİ sağlanırsa çalışır:
Ed25519 imzası doğrulanmış + manifestte `abi=ring3-v1` + yük, W^X/aralık kurallarını geçen statik bir ELF
(`NtzcPackage::runnable()`). Süreç Ring 3'te, kendi CR3'ünde, watchdog'lu ve en fazla 4 yetkiyle
(`SETTINGS_READ`, `FS_USER_READ/WRITE`, `NETWORK_CLIENT`) çalışır. Ayrıntı: `docs/RING3_YOL_HARITASI.md` bölüm 9.
