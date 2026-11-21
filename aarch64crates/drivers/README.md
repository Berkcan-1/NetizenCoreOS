# drivers

Cihaz sürücü katmanı. Her alt klasör **tek bir cihazı** (örn. `x6826c`, `rpi4`, `qemu-virt`, `samsung-a54`) anlatır.
`bash build.sh <mimari> <etiket> <cihaz>` komutundaki cihaz adı bu klasör adıdır; o cihazın sürücüleri çekirdeğe dahil edilir.

    bash build.sh armv8a android x6826c     boot.img + super.img (x6826c sürücüleriyle)
    bash build.sh armv8a qemu               qemu-virt cihazıyla QEMU'da dene
    bash build.sh devices                   tanımlı cihazları listeler

## Bir cihaz klasörünün içeriği

| Dosya | Görev |
|---|---|
| `device.conf` | Derleme üst verisi: mimari, SoC, çekirdek yükleme adresi, boot.img parametreleri, super bölümü boyutu, QEMU ayarı |
| `Cargo.toml` | `nc-dev-<cihaz>` crate'i (`nc-device` ve `nc-hal`'a bağlanır) |
| `src/lib.rs` | `pub const PROFILE: nc_device::DeviceProfile` ve isteğe bağlı `register` işlevi |
| `src/*.rs` | Cihaza özel sürücüler (dokunmatik, eMMC, ekran, güç...) |
| `README.md` | Cihazda doğrulanması gereken değerler, adb ile nasıl bulunacakları |
| `stock/` | (isteğe bağlı, git'e girmez) stok `boot.img`, `dtb`, `dtbo` |

Yeni cihaz eklemek: `cp -r aarch64crates/drivers/_template aarch64crates/drivers/<ad>`, içindeki `Cargo.toml.in` dosyasını `Cargo.toml` yapıp `__ID__`,
`__CRATE__` yer tutucularını doldur, kök `Cargo.toml` üyelerine `aarch64crates/drivers/<ad>` ekle (ya da `python3 aarch64crates/tools/select_device.py --add <ad>`
bunu yapar), `device.conf` değerlerini gir.

Kural: cihaza özel adres/IRQ/ayar yalnızca burada durur. Çekirdek ve `crates/` cihaz adı bilmez; yalnızca `nc-device` sözleşmesini görür.
