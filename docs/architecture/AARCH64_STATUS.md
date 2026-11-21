# AArch64 durum notu (bağlam kaybı olursa buradan devam edin)

## Tamamlananlar
- Açılış: Image başlığı, EL2→EL1, DTB (sınır denetimli, `/reserved-memory`, bellek ayırma haritası, `timer` frekansı).
- Bellek: 16 GiB kimlik eşleme, çekirdek 4 KiB W^X, WXN, XN veri, heap (`nc-mem::LockedHeap`), arena (`mark`/`release_to`), kare havuzu.
- Kesme: GICv3 (CPU0), sanal zamanlayıcı 100 Hz, `wfi` ile boşta uyku, `CNTFRQ_EL0=0` için DTB yedeği.
- Ring 3 (EL0): adres uzayı, `svc` sistem çağrıları, görev zamanlayıcı, hata yalıtımı, kare sızıntı denetimi (öz-sınama), süreç kimliği (`app_id`, `Tier`, yetki bitleri).
- Sağlamlaştırma: yığın kanaryası, ayrı acil istisna yığını, panik/istisna yeniden giriş koruması, bayat önbellek temizliği.
- Depolama:
  - `blk.rs` (virtio-blk legacy v1 + modern v2, okuma/yazma + `REQ_FLUSH`).
  - `lp.rs` (Android LP / `super.img` okuyucu) + `storage.rs` (FAT32 salt-okunur `/system` bölümü ve exFAT yazılabilir `/data` bölümü `userdata.img`).
  - `/system/packages/*.ntzc` ve `/data/app/*.ntzc` paket doğrulama ve kayıt tablosu (`nc-pkg`).
  - `SYS_VFS_CALL` (`OP_LIST`, `OP_STAT`, `OP_READ_RANGE`, `OP_WRITE`, `OP_MKDIR`, `OP_REMOVE`, `nc-fs-policy` yol ve yetki denetimli) ve `SYS_PKG_INSTALL`.
- Android usulü uygulama yaşam döngüsü ve Ring 3 Birleştirici (`nc.home`, `host.rs`, `task.rs`):
  - `OP_INIT`, `OP_UPDATE`, `OP_DRAW`, `OP_PAUSE`, `OP_RESUME`, `OP_CLOSE` olayları, `RET_ANIMATING` (50 Hz akıcı animasyon zamanlaması), `OP_EMBED_SURFACE` (alt uygulama yüzeylerini `nc.home` içinde birleştirme) ve `SYS_APP_LAUNCH` (`0` = başlat/devam ettir, `1` = durdur/RAM boşalt, `2..5` = Ring 3 pencere yöneticisi eylemleri).
  - Tüm pencere birleştirme (compositor), üst durum çubuğu, animasyonlu Kontrol Merkezi, animasyonlu 3 düğmeli alt gezinme çubuğu (`Geri`, `Ana Ekran`, `Son Uygulamalar`) ve ana uygulama yönetimi çekirdekten çıkarılıp tamamen Ring 3'teki `NetizenHome` (`aarch64crates/apps/home/nc-home-r3`) ve paylaşılan `nc_app_sdk::ui` altyapısına taşındı.
  - Ön plan uygulaması dışındaki süreçler arka planda (`Paused`) kendi EL0 adres uzayında korunur; **Son Uygulamalar** ekranında canlı önizlemeleri gösterilir ve yukarı kaydırılarak (`swipe-up`) bellekten tamamen kaldırılabilir.
  - Uygulama çökerse veya çıkarsa çekirdek paniklemez, adres uzayını temizleyip `NetizenHome`'a döner.
- EL0 Uygulamaları (`aarch64-unknown-none`, `packages/mobile/*.ntzc` olarak yalnızca `super.img` `/packages` ve `/data/app` üzerinden yüklenir; çekirdekte gömülü uygulama yedeği yoktur):
  - `NetizenHome` (`aarch64crates/apps/home/nc-home-r3`): Ring 3 birleştirici (compositor), ana ekran widget sistemi (Saat/Hava/Bağlantı ve Depolama/Görev widget'ları), basılı tutarak (`long-press`) serbest 4×3 ızgara yerleşimi, 4'lü uygulama klasörleme (2×2 mini önizlemeli klasör simgesi ve klasör içi pencere), animasyonlu Kontrol Merkezi (Wi-Fi, Bluetooth, Parlaklık kaydırıcısı), animasyonlu alt gezinme çubuğu ve Son Uygulamalar ekranı.
  - `Dosyalar` (`aarch64crates/apps/files/nc-files-v8a`, `nc.files.ntzc`): `/data` ve `/system` gezinme, klasör/not oluşturma, metin düzenleme ve kaydetme, dosya silme ve `.ntzc` paket kurma.
  - `Ayarlar` (`aarch64crates/apps/settings/nc-settings-v8a`, `nc.settings.ntzc`): Kontrol Merkezi ile çift yönlü bağlı **Bağlantı** (Wi-Fi / `virtio-net` MAC ve güç durumu, Bluetooth), **Görünüm** (Canlı ekran parlaklığı %20–%100, duvar kağıdı ve vurgu rengi), **Depolama** (Kullanım çubuğu, saat dilimi ve dil) ve **Sistem** (Cihaz özeti ve arka plan uygulama yönetimi) sekmeleri (`/data/appdata/nc.settings/settings.cfg` üzerinde kalıcı).

## Bilinen sınırlar / yapılacaklar
- Girdi: yalnızca QEMU virtio-input (yoklamalı, kesme yok, imleç yok, ABD QWERTY). Cihaz dokunmatiği (I2C/SPI) yok.
- Tarayıcı (`nc.browser`), Kronometre (`nc.stopwatch`) ve masaüstü (`nc-desktop`) aarch64 EL0'a taşınmadı; ağ (`SYS_NET_CALL`) ve ses (`SYS_SHELL_CALL`) hizmetleri aarch64'te henüz yok.
- Yalnızca CPU0 (SMP yok); GICv2 yok (GICv3 yoksa kesmesiz tarama moduna düşer, öz-sınama atlanır).
- Kullanıcı yığını sabit 16 sayfa (isteğe bağlı büyütme yok); kullanıcı heap'i/`brk` sistem çağrısı yok.
- MMU: 16 GiB üstü RAM/MMIO eşlenmez; ince taneli eşleme yalnızca çekirdek imajında.
- Cihaz GIC adresleri (`gic_dist`, `gic_redist`) x6826c ve rpi4 profillerinde 0 (bilinmiyor) → bu cihazlarda kesmesiz mod.
