# nc-apps

Cekirdege dahil edilen yerlesik uygulamalarin listesi. Kernel sadece bu crate'e baglanir; hangi uygulamalarin
gomuldugunu burasi belirler.

## Genel API

- builtin() -> Vec<Box<dyn App>>: Ayarlar, Sistem, Kronometre (bu sirayla). Sira, simge ve baslat menusu sirasidir.

## Uygulama eklemek / cikarmak

lib.rs icindeki apps.push satirini ekle veya sil, Cargo.toml'daki bagimliligi buna gore guncelle. Baska hicbir yeri
degistirmek gerekmez (nc-desktop uygulamalari adiyla bilmez).

Calisma aninda uygulama eklemek icin (ileride paket yoneticisi) DesktopState::install_app kullanilir.
