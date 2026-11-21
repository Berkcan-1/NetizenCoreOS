# nc-driver

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/hardware/nc-driver` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-arch`, `nc-efra`, `nc-hal` |
| **Kullananlar** | `nc-input`, `nc-kernel`, `nc-net`, `nc-setup`, `nc-storage`, `nc-sys-facade` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Aygıt sürücü çerçevesi. `no_std`.

| Dosya | İçerik |
|---|---|
| `lib.rs` | `DriverRegistry`, `HardwareDeviceNode`; `init`, `init_with_ecam`, `global`, `find_pci_by_class`, `optimize_and_scan(_ecam)`, `register_or_update_package`, `rebind_all`, `upgrade_device_driver`; PAT write-combining ayarı |
| `pci.rs` | PCI yapılandırma uzayı: `PciLocation`, `PciEndpoint`, `PciBar`, `DeviceLifecycleState`, okuma/yazma ve kaynak çözme yardımcıları |
| `dma.rs` | `DmaBuffer`, `DmaMask`, `DmaError`; sanal → fiziksel dönüşümler |
| `ps2.rs` | `Ps2Input` |
| `ac97.rs` | `Ac97Output` (ses çıkışı) |

EFRA sürücü paketlerini (`nc-efra`) kayıt defterine işleyip aygıtlara yeniden bağlar. Giriş aygıtları `nc-input`'ta, disk
aygıtları `nc-storage`'ta, ağ kartı `nc-net`'tedir.
