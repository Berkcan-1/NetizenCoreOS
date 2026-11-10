# nc-elf::parse ve nc-abi::draw::validate mantiginin birebir Python portu + Rust testlerinin portu.
import struct, random, os
HERE=os.path.dirname(os.path.abspath(__file__))
PAGE=4096
PF_X,PF_W,PF_R=1,2,4
class E(Exception): pass
def u16(b,o): return struct.unpack_from('<H',b,o)[0]
def u32(b,o): return struct.unpack_from('<I',b,o)[0]
def u64(b,o): return struct.unpack_from('<Q',b,o)[0]
M64=(1<<64)-1
def cadd(a,b):
    r=a+b
    if r>M64: return None
    return r
POL=dict(min=0x80_0100_0000,max=0x80_0800_0000,pages=8192)

def parse(b,pol=POL):
    if len(b)<64: raise E('TooShort')
    if b[0:4]!=b'\x7fELF': raise E('BadMagic')
    if b[4]!=2: raise E('NotElf64')
    if b[5]!=1: raise E('NotLittleEndian')
    if b[6]!=1 or u32(b,20)!=1: raise E('BadVersion')
    if u16(b,16)!=2: raise E('NotExec')
    if u16(b,18)!=62: raise E('NotX86_64')
    if u16(b,52)!=64 or u16(b,54)!=56: raise E('BadHeaderSize')
    entry=u64(b,24); phoff=u64(b,32); phnum=u16(b,56)
    if phnum==0 or phnum>16: raise E('BadPhdrTable')
    end=cadd(phoff,phnum*56)
    if end is None: raise E('BadPhdrTable')
    if phoff<64 or end>len(b): raise E('BadPhdrTable')
    segs=[]
    for i in range(phnum):
        base=phoff+i*56
        t=u32(b,base); fl=u32(b,base+4)
        if t in (0,4,6,0x6474E550,0x6474E552): continue
        if t==0x6474E551:
            if fl&PF_X: raise E('ExecutableStack')
            continue
        if t!=1: raise E('UnsupportedSegment')
        off=u64(b,base+8); va=u64(b,base+16); fsz=u64(b,base+32); msz=u64(b,base+40)
        if msz==0: continue
        if fl==0 or fl&~7: raise E('BadFlags')
        if fl&PF_W and fl&PF_X: raise E('WritableExecutable')
        if fsz>msz: raise E('SizeMismatch')
        fe=cadd(off,fsz)
        if fe is None or fe>len(b): raise E('FileRange')
        if fsz>0 and va%PAGE!=off%PAGE: raise E('Misaligned')
        me=cadd(va,msz)
        if me is None: raise E('OutOfRange')
        pe=cadd(me,PAGE-1)
        if pe is None: raise E('OutOfRange')
        pe&=~(PAGE-1)
        if va<pol['min'] or pe>pol['max']: raise E('OutOfRange')
        if len(segs)>=8: raise E('TooManySegments')
        segs.append(dict(va=va,msz=msz,off=off,fsz=fsz,fl=fl))
    if not segs: raise E('NoLoadSegments')
    ps=lambda s: s['va']&~(PAGE-1)
    pend=lambda s: (s['va']+s['msz']+PAGE-1)&~(PAGE-1)
    for i in range(len(segs)):
        for j in range(i+1,len(segs)):
            a,c=segs[i],segs[j]
            if ps(a)<pend(c) and ps(c)<pend(a): raise E('Overlap')
    segs.sort(key=lambda s:s['va'])
    total=sum((pend(s)-ps(s))//PAGE for s in segs)
    if total>pol['pages']: raise E('TooLarge')
    if not any(s['fl']&PF_X and entry>=s['va'] and entry<s['va']+s['fsz'] for s in segs): raise E('BadEntry')
    return dict(entry=entry,segs=segs,pages=total)

# --- test iskeleti (tests.rs'in birebir portu) ---
LEN=0x3004; ENTRY=0x80_0100_0000
def load(fl,off,va,fsz,msz,t=1): return dict(t=t,fl=fl,off=off,va=va,fsz=fsz,msz=msz)
def base(): return [load(PF_R|PF_X,0x1000,0x80_0100_0000,0x100,0x100), load(PF_R,0x2000,0x80_0100_1000,0x80,0x80), load(PF_R|PF_W,0x3000,0x80_0100_2000,4,0x50)]
def build(entry,phs):
    b=bytearray(0x4000)
    b[0:4]=b'\x7fELF'; b[4]=2;b[5]=1;b[6]=1
    struct.pack_into('<H',b,16,2); struct.pack_into('<H',b,18,62); struct.pack_into('<I',b,20,1)
    struct.pack_into('<Q',b,24,entry); struct.pack_into('<Q',b,32,64)
    struct.pack_into('<H',b,52,64); struct.pack_into('<H',b,54,56); struct.pack_into('<H',b,56,len(phs))
    for i,p in enumerate(phs):
        o=64+i*56
        struct.pack_into('<II',b,o,p['t'],p['fl']); struct.pack_into('<QQQQQQ',b,o+8,p['off'],p['va'],p['va'],p['fsz'],p['msz'],0x1000)
    return b
def pw(entry,phs,pol=POL): return parse(bytes(build(entry,phs)[:LEN]),pol)
def expect(f,err):
    try: f()
    except E as e:
        assert str(e)==err,(str(e),err); return
    raise AssertionError('hata beklendi: '+err)

p=pw(ENTRY,base()); assert len(p['segs'])==3 and p['pages']==3 and p['segs'][2]['msz']==0x50
a,b_,c=base(); p=pw(ENTRY,[c,a,b_]); v=[s['va'] for s in p['segs']]; assert v==sorted(v)
good=build(ENTRY,base())
def chk(mut,err):
    b=bytearray(good); mut(b); expect(lambda: parse(bytes(b[:LEN])),err)
chk(lambda b:b.__setitem__(0,0),'BadMagic'); chk(lambda b:b.__setitem__(4,1),'NotElf64'); chk(lambda b:b.__setitem__(5,2),'NotLittleEndian')
chk(lambda b:b.__setitem__(6,2),'BadVersion'); chk(lambda b:b.__setitem__(16,3),'NotExec'); chk(lambda b:b.__setitem__(18,3),'NotX86_64')
chk(lambda b:b.__setitem__(52,60),'BadHeaderSize'); chk(lambda b:b.__setitem__(54,32),'BadHeaderSize')
chk(lambda b:b.__setitem__(56,0),'BadPhdrTable'); chk(lambda b:b.__setitem__(56,17),'BadPhdrTable')
chk(lambda b:struct.pack_into('<Q',b,32,M64),'BadPhdrTable'); chk(lambda b:struct.pack_into('<Q',b,32,LEN-10),'BadPhdrTable'); chk(lambda b:struct.pack_into('<Q',b,32,8),'BadPhdrTable')
expect(lambda: parse(bytes(good[:63])),'TooShort'); expect(lambda: parse(b''),'TooShort')

text,ro,data=base()
wx=dict(text,fl=7); expect(lambda: pw(ENTRY,[wx,ro,data]),'WritableExecutable')
expect(lambda: pw(ENTRY,[text,dict(ro,fl=0),data]),'BadFlags'); expect(lambda: pw(ENTRY,[text,dict(ro,fl=PF_R|0x01000000),data]),'BadFlags')
expect(lambda: pw(ENTRY,[text,ro,dict(data,fsz=0x60)]),'SizeMismatch')
expect(lambda: pw(ENTRY,[text,ro,dict(data,off=0x3000,fsz=0x1000,msz=0x1000)]),'FileRange')
expect(lambda: pw(ENTRY,[text,ro,dict(data,off=M64-1,fsz=8)]),'FileRange')
expect(lambda: pw(ENTRY,[text,dict(ro,off=0x2010),data]),'Misaligned')
bss=load(PF_R|PF_W,0x3003,0x80_0100_2000,0,0x4000); r=pw(ENTRY,[text,ro,bss]); assert r['segs'][2]['fsz']==0 and r['pages']==6
expect(lambda: pw(ENTRY,[text,ro,dict(data,off=0x3001,fsz=2)]),'Misaligned'); expect(lambda: pw(ENTRY,[text,ro,dict(bss,off=LEN+1)]),'FileRange')
for bad in [0,0x1000,0x80_0000_0000,0x80_00FF_F000,0xFFFF_8000_0000_0000,0x80_0800_0000]:
    expect(lambda: pw(bad,[dict(text,va=bad,off=0x1000+(bad&0xFFF)),ro,data]),'OutOfRange')
expect(lambda: pw(ENTRY,[text,ro,dict(data,va=0x80_07FF_F000,msz=0x2000,fsz=4)]),'OutOfRange')
expect(lambda: pw(ENTRY,[text,ro,dict(data,va=(M64-0xFFF)&~0xFFF,msz=0x2000)]),'OutOfRange')
expect(lambda: pw(ENTRY,[text,ro,dict(data,msz=M64)]),'OutOfRange')
expect(lambda: pw(ENTRY,[text,dict(ro,va=0x80_0100_0800,off=0x2800),data]),'Overlap')
expect(lambda: pw(ENTRY,[text,ro,dict(data,va=text['va'],off=0x1000)]),'Overlap')
expect(lambda: pw(ENTRY,[text,ro,dict(data,msz=8193*4096)]),'TooLarge')
expect(lambda: pw(ro['va'],base()),'BadEntry'); expect(lambda: pw(data['va'],base()),'BadEntry')
expect(lambda: pw(text['va']+0x100,base()),'BadEntry'); expect(lambda: pw(0,base()),'BadEntry')
assert len(pw(text['va']+0xFF,base())['segs'])==3
expect(lambda: pw(ENTRY,[load(0,0,0,0,0,t=0)]),'NoLoadSegments'); expect(lambda: pw(ENTRY,[load(PF_R,0x2000,0x80_0100_5000,0,0)]),'NoLoadSegments')
for k in [2,3,7,0x6474E553,0x70000000]:
    expect(lambda: pw(ENTRY,[text,ro,data,load(PF_R,0,0,0,0,t=k)]),'UnsupportedSegment')
expect(lambda: pw(ENTRY,[text,ro,data,load(7,0,0,0,0,t=0x6474E551)]),'ExecutableStack')
pw(ENTRY,[text,ro,data,load(6,0,0,0,0,t=0x6474E551)]); pw(ENTRY,[text,ro,data,load(PF_R,0x100,0,8,8,t=4)])
phs=[load(PF_R,0x2000,0x80_0100_1000+i*0x1000,8,8) for i in range(9)]; phs[0]['fl']=PF_R|PF_X
expect(lambda: pw(0x80_0100_1000,phs),'TooManySegments'); pw(0x80_0100_1000,phs[:8])

REAL=open(os.path.join(HERE,'..','..','kernel','faz6','faz6_app.elf'),'rb').read()
r=parse(REAL); s=r['segs']
assert r['entry']==0x80_0100_0000 and len(s)==3 and r['pages']==3
assert s[0]['fl']==5 and s[1]['fl']==4 and s[2]['fl']==6 and s[2]['msz']>s[2]['fsz']
print('nc-elf: tum sentetik testler + gercek ELF gecti')

rnd=random.Random(7); hl=64+3*56; acc=0
for _ in range(60000):
    b=bytearray(REAL); n=len(b)
    for _ in range(1+rnd.randrange(4)): b[rnd.randrange(hl)]=rnd.randrange(256)
    if rnd.randrange(5)==0: n=rnd.randrange(len(REAL)+1)
    try: plan=parse(bytes(b[:n]))
    except E: continue
    acc+=1
    tot=0
    for i,sg in enumerate(plan['segs']):
        assert (sg['fl']&3)!=3
        ps=sg['va']&~4095; pe=(sg['va']+sg['msz']+4095)&~4095
        assert ps>=POL['min'] and pe<=POL['max'] and sg['fsz']<=sg['msz'] and sg['off']+sg['fsz']<=n and (sg['fsz']==0 or sg['va']%4096==sg['off']%4096)
        tot+=(pe-ps)//4096
        for o in plan['segs'][i+1:]:
            ops=o['va']&~4095; ope=(o['va']+o['msz']+4095)&~4095
            assert pe<=ops or ope<=ps
    assert tot==plan['pages']
print('nc-elf fuzz: 60000 mutasyon, kabul edilen =',acc)
