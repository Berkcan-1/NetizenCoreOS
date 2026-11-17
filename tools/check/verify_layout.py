#!/usr/bin/env python3
import argparse, os, re, sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    sys.exit("Python 3.11+ gerekli (tomllib).")

LAYERS = ["foundation", "hardware", "services", "graphics", "ui", "runtime"]
SKIP_TOP = {"build", "target", ".git"}
PATH_TOKEN = re.compile(r"(?<![\w./$<{*\-])((?:aarch64crates|aarch64kernel|crates|kernel|apps|tools|sdk|bootloader|docs|assets)/[A-Za-z0-9_./\-]+)")
INCLUDE_RE = re.compile(r'include_(?:bytes|str)!\(\s*"([^"]+)"\s*\)')
BUILD_REL_RE = re.compile(r'((?:\.\./)+[A-Za-z0-9_./\-]*)')


def is_legacy(parts):
    return any(x == "legacy" or x.startswith("legacy-") for x in parts)


def load_manifests(root):
    out = {}
    for p in sorted(root.rglob("Cargo.toml")):
        rel = p.relative_to(root)
        if rel.parts[0] in SKIP_TOP:
            continue
        try:
            out[p.parent] = tomllib.loads(p.read_text(encoding="utf-8"))
        except Exception as e:
            print("HATA  %s ayrıştırılamadı: %s" % (rel, e))
            out[p.parent] = None
    return out


def layer_of(root, d):
    parts = d.relative_to(root).parts
    if parts[0] in ("crates", "aarch64crates") and len(parts) >= 2 and parts[1] in LAYERS:
        return LAYERS.index(parts[1])
    if parts[0] == "aarch64crates" and len(parts) >= 2 and parts[1] == "drivers":
        return LAYERS.index("hardware")
    if parts[0] == "aarch64crates" and len(parts) >= 2 and parts[1] == "apps":
        return len(LAYERS)
    if parts[0] == "aarch64kernel":
        return len(LAYERS) + 1
    if parts[0] == "bootloader":
        return -1
    if parts[0] == "apps":
        return len(LAYERS)
    if parts[0] == "kernel":
        return len(LAYERS) + 1
    return None


def iter_deps(manifest):
    for sec in ("dependencies", "dev-dependencies", "build-dependencies"):
        for name, spec in (manifest.get(sec) or {}).items():
            yield sec, name, spec


