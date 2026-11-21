import math
import os
import re
import sys

from fontTools.ttLib import TTFont
from fontTools.pens.transformPen import TransformPen

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_outline as bo

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT_RS = os.path.join(ROOT, 'src', 'data_latin.rs')
INTER_DIR = os.environ.get('NC_INTER_DIR', '/usr/share/fonts/opentype/inter')
WEIGHTS = (('REGULAR', 'Inter-Medium.otf'), ('BOLD', 'Inter-Bold.otf'))
bo.FLATNESS = float(os.environ.get('NC_LATIN_FLATNESS', '4.0'))

RANGES = [
    (0x21, 0x7E), (0xA1, 0xFF), (0x100, 0x17F), (0x218, 0x21B), (0x2C6, 0x2C7),
    (0x400, 0x45F), (0x490, 0x491),
    (0x2013, 0x2014), (0x2018, 0x201E), (0x2020, 0x2022), (0x2026, 0x2026), (0x2030, 0x2030), (0x2039, 0x203A),
    (0x20A9, 0x20A9), (0x20AC, 0x20AC), (0x20B9, 0x20BA), (0x2122, 0x2122), (0x2190, 0x2193), (0x2212, 0x2212),
]


def wanted(code):
    return any(a <= code <= b for a, b in RANGES) and not (0x300 <= code <= 0x36F)


def build(path):
    font = TTFont(path)
    cmap = font.getBestCmap()
    upm = font['head'].unitsPerEm
    scale = 1000.0 / upm
    gs = font.getGlyphSet()
    rows = {}
    for code in sorted(cmap):
        if not wanted(code):
            continue
        name = cmap[code]
        pen = bo.FlatPen(gs)
        gs[name].draw(TransformPen(pen, (scale, 0, 0, scale, 0, 0)))
        contours = []
        for c in pen.contours:
            pts = bo.simplify_closed(bo.clean_contour(c), 2.2)
            if len(pts) >= 3:
                contours.append(pts)
        advance = int(round(font['hmtx'][name][0] * scale))
        rows[code] = (advance, contours)
    return rows


def main():
    tables = {}
    points = []
    for key, filename in WEIGHTS:
        rows = build(os.path.join(INTER_DIR, filename))
        out = []
        for code in sorted(rows):
            advance, contours = rows[code]
            data = bo.encode(contours)
            out.append((code, advance, len(points), len(data)))
            points.extend(data)
        tables[key] = out
    if max(abs(v) for v in points) > 32000:
        raise SystemExit('coordinate overflow')
    if max(len(t[3:]) and t[3] for k in tables for t in tables[k]) > 65535:
        raise SystemExit('glyph too long')
    lines = []
    lines.append('use crate::data_outline::OutGlyph;')
    lines.append('')
    for key in ('REGULAR', 'BOLD'):
        lines.append('pub static LATIN_%s: [OutGlyph; %d] = [' % (key, len(tables[key])))
        for code, adv, start, ln in tables[key]:
            lines.append('    OutGlyph { code: 0x%X, advance: %d, even_odd: false, start: %d, len: %d },' % (code, adv, start, ln))
        lines.append('];')
        lines.append('')
    lines.append('pub static LATIN_POINTS: [i16; %d] = [' % len(points))
    for i in range(0, len(points), 24):
        lines.append('    ' + ', '.join(str(v) for v in points[i:i + 24]) + ',')
    lines.append('];')
    with open(OUT_RS, 'w', encoding='utf8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')
    print('glyphs: regular %d, bold %d' % (len(tables['REGULAR']), len(tables['BOLD'])))
    print('points: %d i16 = %.1f KiB' % (len(points), len(points) * 2 / 1024))


if __name__ == '__main__':
    main()
