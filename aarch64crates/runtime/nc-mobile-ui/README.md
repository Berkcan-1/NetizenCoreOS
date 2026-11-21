# nc-mobile-ui

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/runtime/nc-mobile-ui` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-app-sdk` |
| **Kullananlar** | `nc-brand`, `nc-home-r3` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

ARMv8-A mobil (telefon) arayüzü için bağımsız Ring 3 UI ve tema kütüphanesi.

`nc.home`, `nc.files` ve `nc.settings` mobil uygulamalarının ortak tasarım dili, `Dp` ölçekleme, renk paleti, vektör simgeler, kart/anahtar/kaydırıcı bileşenleri ve animasyon eğrileri bu crate içindedir. x86_64 masaüstü uygulamalarından ve `nc-app-sdk`'dan tamamen bağımsızdır.

<!-- otomatik:baslangic -->
<!-- otomatik:bitis -->
