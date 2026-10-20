#!/usr/bin/env python3
# sdk/build-ntzc.py
import os, sys, struct, hashlib, subprocess, shutil

# Test için varsayılan geliştirici imza özel anahtarı (32 bayt seed)
# Üretim anahtarları asla bu betiğe veya repoya gömülmemelidir!
DEV_PRIVATE_SEED = bytes.fromhex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")

def sign_ed25519(msg: bytes, seed: bytes) -> bytes:
    try:
        from cryptography.hazmat.primitives.asymmetric import ed25519
        priv = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
        return priv.sign(msg)
    except ImportError:
        # Harici kütüphane yoksa Pure-Python Ed25519 RFC 8032 fallback
        import urllib.request
        raise RuntimeError("Python 'cryptography' paketi gerekli: pip install cryptography")

def get_pubkey_hex(seed: bytes) -> str:
    from cryptography.hazmat.primitives.asymmetric import ed25519
    priv = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
    return priv.public_key().public_bytes_raw().hex()

def create_ntzc_v2(manifest_path, payload_path, out_file):
    payload = open(payload_path, "rb").read()
    payload_hash = hashlib.sha256(payload).digest()
    payload_hash_hex = payload_hash.hex()

    # 1. Manifest ayrıştır ve kanonik hale getir
    meta = {}
    with open(manifest_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            meta[k.strip()] = v.strip().strip('"').strip("'")

    pubkey_hex = get_pubkey_hex(DEV_PRIVATE_SEED)

    # Deterministik manifest gövdesi
    canonical_manifest = (
        f"id={meta.get('id', 'nc.user.app')}\n"
        f"version={meta.get('version', '1.0.0')}\n"
        f"version_code={meta.get('version_code', '1')}\n"
        f"target_arch=x86_64-unknown-none\n"
        f"publisher_key_id=ed25519:{pubkey_hex}\n"
        f"payload_hash={payload_hash_hex}\n"
        f"capabilities={meta.get('capabilities', 'filesystem.user.read')}\n"
    )

    # 2. Manifest'i imzala
    sig = sign_ed25519(canonical_manifest.encode("utf-8"), DEV_PRIVATE_SEED)
    full_manifest = canonical_manifest + f"sig={sig.hex()}\n"
    manifest_bytes = full_manifest.encode("utf-8")

    m_len = len(manifest_bytes)
    p_len = len(payload)
    m_off = 64
    p_off = m_off + m_len

    # 3. 64 baytlık NTZ2 Başlığı
    header = struct.pack(
        "<4sIIIIII32s",
        b"NTZ2",           # Magic
        64,                 # Header Size
        2,                  # Version
        0,                  # Reserved
        m_off,              # Manifest Offset
        m_len,              # Manifest Length
        p_off,              # Payload Offset
        p_len,              # Payload Length
        payload_hash        # SHA-256
    )

    with open(out_file, "wb") as f:
        f.write(header)
        f.write(manifest_bytes)
        f.write(payload)

    print(f"[+] NTZC v2 paketi oluşturuldu: {out_file} (Boyut: {len(header) + m_len + p_len} B)")
    print(f"    Payload SHA-256: {payload_hash_hex}")
    print(f"    Yayıncı Key: ed25519:{pubkey_hex[:16]}...")