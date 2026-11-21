# _template

Yeni cihaz şablonu. Elle kopyalamak yerine: `python3 aarch64crates/tools/select_device.py --new <ad> --soc <soc> --name "<görünen ad>"`.
Şablondaki `.in` uzantılı dosyalar yer tutucuları (`__ID__`, `__CRATE__`, `__NAME__`, `__SOC__`) doldurularak üretilir.
