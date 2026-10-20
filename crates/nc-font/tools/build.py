import unicodedata, os, sys
import charset, compose, engine
from builder import *

OUT = os.path.join(os.path.dirname(__file__), '..', 'src', 'data.rs')
SPACE = 300


def assemble():
    ent = charset.build()
    ent = {c: v for c, v in ent.items()}
    ent[0x20] = (SPACE, [])
    ent[0xA0] = (SPACE, [])
    notdef_ops = P().seg(60, 0, 520, 0).l(520, 1000).l(60, 1000).l(60, 0).ops
    nd_key = compose.pool_key('g:notdef', notdef_ops)
    notdef = (640, [(nd_key, 0, 0)])
    pairs = {}
    for code in ent:
        nfd = unicodedata.normalize('NFD', chr(code))
        if len(nfd) == 2:
            pairs[(ord(nfd[0]), ord(nfd[1]))] = code
        elif len(nfd) == 3:
            mid = ord(unicodedata.normalize('NFC', nfd[0] + nfd[1]))
            if mid in ent:
                pairs[(mid, ord(nfd[2]))] = code
    pairs = {k: v for k, v in pairs.items() if 0x300 <= k[1] <= 0x36F}
    return ent, notdef, pairs


def font_dict(ent, notdef, pairs):
    return {'pool': compose.POOL,
            'entries': {c: {'advance': a, 'parts': p} for c, (a, p) in ent.items()},
            'space': SPACE, 'notdef': {'advance': notdef[0], 'parts': notdef[1]},
            'compose': pairs}


def validate(font):
    stats = {}
    bad = []
    for cap in (7, 8, 9, 10, 12, 14, 18, 24, 40, 64, 120):
        for code, e in list(font['entries'].items()) + [(0, font['notdef'])]:
            st = {}
            engine.draw_glyph(font, e, 0, 0, cap, max(18, cap * 2 + 2), lambda x, y, c: None, st)
            for k, v in st.items():
                stats[k] = max(stats.get(k, 0), v)
            if st.get('segs', 0) >= engine.MAX_SEGMENTS or st.get('kx', 0) >= engine.MAX_KNOTS or st.get('ky', 0) >= engine.MAX_KNOTS:
                bad.append((cap, hex(code), st))
    return stats, bad


def emit(ent, notdef, pairs):
    ops_pool = []
    index = {}
    for key, ops in compose.POOL.items():
        index[key] = (len(ops_pool), len(ops))
        ops_pool.extend(ops)
    parts = []
    rows = []

    def add(code, adv, plist):
        first = len(parts)
        for key, dx, dy in plist:
            s, n = index[key]
            parts.append((s, n, dx, dy))
        return (code, adv, first, len(plist))

    for code in sorted(ent):
        adv, plist = ent[code]
        rows.append(add(code, adv, plist))
    nd = add(0xFFFD, notdef[0], notdef[1])
    L = []
    L.append("pub struct Entry {\n    pub code: u32,\n    pub advance: u16,\n    pub first_part: u16,\n    pub part_count: u8,\n}\n")
    L.append("pub struct Part {\n    pub start: u32,\n    pub len: u16,\n    pub dx: i16,\n    pub dy: i16,\n}\n")
    L.append("pub const SPACE_ADVANCE: i32 = %d;\n" % SPACE)
    L.append("pub static NOTDEF: Entry = Entry { code: 0xFFFD, advance: %d, first_part: %d, part_count: %d };\n" % (nd[1], nd[2], nd[3]))
    L.append("pub static ENTRIES: [Entry; %d] = [" % len(rows))
    for r in rows:
        L.append("    Entry { code: 0x%X, advance: %d, first_part: %d, part_count: %d }," % r)
    L.append("];\n")
    L.append("pub static PARTS: [Part; %d] = [" % len(parts))
    for p in parts:
        L.append("    Part { start: %d, len: %d, dx: %d, dy: %d }," % p)
    L.append("];\n")
    L.append("pub static COMPOSE: [(u32, u32, u32); %d] = [" % len(pairs))
    for (b, m), c in sorted(pairs.items()):
        L.append("    (0x%X, 0x%X, 0x%X)," % (b, m, c))
    L.append("];\n")
    L.append("pub static OPS: [i16; %d] = [" % len(ops_pool))
    for i in range(0, len(ops_pool), 24):
        L.append("    " + ", ".join(str(v) for v in ops_pool[i:i + 24]) + ",")
    L.append("];\n")
    L.append("""pub fn find(code: u32) -> Option<&'static Entry> {
    match ENTRIES.binary_search_by_key(&code, |entry| entry.code) {
        Ok(index) => Some(&ENTRIES[index]),
        Err(_) => None,
    }
}

pub fn is_combining(code: u32) -> bool {
    code >= 0x300 && code <= 0x36F
}

pub fn compose(base: u32, mark: u32) -> Option<u32> {
    for &(first, second, result) in COMPOSE.iter() {
        if first == base && second == mark {
            return Some(result);
        }
    }
    None
}
""")
    for v in (ops_pool, [p[2] for p in parts], [p[3] for p in parts]):
        assert all(-32768 <= x <= 32767 for x in v)
    assert all(r[2] < 65536 for r in rows)
    open(OUT, 'w').write("\n".join(L))
    return len(rows), len(parts), len(ops_pool), len(pairs)


if __name__ == '__main__':
    ent, notdef, pairs = assemble()
    font = font_dict(ent, notdef, pairs)
    stats, bad = validate(font)
    print('stats', stats, 'bad', bad[:5])
    print('emitted', emit(ent, notdef, pairs))
