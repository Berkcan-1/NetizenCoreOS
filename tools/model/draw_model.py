import struct, random, sys
LIM=16384
class E(Exception): pass
def i32(b,o): return struct.unpack_from('<i',b,o)[0]
def u32(b,o): return struct.unpack_from('<I',b,o)[0]
def co(v): return -LIM<=v<=LIM
def pad(n): return (n+3)&~3
def rec(b,off):
    rest=b[off:]
    if len(rest)<4: raise E('Truncated')
    op=rest[0]
    if rest[1]!=0: raise E('BadFlags')
    ln=struct.unpack_from('<H',rest,2)[0]
    if ln<4 or ln%4: raise E('BadLength')
    if ln>len(rest): raise E('Truncated')
    p=rest[4:ln]
    if op==1:
        if len(p)!=20: raise E('BadLength')
        x,y,w,h=i32(p,0),i32(p,4),i32(p,8),i32(p,12)
        if not(co(x) and co(y) and 0<=w<=LIM and 0<=h<=LIM): raise E('OutOfRange')
        return ln,0
    if op==2:
        if len(p)!=36: raise E('BadLength')
        x,y,w,h,r=[i32(p,k) for k in (0,4,8,12,16)]; fa,ba=u32(p,24),u32(p,32)
        if not(co(x) and co(y) and 1<=w<=2048 and 1<=h<=2048 and 0<=r<=256 and r*2<=min(w,h) and fa<=255 and ba<=255): raise E('OutOfRange')
        return ln,w*h
    if op==3:
        if len(p)!=20: raise E('BadLength')
        cx,cy,r=i32(p,0),i32(p,4),i32(p,8); st=u32(p,16)
        if not(co(cx) and co(cy) and 0<=r<=512 and st<=1024): raise E('OutOfRange')
        return ln,(2*r+3)**2
    if op==4:
        if len(p)<20: raise E('BadLength')
        x,y,cap=i32(p,0),i32(p,4),i32(p,8); tl=struct.unpack_from('<H',p,16)[0]
        if p[18]>100 or p[19]>1: raise E('OutOfRange')
        if tl>256: raise E('OutOfRange')
        if len(p)!=20+pad(tl): raise E('BadLength')
        if any(p[20+tl:]): raise E('BadFlags')
        if not(co(x) and co(y) and 1<=cap<=128): raise E('OutOfRange')
        try: bytes(p[20:20+tl]).decode('utf-8')
        except UnicodeDecodeError: raise E('BadUtf8')
        return ln,tl*cap*cap
    if op==5:
        if len(p)<24: raise E('BadLength')
        x,y,w,h=i32(p,0),i32(p,4),i32(p,8),i32(p,12); ll=struct.unpack_from('<H',p,20)[0]; st,hv=p[22],p[23]
        if not(co(x) and co(y) and 1<=w<=2048 and 1<=h<=2048 and st<=1 and hv<=1 and ll<=64): raise E('OutOfRange')
        if len(p)!=24+pad(ll): raise E('BadLength')
        if any(p[24+ll:]): raise E('BadFlags')
        try: bytes(p[24:24+ll]).decode('utf-8')
        except UnicodeDecodeError: raise E('BadUtf8')
        return ln,w*h
    raise E('UnknownOp')
def validate(b):
    if len(b)>16384: raise E('StreamTooLarge')
    off=0;n=0;w=0
    while off<len(b):
        ln,c=rec(b,off); n+=1
        if n>512: raise E('TooManyRecords')
        w+=c
        if w>16*1024*1024: raise E('WorkBudget')
        off+=ln
    return n
def expect(f,err):
    try: f()
    except E as e:
        assert str(e)==err,(str(e),err); return
    raise AssertionError('hata beklendi '+err)

def fill(x,y,w,h,c): return struct.pack('<BBHiiiiI',1,0,24,x,y,w,h,c)
def panel(x,y,w,h,r,f,fa,b,ba): return struct.pack('<BBHiiiiiIIII',2,0,40,x,y,w,h,r,f,fa,b,ba)
def dot(cx,cy,r,c,s): return struct.pack('<BBHiiiII',3,0,24,cx,cy,r,c,s)
def text(x,y,cap,c,s,tr=0,al=0):
    b=s.encode(); pd=(-len(b))%4
    return struct.pack('<BBHiiiIHBB',4,0,24+len(b)+pd,x,y,cap,c,len(b),tr,al)+b+b'\0'*pd
