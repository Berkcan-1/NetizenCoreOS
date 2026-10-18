# nc-desktop/src

Moduller

- `lib.rs`: Durum makinesi. Pencere yasam dongusu (acilis, kucultme, kapatma, buyutme), hedef bulma (`Target`), tiklama, surukleme, klavye, animasyon zamanlayicilari ve `draw` sirasi burada.
- `motion.rs`: Animasyon matematigi. Ilerleme degerleri 0..4096 (Q12) tamsayidir. Easing: out-cubic, out-quart, in-out-cubic, out-back. `approach` gecen sureye (ms) gore ilerler, kare hizindan bagimsizdir.
- `paint.rs`: Cizim ilkelleri. Kenar yumusatmali yuvarlak dikdortgen (dolgu, gradyan, cizgi), yumusak golge, kutu bulanikligi (iki gecis, yuvarlak kose onarimi), `glass_surface` cam yuzey, yumusak kure, cizgi ve halka.
- `theme.rs`: Renkler, karo renk paleti, cam on ayarlari (dock, baslat, menu, diyalog, pencere).
- `layout.rs`: Butun geometri. `unit_for(width, height)` ekran oranina gore tek olcu birimi verir; her boyut bu birimin katidir, bu yuzden her cozunurluk ve dikey ekranda uyum saglar.
- `icons.rs`: Parlak cam karo, klasor ve belge simgeleri, guc/yeniden baslat/wifi simgeleri, baslat logosu.
- `window.rs`: Cam pencere cercevesi. Baslik seridi bulaniktir, kenar ve baslik cam, icerik alani opak.
- `wallpaper.rs`: Capraz gradyan, buyuk yumusak kureler, duvar kagidi isiklari ve titresim giderici dither. Canli duvar kagidi icin suzulen kureler.
- `shell.rs`: Masaustu simgeleri, dock, baslat menusu, sag tik menusu, gorev yoneticisi ve diyalog cizimi.

Masaustu izgarasi

`grid_for` hucre boyutunu birimden alir ve ekrana sigan sutun ve satir sayisini bulur. Simgeler hucre (sutun, satir) olarak saklanir. Yeri secilmemis simgeler bos hucreleri sutun sutun doldurur. Surukleme sirasinda en yakin bos hucre hayalet olarak gosterilir, birakinca simge hucreye yumusakca oturur. `Simgeleri Duzenle` menu ogesi tum konumlari sifirlar.

Dock

Ortada yuzen cam bar: baslat dugmesi, uygulamalar, durum alani (ag cubuklari ve calisma suresi). Imlec yaklastikca simgeler macOS tarzi cosinus egrisiyle buyur, bar genisler. Acilan uygulamanin simgesi ziplar, calisan uygulamanin altinda nokta durur, odaktaki uzun ve renklidir. Sigmayan uygulama sayisinda simgeler kuculur. Otomatik gizleme acikken dock asagi kayar ve alt kenarda ince bir ipucu cizgisi kalir.

Animasyonlar

Pencere acilisi kucuk bir olcek ve yukselme ile solma, icerik gec solar. Kucultme dock simgesine dogru kuculur, geri getirme dock simgesinden buyur. Kapatma kuculup solar. Buyutme ile eski boyut arasi yumusak gecer. Baslat menusu dock'tan buyur, simgeler sirayla belirir. Sag tik menusu ve diyaloglar imlec noktasindan olceklenir. Sureler `launch_anim_speed` ayarina baglidir.

Cam kalitesi

`glass_blur` ayari bulaniklik yaricapini ve gecis sayisini belirler (0 kapali). Yaricap birimle birlikte olceklenir.
