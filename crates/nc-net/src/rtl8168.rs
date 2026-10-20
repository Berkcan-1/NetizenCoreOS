// crates/nc-net/src/rtl8168.rs
extern crate alloc;

use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use nc_driver::dma::{DmaBuffer, DmaMask};
use nc_driver::pci::PciEndpoint;

const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 16;
const RX_BUF_SIZE: usize = 2048;
const TX_BUF_SIZE: usize = 2048;

const DESC_OWN: u32 = 1 << 31;
const DESC_EOR: u32 = 1 << 30;
const DESC_FS: u32 = 1 << 29;
const DESC_LS: u32 = 1 << 28;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct RtlDesc {
    opts1: u32,
    opts2: u32,
    addr_low: u32,
    addr_high: u32,
}

pub struct Rtl8168Driver {
    pub ep: PciEndpoint,
    pub mmio_base: usize,
    pub mac_addr: [u8; 6],
    rx_ring_dma: DmaBuffer,
    tx_ring_dma: DmaBuffer,
    rx_bufs_dma: Vec<DmaBuffer>,
    tx_bufs_dma: Vec<DmaBuffer>,
    rx_cur: usize,
    tx_cur: usize,
}

#[inline]
unsafe fn r8(base: usize, reg: usize) -> u8 {
    read_volatile((base + reg) as *const u8)
}
#[inline]
unsafe fn w8(base: usize, reg: usize, val: u8) {
    write_volatile((base + reg) as *mut u8, val);
}
#[inline]
unsafe fn w16(base: usize, reg: usize, val: u16) {
    write_volatile((base + reg) as *mut u16, val);
}
#[allow(dead_code)]
unsafe fn r32(base: usize, reg: usize) -> u32 {
    read_volatile((base + reg) as *const u32)
}
#[inline]
unsafe fn w32(base: usize, reg: usize, val: u32) {
    write_volatile((base + reg) as *mut u32, val);
}

