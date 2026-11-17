import math

CAP = 1000
XH = 270
BASE = 1000
DESC = 1250
XMID = (XH + BASE) // 2
MID = 500


class P:
    def __init__(self):
        self.ops = []
        self.cur = None

    def m(self, x, y):
        x, y = round(x), round(y)
        self.ops.append((1, x, y))
        self.cur = (x, y)
        return self

    def l(self, x, y):
        x, y = round(x), round(y)
        if self.cur is None:
            self.m(x, y)
        self.ops.append((2, x, y))
        self.cur = (x, y)
        return self

    def seg(self, x0, y0, x1, y1):
        x0, y0, x1, y1 = round(x0), round(y0), round(x1), round(y1)
        if self.cur != (x0, y0):
            self.m(x0, y0)
        return self.l(x1, y1)

    def dot(self, x, y):
        return self.seg(x, y, x, y)

    def q(self, cx, cy, x, y):
        if self.cur is None:
            raise ValueError
        self.ops.append((3, round(cx), round(cy), round(x), round(y)))
        self.cur = (round(x), round(y))
        return self

    def c(self, x1, y1, x2, y2, x, y):
        if self.cur is None:
            raise ValueError
        self.ops.append((4, round(x1), round(y1), round(x2), round(y2), round(x), round(y)))
        self.cur = (round(x), round(y))
        return self

    def arc(self, cx, cy, rx, ry, a0, a1):
        def pt(a):
            r = math.radians(a)
            return (cx + rx * math.cos(r), cy - ry * math.sin(r))
        sign = 1 if a1 > a0 else -1
        cuts = [a0]
        a = a0
        while True:
            nxt = (math.floor(a / 90.0 + 1e-9) + 1) * 90.0 if sign > 0 else (math.ceil(a / 90.0 - 1e-9) - 1) * 90.0
            if (sign > 0 and nxt >= a1 - 1e-9) or (sign < 0 and nxt <= a1 + 1e-9):
                cuts.append(a1)
                break
            cuts.append(nxt)
            a = nxt
        sx, sy = pt(a0)
        if self.cur is None or abs(self.cur[0] - sx) > 1.5 or abs(self.cur[1] - sy) > 1.5:
            self.m(sx, sy)
        for i in range(len(cuts) - 1):
            a, b = cuts[i], cuts[i + 1]
            ra, rb = math.radians(a), math.radians(b)
            k = 4.0 / 3.0 * math.tan((rb - ra) / 4.0)
            p0 = (math.cos(ra), math.sin(ra))
            p3 = (math.cos(rb), math.sin(rb))
            p1 = (p0[0] - k * math.sin(ra), p0[1] + k * math.cos(ra))
            p2 = (p3[0] + k * math.sin(rb), p3[1] - k * math.cos(rb))
            f = lambda p: (cx + rx * p[0], cy - ry * p[1])
            c1, c2, e = f(p1), f(p2), f(p3)
            self.c(c1[0], c1[1], c2[0], c2[1], e[0], e[1])
        return self

    def ell(self, cx, cy, rx, ry):
        return self.arc(cx, cy, rx, ry, 90, 450)

    def extend(self, other):
        self.ops.extend(other.ops)
        self.cur = other.cur
        return self


def points(ops):
    for op in ops:
        n = (len(op) - 1) // 2
        for i in range(n):
            yield op[1 + 2 * i], op[2 + 2 * i]


def bbox(ops):
    pts = list(points(ops))
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs), min(ys), max(xs), max(ys)


def shift(ops, dx, dy):
    out = []
    for op in ops:
        n = (len(op) - 1) // 2
        row = [op[0]]
        for i in range(n):
            row.append(op[1 + 2 * i] + dx)
            row.append(op[2 + 2 * i] + dy)
        out.append(tuple(row))
    return out


def scale_ops(ops, sx, sy, ox=0, oy=0):
    out = []
    for op in ops:
        n = (len(op) - 1) // 2
        row = [op[0]]
        for i in range(n):
            row.append(round(ox + op[1 + 2 * i] * sx))
            row.append(round(oy + op[2 + 2 * i] * sy))
        out.append(tuple(row))
    return out


def mirror_x(ops):
    x0, _, x1, _ = bbox(ops)
    out = []
    for op in ops:
        n = (len(op) - 1) // 2
        row = [op[0]]
        for i in range(n):
            row.append(x0 + x1 - op[1 + 2 * i])
            row.append(op[2 + 2 * i])
        out.append(tuple(row))
    return out


def rotate180(ops, cx, cy):
    out = []
    for op in ops:
        n = (len(op) - 1) // 2
        row = [op[0]]
        for i in range(n):
            row.append(2 * cx - op[1 + 2 * i])
            row.append(2 * cy - op[2 + 2 * i])
        out.append(tuple(row))
    return out


def flat(ops):
    out = []
    for op in ops:
        out.extend(op)
    return tuple(out)
