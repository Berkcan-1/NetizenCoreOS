import glob
import math
import os
import re
import sys

import numpy as np
from contourpy import LineType, contour_generator
from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTCollection
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
LANG_DIR = os.environ.get('NC_LANG_DIR', os.path.join(ROOT, '..', 'nc-i18n', 'langs'))
EXTRA_FILE = os.path.join(HERE, 'extra_chars.txt')
OUT_RS = os.path.join(ROOT, 'src', 'data_outline.rs')

CJK_TTC = '/usr/share/fonts/opentype/noto/NotoSansCJK-Medium.ttc'
DEVA_TTF = '/usr/share/fonts/truetype/google-fonts/Poppins-Medium.ttf'

VARIANTS = {'zh': 'SC', 'ja': 'JP', 'ko': 'KR'}
FLATNESS = 2.0
SIMPLIFY = 1.4

DEVA_CONSONANTS = list(range(0x0915, 0x093A)) + list(range(0x0958, 0x0960))
DEVA_VOWELS = list(range(0x0904, 0x0915))
DEVA_MATRAS = [0x093E, 0x093F, 0x0940, 0x0941, 0x0942, 0x0943, 0x0947, 0x0948, 0x094B, 0x094C]
DEVA_MARKS = [0x0901, 0x0902, 0x0903]
DEVA_DIGITS = list(range(0x0966, 0x0970))


def is_deva(code):
    return 0x0900 <= code <= 0x097F


def is_deva_extend(prev, nxt):
    if nxt in (0x200C, 0x200D):
        return prev == 0x094D or is_deva(prev)
    if 0x0900 <= nxt <= 0x0903 or 0x093A <= nxt <= 0x094F and nxt != 0x093D:
        return True
    if nxt == 0x093C or 0x0951 <= nxt <= 0x0957 or nxt in (0x0962, 0x0963):
        return True
    if prev == 0x094D and (0x0915 <= nxt <= 0x0939 or 0x0958 <= nxt <= 0x095F):
        return True
    if prev in (0x200D, 0x200C) and (0x0915 <= nxt <= 0x0939 or 0x0958 <= nxt <= 0x095F):
        return True
    return False


def deva_clusters(text):
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = ord(text[i])
        if not is_deva(c):
            i += 1
            continue
        j = i + 1
        while j < n and is_deva_extend(ord(text[j - 1]), ord(text[j])):
            j += 1
        out.append(text[i:j])
        i = j
    return out


def load_strokes_covered():
    covered = set()
    with open(os.path.join(ROOT, 'src', 'data.rs'), encoding='utf8') as f:
        for m in re.finditer(r'Entry \{ code: 0x([0-9A-Fa-f]+),', f.read()):
            covered.add(int(m.group(1), 16))
    return covered


def combining(code):
    return 0x0300 <= code <= 0x036F


def read_lang_texts():
    texts = {}
    for path in sorted(glob.glob(os.path.join(LANG_DIR, '*.lang'))):
        code = os.path.basename(path)[:-5]
        buf = []
        with open(path, encoding='utf8') as f:
            for line in f:
                line = line.rstrip('\n')
                if not line.strip() or line.lstrip().startswith('#') or '=' not in line:
                    continue
                value = line.split('=', 1)[1].strip()
                value = value.replace('\\n', '\n')
                buf.append(value)
        texts[code] = '\n'.join(buf)
    return texts