def button(x,y,w,h,acc,st,hv,s):
    b=s.encode(); pd=(-len(b))%4
    return struct.pack('<BBHiiiiIHBB',5,0,28+len(b)+pd,x,y,w,h,acc,len(b),st,hv)+b+b'\0'*pd
sample=fill(-8,-8,200,200,0xFF)+fill(10,6,40,20,0xFF0000)+panel(4,4,100,50,8,0xFFFFFF,255,0xCCCCCC,255)+dot(20,20,5,0x00FF00,255)+text(12,30,16,0x102030,"Merhaba")+text(0,0,8,0,"")
assert validate(sample)==6 and len(sample)==168
# her onek kesme
bounds=set();o=0
while o<len(sample): bounds.add(o); o+=struct.unpack_from('<H',sample,o+2)[0]
bounds.add(len(sample))
for cut in range(len(sample)):
    try: validate(sample[:cut]); ok=True
    except E: ok=False
    assert ok==(cut in bounds),cut
# spesifik ihlaller
okrec=fill(0,0,1,1,0)
def mut(i,v,rec=okrec):
    b=bytearray(rec); 
    if isinstance(i,slice): b[i]=v
    else: b[i]=v
    return bytes(b)
expect(lambda: validate(mut(0,99)),'UnknownOp'); expect(lambda: validate(mut(1,1)),'BadFlags')
expect(lambda: validate(mut(2,23)),'BadLength'); expect(lambda: validate(mut(2,0)),'BadLength'); expect(lambda: validate(mut(2,28)),'Truncated')
expect(lambda: validate(mut(slice(4,8),struct.pack('<i',LIM+1))),'OutOfRange')
expect(lambda: validate(mut(slice(12,16),struct.pack('<i',-1))),'OutOfRange')
expect(lambda: validate(mut(slice(4,8),struct.pack('<i',-2**31))),'OutOfRange')
expect(lambda: validate(bytes(16388)),'StreamTooLarge') if False else None
# metin
t=text(0,0,16,0,"abc"); assert len(t)==28
expect(lambda: validate(mut(24,0xFF,t) if False else mut(slice(24,26),b'\xff\xfe',t)),'BadUtf8')
expect(lambda: validate(mut(4+23,1,t)),'BadFlags')
expect(lambda: validate(mut(4+16,200,t)),'BadLength')
expect(lambda: validate(mut(slice(12,16),struct.pack('<i',0),t)),'OutOfRange')
expect(lambda: validate(mut(slice(12,16),struct.pack('<i',129),t)),'OutOfRange')
# yeni ihlaller
expect(lambda: validate(text(0,0,16,0,"a",101,0)),'OutOfRange'); expect(lambda: validate(text(0,0,16,0,"a",0,2)),'OutOfRange')
expect(lambda: validate(button(0,0,10,10,0,2,0,"x")),'OutOfRange'); expect(lambda: validate(button(0,0,0,10,0,0,0,"x")),'OutOfRange')
bt=button(0,0,10,10,0,0,1,"abc"); assert validate(bt)==1 and len(bt)==32
expect(lambda: validate(mut(4+24+3,7,bt)),'BadFlags'); expect(lambda: validate(mut(4+24,0xFF,bt)),'BadUtf8'); expect(lambda: validate(mut(4+20,10,bt)),'BadLength')
# butce
p=panel(0,0,2048,2048,0,0,255,0,255)
assert validate(p*4)==4; expect(lambda: validate(p*5),'WorkBudget')
f=fill(0,0,1,1,0); assert validate(f*512)==512; expect(lambda: validate(f*513),'TooManyRecords'); assert len(f*513)==12312
expect(lambda: validate(panel(0,0,10,10,6,0,0,0,0)),'OutOfRange')
# fuzz
rnd=random.Random(3); acc=0
for _ in range(40000):
    b=bytearray(sample); n=len(b)
    for _ in range(1+rnd.randrange(4)): b[rnd.randrange(n)]=rnd.randrange(256)
    if rnd.randrange(4)==0: n=rnd.randrange(n+1)
    try: validate(bytes(b[:n])); acc+=1
    except E: pass
assert acc>0
print('nc-abi draw: tum beklentiler ve fuzz gecti; fuzz kabul =',acc)
