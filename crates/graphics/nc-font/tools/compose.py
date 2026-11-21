import unicodedata
from builder import *
import glyphs_latin as gl
import glyphs_extra as ge
from glyphs_latin import G

COMB = {
    0x300: 'grave', 0x301: 'acute', 0x302: 'circumflex', 0x303: 'tilde', 0x304: 'macron',
    0x306: 'breve', 0x307: 'dotabove', 0x308: 'diaeresis', 0x30A: 'ring', 0x30B: 'dblacute',
    0x30C: 'caron', 0x326: 'commabelow', 0x327: 'cedilla', 0x328: 'ogonek',
}

LOW_Y = {'acute': -50, 'grave': -50, 'circumflex': -50, 'caron': -50, 'diaeresis': 0, 'dotabove': 0,
         'macron': -30, 'breve': -40, 'ring': -55, 'tilde': -45, 'dblacute': -50, 'tick': 70}
HIGH_Y = {'acute': -250, 'grave': -250, 'circumflex': -250, 'caron': -250, 'diaeresis': -190, 'dotabove': -190,
          'macron': -220, 'breve': -230, 'ring': -270, 'tilde': -230, 'dblacute': -250, 'tick': 70}

ALIAS = {}
POOL = {}


def pool_key(name, ops):
    if name not in POOL:
        POOL[name] = flat(ops)
    return name


def base_key(ch):
    if ch in G:
        return ch
    return ALIAS.get(ch)


def info(key):
    ops, adv = G[key]
    x0, y0, x1, y1 = bbox(ops)
    return ops, adv, x0, y0, x1, y1


def place_marks(base, marks):
    key = base_key(base)
    if key is None:
        return None
    dotless = {'i': 'ı', 'j': 'ȷ'}
    if marks and key in dotless and any(m not in ('ogonek', 'cedilla', 'commabelow') for m in marks):
        key = dotless[key]
    ops, adv, x0, y0, x1, y1 = info(key)
    cx = (x0 + x1) // 2
    if key == 'ȷ':
        cx = x1
    tall = y0 < 150
    parts = [(pool_key('g:' + key, ops), 0, 0)]
    extra_adv = adv
    names = [COMB.get(m) if isinstance(m, int) else m for m in marks]
    if any(n is None for n in names):
        return None
    if names == ['acute'] and 0x391 <= ord(base) <= 0x3A9:
        parts = [(pool_key('g:' + key, ops), 230, 0), (pool_key('m:acute', ge.MARKS['acute']), 90, 170)]
        return parts, adv + 230
    if names == ['diaeresis', 'acute']:
        names = ['dialytika_tonos']
    for n in names:
        if n == 'dialytika_tonos':
            parts.append((pool_key('m:diaeresis', ge.MARKS['diaeresis']), cx, 0))
            parts.append((pool_key('m:acute', ge.MARKS['acute']), cx, -230))
            continue
        if n == 'caron' and key in ('d', 'l', 't', 'L'):
            ty = {'d': 70, 'l': 70, 't': 150, 'L': 70}[key]
            tx = x1 + 100 if key in ('d', 'l', 'L') else x1 + 60
            if key in ('l',):
                tx = x0 + 140
            if key == 'L':
                tx = x0 + 150
            parts.append((pool_key('m:tick', ge.MARKS['tick']), tx, ty))
            extra_adv = max(extra_adv, tx + 90)
            continue
        if n in ('cedilla', 'commabelow'):
            use = 'cedilla' if key in ('c', 'C', 's', 'S') else 'commabelow'
            mx = cx
            if key in ('t', 'T'):
                mx = cx
            parts.append((pool_key('m:' + use, ge.MARKS[use]), mx, 1000))
            continue
        if n == 'ogonek':
            ox = {'a': x1, 'A': x1 - 40, 'u': x1, 'U': x1 - 40, 'e': x1 - 100, 'E': x1 - 140,
                  'i': cx, 'ı': cx, 'I': cx, 'o': x1 - 100, 'O': x1 - 150}.get(key, x1 - 100)
            parts.append((pool_key('m:ogonek', ge.MARKS['ogonek']), ox, 1000))
            continue
        y = (HIGH_Y if tall else LOW_Y)[n]
        parts.append((pool_key('m:' + n, ge.MARKS[n]), cx, y))
    return parts, extra_adv


def seq_parts(keys):
    parts = []
    x = 0
    for k in keys:
        ops, adv = G[k]
        parts.append((pool_key('g:' + k, ops), x, 0))
        x += adv
    return parts, x


def make_entries(codes):
    entries = {}
    for code in codes:
        ch = chr(code)
        key = base_key(ch)
        if key is not None:
            ops, adv = G[key]
            entries[code] = (adv, [(pool_key('g:' + key, ops), 0, 0)])
            continue
        nfd = unicodedata.normalize('NFD', ch)
        if len(nfd) > 1:
            res = place_marks(nfd[0], [ord(m) for m in nfd[1:]])
            if res is not None:
                parts, adv = res
                entries[code] = (adv, parts)
    return entries