class FlatPen(BasePen):
    def __init__(self, glyph_set):
        super().__init__(glyph_set)
        self.contours = []
        self.cur = None
        self.pos = (0.0, 0.0)

    def _moveTo(self, pt):
        self.cur = [pt]
        self.pos = pt

    def _lineTo(self, pt):
        self.cur.append(pt)
        self.pos = pt

    def _curveToOne(self, p1, p2, p3):
        self._cubic(self.pos, p1, p2, p3, 0)
        self.pos = p3

    def _qCurveToOne(self, p1, p2):
        p0 = self.pos
        c1 = (p0[0] + 2 / 3 * (p1[0] - p0[0]), p0[1] + 2 / 3 * (p1[1] - p0[1]))
        c2 = (p2[0] + 2 / 3 * (p1[0] - p2[0]), p2[1] + 2 / 3 * (p1[1] - p2[1]))
        self._cubic(p0, c1, c2, p2, 0)
        self.pos = p2

    def _cubic(self, p0, p1, p2, p3, depth):
        dx = p3[0] - p0[0]
        dy = p3[1] - p0[1]
        length = math.hypot(dx, dy)
        if length < 1e-9:
            d1 = math.hypot(p1[0] - p0[0], p1[1] - p0[1])
            d2 = math.hypot(p2[0] - p0[0], p2[1] - p0[1])
        else:
            d1 = abs((p1[0] - p0[0]) * dy - (p1[1] - p0[1]) * dx) / length
            d2 = abs((p2[0] - p0[0]) * dy - (p2[1] - p0[1]) * dx) / length
        if max(d1, d2) <= FLATNESS or depth >= 8:
            self.cur.append(p3)
            return
        m01 = ((p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2)
        m12 = ((p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2)
        m23 = ((p2[0] + p3[0]) / 2, (p2[1] + p3[1]) / 2)
        a = ((m01[0] + m12[0]) / 2, (m01[1] + m12[1]) / 2)
        b = ((m12[0] + m23[0]) / 2, (m12[1] + m23[1]) / 2)
        m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
        self._cubic(p0, m01, a, m, depth + 1)
        self._cubic(m, b, m23, p3, depth + 1)

    def _closePath(self):
        if self.cur:
            self.contours.append(self.cur)
        self.cur = None

    def _endPath(self):
        self._closePath()


def clean_contour(points):
    pts = [(int(round(x)), int(round(y))) for x, y in points]
    out = []
    for p in pts:
        if not out or out[-1] != p:
            out.append(p)
    if len(out) > 1 and out[0] == out[-1]:
        out.pop()
    return out


def rdp_open(points, eps):
    if len(points) < 3:
        return list(points)
    keep = [False] * len(points)
    keep[0] = keep[-1] = True
    stack = [(0, len(points) - 1)]
    while stack:
        a, b = stack.pop()
        ax, ay = points[a]
        bx, by = points[b]
        dx, dy = bx - ax, by - ay
        norm = math.hypot(dx, dy)
        best, idx = -1.0, -1
        for i in range(a + 1, b):
            px, py = points[i]
            if norm < 1e-9:
                d = math.hypot(px - ax, py - ay)
            else:
                d = abs((px - ax) * dy - (py - ay) * dx) / norm
            if d > best:
                best, idx = d, i
        if best > eps:
            keep[idx] = True
            stack.append((a, idx))
            stack.append((idx, b))
    return [p for p, k in zip(points, keep) if k]


def simplify_closed(points, eps):
    if len(points) < 8:
        return points
    far = max(range(len(points)), key=lambda i: (points[i][0] - points[0][0]) ** 2 + (points[i][1] - points[0][1]) ** 2)
    first = rdp_open(points[: far + 1], eps)
    second = rdp_open(points[far:] + [points[0]], eps)
    merged = first[:-1] + second[:-1]
    return merged if len(merged) >= 3 else points


class CjkFonts:
    def __init__(self):
        self.fonts = {}
        collection = TTCollection(CJK_TTC)
        for font in collection.fonts:
            family = font['name'].getDebugName(1) or ''
            for tag in ('SC', 'JP', 'KR'):
                if family.startswith('Noto Sans CJK ' + tag):
                    self.fonts[tag] = font
        missing = [t for t in ('SC', 'JP', 'KR') if t not in self.fonts]
        if missing:
            raise SystemExit('CJK fonts missing: ' + ','.join(missing))
        self.cache = {}

    def glyph(self, tag, ch):
        key = (tag, ch)
        if key in self.cache:
            return self.cache[key]
        font = self.fonts[tag]
        cmap = font.getBestCmap()
        result = None
        code = ord(ch)
        if code in cmap:
            name = cmap[code]
            gs = font.getGlyphSet()
            pen = FlatPen(gs)
            gs[name].draw(pen)
            contours = []
            for c in pen.contours:
                pts = clean_contour(c)
                if len(pts) >= 3:
                    contours.append(pts)
            advance = int(round(font['hmtx'][name][0]))
            result = (advance, contours)
        self.cache[key] = result
        return result


def trace_cluster(font, text):
    size = 1000
    pad = 300
    width = int(font.getlength(text)) + pad * 2
    height = 2000
    base = 1250
    image = Image.new('L', (width, height), 0)
    draw = ImageDraw.Draw(image)
    draw.text((pad, base), text, font=font, fill=255, anchor='ls')
    arr = np.asarray(image, dtype=np.float64)
    gen = contour_generator(z=arr, line_type=LineType.Separate)
    lines = gen.lines(127.5)
    contours = []
    for line in lines:
        pts = [(x + 0.5 - pad, base - (y + 0.5)) for x, y in line]
        pts = clean_contour(pts)
        pts = simplify_closed(pts, SIMPLIFY)
        if len(pts) >= 3:
            contours.append(pts)
    advance = int(round(font.getlength(text)))
    return advance, contours


def encode(contours):
    flat = []
    for pts in contours:
        flat.append(len(pts))
        for x, y in pts:
            flat.append(int(x))
            flat.append(int(y))
    return flat


def main():
    texts = read_lang_texts()
    covered = load_strokes_covered()
    extra = ''
    if os.path.exists(EXTRA_FILE):
        extra = open(EXTRA_FILE, encoding='utf8').read()

    needed = {'zh': set(), 'ja': set(), 'ko': set()}
    deva = set()
    uncovered_other = {}
    for lang, text in texts.items():
        for ch in text + (extra if lang in needed else ''):
            code = ord(ch)
            if ch in '\n\r' or code < 0x20:
                continue
            if code in covered or combining(code) or 0x200B <= code <= 0x200F:
                continue
            if is_deva(code):
                continue
            if lang in needed:
                needed[lang].add(ch)
            else:
                uncovered_other.setdefault(lang, set()).add(ch)
    if extra:
        for ch in extra:
            if ord(ch) > 0x20 and ord(ch) not in covered and not is_deva(ord(ch)) and not combining(ord(ch)):
                for lang in needed:
                    needed[lang].add(ch)
    for lang, chars in uncovered_other.items():
        print('WARNING %s has characters outside every font: %s' % (lang, ''.join(sorted(chars))))

    hi_text = texts.get('hi', '')
    used_clusters = set(deva_clusters(hi_text))
    basic = set()
    for c in DEVA_CONSONANTS:
        base = chr(c)
        basic.add(base)
        for m in DEVA_MATRAS + DEVA_MARKS + [0x094D]:
            basic.add(base + chr(m))
    for v in DEVA_VOWELS:
        basic.add(chr(v))
        for m in DEVA_MARKS:
            basic.add(chr(v) + chr(m))
    for d in DEVA_DIGITS:
        basic.add(chr(d))
    basic.add(chr(0x0964))
    basic.add(chr(0x0965))
    clusters = sorted(used_clusters | basic) if os.environ.get('NC_DEVA_BASIC') == '1' else sorted(used_clusters | {chr(d) for d in DEVA_DIGITS} | {chr(0x0964)})

    cjk = CjkFonts()
    sc = {}
    variants = {'JP': {}, 'KR': {}}
    all_chars = set()
    for lang in needed:
        all_chars |= needed[lang]
    for ch in sorted(all_chars):
        g = cjk.glyph('SC', ch)
        if g is None:
            for alt in ('JP', 'KR'):
                g = cjk.glyph(alt, ch)
                if g is not None:
                    break
        if g is None:
            print('WARNING no glyph for U+%04X %s' % (ord(ch), ch))
            continue
        sc[ch] = g
    for lang, tag in (('ja', 'JP'), ('ko', 'KR')):
        for ch in sorted(needed[lang]):
            g = cjk.glyph(tag, ch)
            if g is None:
                continue
            base = sc.get(ch)
            if base is None or base[1] != g[1] or base[0] != g[0]:
                variants[tag][ch] = g

    deva_font = ImageFont.truetype(DEVA_TTF, 1000, layout_engine=ImageFont.Layout.RAQM)
    cluster_data = []
    for text in clusters:
        advance, contours = trace_cluster(deva_font, text)
        if contours:
            cluster_data.append((text, advance, contours))

    points = []
    tables = {}

    def emit_table(name, mapping, evenodd):
        rows = []
        for ch in sorted(mapping, key=ord):
            advance, contours = mapping[ch]
            data = encode(contours)
            rows.append((ord(ch), advance, evenodd, len(points), len(data)))
            points.extend(data)
        tables[name] = rows

    emit_table('SC', sc, False)
    emit_table('JP', variants['JP'], False)
    emit_table('KR', variants['KR'], False)

    cluster_rows = []
    for text, advance, contours in cluster_data:
        data = encode(contours)
        cluster_rows.append((text, advance, len(points), len(data)))
        points.extend(data)

    if max(abs(v) for v in points) > 32000:
        raise SystemExit('coordinate overflow')

    lines = []
    lines.append('pub struct OutGlyph {')
    lines.append('    pub code: u32,')
    lines.append('    pub advance: u16,')
    lines.append('    pub even_odd: bool,')
    lines.append('    pub start: u32,')
    lines.append('    pub len: u16,')
    lines.append('}')
    lines.append('')
    lines.append('pub struct Cluster {')
    lines.append('    pub key: &\'static str,')
    lines.append('    pub advance: u16,')
    lines.append('    pub start: u32,')
    lines.append('    pub len: u16,')
    lines.append('}')
    lines.append('')
    for name in ('SC', 'JP', 'KR'):
        lines.append('pub static %s: [OutGlyph; %d] = [' % (name, len(tables[name])))
        for code, adv, eo, start, ln in tables[name]:
            lines.append('    OutGlyph { code: 0x%X, advance: %d, even_odd: %s, start: %d, len: %d },' % (
                code, adv, 'true' if eo else 'false', start, ln))
        lines.append('];')
        lines.append('')
    lines.append('pub static CLUSTERS: [Cluster; %d] = [' % len(cluster_rows))
    for text, adv, start, ln in cluster_rows:
        escaped = ''.join('\\u{%x}' % ord(c) for c in text)
        lines.append('    Cluster { key: "%s", advance: %d, start: %d, len: %d },' % (escaped, adv, start, ln))
    lines.append('];')
    lines.append('')
    lines.append('pub static POINTS: [i16; %d] = [' % len(points))
    for i in range(0, len(points), 24):
        lines.append('    ' + ', '.join(str(v) for v in points[i:i + 24]) + ',')
    lines.append('];')
    with open(OUT_RS, 'w', encoding='utf8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')

    print('CJK glyphs: SC %d, JP-only %d, KR-only %d' % (len(sc), len(variants['JP']), len(variants['KR'])))
    print('Devanagari clusters: %d (used %d)' % (len(cluster_rows), len(used_clusters)))
    print('points: %d i16 = %.1f KiB' % (len(points), len(points) * 2 / 1024))
    print('wrote', OUT_RS)

    import json
    dump = {
        'points': points,
        'tables': {k: v for k, v in tables.items()},
        'clusters': cluster_rows,
    }
    with open(os.path.join(HERE, '_outline_dump.json'), 'w') as f:
        json.dump(dump, f)


if __name__ == '__main__':
    main()
