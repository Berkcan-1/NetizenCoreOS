// crates/nc-storage/src/ahci.rs
extern crate alloc;

use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};
use nc_driver::dma::{DmaBuffer, DmaMask};
use nc_driver::pci::{enumerate_pci_bus, PciEndpoint};
use nc_hal::{BlockDeviceInfo, DeviceType};

const AHCI_GHC_AE: u32 = 1 << 31;
const PORT_CMD_ST: u32 = 1 << 0;
const PORT_CMD_FRE: u32 = 1 << 4;
const PORT_CMD_FR: u32 = 1 << 14;
const PORT_CMD_CR: u32 = 1 << 15;
const PORT_TFD_BSY: u32 = 0x80;
const PORT_TFD_DRQ: u32 = 0x08;
const PORT_IS_TFES: u32 = 1 << 30;

const SATA_SIG_ATA: u32 = 0x0000_0101;
const SATA_SIG_ATAPI: u32 = 0xEB14_0101;

const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
const ATA_CMD_FLUSH_EXT: u8 = 0xEA;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct HbaCmdHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct HbaPrdtEntry {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc_and_intr: u32,
}

#[repr(C, packed)]
struct HbaCmdTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    reserved: [u8; 48],
    prdt: [HbaPrdtEntry; 1],
}

struct AhciPortRuntime {
    port_mmio: usize,
    info: BlockDeviceInfo,
    cmd_list_dma: DmaBuffer,
    _fis_dma: DmaBuffer, 
    cmd_table_dma: DmaBuffer,
    io_bounce_dma: DmaBuffer,
}

static AHCI_LOCK: AtomicBool = AtomicBool::new(false);
static mut AHCI_PORTS: Option<Vec<AhciPortRuntime>> = None;
static AHCI_INIT_DONE: AtomicBool = AtomicBool::new(false);

struct AhciGuard;
impl AhciGuard {
    fn acquire() -> Result<Self, ()> {
        let mut spins = 0u32;
        while AHCI_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spins += 1;
            if spins > 2_000_000 {
                return Err(());
            }
            core::hint::spin_loop();
        }
        Ok(AhciGuard)
    }
}
impl Drop for AhciGuard {
    fn drop(&mut self) {
        AHCI_LOCK.store(false, Ordering::Release);
    }
}

#[inline]
unsafe fn mmio_read32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

#[inline]
unsafe fn mmio_write32(addr: usize, val: u32) {
    write_volatile(addr as *mut u32, val);
}

