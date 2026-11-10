#!/usr/bin/env python3
# sdk/gen-signing-key.py
#
# Yayıncı (imzalayıcı) anahtar çifti üretir.
#   * ÖZEL anahtar (seed)  -> sdk/keys/publisher.seed   (asla depoya koyma! .gitignore'da)
#   * AÇIK anahtar (hex)   -> ekrana yazılır; crates/nc-pkg/src/lib.rs içindeki
#                             TRUSTED_PUBLISHERS_HEX listesine eklenir.
#
# Kullanım:  python3 sdk/gen-signing-key.py
import os
import sys
import stat

HERE = os.path.dirname(os.path.abspath(__file__))
KEY_DIR = os.path.join(HERE, "keys")
SEED_PATH = os.path.join(KEY_DIR, "publisher.seed")


def main() -> int:
    try:
        from cryptography.hazmat.primitives.asymmetric import ed25519
        from cryptography.hazmat.primitives import serialization
    except ImportError:
        print("hata: 'cryptography' paketi gerekli: pip install cryptography", file=sys.stderr)
        return 1

    if os.path.exists(SEED_PATH):
        print(f"hata: {SEED_PATH} zaten var; üzerine yazılmadı.", file=sys.stderr)
        print("      Anahtarı gerçekten yenilemek istiyorsan önce dosyayı kendin sil/yedekle.", file=sys.stderr)
        return 1

    os.makedirs(KEY_DIR, exist_ok=True)
    priv = ed25519.Ed25519PrivateKey.generate()
    seed = priv.private_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PrivateFormat.Raw,
        encryption_algorithm=serialization.NoEncryption(),
    )
    pub = priv.public_key().public_bytes_raw()

    fd = os.open(SEED_PATH, os.O_WRONLY | os.O_CREAT | os.O_EXCL, stat.S_IRUSR | stat.S_IWUSR)
    with os.fdopen(fd, "w") as f:
        f.write(seed.hex() + "\n")

    print(f"[+] Özel anahtar yazıldı: {SEED_PATH}  (gizli tut, yedekle, depoya KOYMA)")
    print("[+] Açık anahtar (hex):")
    print(f"    {pub.hex()}")
    print()
    print("crates/nc-pkg/src/lib.rs içinde şu satırı ekle:")
    print("pub static TRUSTED_PUBLISHERS_HEX: &[&str] = &[")
    print(f'    "{pub.hex()}",')
    print("];")
    return 0


if __name__ == "__main__":
    sys.exit(main())