impl Rtl8168Driver {
    pub fn try_new(mut ep: PciEndpoint) -> Option<Self> {
        // Realtek PCIe kartlarda MMIO genellikle BAR2 veya BAR0'dadır
        let mmio_base = ep
            .bar(2)
            .mmio_base()
            .or_else(|| ep.bar(0).mmio_base())
            .or_else(|| ep.bar(4).mmio_base())?;

        if mmio_base == 0 || mmio_base >= 0xFFFF_0000 {
            return None;
        }

        ep.enable_resource_decoding(true, true);

        let rx_ring_dma = DmaBuffer::allocate(
            NUM_RX_DESC * core::mem::size_of::<RtlDesc>(),
            256,
            DmaMask::Bits64,
        )
        .ok()?;
        let tx_ring_dma = DmaBuffer::allocate(
            NUM_TX_DESC * core::mem::size_of::<RtlDesc>(),
            256,
            DmaMask::Bits64,
        )
        .ok()?;

        let mut rx_bufs_dma = Vec::with_capacity(NUM_RX_DESC);
        for _ in 0..NUM_RX_DESC {
            rx_bufs_dma.push(DmaBuffer::allocate(RX_BUF_SIZE, 64, DmaMask::Bits64).ok()?);
        }

        let mut tx_bufs_dma = Vec::with_capacity(NUM_TX_DESC);
        for _ in 0..NUM_TX_DESC {
            tx_bufs_dma.push(DmaBuffer::allocate(TX_BUF_SIZE, 64, DmaMask::Bits64).ok()?);
        }

        let mut mac_addr = [0x52, 0x54, 0x00, 0x81, 0x68, 0x01];

        unsafe {
            // 1. Yazılımsal Reset (Command Register 0x37, Bit 4 = RST)
            w8(mmio_base, 0x37, 0x10);
            for _ in 0..200_000 {
                if (r8(mmio_base, 0x37) & 0x10) == 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            // 2. MAC Adresini IDR0..IDR5 (0x00..0x05) yazmaçlarından oku
            let mut valid_mac = false;
            for i in 0..6 {
                mac_addr[i] = r8(mmio_base, i);
                if mac_addr[i] != 0 && mac_addr[i] != 0xFF {
                    valid_mac = true;
                }
            }
            if !valid_mac {
                mac_addr = [0x52, 0x54, 0x00, 0x81, 0x68, 0x01];
            }

            // 3. RX ve TX Descriptor halkalarını hazırla
            let rx_descs = rx_ring_dma.virt_ptr() as *mut RtlDesc;
            for i in 0..NUM_RX_DESC {
                let paddr = rx_bufs_dma[i].phys_addr();
                let eor = if i == NUM_RX_DESC - 1 { DESC_EOR } else { 0 };
                write_volatile(
                    rx_descs.add(i),
                    RtlDesc {
                        opts1: DESC_OWN | eor | (RX_BUF_SIZE as u32),
                        opts2: 0,
                        addr_low: (paddr & 0xFFFF_FFFF) as u32,
                        addr_high: (paddr >> 32) as u32,
                    },
                );
            }

            let tx_descs = tx_ring_dma.virt_ptr() as *mut RtlDesc;
            for i in 0..NUM_TX_DESC {
                let paddr = tx_bufs_dma[i].phys_addr();
                let eor = if i == NUM_TX_DESC - 1 { DESC_EOR } else { 0 };
                write_volatile(
                    tx_descs.add(i),
                    RtlDesc {
                        opts1: eor, // CPU sahipliğinde (OWN = 0)
                        opts2: 0,
                        addr_low: (paddr & 0xFFFF_FFFF) as u32,
                        addr_high: (paddr >> 32) as u32,
                    },
                );
            }

            rx_ring_dma.sync_for_device();
            tx_ring_dma.sync_for_device();

            // 4. 93C46 Konfigürasyon kilidini aç (0x50 = 0xC0)
            w8(mmio_base, 0x50, 0xC0);

            // Kesmeleri maskele ve bekleyenleri temizle (IMR=0x3C, ISR=0x3E)
            w16(mmio_base, 0x3C, 0x0000);
            w16(mmio_base, 0x3E, 0xFFFF);

            // Maksimum RX paket boyutu (RMS = 0xDA) ve Maksimum TX boyutu (MTPS = 0xEC)
            w16(mmio_base, 0xDA, RX_BUF_SIZE as u16);
            w8(mmio_base, 0xEC, 0x3B);

            // C+ Command (0xE0): MulRW (Bit 3) aktif
            w16(mmio_base, 0xE0, 0x0008);

            // Descriptor halka fiziksel adreslerini yaz (TNPDS = 0x20, RDSAR = 0xE4)
            let tx_phys = tx_ring_dma.phys_addr();
            let rx_phys = rx_ring_dma.phys_addr();
            w32(mmio_base, 0x20, (tx_phys & 0xFFFF_FFFF) as u32);
            w32(mmio_base, 0x24, (tx_phys >> 32) as u32);
            w32(mmio_base, 0xE4, (rx_phys & 0xFFFF_FFFF) as u32);
            w32(mmio_base, 0xE8, (rx_phys >> 32) as u32);

            // RCR (0x44): Broadcast(AB) + Multicast(AM) + Unicast(APM) + Sınırsız DMA burst
            w32(mmio_base, 0x44, 0x0000_E70E);
            // TCR (0x40): InterFrameGap + Max DMA burst
            w32(mmio_base, 0x40, 0x0300_0700);

            // Konfigürasyon kilidini kapat (0x50 = 0x00)
            w8(mmio_base, 0x50, 0x00);

            // DMA halkaları hazır, şimdi Bus Mastering ve TX/RX motorunu aç
            ep.enable_bus_mastering();
            w8(mmio_base, 0x37, 0x0C); // TE (Bit 2) | RE (Bit 3)
        }

        Some(Self {
            ep,
            mmio_base,
            mac_addr,
            rx_ring_dma,
            tx_ring_dma,
            rx_bufs_dma,
            tx_bufs_dma,
            rx_cur: 0,
            tx_cur: 0,
        })
    }

    /// PHYstatus (0x6C) yazmacının Bit 1 (LinkSts) değerinden gerçek kablo bağlantısını okur
    #[inline]
    pub fn is_link_up(&self) -> bool {
        unsafe { (r8(self.mmio_base, 0x6C) & 0x02) != 0 }
    }

    pub fn send_packet(&mut self, data: &[u8]) -> Result<(), ()> {
        if data.is_empty() || data.len() > TX_BUF_SIZE {
            return Err(());
        }
        let idx = self.tx_cur;
        let tx_descs = self.tx_ring_dma.virt_ptr() as *mut RtlDesc;

        unsafe {
            self.tx_ring_dma.sync_for_cpu();
            let cur_desc = read_volatile(tx_descs.add(idx));
            if (cur_desc.opts1 & DESC_OWN) != 0 {
                // Donanım bu slotu henüz bitirmedi
                return Err(());
            }

            self.tx_bufs_dma[idx].as_mut_slice()[..data.len()].copy_from_slice(data);
            self.tx_bufs_dma[idx].sync_for_device();

            let eor = if idx == NUM_TX_DESC - 1 { DESC_EOR } else { 0 };
            let paddr = self.tx_bufs_dma[idx].phys_addr();
            write_volatile(
                tx_descs.add(idx),
                RtlDesc {
                    opts1: DESC_OWN | eor | DESC_FS | DESC_LS | (data.len() as u32),
                    opts2: 0,
                    addr_low: (paddr & 0xFFFF_FFFF) as u32,
                    addr_high: (paddr >> 32) as u32,
                },
            );
            self.tx_ring_dma.sync_for_device();

            self.tx_cur = (idx + 1) % NUM_TX_DESC;
            // TPPOLL (0x38): Normal Priority Queue Polling (Bit 6 = NPQ)
            w8(self.mmio_base, 0x38, 0x40);
        }
        Ok(())
    }

    pub fn poll_rx(&mut self) -> Option<Vec<u8>> {
        let idx = self.rx_cur;
        let rx_descs = self.rx_ring_dma.virt_ptr() as *mut RtlDesc;

        unsafe {
            self.rx_ring_dma.sync_for_cpu();
            let desc = read_volatile(rx_descs.add(idx));
            if (desc.opts1 & DESC_OWN) != 0 {
                return None;
            }

            let frame_len = (desc.opts1 & 0x3FFF) as usize;
            let clean_len = frame_len.saturating_sub(4).min(RX_BUF_SIZE);
            self.rx_bufs_dma[idx].sync_for_cpu();

            let packet = if clean_len > 0 {
                Some(self.rx_bufs_dma[idx].as_slice()[..clean_len].to_vec())
            } else {
                None
            };

            let eor = if idx == NUM_RX_DESC - 1 { DESC_EOR } else { 0 };
            let paddr = self.rx_bufs_dma[idx].phys_addr();
            write_volatile(
                rx_descs.add(idx),
                RtlDesc {
                    opts1: DESC_OWN | eor | (RX_BUF_SIZE as u32),
                    opts2: 0,
                    addr_low: (paddr & 0xFFFF_FFFF) as u32,
                    addr_high: (paddr >> 32) as u32,
                },
            );
            self.rx_ring_dma.sync_for_device();
            self.rx_cur = (idx + 1) % NUM_RX_DESC;

            // ISR temizle
            w16(self.mmio_base, 0x3E, 0x0005);
            packet
        }
    }
}
