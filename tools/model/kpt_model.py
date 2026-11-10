# kpt.rs'in algoritmasinin birebir Python modeli (Rust DERLENMEDI; bu yalnizca mantik denetimi).
P,W,U,PWT,PCD,HUGE,NX = 1,2,4,8,16,128,1<<63
PAGE,TWO,GIB = 4096,1<<21,1<<30
FOUR,MMIO,SPAN = 4*GIB,3*GIB,512*GIB
class Err(Exception): pass

def rup(v,a): return (v+a-1)&~(a-1)
def inr(a,r): return r[0]<=a<r[1]

class Tables:
    def __init__(s):
        s.mem={}  # fiziksel adres -> 512'lik sayfa (dict); sahte "fiziksel" adres atamasi
        s.next=0x1000_0000_0000
    def page(s):
        a=s.next; s.next+=PAGE; s.mem[a]=[0]*512; return a

def classify(L,a):
    if L['guard'] and a==L['guard']: return 'G'
    if inr(a,L['text']): return 'X'
    if inr(a,L['ro']): return 'R'
    return 'W'

def leaf(k,ph,nx):
    n=NX if nx else 0
    return {'X':ph|P,'R':ph|P|n,'W':ph|P|W|n,'G':0}[k]

def validate(L):
    for v in (*L['text'],*L['ro'],*L['rw']):
        if v&0xFFF: raise Err('Unaligned')
    if L['guard']&0xFFF: raise Err('Unaligned')
    ok=(L['text'][0]<L['text'][1]<=L['ro'][0]<=L['ro'][1]<=L['rw'][0]<L['rw'][1])
    if not ok: raise Err('InvalidMemoryMap')
    if L['guard'] and not inr(L['guard'],L['rw']): raise Err('InvalidMemoryMap')
    if L['rw'][1]>FOUR: raise Err('InvalidMemoryMap')

def build(T,L,nx,wc):
    validate(L)
    T.mem.clear(); T.next=0x1000_0000_0000
    nxb=NX if nx else 0
    fine_end=rup(L['rw'][1],TWO); chunks=fine_end//TWO
    if chunks>8: raise Err('TransactionCapacity')
    pml4=T.page(); pdpt=T.page(); pd=[T.page() for _ in range(4)]
    fine=[T.page() for _ in range(chunks)]; fbpd=T.page()
    T.mem[pml4][0]=pdpt|P|W
    for g in range(4):
        T.mem[pdpt][g]=pd[g]|P|W
        for sl in range(512):
            a=g*GIB+sl*TWO
            T.mem[pd[g]][sl]=a|P|W|HUGE|(PCD if a>=MMIO else 0)|nxb
    for c in range(chunks):
        base=c*TWO
        for sl in range(512):
            a=base+sl*PAGE
            T.mem[fine[c]][sl]=leaf(classify(L,a),a,nx)
        T.mem[pd[0]][c]=fine[c]|P|W
    fa,fl=L['fb']
    if fl:
        if fl>64<<20: raise Err('InvalidMemoryMap')
        fe=fa+fl
        if fe>SPAN: raise Err('InvalidFrame')
        st=fa&~(TWO-1); en=rup(fe,TWO)
        cache=PWT if wc else PCD
        fbg=None; a=st
        while a<en:
            if a<FOUR:
                if a>=fine_end:
                    g=a//GIB; sl=(a%GIB)//TWO
                    T.mem[pd[g]][sl]=a|P|W|HUGE|cache|nxb
            else:
                g=a>>30
                if fbg is None:
                    if T.mem[pdpt][g]!=0: raise Err('AlreadyMapped')
                    T.mem[pdpt][g]=fbpd|P|W; fbg=g
                elif fbg!=g: raise Err('InvalidMemoryMap')
                sl=(a&(GIB-1))//TWO
                T.mem[fbpd][sl]=a|P|W|HUGE|cache|nxb
            a+=TWO
    return pml4,chunks,pd

M4K=0x000F_FFFF_FFFF_F000; M2M=0x000F_FFFF_FFE0_0000
def translate(T,pml4,v):
    e=T.mem[pml4][(v>>39)&511]
    if not e&P: return None
    e2=T.mem[e&M4K][(v>>30)&511]
    if not e2&P: return None
    pdp=e2&M4K
    pde=T.mem[pdp][(v>>21)&511]
    if not pde&P: return None
    user=bool(e&U and e2&U and pde&U); wr=bool(e&W and e2&W and pde&W); nx=bool(e&NX or e2&NX or pde&NX)
    if pde&HUGE:
        ph=(pde&M2M)|(v&0x1FFFFF)
        return ph,(P|HUGE|(U if user else 0)|(W if wr else 0)|(NX if nx else 0))
    pte=T.mem[pde&M4K][(v>>12)&511]
    if not pte&P: return None
    eu=user and pte&U; ew=wr and pte&W; en=nx or pte&NX
    return (pte&M4K)|(v&0xFFF),(P|(U if eu else 0)|(W if ew else 0)|(NX if en else 0))