unsafe fn stop_port(port_mmio: usize) -> bool {
    let mut cmd = mmio_read32(port_mmio + 0x18);
    cmd &= !(PORT_CMD_ST | PORT_CMD_FRE);
    mmio_write32(port_mmio + 0x18, cmd);

    for _ in 0..200_000 {
        let c = mmio_read32(port_mmio + 0x18);
        if (c & (PORT_CMD_FR | PORT_CMD_CR)) == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn start_port(port_mmio: usize) {
    for _ in 0..100_000 {
        if (mmio_read32(port_mmio + 0x18) & PORT_CMD_CR) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
    let cmd = mmio_read32(port_mmio + 0x18) | PORT_CMD_FRE | PORT_CMD_ST;
    mmio_write32(port_mmio + 0x18, cmd);
}

unsafe fn issue_port_cmd(
    rt: &mut AhciPortRuntime,
    ata_cmd: u8,
    lba: u64,
    sector_count: u16,
    is_write: bool,
) -> Result<(), ()> {
    let port_mmio = rt.port_mmio;

    // Bekleyen kesme durumlarını temizle
    mmio_write32(port_mmio + 0x10, 0xFFFF_FFFF);

    let mut spins = 0u32;
    while (mmio_read32(port_mmio + 0x20) & (PORT_TFD_BSY | PORT_TFD_DRQ)) != 0 {
        spins += 1;
        if spins > 500_000 {
            return Err(());
        }
        core::hint::spin_loop();
    }

    let hdr_ptr = rt.cmd_list_dma.virt_ptr() as *mut HbaCmdHeader;
    let tbl_ptr = rt.cmd_table_dma.virt_ptr() as *mut HbaCmdTable;
    core::ptr::write_bytes(tbl_ptr as *mut u8, 0, core::mem::size_of::<HbaCmdTable>());

    let cfl: u16 = 5; // 20 bayt H2D FIS = 5 DWORD
    let w_bit: u16 = if is_write { 1 << 6 } else { 0 };
    let prdtl: u16 = if sector_count > 0 { 1 } else { 0 };

    let tbl_phys = rt.cmd_table_dma.phys_addr();
    (*hdr_ptr).flags = cfl | w_bit;
    (*hdr_ptr).prdtl = prdtl;
    (*hdr_ptr).prdbc = 0;
    (*hdr_ptr).ctba = (tbl_phys & 0xFFFF_FFFF) as u32;
    (*hdr_ptr).ctbau = (tbl_phys >> 32) as u32;

    if sector_count > 0 {
        let data_phys = rt.io_bounce_dma.phys_addr();
        let byte_count = (sector_count as u32) * 512;
        (*tbl_ptr).prdt[0] = HbaPrdtEntry {
            dba: (data_phys & 0xFFFF_FFFF) as u32,
            dbau: (data_phys >> 32) as u32,
            reserved: 0,
            dbc_and_intr: (byte_count - 1) & 0x003F_FFFF,
        };
    }

    // Host-to-Device (H2D) Register FIS (Type 0x27)
    let cfis = &mut (*tbl_ptr).cfis;
    cfis[0] = 0x27;
    cfis[1] = 0x80; // Command bit
    cfis[2] = ata_cmd;
    cfis[3] = 0;

    cfis[4] = (lba & 0xFF) as u8;
    cfis[5] = ((lba >> 8) & 0xFF) as u8;
    cfis[6] = ((lba >> 16) & 0xFF) as u8;
    cfis[7] = 1 << 6; // LBA mode (0x40)

    cfis[8] = ((lba >> 24) & 0xFF) as u8;
    cfis[9] = ((lba >> 32) & 0xFF) as u8;
    cfis[10] = ((lba >> 40) & 0xFF) as u8;
    cfis[11] = 0;

    cfis[12] = (sector_count & 0xFF) as u8;
    cfis[13] = ((sector_count >> 8) & 0xFF) as u8;

    rt.cmd_list_dma.sync_for_device();
    rt.cmd_table_dma.sync_for_device();
    rt.io_bounce_dma.sync_for_device();

    // Slot 0'ı tetikle
    mmio_write32(port_mmio + 0x38, 1);

    let mut wait = 0u32;
    loop {
        let ci = mmio_read32(port_mmio + 0x38);
        let is = mmio_read32(port_mmio + 0x10);
        if (is & PORT_IS_TFES) != 0 {
            return Err(());
        }
        if (ci & 1) == 0 {
            break;
        }
        wait += 1;
        if wait > 2_000_000 {
            return Err(());
        }
        core::hint::spin_loop();
    }

    if (mmio_read32(port_mmio + 0x20) & 0x01) != 0 {
        return Err(());
    }

    rt.io_bounce_dma.sync_for_cpu();
    Ok(())
}

unsafe fn probe_ahci_controller(mut ep: PciEndpoint, out_ports: &mut Vec<AhciPortRuntime>) {
    // ABAR = BAR5 (Index 5)
    let Some(abar) = ep.bar(5).mmio_base() else {
        return;
    };
    // MMIO adresinin 4 GiB kimlik eşlemesi içinde olduğunu doğrula
    if abar == 0 || abar >= 0xFFFF_0000 {
        return;
    }

    ep.enable_resource_decoding(true, false);

    let cap = mmio_read32(abar + 0x00);
    let s64a = (cap & (1 << 31)) != 0;
    let dma_mask = if s64a { DmaMask::Bits64 } else { DmaMask::Bits32 };

    // AHCI Enable (GHC.AE)
    let ghc = mmio_read32(abar + 0x04);
    mmio_write32(abar + 0x04, ghc | AHCI_GHC_AE);

    let pi = mmio_read32(abar + 0x0C);

    for port_idx in 0..32u8 {
        if (pi & (1 << port_idx)) == 0 || out_ports.len() >= 4 {
            continue;
        }

        let port_mmio = abar + 0x100 + (port_idx as usize) * 0x80;
        let ssts = mmio_read32(port_mmio + 0x28);
        let det = ssts & 0x0F;
        let ipm = (ssts >> 8) & 0x0F;
        if det != 3 || ipm != 1 {
            continue;
        }

        let sig = mmio_read32(port_mmio + 0x24);
        if sig != SATA_SIG_ATA && sig != SATA_SIG_ATAPI {
            continue;
        }
        let is_atapi = sig == SATA_SIG_ATAPI;

        if !stop_port(port_mmio) {
            continue;
        }

        let Ok(cmd_list_dma) = DmaBuffer::allocate(1024, 1024, dma_mask) else {
            continue;
        };
        let Ok(fis_dma) = DmaBuffer::allocate(256, 256, dma_mask) else {
            continue;
        };
        let Ok(cmd_table_dma) = DmaBuffer::allocate(256, 128, dma_mask) else {
            continue;
        };
        let Ok(io_bounce_dma) = DmaBuffer::allocate(4096, 4096, dma_mask) else {
            continue;
        };

        let clb = cmd_list_dma.phys_addr();
        let fb = fis_dma.phys_addr();
        mmio_write32(port_mmio + 0x00, (clb & 0xFFFF_FFFF) as u32);
        mmio_write32(port_mmio + 0x04, (clb >> 32) as u32);
        mmio_write32(port_mmio + 0x08, (fb & 0xFFFF_FFFF) as u32);
        mmio_write32(port_mmio + 0x0C, (fb >> 32) as u32);

        // SERR ve IS temizle, Bus Mastering aç ve portu başlat
        mmio_write32(port_mmio + 0x30, 0xFFFF_FFFF);
        mmio_write32(port_mmio + 0x10, 0xFFFF_FFFF);
        ep.enable_bus_mastering();
        start_port(port_mmio);

        let channel_id = out_ports.len() as u8;
        let mut rt = AhciPortRuntime {
            port_mmio,
            info: BlockDeviceInfo::EMPTY,
            cmd_list_dma,
            _fis_dma: fis_dma, // Burayı güncelledik
            cmd_table_dma,
            io_bounce_dma,
        };

        if is_atapi {
            let mut model = [b' '; 40];
            model[..14].copy_from_slice(b"AHCI ATAPI ROM");
            rt.info = BlockDeviceInfo {
                channel: channel_id,
                model,
                model_len: 14,
                block_count: 0,
                block_size: 2048,
                device_type: DeviceType::OpticalDisc,
            };
            out_ports.push(rt);
            continue;
        }

        // ATA IDENTIFY (0xEC) komutu ile gerçek 64-bit LBA48 kapasitesini oku
        if issue_port_cmd(&mut rt, ATA_CMD_IDENTIFY, 0, 1, false).is_ok() {
            let id_bytes = rt.io_bounce_dma.as_slice();
            let mut words = [0u16; 256];
            for i in 0..256 {
                words[i] = u16::from_le_bytes([id_bytes[i * 2], id_bytes[i * 2 + 1]]);
            }

            let mut model = [0u8; 40];
            for index in 0..20 {
                let w = words[27 + index];
                model[index * 2] = (w >> 8) as u8;
                model[index * 2 + 1] = (w & 0xFF) as u8;
            }
            let mut model_len = 40;
            while model_len > 0 && (model[model_len - 1] == b' ' || model[model_len - 1] == 0) {
                model_len -= 1;
            }

            let lba48 = (words[83] & (1 << 10)) != 0;
            let sectors = if lba48 {
                (words[100] as u64)
                    | ((words[101] as u64) << 16)
                    | ((words[102] as u64) << 32)
                    | ((words[103] as u64) << 48)
            } else {
                (words[60] as u64) | ((words[61] as u64) << 16)
            };

            if sectors > 0 {
                rt.info = BlockDeviceInfo {
                    channel: channel_id,
                    model,
                    model_len,
                    block_count: sectors, // 128 GiB kırpması YOK! Tam kapasite.
                    block_size: 512,
                    device_type: DeviceType::HardDisk,
                };
                out_ports.push(rt);
            }
        }
    }
}

fn ensure_initialized() {
    if AHCI_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    let Ok(_guard) = AhciGuard::acquire() else {
        return;
    };
    if AHCI_INIT_DONE.load(Ordering::Relaxed) {
        return;
    }

    let mut ports = Vec::new();
    for ep in enumerate_pci_bus() {
        // PCI Class 01 (Storage), Subclass 06 (SATA AHCI)
        if ep.class_code == 0x01 && ep.subclass == 0x06 {
            unsafe {
                probe_ahci_controller(ep, &mut ports);
            }
        }
    }

    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(AHCI_PORTS), Some(ports));
    }
    AHCI_INIT_DONE.store(true, Ordering::Release);
}

pub fn probe_port(channel: u8) -> Option<BlockDeviceInfo> {
    ensure_initialized();
    let _guard = AhciGuard::acquire().ok()?;
    unsafe {
        let list = (*core::ptr::addr_of!(AHCI_PORTS)).as_ref()?;
        list.get(channel as usize).map(|p| p.info)
    }
}

pub fn read_sector(channel: u8, lba: u64, buf: &mut [u8]) -> Result<(), ()> {
    if buf.len() < 512 {
        return Err(());
    }
    ensure_initialized();
    let _guard = AhciGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(AHCI_PORTS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        if lba >= rt.info.block_count {
            return Err(());
        }
        issue_port_cmd(rt, ATA_CMD_READ_DMA_EXT, lba, 1, false)?;
        buf[..512].copy_from_slice(&rt.io_bounce_dma.as_slice()[..512]);
        Ok(())
    }
}

pub fn write_sector(channel: u8, lba: u64, buf: &[u8]) -> Result<(), ()> {
    if buf.len() < 512 {
        return Err(());
    }
    ensure_initialized();
    let _guard = AhciGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(AHCI_PORTS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        if lba >= rt.info.block_count {
            return Err(());
        }
        rt.io_bounce_dma.as_mut_slice()[..512].copy_from_slice(&buf[..512]);
        issue_port_cmd(rt, ATA_CMD_WRITE_DMA_EXT, lba, 1, true)
    }
}

pub fn flush_port(channel: u8) -> Result<(), ()> {
    ensure_initialized();
    let _guard = AhciGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(AHCI_PORTS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        issue_port_cmd(rt, ATA_CMD_FLUSH_EXT, 0, 0, false)
    }
}
