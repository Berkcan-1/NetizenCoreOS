# Birebir port of nc-app-browser/src/gif.rs (control flow and arithmetic identical).
def le16(d,a): return d[a]|(d[a+1]<<8)
def read_palette(d,pos,flags):
    n=1<<((flags&7)+1); end=pos+n*3
    if end>len(d): return None,pos
    return [tuple(d[pos+i*3:pos+i*3+3]) for i in range(n)], end
def skip_sub(d,pos):
    while True:
        size=d[pos]; pos+=1
        if size==0: return pos
        pos+=size
def lzw(min_code,data,out_len):
    clear=1<<min_code; eoi=clear+1; size=min_code+1
    prefix=[0]*4096; suffix=[0]*4096; nxt=eoi+1; prev=None; out=[]; buf=bits=idx=0
    while len(out)<out_len:
        while bits<size:
            if idx>=len(data): return out
            buf|=data[idx]<<bits; bits+=8; idx+=1
        code=buf&((1<<size)-1); buf>>=size; bits-=size
        if code==clear: size=min_code+1; nxt=eoi+1; prev=None; continue
        if code==eoi: break
        if prev is None:
            if code>=clear: return out
            out.append(code); prev=code; continue
        p=prev; kw=(code==nxt)
        if code>nxt: return out
        c=p if kw else code
        stack=[]
        while c>=clear: stack.append(suffix[c]); c=prefix[c]
        stack.append(c)
        first=stack[-1]
        out.extend(reversed(stack))
        if kw: out.append(first)
        if nxt<4096:
            prefix[nxt]=p; suffix[nxt]=first; nxt+=1
            if nxt==(1<<size) and size<12: size+=1
        prev=code
    return out[:out_len]
def decode(d):
    sw=le16(d,6); sh=le16(d,8); flags=d[10]; pos=13
    glob=None
    if flags&0x80: glob,pos=read_palette(d,pos,flags)
    rgba=bytearray(sw*sh*4); trans=None
    while True:
        b=d[pos]
        if b==0x21:
            label=d[pos+1]
            if label==0xF9 and d[pos+2]>=4:
                trans=d[pos+6] if d[pos+3]&1 else None
            pos+=2; pos=skip_sub(d,pos)
        elif b==0x2C:
            left=le16(d,pos+1); top=le16(d,pos+3); w=le16(d,pos+5); h=le16(d,pos+7); packed=d[pos+9]; pos+=10
            loc=None
            if packed&0x80: loc,pos=read_palette(d,pos,packed)
            pal=loc or glob; inter=packed&0x40; mc=d[pos]; pos+=1
            stream=bytearray()
            while True:
                size=d[pos]; pos+=1
                if size==0: break
                stream+=d[pos:pos+size]; pos+=size
            idxs=lzw(mc,bytes(stream),w*h)
            rows=[]
            if inter:
                for st,step in [(0,8),(4,8),(2,4),(1,2)]:
                    y=st
                    while y<h: rows.append(y); y+=step
            else: rows=list(range(h))
            for sr,y in enumerate(rows):
                for x in range(w):
                    if sr*w+x>=len(idxs): break
                    ix=idxs[sr*w+x]
                    if trans==ix: continue
                    if ix>=len(pal): continue
                    px,py=left+x,top+y
                    if px<sw and py<sh:
                        o=(py*sw+px)*4; rgba[o:o+3]=bytes(pal[ix]); rgba[o+3]=255
            return sw,sh,bytes(rgba)
        else: return None
