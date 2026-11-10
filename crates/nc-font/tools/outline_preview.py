import json
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import build_outline as bo
import engine

SUB = 4
MAX_W = 256


def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def em_pixels(cap):
    return (cap * 138 + 50) // 100


def adv_px(advance, cap):
    return max(1, (advance * em_pixels(cap) + 500) // 1000)


class Data:
    def __init__(self):
        d = json.load(open(os.path.join(HERE, '_outline_dump.json')))
        self.points = d['points']
        self.sc = {r[0]: r for r in d['tables']['SC']}
        self.jp = {r[0]: r for r in d['tables']['JP']}
        self.kr = {r[0]: r for r in d['tables']['KR']}
        self.clusters = {r[0]: r for r in d['clusters']}


def draw_shape(points, even_odd, x, y, cap, plot):
    em = em_pixels(cap)
    base = (y + cap) * 256
    origin = x * 256
    tx = lambda u: origin + tdiv(u * em * 256, 1000)
    ty = lambda u: base - tdiv(u * em * 256, 1000)
    xs, ys = [], []
    i = 0
    contours = []
    while i < len(points):
        n = points[i]
        i += 1
        pts = [(tx(points[i + k * 2]), ty(points[i + k * 2 + 1])) for k in range(n)]
        contours.append(pts)
        i += n * 2
        for px, py in pts:
            xs.append(px)
            ys.append(py)
    if not xs:
        return
    x_first = min(xs) // 256
    x_last = (max(xs) + 255) // 256
    width = x_last - x_first + 1
    if width <= 0 or width > MAX_W:
        return
    x_origin = x_first * 256
    row_first = min(ys) // 256
    row_last = (max(ys) + 255) // 256
    for row in range(row_first, row_last + 1):
        cover = [0] * (MAX_W + 1)
        used = False
        for sub in range(SUB):
            sample = row * 256 + (2 * sub + 1) * 256 // (2 * SUB)
            cross = []
            for pts in contours:
                n = len(pts)
                for j in range(n):
                    x0, y0 = pts[j]
                    x1, y1 = pts[(j + 1) % n]
                    if y0 == y1:
                        continue
                    if y0 < y1:
                        low, high, d = y0, y1, 1
                    else:
                        low, high, d = y1, y0, -1
                    if sample < low or sample >= high:
                        continue
                    cx = x0 + tdiv((sample - y0) * (x1 - x0), (y1 - y0))
                    cross.append((cx, d))
            cross.sort(key=lambda t: t[0])
            wind = 0
            for c in range(len(cross) - 1):
                wind += cross[c][1]
                inside = (wind & 1) != 0 if even_odd else wind != 0
                if inside:
                    a = max(0, min(width * 256, cross[c][0] - x_origin))
                    b = max(0, min(width * 256, cross[c + 1][0] - x_origin))
                    if b <= a:
                        continue
                    used = True
                    fa, fb = a >> 8, b >> 8
                    if fa == fb:
                        cover[fa] += b - a
                    else:
                        cover[fa] += 256 - (a & 255)
                        for k in range(fa + 1, fb):
                            cover[k] += 256
                        cover[fb] += b & 255
        if used:
            for col in range(width):
                v = cover[col]
                if v > 0:
                    alpha = min(255, (v * 255 + (256 * SUB) // 2) // (256 * SUB))
                    if alpha > 0:
                        plot(x_first + col, row, alpha)


def render(data, text, cap, hint, canvas, ox, oy):
    pen = 0
    i = 0
    table = {1: data.jp, 2: data.kr}.get(hint, {})
    while i < len(text):
        ch = text[i]
        code = ord(ch)
        if bo.is_deva(code):
            j = i + 1
            while j < len(text) and bo.is_deva_extend(ord(text[j - 1]), ord(text[j])):
                j += 1
            key = text[i:j]
            r = data.clusters.get(key)
            if r:
                _, adv, start, ln = r
                draw_shape(data.points[start:start + ln], True, ox + pen, oy, cap, canvas)
                pen += adv_px(adv, cap)
            else:
                pen += cap
            i = j
            continue
        r = table.get(code) or data.sc.get(code)
        if r:
            _, adv, eo, start, ln = r
            draw_shape(data.points[start:start + ln], eo, ox + pen, oy, cap, canvas)
            pen += adv_px(adv, cap)
        else:
            pen += cap // 2 + 2
        i += 1
    return pen


def main():
    data = Data()
    texts = bo.read_lang_texts()
    samples = []
    for lang, hint in (('zh', 0), ('ja', 1), ('ko', 2), ('hi', 0)):
        lines = [l.strip() for l in texts[lang].split('\n') if l.strip()]
        picks = [lines[1], lines[2], lines[4], lines[12], lines[60], lines[70]] if len(lines) > 70 else lines[:6]
        samples.append((lang, hint, picks))
    caps = [8, 10, 13, 18]
    W = 1500
    img = Image.new('L', (W, 1400), 255)
    px = img.load()

    def plot(x, y, a):
        if 0 <= x < img.width and 0 <= y < img.height:
            cur = px[x, y]
            px[x, y] = cur - (cur * a) // 255

    y = 10
    for lang, hint, picks in samples:
        for cap in caps:
            x = 10
            for text in picks[:4]:
                w = render(data, text, cap, hint, plot, x, y)
                x += w + 24
            y += int(cap * 1.9) + 10
        y += 16
    img = img.crop((0, 0, W, y))
    img.save(os.path.join(HERE, '..', '..', 'outline_preview.png'))
    img2 = img.resize((img.width * 2, img.height * 2), Image.NEAREST)
    img2.save(os.path.join(HERE, '..', '..', 'outline_preview_x2.png'))
    print('ok', img.size)


main()
