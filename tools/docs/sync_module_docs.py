#!/usr/bin/env python3
import argparse, os, re, sys
from pathlib import Path
import tomllib

LAYERS = ["foundation", "hardware", "services", "graphics", "ui", "runtime"]
LAYER_TITLE = {
    "foundation": "Temel", "hardware": "Donanım", "services": "Çekirdek hizmetleri",
    "graphics": "Grafik", "ui": "Arayüz", "runtime": "Ring 3 çalışma zamanı",
    "apps": "Uygulamalar", "kernel": "Çekirdek", "bootloader": "Önyükleyici",
}
BEGIN = "<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->"
END = "<!-- otomatik:bitti -->"
SKIP = {"build", "target", ".git"}


def layer_of(rel):
    p = rel.parts
    if p[0] == "crates":
        return p[1]
    if p[0] == "aarch64kernel":
        return "kernel"
    if p[0] == "aarch64crates" and len(p) >= 2:
        if p[1] in LAYERS:
            return p[1]
        if p[1] == "drivers":
            return "hardware"
        if p[1] == "apps":
            return "apps"
    return p[0]


def load(root):
    summaries = tomllib.loads((root / "tools/docs/modules.toml").read_text(encoding="utf-8"))
    pk = {}
    for f in sorted(root.rglob("Cargo.toml")):
        rel = f.parent.relative_to(root)
        if rel.parts and rel.parts[0] in SKIP or any(x == "legacy" or x.startswith("legacy-") for x in rel.parts):
            continue
        m = tomllib.loads(f.read_text(encoding="utf-8"))
        if "package" not in m or "host-tests" in rel.parts or "ring3-sample" in rel.parts:
            continue
        name = m["package"]["name"]
        deps = {}
        for sec, tag in (("dependencies", ""), ("build-dependencies", ""), ("dev-dependencies", "dev")):
            for dn, spec in (m.get(sec) or {}).items():
                if isinstance(spec, dict) and "path" in spec:
                    pkgname = spec.get("package", dn)
                    flag = tag or ("opt" if spec.get("optional") else "")
                    if pkgname not in deps or not flag:
                        deps[pkgname] = flag
        pk[name] = {
            "dir": rel, "layer": layer_of(rel) if rel.parts else "root", "deps": deps,
            "features": sorted((m.get("features") or {}).keys()),
            "summary": summaries.get(name, {}).get("summary", "(özet yok: tools/docs/modules.toml)"),
        }
    users = {n: {} for n in pk}
    for n, d in pk.items():
        for dep, flag in d["deps"].items():
            if dep in users:
                users[dep][n] = flag
    return pk, users


def fmt(names):
    return ", ".join("`%s`" % n for n in names) if names else "—"


def split(dm):
    normal = sorted(k for k, v in dm.items() if v == "")
    opt = sorted(k for k, v in dm.items() if v == "opt")
    dev = sorted(k for k, v in dm.items() if v == "dev")
    return normal, opt, dev


def crate_block(name, pk, users):
    d = pk[name]
    n, o, dv = split(d["deps"])
    un, uo, ud = split(users[name])
    lines = [BEGIN,
             "| | |", "|---|---|",
             "| **Konum** | `%s` |" % d["dir"].as_posix(),
             "| **Katman** | %s |" % LAYER_TITLE.get(d["layer"], d["layer"]),
             "| **Bağımlılıklar** | %s |" % fmt(n)]
    if o: lines.append("| **İsteğe bağlı bağımlılıklar** | %s |" % fmt(o))
    if dv: lines.append("| **Yalnızca test bağımlılıkları** | %s |" % fmt(dv))
    lines.append("| **Kullananlar** | %s |" % fmt(un + uo))
    if ud: lines.append("| **Testlerde kullananlar** | %s |" % fmt(ud))
    if d["features"]: lines.append("| **Özellikler (features)** | %s |" % fmt(d["features"]))
    lines.append("")
    ctx = "Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü"
    if n + o:
        ctx += " + bağımlılıklarının README dosyaları"
    ctx += ". Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez."
    lines += [ctx, END]
    return "\n".join(lines)


def upsert(path, block, title_hint):
    if path.exists():
        text = path.read_text(encoding="utf-8")
    else:
        text = "# %s\n\n" % title_hint
    if BEGIN in text and END in text:
        pre = text.split(BEGIN, 1)[0]
        post = text.split(END, 1)[1]
        new = pre + block + post
    else:
        m = re.search(r"^# .*\n", text, re.M)
        if m:
            i = m.end()
            new = text[:i] + "\n" + block + "\n" + text[i:]
        else:
            new = "# %s\n\n%s\n\n%s" % (title_hint, block, text)
    new = re.sub(r"\n{3,}", "\n\n", new)
    return new


