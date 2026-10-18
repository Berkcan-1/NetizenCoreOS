#!/usr/bin/env python3
import os, sys, struct, shutil, subprocess, http.server, socketserver

ROOT = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(ROOT)
DIST_DIR = os.path.join(PROJECT_ROOT, "build", "ntzc-packages")

def pick_objcopy():
    for tool in ["llvm-objcopy", "rust-objcopy", "objcopy"]:
        if shutil.which(tool):
            return tool
    raise RuntimeError("objcopy bulunamadi!")

def parse_manifest(path):
    meta = {
        "id": "nc.user.app",
        "name": "Harici Uygulama",
        "version": "1.0.0",
        "icon": "generic",
        "width_units": "32",
        "height_units": "22",
        "uid": "1000",
        "perms": "1",
    }
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            meta[k.strip()] = v.strip().strip('"').strip("'")
    return meta

def build_app(app_dir):
    app_dir = os.path.abspath(app_dir)
    manifest_path = os.path.join(app_dir, "manifest.toml")
    cargo_toml = os.path.join(app_dir, "Cargo.toml")
    if not os.path.exists(manifest_path) or not os.path.exists(cargo_toml):
        raise RuntimeError(f"{app_dir} icinde manifest.toml ve Cargo.toml olmali!")

    meta = parse_manifest(manifest_path)
    linker_script = os.path.join(ROOT, "app.ld")
    target_dir = os.path.join(PROJECT_ROOT, "build", "sdk-target")

    env = os.environ.copy()
    env["RUSTFLAGS"] = f"-C relocation-model=pic -C link-arg=-T{linker_script}"

    print(f"[*] Derleniyor: {meta['name']} ({meta['id']})...")
    subprocess.run([
        "cargo", "build", "--release",
        "--manifest-path", cargo_toml,
        "--target", "x86_64-unknown-none",
        "--target-dir", target_dir
    ], env=env, check=True)

    # Cargo paket adini bul
    pkg_name = os.path.basename(app_dir)
    with open(cargo_toml, "r", encoding="utf-8") as f:
        for line in f:
            if line.strip().startswith("name") and "=" in line:
                pkg_name = line.split("=", 1)[1].strip().strip('"').strip("'")
                break

    elf_path = os.path.join(target_dir, "x86_64-unknown-none", "release", pkg_name)
    os.makedirs(DIST_DIR, exist_ok=True)
    raw_bin = os.path.join(DIST_DIR, f"{pkg_name}.raw")

    subprocess.run([pick_objcopy(), "-O", "binary", elf_path, raw_bin], check=True)
    payload = open(raw_bin, "rb").read()
    os.remove(raw_bin)

    # NTZC Konteyner Formatini Olustur (nc-storage make_ntzc_pkg ile birebir uyumlu)
    manifest_str = (
        f"id={meta['id']}\n"
        f"name={meta['name']}\n"
        f"version={meta['version']}\n"
        f"icon={meta['icon']}\n"
        f"width_units={meta['width_units']}\n"
        f"height_units={meta['height_units']}\n"
    )
    manifest_bytes = manifest_str.encode("utf-8")
    m_len = len(manifest_bytes)
    payload_off = 28 + m_len
    p_len = len(payload)

    header = struct.pack(
        "<4sIIIIII",
        b"NTZC",
        1,                      # Format surumu
        int(meta["uid"]),
        int(meta["perms"]),
        m_len,
        payload_off,
        p_len
    )

    out_ntzc = os.path.join(DIST_DIR, f"{meta['id'].split('.')[-1]}.ntzc")
    with open(out_ntzc, "wb") as f:
        f.write(header)
        f.write(manifest_bytes)
        f.write(payload)

    print(f"[+] Paket hazir: {out_ntzc} ({28 + m_len + p_len} bayt)")
    return out_ntzc

def serve_packages(port=8080):
    os.makedirs(DIST_DIR, exist_ok=True)
    os.chdir(DIST_DIR)
    handler = http.server.SimpleHTTPRequestHandler
    with socketserver.TCPServer(("0.0.0.0", port), handler) as httpd:
        print(f"[*] NetizenCore .ntzc Indirme Sunucusu aktif: http://10.0.2.2:{port}/ (QEMU icinden)")
        httpd.serve_forever()

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] == "build":
        target = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "sample-app")
        build_app(target)
    elif sys.argv[1] == "serve":
        build_app(os.path.join(ROOT, "sample-app"))
        serve_packages()
    else:
        print("Kullanim: python3 sdk/build-ntzc.py [build <klasor> | serve]")
        