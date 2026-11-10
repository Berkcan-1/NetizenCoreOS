import numpy as np
from PIL import Image

def axis(win_len, dst_len):
    ratio = win_len * 256 // dst_len
    support = min(max(256, ratio), 24 * 256)
    out = []
    for i in range(dst_len):
        c = ((2 * i + 1) * win_len * 256) // (2 * dst_len) - 128
        lo = -((-(c - support)) // 256)
        hi = (c + support) // 256
        lo = max(lo, 0)
        hi = min(hi, win_len - 1)
        ws = []
        for s in range(lo, hi + 1):
            w = support - abs(s * 256 - c)
            if w > 0:
                ws.append((s, w))
        if not ws:
            s = min(max((c + 128) // 256, 0), win_len - 1)
            ws = [(s, 1)]
        tot = sum(w for _, w in ws)
        norm = [(s, w * 65536 // tot) for s, w in ws]
        diff = 65536 - sum(w for _, w in norm)
        s_last, w_last = norm[-1]
        norm[-1] = (s_last, w_last + diff)
        out.append((norm[0][0], [w for _, w in norm]))
    return out

def resample(src, dw, dh):
    sh, sw = src.shape[:2]
    xs = axis(sw, dw)
    ys = axis(sh, dh)
    a = np.full((sh, sw), 255, dtype=np.int64)
    pm = src.astype(np.int64) * a[..., None] // 255
    cache = {}
    def hrow(y):
        if y in cache: return cache[y]
        row = pm[y]
        out = np.zeros((dw, 3), dtype=np.int64)
        for x, (first, ws) in enumerate(xs):
            acc = np.zeros(3, dtype=np.int64)
            for k, w in enumerate(ws):
                acc += w * row[first + k]
            out[x] = acc >> 8
        cache[y] = out
        return out
    res = np.zeros((dh, dw, 3), dtype=np.uint8)
    for y, (first, ws) in enumerate(ys):
        acc = np.zeros((dw, 3), dtype=np.int64)
        for k, w in enumerate(ws):
            acc += w * hrow(first + k)
        v = (acc >> 16)
        res[y] = np.clip((v + 128) >> 8, 0, 255)
    return res

if __name__ == '__main__':
    rng = np.random.default_rng(1)
    base = Image.open('/home/claude/work/outline_preview_x2.png').convert('RGB')
    for (dw, dh) in [(base.width // 3, base.height // 3), (base.width * 2, base.height * 2 // 1), (97, 61)]:
        mine = resample(np.asarray(base), dw, dh)
        ref = np.asarray(base.resize((dw, dh), Image.BILINEAR)).astype(int)
        err = np.abs(mine.astype(int) - ref)
        print((base.width, base.height), '->', (dw, dh), 'mean abs err %.3f max %d' % (err.mean(), err.max()))
