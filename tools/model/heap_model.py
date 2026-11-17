# nc-app-host/src/heap.rs boş-liste ayırıcısının mantık portu (adres aritmetiği) + rastgele iş yükü.
import random
A=16
def up(v,a): return (v+a-1)&~(a-1)
class Heap:
    def __init__(s,base,size):
        st=up(base,A); en=(base+size)&~(A-1); s.holes=[[st,en-st]] if en>st+A else []
    def alloc(s,size,align):
        size=up(max(size,1),A); align=max(align,A)
        for i,(hs,hz) in enumerate(s.holes):
            he=hs+hz; st=up(hs,align); en=st+size
            if en<=he:
                front=st-hs; back=he-en
                new=[]
                if front: new.append([hs,front])
                if back: new.append([en,back])
                s.holes[i:i+1]=new
                return st
        return None
    def free(s,addr,size,align=1):
        size=up(max(size,1),A)
        i=0
        while i<len(s.holes) and s.holes[i][0]<addr: i+=1
        s.holes.insert(i,[addr,size])
        if i+1<len(s.holes) and addr+size==s.holes[i+1][0]:
            s.holes[i][1]+=s.holes[i+1][1]; del s.holes[i+1]
        if i>0 and s.holes[i-1][0]+s.holes[i-1][1]==addr:
            s.holes[i-1][1]+=s.holes[i][1]; del s.holes[i]
base=0x1000_0000; total=8192*16
h=Heap(base,total); live=[]; r=random.Random(5)
for step in range(60000):
    if len(live)<400 and (r.randrange(5)!=0 or not live):
        size=1+r.randrange(700); align=1<<r.randrange(8)
        p=h.alloc(size,align)
        if p is None: continue
        assert p%max(align,16)==0 and base<=p and p+size<=base+total
        live.append((p,size,align))
    elif live:
        a,sz,al=live.pop(r.randrange(len(live))); h.free(a,sz,al)
    if step%5000==0:
        rg=sorted((a,a+up(sz,16)) for a,sz,al in live)
        for x,y in zip(rg,rg[1:]): assert x[1]<=y[0]
        # delikler ile canli bloklar da cakismamali
        for hs,hz in h.holes:
            for a,b in rg: assert hs+hz<=a or b<=hs
for a,sz,al in live: h.free(a,sz,al)
assert h.holes==[[up(base,16),total]],h.holes
print("heap modeli: 60000 islem, cakisma yok, tam geri kazanim")
