# nc-gfx

Bellekte piksel tamponu uzerinde cizim, karistirma ve ekrana aktarim. Bagimlilik: nc-hal (FramebufferInfo).

## Kavramlar

- Piksel bicimi: u32, 0x00RRGGBB. Framebuffer'a yazarken present gerektiginde kanal konumlarina gore donusturur.
- Canvas: bir u32 dilimi uzerinde genislik*yukseklik gorunum. Bellegi cagiran saglar (nc-mem::Arena).
- Murekkep katmani: siyah zemin uzerine, renkleri tersine cevrilmis (invert) cizilmis Canvas; siyah = bos, parlak = murekkep.
  compose_ink bunu "multiply" karistirmasiyla bir zemine uygular; boylece bulaniklastirma ve solma tek bir islemle
  yapilir. Acik zeminde "screen" ise hicbir sey gostermez, bu yuzden murekkep yontemi kullanilir.
- Rect: x, y, genislik, yukseklik.

## Genel API

- rgb, invert, lerp_color
- Canvas: new, width, height, pixels, pixels_mut, fill, vertical_gradient, add_glow, blend_glow, fill_rect, blend_pixel,
  draw_rgba, copy_rect_from
- present(canvas, framebuffer), present_region(canvas, framebuffer, rect) (unsafe)
- blur::box_blur(kaynak, hedef, gecici, yaricap)
- compose::compose_ink, compose::lerp_canvases, compose::lerp_packed

## Performans notlari

- Kayan nokta yok. Karistirma islemleri 0..256 sabit noktadir; lerp_packed R-B ve G kanallarini ayri maskeleyerek
  uc kanali iki carpmada karistirir.
- vertical_gradient ve blend_glow 4x4 sirali dithering uygular; acik zeminlerdeki ince gecislerde bant olusmaz.
- Framebuffer onbelleksiz bellektir; present_region sadece degisen alani yazar. Bunu kullan.