def group_block(layer, pk, users):
    rows = ["| Crate | Ne yapar | Bağımlılıkları | Kullananlar |", "|---|---|---|---|"]
    for n in sorted(pk):
        d = pk[n]
        if d["layer"] != layer or d["dir"].parts[0] != "crates": continue
        a, b, _ = split(d["deps"])
        ua, ub, _ = split(users[n])
        rows.append("| [`%s`](%s/README.md) | %s | %s | %s |" % (
            n, os.path.relpath(d["dir"], "crates/" + layer).replace(os.sep, "/"), d["summary"], ", ".join(a + [x + "*" for x in b]) or "—", ", ".join(ua + [x + "*" for x in ub]) or "—"))
    return "\n".join([BEGIN] + rows + ["", "`*` = isteğe bağlı (feature ile açılan) bağımlılık.", END])


def aarch64_block(pk, users):
    rows = ["| Crate | Konum | Ne yapar | Bağımlılıkları | Kullananlar |", "|---|---|---|---|---|"]
    for n in sorted(pk):
        d = pk[n]
        if d["dir"].parts[0] not in ("aarch64crates", "aarch64kernel"): continue
        a, b, _ = split(d["deps"])
        ua, ub, _ = split(users[n])
        rows.append("| [`%s`](%s/README.md) | `%s` | %s | %s | %s |" % (
            n, os.path.relpath(d["dir"], "aarch64crates").replace(os.sep, "/"), d["dir"].as_posix(), d["summary"],
            ", ".join(a + [x + "*" for x in b]) or "—", ", ".join(ua + [x + "*" for x in ub]) or "—"))
    return "\n".join([BEGIN] + rows + ["", "`*` = isteğe bağlı (feature ile açılan) bağımlılık.", END])


def dependencies_md(pk, users):
    out = ["# Bağımlılık haritası", "",
           "> Bu dosya `python3 tools/docs/sync_module_docs.py` ile üretilir; elle düzenlemeyin. "
           "Özetler `tools/docs/modules.toml` içindedir.", "",
           "Kural: bir crate yalnızca **kendi katmanındaki veya altındaki** crate'lere bağlanır. "
           "`python3 tools/check/verify_layout.py` bunu denetler.", "",
           "```", "foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel", "```", ""]
    for layer in LAYERS + ["apps", "bootloader", "kernel"]:
        names = sorted(n for n in pk if pk[n]["layer"] == layer)
        if not names: continue
        out += ["## %s (`%s`)" % (LAYER_TITLE[layer], layer if layer in ("apps", "kernel", "bootloader") else "crates/" + layer), "",
                "| Crate | Konum | Bağımlılık | Kullanan |", "|---|---|---|---|"]
        for n in names:
            d = pk[n]
            a, b, dv = split(d["deps"])
            ua, ub, ud = split(users[n])
            dep = ", ".join(a + [x + "*" for x in b]) or "—"
            use = ", ".join(ua + [x + "*" for x in ub]) or "—"
            out.append("| `%s` | `%s` | %s | %s |" % (n, d["dir"].as_posix(), dep, use))
        out.append("")
    out += ["`*` = isteğe bağlı (feature ile açılan) bağımlılık. Yalnızca test bağımlılıkları bu tabloya yazılmaz.", ""]
    return "\n".join(out)


def modules_md(pk):
    out = ["# Modüller", "",
           "> Bu dosya `python3 tools/docs/sync_module_docs.py` ile üretilir; elle düzenlemeyin. Özetler `tools/docs/modules.toml` içindedir.", ""]
    for layer in LAYERS + ["apps", "bootloader", "kernel"]:
        names = sorted(n for n in pk if pk[n]["layer"] == layer)
        if not names: continue
        out += ["## %s" % LAYER_TITLE[layer], "", "| Modül | Konum | Görev |", "|---|---|---|"]
        for n in names:
            out.append("| `%s` | `%s` | %s |" % (n, pk[n]["dir"].as_posix(), pk[n]["summary"]))
        out.append("")
    out.append("Her modülün klasöründe ayrıntılı README.md, `src/` klasöründe dosya dosya açıklama vardır.")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    root = Path(a.root).resolve()
    pk, users = load(root)
    jobs = {}
    for n, d in pk.items():
        if n == "nc-kernel":
            pass
        jobs[root / d["dir"] / "README.md"] = upsert(root / d["dir"] / "README.md", crate_block(n, pk, users), n)
    for layer in LAYERS:
        p = root / "crates" / layer / "README.md"
        jobs[p] = upsert(p, group_block(layer, pk, users), layer)
    ap = root / "aarch64crates" / "README.md"
    jobs[ap] = upsert(ap, aarch64_block(pk, users), "aarch64crates")
    jobs[root / "docs/architecture/DEPENDENCIES.md"] = dependencies_md(pk, users)
    jobs[root / "docs/architecture/MODULES.md"] = modules_md(pk)
    stale = []
    for p, text in jobs.items():
        old = p.read_text(encoding="utf-8") if p.exists() else None
        if old != text:
            stale.append(p.relative_to(root).as_posix())
            if not a.check:
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(text, encoding="utf-8")
    if a.check:
        for s in stale: print("güncel değil:", s)
        print("güncel değil: %d" % len(stale))
        return 1 if stale else 0
    print("güncellendi: %d dosya, toplam %d" % (len(stale), len(jobs)))


if __name__ == "__main__":
    sys.exit(main())
