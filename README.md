# NetizenCore

Sifirdan yazilan, Unix/Linux tabanli olmayan isletim sistemi. Bootloader Assembly, cekirdek ve modullerin tamami Rust.
Kodlarda yorum satiri yoktur; aciklamalar her klasordeki README.md dosyalarindadir.

## Su an ne yapiyor

1. BIOS, ISO'dan stage1'i yukler. stage1, stage2 ve cekirdegi bellege okur.
2. stage2 64 bit islemciyi kontrol eder, VBE ile grafik modunu secer, 4 GB sayfalamayi kurar ve long mode'a gecer.
3. Cekirdek ekrani acar, saati kalibre eder ve acilis animasyonunu oynatir: bulanik baslayip netlesen logo ve
   NetizenCore yazisi, altta nefes alan cizgi, PREPARING SETUP yazisi ve yanip sonen noktalar.
4. Hazirlik bitince animasyon bulaniklasarak kaybolur. Sistem kurulu degilse kurulum sihirbazi, kuruluysa masaustu acilir.
5. Masaustu: renkli duvar kagidi, simgeler, gorev cubugu, baslat menusu ve surukleyebildigin pencereler. Uygulamalar
   (Ayarlar, Sistem, Kronometre) ayri crate'lerdir ve ortak bir sozlesme uzerinden calisir (crates/nc-app/README.md).

## Gorunum

Sade, HarmonyOS/EMUI beyazi bir arayuz; Frutiger Aero sadece kucuk vurgularda (azure/aqua cam kureler, gradyanli hap, kartta
hafif parlama). Ayrintilar crates/nc-ui/README.md icinde.

## Profiller

| Profil  | Hedef                | Cozunurluk  | Log cikisi                          |
|---------|----------------------|-------------|-------------------------------------|
| qemu    | QEMU sanal donanimi  | 1280x720    | seri port, debugcon, isa-debug-exit |
| generic | genel PC, VirtualBox | 1024x768    | seri port (varsa)                   |

## Komutlar

    bash build.sh            qemu ISO uretir: build/netizencore-qemu.iso
    bash build.sh generic    generic ISO uretir: build/netizencore-generic.iso
    bash build.sh all        ikisini birden uretir
    bash build.sh run        qemu ISO uretir ve QEMU'da acar
    bash build.sh vbox       generic ISO uretir ve VirtualBox'ta acar
    bash build.sh clean      build klasorunu siler

Gereksinimler (Debian/Ubuntu):

    sudo apt install nasm xorriso binutils qemu-system-x86 zip
    rustup (build.sh x86_64-unknown-none hedefini kendisi ekler)

Ilk derlemede png kutuphanesi internetten indirilir.

## Klasorler

| Klasor        | Icerik                                                              |
|---------------|---------------------------------------------------------------------|
| assets/       | Derleme zamaninda cekirdege gomulen dosyalar (su an sadece logo)    |
| bootloader/   | stage1.asm ve stage2.asm (Assembly)                                 |
| crates/       | Yeniden kullanilabilir Rust modulleri (hal, arch, gfx, font, ...)   |
| docs/         | Mimari, bellek haritasi, modul listesi                              |
| kernel/       | Giris noktasi, platform secimi, acilis akisi                        |

Her klasorde ve her src klasorunde o klasoru ayrintili anlatan bir README.md vardir.

## Hata ayiklama (QEMU)

Seri cikti terminalde gorunur, debugcon cikti build/debugcon.log dosyasina yazilir. Cekirdek panik olursa mesaj
ikisine de yazilir. QEMU, /dev/kvm erisilebiliyorsa otomatik hizlandirilir; KVM yoksa animasyon daha az kare ile
oynar ama sure ayni kalir.

## Yapay zeka araclariyla calisma

Tum projeyi vermek gerekmez. Ilgili modulun klasorunu (README.md + src) ve bagimli oldugu modullerin README.md
dosyalarini ver. Bagimlilik grafigi docs/ARCHITECTURE.md icindedir.
