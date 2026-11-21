# assets

Derleme zamaninda cekirdege gomulen kaynak dosyalari. Calisma zamaninda dosya sistemi yoktur; bu klasordeki
dosyalar crates/graphics/nc-assets tarafindan okunur, donusturulur ve cekirdek ikilisinin icine konur.

Alt klasorler

    logo/    acilis ekraninda gorunen PNG logo

Yeni bir kaynak turu eklemek icin: once crates/graphics/nc-assets/build.rs icine okuma ve donusturme kodunu ekle, sonra
crates/graphics/nc-assets/src/lib.rs icinde bir erisim fonksiyonu yaz. Kaynaklar cekirdegin boyutuna eklenir; cekirdek
sinirini build.sh icindeki KERNEL_LIMIT belirler (589824 bayt).