def check_one(T,pml4,a,exp,nx):
    h=translate(T,pml4,a)
    if exp=='A': return h is None
    if h is None: return False
    ph,f=h
    if ph!=a or f&U: return False
    w=bool(f&W); x=(not nx) or not f&NX
    return {'X':x and not w,'R':(not w) and ((not nx) or not x),'W':w and ((not nx) or not x)}[exp]

def verify(T,pml4,L,nx,extra=()):
    fe=rup(L['rw'][1],TWO)
    a=0
    while a<fe:
        e={'X':'X','R':'R','W':'W','G':'A'}[classify(L,a)]
        if not check_one(T,pml4,a,e,nx): return (a,e)
        a+=PAGE
    a=fe
    while a<FOUR:
        if not check_one(T,pml4,a,'W',nx): return (a,'W')
        a+=TWO
    fa,fl=L['fb']
    if fl:
        for p in (fa,fa+fl-1):
            if not check_one(T,pml4,p,'W',nx): return (p,'W')
    for a,e in extra:
        if not check_one(T,pml4,a,e,nx): return (a,e)
    return None

def base():
    return dict(text=(0x100000,0x300000),ro=(0x300000,0x400000),rw=(0x400000,0x700000),guard=0x6E0000,fb=(0xFD000000,8<<20))

T=Tables()
L=base()
pml4,chunks,pd=build(T,L,True,True)
assert chunks==4
assert verify(T,pml4,L,True) is None
# W^X taramasi
a=0
while a<0x800000:
    h=translate(T,pml4,a)
    if h:
        ph,f=h; assert not (f&W and not f&NX),hex(a); assert not f&U
    a+=PAGE
assert translate(T,pml4,0x6E0000) is None and translate(T,pml4,0x6DF000) and translate(T,pml4,0x6E1000)
f=translate(T,pml4,0x100000)[1]; assert not f&NX and not f&W
f=translate(T,pml4,0x300000)[1]; assert f&NX and not f&W
f=translate(T,pml4,0x400000)[1]; assert f&NX and f&W
f=translate(T,pml4,0x900)[1]; assert f&NX and f&W
# onbellek bitleri
assert T.mem[pd[3]][(0xFEE00000%GIB)//TWO]&PCD
assert T.mem[pd[1]][0]&(PCD|PWT)==0
e=T.mem[pd[3]][(0xFD000000%GIB)//TWO]; assert e&PWT and not e&PCD
# 4 GiB ustu fb
H=base(); H['fb']=(0x800200000,8<<20)
pml4,_,_=build(T,H,True,True); assert verify(T,pml4,H,True) is None
assert translate(T,pml4,0x800200000)[0]==0x800200000
# GiB'a sigmayan
S=base(); S['fb']=(0x83F800000,16<<20)
try: build(T,S,True,True); assert False
except Err as e: assert str(e)=='InvalidMemoryMap'
# hizasiz / sira disi / guard disinda
for mut,exp in (({'text':(0x100000,0x300001)},'Unaligned'),({'ro':(0x200000,0x400000)},'InvalidMemoryMap'),({'guard':0x200000},'InvalidMemoryMap')):
    B=base(); B.update(mut)
    try: build(T,B,True,True); assert False
    except Err as e: assert str(e)==exp,(str(e),exp)
# NX yok
pml4,_,pd=build(T,base(),False,False)
for g in range(4):
    assert all(x&NX==0 for x in T.mem[pd[g]])
assert translate(T,pml4,0x400000)[1]&NX==0
# dogrulayici hatayi yakalar
pml4,_,_=build(T,base(),True,True)
assert verify(T,pml4,base(),True,[(0x400000,'X')]) is not None
assert verify(T,pml4,base(),True,[(0x100000,'W')]) is not None
# gercekci: 6 MiB'lik imaj tam sinirda (0x100000 + 0x600000 = 0x700000)
R=dict(text=(0x100000,0x400000),ro=(0x400000,0x500000),rw=(0x500000,0x700000),guard=0x6C0000,fb=(0xFD000000,1280*720*4))
pml4,ch,_=build(T,R,True,True); assert verify(T,pml4,R,True) is None; print('imaj 6 MiB siniri: parca =',ch)
print('MODEL TAMAM: tum senaryolar gecti')
