#!/usr/bin/env python3
# sdk/build-ntzc.py  --  imzalı NTZ2 (Ring 3) paketi üretir.
#
# Kullanım:
#   python3 sdk/build-ntzc.py <manifest.toml> <uygulama.elf> <cikti.ntzc> [--seed-file YOL]
#
# Yük, nc-abi Ring 3 sözleşmesine uygun STATİK bir ELF64 olmalıdır (bkz. sdk/build.sh); çekirdek de aynı
# kuralları yükleme sırasında yeniden doğrular. Paket manifestine `abi=ring3-v1` yazılır; bu olmadan
# paket çalıştırılamaz.
#
# İmzalama anahtarı (ÖZEL seed, 64 hex karakter) sırasıyla şuradan okunur:
#   1) --seed-file YOL
#   2) NTZC_SIGNING_SEED_FILE ortam değişkeni
#   3) sdk/keys/publisher.seed   (python3 sdk/gen-signing-key.py ile üretilir)
# Anahtar yoksa paket ÜRETİLMEZ. Betikte gömülü/varsayılan bir anahtar YOKTUR.
#
# Biçim (çekirdekteki nc-pkg ile birebir aynı olmalı):
#   64 bayt başlık: "NTZ2", u32 başlık_uzunluğu(64), u32 sürüm(2), u32 ayrılmış,
#                   u32 manifest_ofs, u32 manifest_len, u32 payload_ofs, u32 payload_len,
#                   [32] SHA-256(payload)
#   Manifest: "anahtar=değer" satırları + SON satır "sig=<128 hex>".
#   İmza, "sig=" satırından ÖNCEKİ ham baytlar üzerindedir (normalizasyon yok).
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_SEED_PATH = os.path.join(HERE, "keys", "publisher.seed")

HEADER_FMT = "<4sIIIIIII32s"
assert struct.calcsize(HEADER_FMT) == 64, "NTZ2 başlığı 64 bayt olmalı"


def load_seed(seed_file):
    path = seed_file or os.environ.get("NTZC_SIGNING_SEED_FILE") or DEFAULT_SEED_PATH
    if not os.path.isfile(path):
        sys.exit(
            f"hata: imzalama anahtarı bulunamadı: {path}\n"
            "      Önce: python3 sdk/gen-signing-key.py"
        )
    text = open(path, "r", encoding="ascii").read().strip()
    try:
        seed = bytes.fromhex(text)
    except ValueError:
        sys.exit(f"hata: {path} geçerli hex değil")
    if len(seed) != 32:
        sys.exit(f"hata: {path} 32 bayt (64 hex) seed içermeli")
    return seed


def read_meta(manifest_path):
    meta = {}
    with open(manifest_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            meta[k.strip()] = v.strip().strip('"').strip("'")
    return meta


USER_IMAGE_MIN = 0x80_0100_0000
USER_IMAGE_MAX = 0x80_0800_0000


def check_ring3_elf(elf):
    """nc-elf politikasının (kaba) ön denetimi: hatayı paketlemeden ÖNCE yakalar."""
    def fail(msg):
        sys.exit(f"hata: yük geçerli bir Ring 3 ELF'i değil: {msg}")
    if len(elf) < 64 or elf[:4] != b"\x7fELF" or elf[4] != 2 or elf[5] != 1:
        fail("ELF64 little-endian değil")
    etype, machine = struct.unpack_from("<HH", elf, 16)
    if etype != 2 or machine != 62:
        fail("ET_EXEC / x86-64 değil (statik, konuma bağımlı bağlayın)")
    entry, phoff = struct.unpack_from("<QQ", elf, 24)
    phentsize, phnum = struct.unpack_from("<HH", elf, 54)
    if phentsize != 56 or not 0 < phnum <= 16 or phoff + phnum * 56 > len(elf):
        fail("program başlıkları geçersiz")
    loads = 0
    for i in range(phnum):
        p_type, p_flags, p_off, p_vaddr, _pa, p_filesz, p_memsz, _al = struct.unpack_from("<IIQQQQQQ", elf, phoff + i * 56)
        if p_type == 1 and p_memsz:
            loads += 1
            if p_flags & 3 == 3:
                fail("segment hem yazılabilir hem yürütülebilir (W^X)")
            if p_vaddr < USER_IMAGE_MIN or p_vaddr + p_memsz > USER_IMAGE_MAX:
                fail(f"segment 0x{p_vaddr:x} izinli aralığın (0x{USER_IMAGE_MIN:x}..0x{USER_IMAGE_MAX:x}) dışında; crates/nc-app-sdk/app.ld kullanın")
        elif p_type in (2, 3, 7):
            fail("INTERP/DYNAMIC/TLS desteklenmiyor")
    if loads == 0:
        fail("PT_LOAD yok")


def create_ntzc_v2(manifest_path, payload_path, out_file, seed_file=None):
    from cryptography.hazmat.primitives.asymmetric import ed25519

    seed = load_seed(seed_file)
    priv = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
    pubkey_hex = priv.public_key().public_bytes_raw().hex()

    payload = open(payload_path, "rb").read()
    check_ring3_elf(payload)
    payload_hash = hashlib.sha256(payload).digest()
    meta = read_meta(manifest_path)

    for key in ("id", "name"):
        if not meta.get(key):
            sys.exit(f"hata: manifest.toml içinde '{key}' zorunlu")

    # En az ayrıcalık: manifest yetki bildirmezse HİÇ yetki istenmez.
    canonical_manifest = (
        f"id={meta['id']}\n"
        f"name={meta['name']}\n"
        f"version={meta.get('version', '1.0.0')}\n"
        f"version_code={meta.get('version_code', '1')}\n"
        f"icon={meta.get('icon', 'generic')}\n"
        f"abi=ring3-v1\n"
        f"width_units={meta.get('width_units', '34')}\n"
        f"height_units={meta.get('height_units', '22')}\n"
        f"target_arch=x86_64-unknown-none\n"
        f"publisher_key_id=ed25519:{pubkey_hex}\n"
        f"payload_hash={payload_hash.hex()}\n"
        f"capabilities={meta.get('capabilities', '')}\n"
    )
    sig = priv.sign(canonical_manifest.encode("utf-8"))
    manifest_bytes = (canonical_manifest + f"sig={sig.hex()}\n").encode("utf-8")

    m_off = 64
    p_off = m_off + len(manifest_bytes)
    header = struct.pack(
        HEADER_FMT, b"NTZ2", 64, 2, 0, m_off, len(manifest_bytes), p_off, len(payload), payload_hash
    )

    with open(out_file, "wb") as f:
        f.write(header)
        f.write(manifest_bytes)
        f.write(payload)

    total = len(header) + len(manifest_bytes) + len(payload)
    print(f"[+] NTZ2 paketi oluşturuldu: {out_file} ({total} B)")
    print(f"    Payload SHA-256 : {payload_hash.hex()}")
    print(f"    Yayıncı anahtarı: ed25519:{pubkey_hex[:16]}...")


def main(argv):
    args = list(argv[1:])
    seed_file = None
    if "--seed-file" in args:
        i = args.index("--seed-file")
        try:
            seed_file = args[i + 1]
        except IndexError:
            sys.exit("hata: --seed-file bir yol bekliyor")
        del args[i : i + 2]
    if len(args) != 3:
        sys.exit("kullanım: build-ntzc.py <manifest.toml> <uygulama.elf> <cikti.ntzc> [--seed-file YOL]")
    create_ntzc_v2(args[0], args[1], args[2], seed_file)


if __name__ == "__main__":
    main(sys.argv)
