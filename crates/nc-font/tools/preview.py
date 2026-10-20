from PIL import Image
import engine


def make_font(G, extra=None):
    pool = {}
    entries = {}
    for name, (ops, adv) in G.items():
        key = name
        pool[key] = tuple(v for op in ops for v in op)
        entries[ord(name) if len(name) == 1 else name] = {'advance': adv, 'parts': [(key, 0, 0)]}
    return {'pool': pool, 'entries': {k: v for k, v in entries.items() if isinstance(k, int)},
            'space': 300, 'notdef': {'advance': 700, 'parts': []}, 'compose': {}}


def render_lines(font, lines, cap, stroke16=None, tracking=0, scale=4, bg=(255, 255, 255), fg=(20, 24, 40), pad=10, width=None):
    if stroke16 is None:
        stroke16 = max(18, cap * 2 + 2)
    lh = int(cap * 2.3) + 6
    w = max(engine.text_width(font, l, cap, tracking) for l in lines) + pad * 2
    if width:
        w = max(w, width)
    h = lh * len(lines) + pad * 2
    img = Image.new('RGB', (w, h), bg)
    px = img.load()

    def plot(x, y, c):
        if 0 <= x < w and 0 <= y < h:
            b = px[x, y]
            a = c / 255.0
            px[x, y] = tuple(int(round(b[i] * (1 - a) + fg[i] * a)) for i in range(3))

    for i, l in enumerate(lines):
        engine.draw_text(font, l, pad, pad + int(cap * 0.6) + 2 + i * lh, cap, stroke16, tracking, plot)
    return img.resize((w * scale, h * scale), Image.NEAREST) if scale > 1 else img
