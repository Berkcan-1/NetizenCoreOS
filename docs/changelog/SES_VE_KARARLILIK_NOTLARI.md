# Ses motoru ve kararlılık turu

## Ne değişti
* Yeni crate `crates/graphics/nc-audio`: kesmesiz AC'97 halka akışı, 12 sesli mikser, sentezli ses teması (14 ses), IMA-ADPCM açılış müziği. Ayrıntı: crate README'si.
* `nc-driver/ac97.rs` yeniden yazıldı: bloklamayan 32 yuvalı DMA halkası, kopma sonrası otomatik toparlanma. Eski sürücü 16 bit yazmaçları iki ayrı bayt okumasıyla okuyor, "codec hazır" bitini yanlış yerden (GLOB_STA bit 0) bekliyor ve sınırı 8 milyon turdu; donanım yanıt vermezse açılış dakikalarca donabilirdi. Yeni sürücü `inw/inl` kullanır, hazır bitini (bit 8) ve codec güç durumunu (0x26) kontrol eder, bekleme süreleri sınırlıdır.
* Açılış müziği artık splash başlarken çalar. Eskiden ana masaüstü hazırlandıktan sonra, kuyruğun en sonunda tetikleniyordu; ayrıca QEMU'da ana makinede ffplay başlatan COM2 komutuna bağlıydı. COM2 yolu yalnızca ses donanımı yoksa yedek olarak kalır.
* Ses her yerde: pencere aç/kapat, ses tuşları (ses kısıldıkça tık), kilit açma/yanlış parola, kurulum başarı/hata, güvenli açılış hatası, kapat/yeniden başlat.
* Masaüstündeki ses düzeyi/sessiz durumu artık gerçekten sesi kontrol eder (önceden yalnızca ekranda gösteriliyordu).
* `nc-net`: E1000 halka bellek ayırma hatalarında çekirdek paniği yerine sürücü devre dışı kalır.
* `tools/build`: ISO yolunda da AC'97 aygıtı, `NC_AUDIO_BACKEND=pa|alsa|sdl|wav|none`, VirtualBox'ta AC'97 ses otomatik açılır, yedek ses çaların sırası paplay/aplay/mpv/ffplay, `NC_KERNEL_EXTRA` ile ek kernel özelliği.

## Doğrulama
* `cargo test -p nc-audio`: 10 test (sinüs doğruluğu, sınırlayıcı monotonluğu, ADPCM SNR > 24 dB, açılış süresi, her sesin sınırlı ve sonlanır olması, sessiz/ses 0, ses eğrisi, ses havuzu taşması, öncelik).
* Gerçek çekirdek `build.sh qemu` ile derlenip QEMU'da AC'97 ile açıldı, çıkış WAV'a kaydedildi. 14 sesin tamamı ana makine render'ıyla çapraz korelasyonla eşleşti (0,993–1,000), doğru sırada, kopmasız.
* `NC_KERNEL_EXTRA=audio-selftest bash build.sh qemu`: açılışta tüm temayı çalar ve istatistik loglar.

## Henüz yapılmadı
* Gerçek donanımda (Intel HDA kullanan çoğu yeni PC) AC'97 yoktur; motor sessiz kalır ve PC hoparlörü yedeği çalışır. HDA sürücüsü ayrı iş.
* SDK uygulamalarının ses çıkarması (Ring 3 sistem çağrısı) ve bildirim sistemi sıradaki adım.

---

# Arayüz turu

