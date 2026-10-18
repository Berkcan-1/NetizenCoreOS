# nc-desktop

NetizenCore masaustu kabugu. Duvar kagidi, masaustu simge izgarasi, pencere yoneticisi, dock, baslat menusu, sag tik menusu, gorev yoneticisi ve masaustu diyaloglari bu crate icinde calisir. `no_std` + `alloc`, tum cizim tamsayi matematigi ile yapilir.

Genel API (kernel tarafi): `DesktopState::new`, `install_app`, `update`, `take_dirty`, `take_wallpaper_dirty`, `wallpaper_index`, `app_count`, `sync_desktop_fs`, `draw_wallpaper`, `draw`, `DesktopAction`.

Tasarim dili: Frutiger Aero cam. Her yuzey arkasindaki goruntuyu bulaniklastirir, ustune beyaz bir cam tonu, ust yansima ve ince beyaz kenar cizgisi koyar. Uygulama simgeleri parlak, yuvarlak kose renkli cam karolardir (uygulama kimliginden sabit renk secilir). Yazilar cam ustunde okunsun diye acik zeminde koyu, havadar zeminde golgeli beyazdir.

Dosyalar: `src/README.md` her modulun gorevini anlatir.
