# nc-setup

NetizenCore kurulum sihirbazi. `no_std` Rust ile cizilen arayuz; dil/bolge, sistem mimarisi, hedef disk, kurulum ilerlemesi ve sonuc adimlarini sunar.

## Bagimliliklar

- `nc-hal`
- `nc-gfx`
- `nc-font`
- `nc-ui`
- `nc-driver`

## Genel API

- `SetupState::new(&[BlockDeviceInfo])`
- `update(mouse, key, width, height) -> SetupAction`
- `draw(canvas, platform, state, mouse) -> bool`

## Arayuz

- Koyu lacivert/cam yuzeyler; mavi ve yesil vurgu renkleri.
- Saydam yuzey katmanlari, yumusak ambient glow, hover vurgulari ve derinlik golgeleri.
- Ust adim gostergesi, sol sistem ozeti, sag icerik paneli ve sabit alt eylem cubugu.
- 640x480 ve uzeri yaygin ekran boyutlarinda dis cerceve ve icerik panelleri ekran sinirlarinda kalacak sekilde olceklenir.
- Dil, saat dilimi, klavye duzeni, mimari profili, disk secimi, donanim raporu ve kurulum sonucu akislari korunur.

## Klavye

- `Enter` / `Space`: ilerle
- `Escape`: onceki adim
- `Up` / `Down`: ilgili listede gezin

Kurulumun gercek disk yazma ve dogrulama islemi cekirdek tarafinda yurutulur; bu crate arayuzu ve kullanici etkilesimini yonetir.
