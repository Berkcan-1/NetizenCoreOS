#!/usr/bin/env python3
import argparse, re, shlex, shutil, sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    sys.exit("Python 3.11+ gerekli (tomllib).")

ROOT = Path(__file__).resolve().parents[2]
DRIVERS = ROOT / "aarch64crates" / "drivers"
SELECT = ROOT / "aarch64crates" / "hardware" / "nc-device-select"
ARCHS = ("x86_64", "arm64", "armv8a", "armv9a")


def device_dirs():
    return sorted(p for p in DRIVERS.iterdir() if p.is_dir() and not p.name.startswith("_") and (p / "device.conf").exists())


def load_conf(device_id):
    path = DRIVERS / device_id / "device.conf"
    if not path.exists():
        sys.exit("hata: cihaz yok: aarch64crates/drivers/%s/device.conf" % device_id)
    conf = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        conf[key.strip()] = value.strip()
    conf.setdefault("id", device_id)
    if conf["id"] != device_id:
        sys.exit("hata: %s: id (%s) klasor adiyla ayni olmali" % (path, conf["id"]))
    if conf.get("arch") not in ARCHS:
        sys.exit("hata: %s: arch su degerlerden biri olmali: %s" % (path, ", ".join(ARCHS)))
    return conf


def crate_name(device_id):
    manifest = DRIVERS / device_id / "Cargo.toml"
    if not manifest.exists():
        return None
    return tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["name"]


def cmd_list():
    for path in device_dirs():
        conf = load_conf(path.name)
        print("%-16s %-8s %-10s %-8s %s" % (conf["id"], conf["arch"], conf.get("soc", "-"), conf.get("ui", "-"), conf.get("name", "")))


def cmd_env(device_id):
    conf = load_conf(device_id)
    for key, value in conf.items():
        print("DEV_%s=%s" % (re.sub(r"[^A-Za-z0-9_]", "_", key).upper(), shlex.quote(value)))
    print("DEV_CRATE=%s" % shlex.quote(crate_name(device_id) or ""))


def cmd_select(device_id):
    load_conf(device_id)
    name = crate_name(device_id)
    if not name:
        sys.exit("hata: aarch64crates/drivers/%s/Cargo.toml yok" % device_id)
    ident = name.replace("-", "_")
    manifest = (
        '[package]\nname = "nc-device-select"\nversion = "0.1.0"\nedition = "2021"\n\n'
        '[dependencies]\nnc-device = { path = "../nc-device" }\n'
        '%s = { path = "../../drivers/%s" }\n' % (name, device_id)
    )
    lib = '#![no_std]\n\npub use %s::PROFILE;\n\npub const SELECTED_ID: &str = "%s";\n' % (ident, device_id)
    (SELECT / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (SELECT / "src" / "lib.rs").write_text(lib, encoding="utf-8")
    print("cihaz secildi: %s (%s)" % (device_id, name))


def add_member(device_id):
    path = ROOT / "Cargo.toml"
    text = path.read_text(encoding="utf-8")
    entry = '"aarch64crates/drivers/%s"' % device_id
    if entry in text:
        return
    match = re.search(r"(members = \[.*?)(\n\])", text, re.S)
    if not match:
        sys.exit("hata: kok Cargo.toml uyeler listesi bulunamadi")
    text = text[: match.end(1)] + "\n    %s," % entry + text[match.start(2):]
    path.write_text(text, encoding="utf-8")


def cmd_new(device_id, soc, name, arch):
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", device_id):
        sys.exit("hata: cihaz adi kucuk harf, rakam ve tire icerebilir")
    target = DRIVERS / device_id
    if target.exists():
        sys.exit("hata: aarch64crates/drivers/%s zaten var" % device_id)
    template = DRIVERS / "_template"
    (target / "src").mkdir(parents=True)
    crate = "nc-dev-" + device_id
    subs = {"__ID__": device_id, "__CRATE__": crate, "__NAME__": name or device_id, "__SOC__": soc or "unknown"}

    def fill(text):
        for key, value in subs.items():
            text = text.replace(key, value)
        return text

    (target / "device.conf").write_text(fill((template / "device.conf").read_text(encoding="utf-8")).replace("arch=armv8a", "arch=" + arch), encoding="utf-8")
    (target / "Cargo.toml").write_text(fill((template / "Cargo.toml.in").read_text(encoding="utf-8")), encoding="utf-8")
    (target / "src" / "lib.rs").write_text(fill((template / "src" / "lib.rs.in").read_text(encoding="utf-8")), encoding="utf-8")
    (target / "README.md").write_text("# %s\n\nDurum: bos iskelet. `device.conf` ve `src/lib.rs` degerleri cihazdan dogrulanarak doldurulmalidir.\n" % device_id, encoding="utf-8")
    add_member(device_id)
    print("olusturuldu: aarch64crates/drivers/%s (crate %s); kok Cargo.toml uyelerine eklendi" % (device_id, crate))


def main():
    ap = argparse.ArgumentParser(description="NetizenCore cihaz katmani araci")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--env", metavar="CIHAZ")
    ap.add_argument("--select", metavar="CIHAZ")
    ap.add_argument("--new", metavar="CIHAZ")
    ap.add_argument("--add", metavar="CIHAZ")
    ap.add_argument("--soc", default="")
    ap.add_argument("--name", default="")
    ap.add_argument("--arch", default="armv8a", choices=ARCHS)
    args = ap.parse_args()
    if args.list:
        cmd_list()
    elif args.env:
        cmd_env(args.env)
    elif args.select:
        cmd_select(args.select)
    elif args.new:
        cmd_new(args.new, args.soc, args.name, args.arch)
    elif args.add:
        add_member(args.add)
    else:
        ap.print_help()


if __name__ == "__main__":
    main()
