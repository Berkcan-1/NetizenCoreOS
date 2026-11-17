# nc-audio

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/graphics/nc-audio` |
| **Katman** | Grafik |
| **Bağımlılıklar** | `nc-assets`, `nc-driver`, `nc-hal` |
| **Kullananlar** | `nc-desktop`, `nc-kernel`, `nc-setup` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Sistem genelinde tek ses motoru. `no_std`, kesmesiz, bloklamaz. Uygulamalar, masaüstü, kurulum sihirbazı ve çekirdek ses çıkarmak için yalnızca bu crate'i çağırır; PCI, DMA veya AC'97 yazmaçlarına dokunmaz.

## Genel API

| Çağrı | Ne yapar |
|---|---|
| `init()` | AC'97 denetleyicisini bulur, 48 kHz stereo akışı açar. Ses donanımı yoksa `Err` döner; kalan çağrılar sessizce `false` döner |
| `play(Sound)` | Bir sistem sesi başlatır. Aynı ses 40 ms içinde tekrar istenirse yok sayılır (tıklama yağmuru korunması) |
| `pump()` | Karede bir kez çağrılır: mikseri çalıştırır, halkayı doldurur. Çalan ses yoksa neredeyse bedavadır |
| `busy()` | Çalan ses var mı |
| `set_volume(0..=100)`, `set_muted(bool)` | Yazılım ana ses düzeyi (kuadratik eğri, pürüzsüz geçiş) |
| `stats()` | Çalınan yuva sayısı, kopma (underrun) sayısı, tampon derinliği, açık ses sayısı |
| `stop_all()` | Tüm sesleri keser |

`Sound`: `Startup`, `Shutdown`, `Click`, `Open`, `Close`, `Notify`, `Success`, `Warning`, `Error`, `Lock`, `Unlock`, `VolumeTick`, `Connect`, `Disconnect`.

## Nasıl çalışır

1. Sürücü (`nc-driver::ac97`) 32 yuvalık bir DMA halkası kurar; her yuva 512 kare (≈10,7 ms) stereo 16 bit PCM'dir.
2. Ana döngü her karede `pump()` çağırır. Motor halkada `lead` yuvadan az ses kaldıysa mikserden yeni yuva üretip halkaya yazar. Başlangıç derinliği 12 yuvadır (≈128 ms); kopma olursa 4'er yuva artarak en fazla 24'e (≈256 ms) çıkar.
3. Halka biterse denetleyici durur (döngüye girip çöp çalmaz). Sürücü bunu görünce kanalı sıfırlar ve yeni yuvadan sürer; motor bunu kopma olarak sayar.
4. Çalan ses kalmayınca motor yuva üretmeyi bırakır; boşta CPU maliyeti yoktur.

## Mikser

* En fazla 12 eşzamanlı ses. Havuz doluysa en düşük öncelikli, en eski ses çalınır (örnek: `Error` bir `Click`'in yerini alır, tersi olmaz).
* Kaynaklar: sentezleyici (`synth.rs`) ve IMA-ADPCM akışı (`adpcm.rs`). ADPCM 44,1 kHz kaynağı 48 kHz'e doğrusal ara değerlemeyle çevirir.
* Toplam 0,875 ile ölçeklenir, ana ses düzeyiyle çarpılır ve yumuşak diz sınırlayıcıdan (`limit`) geçer; taşma sert kırpma yerine sıkıştırılır.

## Ses teması (`theme.rs`)

Tüm sesler tam sayı sentezle üretilir, dosya gerekmez. Üç harmonikli "çan", saf sinüs, kısa "tık" ve tek harmonikli "vızıltı" (hata) tınıları; hepsinde 5 ms'lik atak ve karesel sönüm vardır. Yeni ses eklemek için `theme.rs` içine nota dizisi yaz ve `Sound` enum'una ekle.

`Startup` varsa `assets/sound/startup.wav`'ı çalar. Dosya derleme sırasında (`nc-assets/build.rs`) tepe 22000'e normalize edilip IMA-ADPCM'e (4:1) sıkıştırılır; 3 sn'lik 44,1 kHz stereo dosya ≈132 KB tutar. Dosya yoksa sentezli `Success` dizisi çalınır.

## Test

    cargo test -p nc-audio
    NC_AUDIO_DUMP=/tmp/ses cargo test -p nc-audio dump_theme

İkinci komut her sesi `/tmp/ses/*.wav` olarak yazar; çekirdeği açmadan dinleyebilirsin. Çekirdekte tüm temayı sırayla çaldırmak için:

    NC_KERNEL_EXTRA=audio-selftest bash build.sh qemu
