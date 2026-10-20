import sys
import build, preview
from PIL import Image

ent, notdef, pairs = build.assemble()
font = build.font_dict(ent, notdef, pairs)


def labels(cap, scale):
    bg = (60, 120, 200)
    lines = ["Dosyalar", "BROWSER", "Ayarlar", "SYSTEM", "STOPWATCH", "Hesap Makinesi", "masaüstü_not.txt"]
    return preview.render_lines(font, lines, cap, scale=scale, bg=bg, fg=(255, 255, 255))


def samples(cap, scale):
    lines = ["Pijamalı hasta yağız şoföre çabucak güvendi.", "Zażółć gęślą jaźń · Příliš žluťoučký kůň · Åse Øre Ærlig",
             "Съешь же ещё этих мягких французских булок", "Ξεσκεπάζω την ψυχοφθόρα βδελυγμία · ΐ ΰ Ά Ή",
             "₺1.250,75  €9,99  ₽300  ½ ¾  © ® ™  → ✓"]
    return preview.render_lines(font, lines, cap, scale=scale)


if __name__ == '__main__':
    imgs = [labels(9, 4), samples(11, 3), samples(18, 2)]
    W = max(i.width for i in imgs)
    H = sum(i.height for i in imgs)
    out = Image.new('RGB', (W, H), 'white')
    y = 0
    for i in imgs:
        out.paste(i, (0, y)); y += i.height
    out.save(sys.argv[1] if len(sys.argv) > 1 else 'sheet.png')
