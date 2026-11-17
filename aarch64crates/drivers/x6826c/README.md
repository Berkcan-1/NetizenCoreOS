# x6826c — Infinix Hot 20 (MT6768, Helio G85)

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/drivers/x6826c` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-device` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

**Durum:** iskelet. Aşağıdaki değerler MT67xx ailesinden tahmindir; cihazda doğrulanmadan çalışacağı varsayılmamalıdır.
Doğrulanmamış alanlar: UART0 ve MSDC0 adresleri, ekran boyutu/piksel sırası, boot.img adresleri, `super_size`, PSCI kanalı.

## Dolduracaklar ve nereden bulunur (root gerekmez, yalnızca adb shell)

| Değer | Komut / kaynak |
|---|---|
| Ekran boyutu | `adb shell wm size`; stride için genişlik x 4 (RGBX) varsayılır |
| super bölümü boyutu | `adb shell cat /proc/partitions` (blok sayısı x 1024 bayt) → `device.conf` `super_size` |
| boot.img başlığı (sürüm, adresler, cmdline) | stok boot.img'i `python3 aarch64crates/tools/android/unpack_bootimg.py stock/boot.img` ile aç, çıkan değerleri `android_*` alanlarına yaz |
| dtb | stok boot.img'den çıkar → `aarch64crates/drivers/x6826c/stock/dtb` |
| UART/MSDC/GIC/I2C adresleri | `x6826c-main.txt` (aygıt ağacı) içindeki `uart0`, `mmc0`, `gic`, `i2c*` düğümleri |
| Dokunmatik IC | `adb shell cat /proc/bus/input/devices` ve aygıt ağacındaki `touch`/`tp` düğümü |

## Bilinen risk

- Önyükleyici kilitliyse imzasız boot.img açılmaz; `fastboot flash` için kilidin açık olması gerekir. Kilit açma verileri sıfırlar.
- Yanlış `super` boyutu veya yanlış boot.img başlığı cihazı açılmaz yapabilir (fastboot/BROM ile kurtarılır). Önce `fastboot boot boot.img`
  ile **kalıcı yazmadan** dene.
- Ekran ve dokunmatik sürücüleri yazılana kadar çıktı yalnızca UART'a gider; UART'a erişim için donanım bağlantısı gerekir.
  Bu yüzden ilk gerçek hedef: LK'nın bıraktığı framebuffer'a doğrudan çizerek "çalışıyor" görüntüsü.
