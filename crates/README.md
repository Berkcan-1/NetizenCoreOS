# crates

Cekirdegin Rust modulleri. Hepsi no_std ve bagimsiz kutulardir; kok Cargo.toml bunlari calisma alani uyesi yapar.

| Klasor              | Katman      | Aciklama                                                      |
|---------------------|-------------|---------------------------------------------------------------|
| nc-hal              | sozlesme    | BootInfo, Platform arayuzu, log! makrosu                      |
| nc-arch             | donanim     | port G/C, seri port, saat                                     |
| nc-math             | yardimci    | tam sayi matematigi, yumusatma egrileri                       |
| nc-mem              | yardimci    | bellek havuzu                                                 |
| nc-assets           | kaynak      | logo PNG hatti                                                |
| nc-gfx              | grafik      | Canvas, bulaniklastirma, karistirma, ekrana aktarim           |
| nc-font             | grafik      | vektor yazi tipi                                              |
| nc-ui               | arayuz      | tema ve ortak cizim yardimcilari                              |
| nc-splash           | ekran       | acilis animasyonu                                             |
| nc-setup            | ekran       | kurulum ekrani                                                |
| nc-platform-qemu    | platform    | QEMU sanal donanimi                                           |
| nc-platform-generic | platform    | genel PC                                                      |

Yeni crate eklerken: klasoru olustur, Cargo.toml yaz, kok Cargo.toml uye listesine ekle, README.md ve src/README.md
yaz, docs/ARCHITECTURE.md'deki bagimlilik grafigini guncelle.