def main():
    ap = argparse.ArgumentParser(description="NetizenCore klasör düzeni ve yol bütünlüğü denetimi")
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    ap.add_argument("--no-layers", action="store_true", help="katman kurallarını denetleme (eski düzen için)")
    ap.add_argument("--strict-docs", action="store_true", help="doküman yol uyarılarını hata say")
    args = ap.parse_args()
    root = Path(args.root).resolve()
    errors, warnings = [], []

    manifests = load_manifests(root)
    pkg_dir = {}
    for d, m in manifests.items():
        if m and "package" in m:
            pkg_dir[m["package"]["name"]] = d

    ws_root = manifests.get(root)
    members = set()
    if ws_root and "workspace" in ws_root:
        for m in ws_root["workspace"].get("members", []):
            md = (root / m).resolve()
            if not (md / "Cargo.toml").exists():
                errors.append("workspace üyesi yok: %s" % m)
            members.add(md)

    implicit = set()
    n_deps = 0
    for d, m in manifests.items():
        if not m:
            continue
        for sec, name, spec in iter_deps(m):
            if not isinstance(spec, dict) or "path" not in spec:
                continue
            n_deps += 1
            target = (d / spec["path"]).resolve()
            tm = manifests.get(target)
            where = "%s -> %s" % (d.relative_to(root), spec["path"])
            if tm is None or "package" not in (tm or {}):
                errors.append("path bağımlılığı çözülmüyor: %s" % where)
                continue
            real = tm["package"]["name"]
            expect = spec.get("package", name)
            if real != expect:
                errors.append("paket adı uyuşmuyor (%s != %s): %s" % (real, expect, where))
            if (d in members or d == root) and root in target.parents:
                implicit.add(target)
            if not args.no_layers:
                a, b = layer_of(root, d), layer_of(root, target)
                if a is not None and b is not None and b > a and not (a == -1):
                    errors.append("katman ihlali (üst katmana bağımlılık): %s [%s]" % (where, sec))
                if a == -1 and b is not None and b > 0:
                    errors.append("bootloader yalnızca foundation'a bağlanabilir: %s" % where)

    standalone = []
    for d, m in manifests.items():
        if not m or "package" not in m or d == root:
            continue
        if is_legacy(d.relative_to(root).parts):
            continue
        if "workspace" in m:
            standalone.append(d)
            continue
        if d not in members and d not in implicit:
            errors.append("workspace dışı paket (üye değil, bağımlılık değil): %s" % d.relative_to(root))
    for d in sorted(members):
        m = manifests.get(d)
        if m and "workspace" in m:
            errors.append("workspace üyesi kendi [workspace] bölümünü taşıyor: %s" % d.relative_to(root))

    n_inc = 0
    for p in sorted(root.rglob("*.rs")):
        rel = p.relative_to(root)
        if rel.parts[0] in SKIP_TOP or is_legacy(rel.parts):
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        for m in INCLUDE_RE.finditer(text):
            n_inc += 1
            target = (p.parent / m.group(1)).resolve()
            if not target.exists():
                errors.append("include yolu yok: %s -> %s" % (rel, m.group(1)))
        for m in re.finditer(r'#\[path\s*=\s*"([^"]+)"\]', text):
            n_inc += 1
            target = (p.parent / m.group(1)).resolve()
            if not target.exists():
                errors.append("#[path] hedefi yok: %s -> %s" % (rel, m.group(1)))

    for p in sorted(root.rglob("build.rs")):
        rel = p.relative_to(root)
        if rel.parts[0] in SKIP_TOP:
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r'"([^"\n]*(?:\.\./)+[^"\n]*)"', text):
            lit = m.group(1)
            sub = BUILD_REL_RE.search(lit)
            if not sub:
                continue
            target = (p.parent / sub.group(1)).resolve()
            if not target.exists():
                warnings.append("build.rs yolu yok (yedek davranışı olabilir): %s -> %s" % (rel, sub.group(1)))

    for d in sorted(manifests):
        m = manifests[d]
        if not m:
            continue
        for b in m.get("bin", []) or []:
            if "path" in b and not (d / b["path"]).exists():
                errors.append("bin yolu yok: %s -> %s" % (d.relative_to(root), b["path"]))
        lib = m.get("lib") or {}
        if "path" in lib and not (d / lib["path"]).exists():
            errors.append("lib yolu yok: %s -> %s" % (d.relative_to(root), lib["path"]))

    scan_ext = {".sh", ".py", ".md"}
    for p in sorted(root.rglob("*")):
        if not p.is_file() or p.suffix not in scan_ext:
            continue
        rel = p.relative_to(root)
        if rel.parts[0] in SKIP_TOP or "changelog" in rel.parts or "migration" in rel.parts or is_legacy(rel.parts):
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        seen = set()
        for m in PATH_TOKEN.finditer(text):
            tok = m.group(1).rstrip(".,;:)'\"`")
            if tok in seen or tok.endswith("/") and (root / tok).is_dir():
                continue
            seen.add(tok)
            if "*" in tok or tok.count("/") == 0 or tok.endswith("-"):
                continue
            if tok.startswith("build/") or tok.startswith("sdk/keys/") or "/stock/" in tok or "<" in tok:
                continue
            if (root / tok).exists():
                continue
            if any((anc / tok).exists() for anc in p.parents if root in anc.parents or anc == root):
                continue
            bucket = errors if (p.suffix == ".sh" or args.strict_docs) else warnings
            bucket.append("yol bulunamadı: %s içinde '%s'" % (rel, tok))

    print("manifest: %d | path bağımlılığı: %d | include/#[path]: %d | bağımsız workspace: %d" % (
        len([1 for m in manifests.values() if m]), n_deps, n_inc, len(standalone)))
    for w in warnings:
        print("UYARI", w)
    for e in errors:
        print("HATA ", e)
    print("sonuç: %d hata, %d uyarı" % (len(errors), len(warnings)))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
