# nc-desktop

NetizenCore masaustu kabugu. Duvar kagidi, masaustu simge izgarasi, pencere yoneticisi, dock, baslat menusu, sag tik menusu, gorev yoneticisi ve masaustu diyaloglari bu crate icinde calisir. `no_std` + `alloc`, tum cizim tamsayi matematigi ile yapilir.

Genel API (kernel tarafi): `DesktopState::new`, `install_app`, `update`, `take_dirty`, `take_wallpaper_dirty`, `wallpaper_index`, `app_count`, `sync_desktop_fs`, `draw_wallpaper`, `draw`, `DesktopAction`.

Tasarim dili: Frutiger Aero cam. Her yuzey arkasindaki goruntuyu bulaniklastirir, ustune beyaz bir cam tonu, ust yansima ve ince beyaz kenar cizgisi koyar. Uygulama simgeleri parlak, yuvarlak kose renkli cam karolardir (uygulama kimliginden sabit renk secilir). Yazilar cam ustunde okunsun diye acik zeminde koyu, havadar zeminde golgeli beyazdir.

Yerlesim `unit_for(width, height)` ile cozumunurluge gore olceklenir. Uygulama sayisi dock'a sigmadiginda orta alan sayfali karusele donusur; oklarla dongusel gezinme ve yeni uygulamaya son sayfadan otomatik gecis desteklenir. Sayfa hareketi easing tabanli yatay animasyonla cizilir.

Belgeler: `src/README.md` modul ve davranislari, `UI_DESIGN_STANDARD.md` ise olcek, dock, renk, hareket ve test standartlarini tanimlar.