* Yeni ikon motoru (`nc-desktop/src/art.rs`): arka plansız, yüksek çözünürlüklü, yumuşak gölgeli ikonlar. Dosyalar (klasör), Browser (küre), Ayarlar (dişli), Bu cihaz hakkında (dizüstü), Kronometre, genel uygulama ve belge ikonları. `Icon` enum'una `Folder` ve `Globe` eklendi; manifestte `"folder"`, `"globe"`.
* Duvar kâğıtları: `assets/wallpapers` içindeki ≤6 PNG/JPEG gömülür (otomatik boyut bütçesi). Ayarlar önizlemeleri resmin renklerinden üretilir (Ring 3 bağlam bloğunda 24 bayt). Hareketli duvar kâğıdı için boş `draw_animated_wallpaper` kancası.
* Arayüz ölçeği (%80–150) Ayarlar'da; varsayılan görünüm eskisinden ~%10 küçük. `ContextBlock`'ta `ui_scale` (bayt 37) ve duvar kâğıdı renkleri (bayt 88–111).
* Tema: yeşilimsi krem palet yerine nötr serin palet, mavi vurgu; Aero cam korundu. Başlat düğmesi grafit.
* Düzeltme: koyu temada "Bu cihaz hakkında" yazıları okunmuyordu.
* Test: `apps/registry/nc-apps/tests/ui_render.rs` ana makinede tüm masaüstünü görüntüye döker (`nc-arch` `host-sim` özelliği donanım portlarını no-op yapar).

Henüz yapılmadı: kontrol paneli (wifi/bluetooth/ses/parlaklık kartları), bildirim sistemi, kurulum sihirbazının yeni palete geçirilmesi, uygulama içi (Browser vb.) koyu tema.

---

# Uygulama sesi ve bildirim sistemi

* Yeni sistem çağrısı `SYS_SHELL_CALL` (29) ve yetkiler `audio.play` (bit 15), `notification.post` (bit 16). Paket uygulamaları bu iki yetkiyi manifestten alabilir (`PACKAGE_GRANTABLE` genişledi).
* `nc-audio`: uygulama PCM akışları (8–48 kHz, mono/stereo, ~0,5 sn halka tampon, doğrusal yeniden örnekleme, akış başına ses). Uygulama başına 2, toplam 4 akış. Süreç kapanınca/çökünce akışlar kapanır.
* `nc-app-sdk`: `audio::{play, Stream}` ve `notify::{post, permission}`; `host` özelliği ana makine testleri için.
* Bildirimler: `nc_app::notify` (kuyruk + izin tablosu). Masaüstü ilk bildirimde izin sorar ("İzin ver / İzin verme"), karar `/data/system/notify.cfg` dosyasında kalır. Toast kartları, saate tıklayınca açılan bildirim merkezi (geçmiş, Temizle, uygulama başına izin anahtarı), okunmamış noktası. Reddedilen uygulamanın `post` çağrısı `Denied` döner. 11 dilde metinler.
* Kronometre (Ring 3) örnek kullanım: tuşlarda ses, her tam dakikada bildirim. SDK örneği `sdk/ring3-sample` akış + bildirim kullanır.
* Davranış değişikliği: dock'taki saat/durum alanının sağ kısmı artık bildirim merkezini açar; sol kısım (ağ simgesi) önceki gibi sesi %10 adımlarla döndürür.

Doğrulama: `cargo test -p nc-audio -p nc-app-sdk -p nc-abi` (akış perdesi, kanal ayrımı, kısmi yazma, sahiplik, sınırlar, tel biçimi), `cargo test -p nc-apps --test ui_render` (izin ver/verme akışları gerçek tıklamayla, ekran görüntüleri), çekirdekte `NC_KERNEL_EXTRA=audio-selftest` 16 kontrol (yetkisiz çağrı, sahiplik, akış sınırı, bildirim izin durumları) ve kaydedilen sesin 880 Hz akışı içermesi.

Henüz yapılmadı: Ring 3 uygulamasından gerçek etkileşimle (Kronometre'ye tıklama) QEMU'da uçtan uca deneme (kurulum sihirbazı gerektirdiği için elle denenmedi); sistem çağrısı kopyalama sarmalayıcısı `sys_call` ile aynı kalıpta yazıldı ve derleniyor ama kendi başına test edilmedi.
