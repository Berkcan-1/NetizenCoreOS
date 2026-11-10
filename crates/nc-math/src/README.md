# nc-math/src

## lib.rs

SIN_TABLE: 0..90 derece, 5'er derece adimli, binde bir degerler (19 eleman). sin_milli degeri ceyreklere katlar,
cos_milli sin_milli(derece + 90)'dir. Negatif ve 360 ustu acilar rem_euclid ile sarilir.
integer_sqrt: baslangic tahmini bit uzunlugundan alinir, Newton adimlari azalan diziyle durur.
smoothstep256: (t*t*(768 - 2t)) >> 16. ratio256: toplam 0 ise 256 doner.
