# nc-ui

Tema ve ortak arayuz cizim yardimcilari. Bagimlilik: nc-gfx, nc-font, nc-math. Splash, kurulum ekrani, masaustu ve
uygulamalar ayni gorunumu buradan alir.

## Genel API

- Renkler: BACKDROP_TOP, BACKDROP_BOTTOM; ACCENT_LIGHT, ACCENT_LINE, ACCENT_DARK, ACCENT_BG; AQUA_LIGHT, AQUA, AQUA_DARK;
  SUCCESS, SUCCESS_BG; TEXT_PRIMARY, TEXT_MUTED, TEXT_DIM; PANEL_FILL, PANEL_BORDER, SHADOW, RAIL, RAIL_DOT, WHITE
- text_style(cap, iz_orani_yuzde) -> Style
- draw_backdrop(canvas): acik gri dikey gradyan
- draw_text_color, draw_text_gradient, draw_fading_line
- draw_dot: duz, kenari yumusak daire (yaricap piksel; strength 0..256 saydamlik)
- draw_panel: yuvarlatilmis dikdortgen (dolgu ve 1 piksellik kenar; alfalar 0..255)
- draw_shadow, draw_card: yumusak golge; kart = golge + beyaz panel
- draw_badge, draw_radio, draw_button (birincil/ikincil), draw_accent_button (verilen vurgu rengiyle)
- draw_cursor, CURSOR_WIDTH, CURSOR_HEIGHT: ok imleci ve boyutu (cekirdek imlec bolgesini bununla gunceller)

## Notlar

- draw_panel ve draw_shadow derin ic kisimlari duz doldurarak hizlandirilmistir; kenar ve kose pikselleri eskisi gibi
  mesafe alanindan hesaplanir. Cikti eski surumle piksel piksel aynidir (kurulum sihirbazi uzerinde dogrulandi).
- draw_shadow kutunun icine (koseler haric derin ic kisim) golge cizmez. Yalnizca opak bir panelin arkasinda kullan
  (draw_card ve pencereler boyle kullanir). Yari saydam panelin arkasinda golge iceride eksik gorunur.
