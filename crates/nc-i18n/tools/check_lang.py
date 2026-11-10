import glob, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
LANGS = os.path.join(HERE, '..', 'langs')


def read(path):
    out = {}
    for n, line in enumerate(open(path, encoding='utf8'), 1):
        line = line.rstrip('\n')
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        if '=' not in line:
            print('%s:%d: no "=" found' % (os.path.basename(path), n))
            continue
        key, value = line.split('=', 1)
        out[key.strip()] = value.strip()
    return out


def holes(value):
    return sorted(re.findall(r'\{\d\}', value))


def main():
    master = read(os.path.join(LANGS, 'en.lang'))
    problems = 0
    for path in sorted(glob.glob(os.path.join(LANGS, '*.lang'))):
        code = os.path.basename(path)[:-5]
        table = read(path)
        for key in master:
            if key not in table or not table[key]:
                print('%s: missing %s' % (code, key))
                problems += 1
            elif holes(table[key]) != holes(master[key]):
                print('%s: placeholder mismatch in %s' % (code, key))
                problems += 1
        for key in table:
            if key not in master:
                print('%s: unknown key %s' % (code, key))
                problems += 1
    print('keys: %d, problems: %d' % (len(master), problems))
    sys.exit(1 if problems else 0)


main()
