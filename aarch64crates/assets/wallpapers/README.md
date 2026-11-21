# ARMv8-A Mobil Duvar Kağıtları

ARMv8-A telefon arayüzünün gerçek PNG duvar kağıtlarını bu klasöre yerleştirin:

- `wallpaper_01.png` (varsayılan)
- `wallpaper_02.png`
- `wallpaper_03.png`
- `wallpaper_04.png`
- `wallpaper_05.png`

`bash build.sh armv8a ...` sırasında bu dosyalar otomatik olarak `super.img` içindeki `/assets/wallpapers/` yoluna paketlenir ve çekirdek tarafından doğrudan PNG olarak, en-boy oranı korunarak (`FitMode::Fill`) ekrana ölçeklenir.
