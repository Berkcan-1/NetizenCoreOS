#!/usr/bin/env python3
# Ring 3 Kronometre'nin dil tablosunu nc-i18n/langs/*.lang'dan uretir. Siralama = nc_i18n::Language sayisal degeri.
import os, re, sys
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
ORDER = ["tr", "en", "fr", "de", "es", "pt", "ru", "zh", "ja", "ko", "hi"]  # Language::Turkish = 0 ...
KEYS = ["SwRunning", "SwStopped", "SwStart", "SwStop", "SwReset"]

def load(code):
    values = {}
    for line in open(os.path.join(ROOT, "crates/nc-i18n/langs", code + ".lang"), encoding="utf-8"):
        m = re.match(r"^(\w+)\s*=\s*(.*?)\s*$", line.rstrip("\n"))
        if m:
            values[m.group(1)] = m.group(2)
    return values

rows = []
for code in ORDER:
    v = load(code)
    missing = [k for k in KEYS if k not in v]
    if missing:
        sys.exit(f"{code}.lang eksik anahtar: {missing}")
    rows.append((code, [v[k] for k in KEYS]))

TAIL = """
/// Bilinmeyen dil kimliği İngilizce'ye (1) düşer.
pub fn for_language(id: u32) -> &'static [&'static str; COLUMNS] {
    let index = if (id as usize) < LANGUAGES { id as usize } else { 1 };
    &TABLE[index]
}
"""
out = ["// Otomatik uretildi: tools/gen_r3_strings.py (elle duzenlemeyin).",
       "// Sutunlar: Running, Stopped, Start, Stop, Reset. Satir sirasi nc_i18n::Language degeri.",
       "pub const RUNNING: usize = 0;", "pub const STOPPED: usize = 1;", "pub const START: usize = 2;", "pub const STOP: usize = 3;", "pub const RESET: usize = 4;", "pub const COLUMNS: usize = 5;", "pub const LANGUAGES: usize = %d;" % len(ORDER),
       "pub const TABLE: [[&str; COLUMNS]; LANGUAGES] = ["]
for code, vals in rows:
    out.append("    // %s\n    [%s]," % (code, ", ".join('"%s"' % x.replace('"', '\\"') for x in vals)))
out.append("];")
out.append(TAIL)
path = os.path.join(ROOT, "crates/nc-r3-stopwatch/src/strings.rs")
open(path, "w", encoding="utf-8").write("\n".join(out) + "\n")
print("yazildi:", path)
