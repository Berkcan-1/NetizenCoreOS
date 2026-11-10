MAX_SEGMENTS = 192
MAX_KNOTS = 24


def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def isqrt(n):
    x = n
    r = 0
    bit = 1 << 62
    while bit > n:
        bit >>= 2
    while bit != 0:
        if x >= r + bit:
            x -= r + bit
            r = (r >> 1) + bit
        else:
            r >>= 1
        bit >>= 2
    return r


class Axis:
    def __init__(self, base, scale, grid):
        self.base = base
        self.scale = scale
        self.grid = grid
        self.unit = []
        self.snapped = []

    def add(self, u):
        if u in self.unit or len(self.unit) >= MAX_KNOTS:
            return
        self.unit.append(u)
        self.unit.sort()

    def plain(self, u):
        return self.base + (u * self.scale) // 1000

    def snap(self, p):
        return ((p - self.grid + 128) // 256) * 256 + self.grid

    def finish(self):
        self.snapped = [self.snap(self.plain(u)) for u in self.unit]

    def map(self, u):
        n = len(self.unit)
        if n == 0:
            return self.plain(u)
        last = n - 1
        if u <= self.unit[0]:
            return self.snapped[0] + self.plain(u) - self.plain(self.unit[0])
        if u >= self.unit[last]:
            return self.snapped[last] + self.plain(u) - self.plain(self.unit[last])
        i = 0
        while self.unit[i + 1] <= u:
            i += 1
        span = self.unit[i + 1] - self.unit[i]
        return self.snapped[i] + ((self.snapped[i + 1] - self.snapped[i]) * (u - self.unit[i])) // span


def walk(font, entry, visit):
    for key, dx, dy in entry['parts']:
        ops = font['pool'][key]
        i = 0
        while i < len(ops):
            code = ops[i]
            if code == 1:
                visit(('M', ops[i + 1] + dx, ops[i + 2] + dy))
                i += 3
            elif code == 2:
                visit(('L', ops[i + 1] + dx, ops[i + 2] + dy))
                i += 3
            elif code == 3:
                visit(('Q', ops[i + 1] + dx, ops[i + 2] + dy, ops[i + 3] + dx, ops[i + 4] + dy))
                i += 5
            else:
                visit(('C', ops[i + 1] + dx, ops[i + 2] + dy, ops[i + 3] + dx, ops[i + 4] + dy, ops[i + 5] + dx, ops[i + 6] + dy))
                i += 7


class Scan:
    def __init__(self, xs, ys):
        self.xs = xs
        self.ys = ys
        self.cur = (0, 0)
        self.min_x = 1 << 30
        self.max_x = -(1 << 30)
        self.min_y = 1 << 30
        self.max_y = -(1 << 30)

    def include(self, x, y):
        self.min_x = min(self.min_x, x)
        self.max_x = max(self.max_x, x)
        self.min_y = min(self.min_y, y)
        self.max_y = max(self.max_y, y)

    def op(self, op):
        k = op[0]
        if k == 'M':
            self.cur = (op[1], op[2])
            self.include(op[1], op[2])
        elif k == 'L':
            x, y = op[1], op[2]
            if x == self.cur[0]:
                self.xs.add(x)
            if y == self.cur[1]:
                self.ys.add(y)
            self.cur = (x, y)
            self.include(x, y)
        else:
            pts = op[1:]
            for i in range(0, len(pts), 2):
                self.include(pts[i], pts[i + 1])
            self.cur = (pts[-2], pts[-1])


class Shaper:
    def __init__(self, xs, ys, ox, oy):
        self.xs = xs
        self.ys = ys
        self.ox = ox
        self.oy = oy
        self.cur = (0, 0)
        self.segs = []

    def point(self, x, y):
        return (self.ox + self.xs.map(x), self.oy + self.ys.map(y))

    def push(self, a, b):
        if len(self.segs) < MAX_SEGMENTS:
            self.segs.append((a[0], a[1], b[0], b[1]))

    def steps(self, pts):
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        ext = max(max(xs) - min(xs), max(ys) - min(ys))
        r = ext // 256
        n = isqrt(6 * r) + 1
        return max(3, min(14, n))

    def op(self, op):
        k = op[0]
        if k == 'M':
            self.cur = self.point(op[1], op[2])
        elif k == 'L':
            p = self.point(op[1], op[2])
            self.push(self.cur, p)
            self.cur = p
        elif k == 'Q':
            c = self.point(op[1], op[2])
            e = self.point(op[3], op[4])
            n = self.steps([self.cur, c, e])
            prev = self.cur
            for i in range(1, n + 1):
                mt = n - i
                d = n * n
                x = (mt * mt * self.cur[0] + 2 * mt * i * c[0] + i * i * e[0]) // d
                y = (mt * mt * self.cur[1] + 2 * mt * i * c[1] + i * i * e[1]) // d
                self.push(prev, (x, y))
                prev = (x, y)
            self.cur = e
        else:
            c1 = self.point(op[1], op[2])
            c2 = self.point(op[3], op[4])
            e = self.point(op[5], op[6])
            n = self.steps([self.cur, c1, c2, e])
            prev = self.cur
            p0 = self.cur
            for i in range(1, n + 1):
                mt = n - i
                d = n * n * n
                x = (mt ** 3 * p0[0] + 3 * mt * mt * i * c1[0] + 3 * mt * i * i * c2[0] + i ** 3 * e[0]) // d
                y = (mt ** 3 * p0[1] + 3 * mt * mt * i * c1[1] + 3 * mt * i * i * c2[1] + i ** 3 * e[1]) // d
                self.push(prev, (x, y))
                prev = (x, y)
            self.cur = e


def dist_sq(px, py, s):
    ax, ay = s[0], s[1]
    dx, dy = s[2] - ax, s[3] - ay
    l2 = dx * dx + dy * dy
    if l2 == 0:
        qx, qy = ax, ay
    else:
        t = (px - ax) * dx + (py - ay) * dy
        if t <= 0:
            qx, qy = ax, ay
        elif t >= l2:
            qx, qy = s[2], s[3]
        else:
            qx = ax + tdiv(dx * t, l2)
            qy = ay + tdiv(dy * t, l2)
    ex, ey = px - qx, py - qy
    return ex * ex + ey * ey


def draw_glyph(font, entry, origin_x, origin_y, cap, stroke16, plot, stats=None):
    s = max(1, min((stroke16 + 8) // 16, max(1, cap // 2)))
    grid = 128 if s % 2 == 1 else 0
    xs = Axis(0, cap * 256, grid)
    ys = Axis(s * 128, (cap - s) * 256, grid)
    scan = Scan(xs, ys)
    walk(font, entry, scan.op)
    if scan.min_x > scan.max_x:
        return
    xs.add(scan.min_x)
    xs.add(scan.max_x)
    ys.add(scan.min_y)
    ys.add(scan.max_y)
    if stats is not None:
        stats['kx'] = max(stats.get('kx', 0), len(xs.unit))
        stats['ky'] = max(stats.get('ky', 0), len(ys.unit))
    xs.finish()
    ys.finish()
    shaper = Shaper(xs, ys, origin_x * 256, origin_y * 256)
    walk(font, entry, shaper.op)
    segs = shaper.segs
    if stats is not None:
        stats['segs'] = max(stats.get('segs', 0), len(segs))
    if not segs:
        return
    half = max(1, stroke16) * 8
    reach = half + 128
    reach_sq = reach * reach
    left = min(min(a[0], a[2]) for a in segs)
    right = max(max(a[0], a[2]) for a in segs)
    top = min(min(a[1], a[3]) for a in segs)
    bottom = max(max(a[1], a[3]) for a in segs)
    min_x = (left - reach) // 256
    max_x = (right + reach) // 256
    min_y = (top - reach) // 256
    max_y = (bottom + reach) // 256
    for py in range(min_y, max_y + 1):
        sy = py * 256 + 128
        active = [a for a in segs if min(a[1], a[3]) - reach <= sy <= max(a[1], a[3]) + reach]
        if not active:
            continue
        for px in range(min_x, max_x + 1):
            sx = px * 256 + 128
            best = reach_sq
            for a in active:
                if sx < min(a[0], a[2]) - reach or sx > max(a[0], a[2]) + reach:
                    continue
                d = dist_sq(sx, sy, a)
                if d < best:
                    best = d
            if best < reach_sq:
                dist = isqrt(best)
                cov = reach - dist
                if cov > 0:
                    plot(px, py, min(cov, 255))


def is_combining(code):
    return 0x300 <= code <= 0x36F


def classify(font, code):
    if code < 0x20:
        return ('blank', font['space'] * 4) if code == 9 else ('skip',)
    if code == 0x7F or 0x80 <= code < 0xA0:
        return ('skip',)
    e = font['entries'].get(code)
    if e is not None:
        return ('glyph', e)
    if code in (0x200B, 0x200C, 0x200D, 0x200E, 0x200F, 0x2060, 0xFEFF) or 0xFE00 <= code <= 0xFE0F or is_combining(code):
        return ('skip',)
    if 0x2000 <= code <= 0x200A or code in (0x202F, 0x205F, 0x3000):
        return ('blank', font['space'])
    return ('glyph', font['notdef'])


def layout(font, text, cap, tracking, visit):
    pen = 0
    count = 0
    i = 0
    chars = [ord(c) for c in text]
    while i < len(chars):
        code = chars[i]
        i += 1
        while i < len(chars) and is_combining(chars[i]):
            comp = font['compose'].get((code, chars[i]))
            if comp is not None:
                code = comp
            i += 1
        item = classify(font, code)
        if item[0] == 'skip':
            continue
        if item[0] == 'blank':
            adv = (item[1] * cap + 500) // 1000
        else:
            adv = (item[1]['advance'] * cap + 500) // 1000
            visit(item[1], pen)
        adv = max(adv, 1)
        pen += adv + tracking
        count += 1
    return 0 if count == 0 else max(0, pen - tracking)


def text_width(font, text, cap, tracking=0):
    return layout(font, text, cap, tracking, lambda e, o: None)


def draw_text(font, text, x, y, cap, stroke16, tracking, plot, stats=None):
    layout(font, text, cap, tracking, lambda e, o: draw_glyph(font, e, x + o, y, cap, stroke16, plot, stats))
