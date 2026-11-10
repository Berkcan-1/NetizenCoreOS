import io, random
from PIL import Image
import gif_port as port
random.seed(7)
def gifbytes(img, **kw):
    b=io.BytesIO(); img.save(b,format='GIF',**kw); return b.getvalue()
def ref(data):
    im=Image.open(io.BytesIO(data)); im.seek(0); return im.convert('RGBA')
cases=[]
# 1 gradient 256 colors
im=Image.new('RGB',(64,64))
for y in range(64):
    for x in range(64): im.putpixel((x,y),(x*4,y*4,(x*y)%256))
cases.append(('gradient',gifbytes(im.convert('P',palette=Image.ADAPTIVE,colors=256))))
# 2 noise (forces dictionary growth/reset past 4096 codes)
im=Image.new('RGB',(150,150))
for y in range(150):
    for x in range(150): im.putpixel((x,y),(random.randrange(256),random.randrange(256),random.randrange(256)))
cases.append(('noise',gifbytes(im.convert('P',palette=Image.ADAPTIVE,colors=256))))
# 3 two color
im=Image.new('P',(16,16)); im.putpalette([0,0,0,255,255,255]+[0]*762)
for y in range(16):
    for x in range(16): im.putpixel((x,y),(x+y)%2)
cases.append(('checker',gifbytes(im)))
# 4 transparent
im=Image.new('RGBA',(32,32),(0,0,0,0))
for y in range(8,24):
    for x in range(8,24): im.putpixel((x,y),(255,0,0,255))
cases.append(('transparent',gifbytes(im)))
# 5 interlaced
im=Image.new('RGB',(40,37))
for y in range(37):
    for x in range(40): im.putpixel((x,y),((x*6)%256,(y*7)%256,128))
cases.append(('interlaced',gifbytes(im.convert('P',palette=Image.ADAPTIVE,colors=64),interlace=True)))
# 6 flat color, long runs (KwKwK heavy)
im=Image.new('P',(100,100),3); im.putpalette(list(range(0,256,1))*3)
cases.append(('flat',gifbytes(im)))
# 7 animated (first frame only)
f1=Image.new('RGB',(20,20),(255,0,0)); f2=Image.new('RGB',(20,20),(0,255,0))
b=io.BytesIO(); f1.save(b,format='GIF',save_all=True,append_images=[f2],duration=100,loop=0)
cases.append(('animated',b.getvalue()))
bad=0
for name,data in cases:
    sw,sh,rgba=port.decode(data)
    r=ref(data)
    want=r.tobytes()
    # compare only opaque pixels' RGB + alpha mask
    same=True
    for i in range(0,len(want),4):
        if want[i+3]==0 and rgba[i+3]==0: continue
        if want[i:i+4]!=rgba[i:i+4]: same=False; break
    print(name,(sw,sh),len(data),'bytes','OK' if same else 'MISMATCH')
    bad+= (not same)
print('failures:',bad)
