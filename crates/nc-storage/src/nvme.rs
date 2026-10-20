// crates/nc-storage/src/nvme.rs
extern crate alloc;

use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};
use nc_driver::dma::{DmaBuffer, DmaMask};
use nc_driver::pci::{enumerate_pci_bus, PciEndpoint};
use nc_hal::{BlockDeviceInfo, DeviceType};

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct NvmeCommand {
    opcode: u8,
    flags: u8,
    cid: u16,
    nsid: u32,
    cdw2: u32,
    cdw3: u32,
    mptr: u64,
    prp1: u64,
    prp2: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct NvmeCompletion {
    result: u32,
    rsvd: u32,
    sq_head: u16,
    sq_id: u16,
    cid: u16,
    status: u16,
}

struct NvmeControllerRuntime {
    mmio_base: usize,
    db_stride: usize,
    nsid: u32,
    lba_shift: u8,
    info: BlockDeviceInfo,
    asq_dma: DmaBuffer,
    acq_dma: DmaBuffer,
    iosq_dma: DmaBuffer,
    iocq_dma: DmaBuffer,
    bounce_dma: DmaBuffer,
    admin_tail: u16,
    admin_head: u16,
    admin_phase: u16,
    io_tail: u16,
    io_head: u16,
    io_phase: u16,
    next_cid: u16,
}

const QUEUE_DEPTH: u16 = 16;

static NVME_LOCK: AtomicBool = AtomicBool::new(false);
static mut NVME_DEVS: Option<Vec<NvmeControllerRuntime>> = None;
static NVME_INIT_DONE: AtomicBool = AtomicBool::new(false);

struct NvmeGuard;
impl NvmeGuard {
    fn acquire() -> Result<Self, ()> {
        let mut spins = 0u32;
        while NVME_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spins += 1;
            if spins > 2_000_000 {
                return Err(());
            }
            core::hint::spin_loop();
        }
        Ok(NvmeGuard)
    }
}
impl Drop for NvmeGuard {
    fn drop(&mut self) {
        NVME_LOCK.store(false, Ordering::Release);
    }
}

#[inline]
unsafe fn mmio_r32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}
#[inline]
unsafe fn mmio_w32(addr: usize, val: u32) {
    write_volatile(addr as *mut u32, val);
}
#[inline]
unsafe fn mmio_r64(addr: usize) -> u64 {
    let low = mmio_r32(addr) as u64;
    let high = mmio_r32(addr + 4) as u64;
    (high << 32) | low
}
#[inline]
unsafe fn mmio_w64(addr: usize, val: u64) {
    mmio_w32(addr, (val & 0xFFFF_FFFF) as u32);
    mmio_w32(addr + 4, (val >> 32) as u32);
}

impl NvmeControllerRuntime {
    unsafe fn submit_admin_cmd(&mut self, mut cmd: NvmeCommand) -> Result<u32, ()> {
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        cmd.cid = self.next_cid;

        let sq_ptr = self.asq_dma.virt_ptr() as *mut NvmeCommand;
        write_volatile(sq_ptr.add(self.admin_tail as usize), cmd);
        self.asq_dma.sync_for_device();

        self.admin_tail = (self.admin_tail + 1) % QUEUE_DEPTH;
        // Admin SQ Tail Doorbell (Index 0)
        mmio_w32(self.mmio_base + 0x1000, self.admin_tail as u32);

        let cq_ptr = self.acq_dma.virt_ptr() as *const NvmeCompletion;
        let mut spins = 0u32;
        loop {
            self.acq_dma.sync_for_cpu();
            let cqe = read_volatile(cq_ptr.add(self.admin_head as usize));
            let status = cqe.status;
            if (status & 1) == self.admin_phase {
                self.admin_head += 1;
                if self.admin_head >= QUEUE_DEPTH {
                    self.admin_head = 0;
                    self.admin_phase ^= 1;
                }
                // Admin CQ Head Doorbell (Index 1)
                mmio_w32(
                    self.mmio_base + 0x1000 + self.db_stride,
                    self.admin_head as u32,
                );
                if (status >> 1) != 0 {
                    return Err(());
                }
                return Ok(cqe.result);
            }
            spins += 1;
            if spins > 2_000_000 {
                return Err(());
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn submit_io_cmd(&mut self, mut cmd: NvmeCommand) -> Result<(), ()> {
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        cmd.cid = self.next_cid;

        let sq_ptr = self.iosq_dma.virt_ptr() as *mut NvmeCommand;
        write_volatile(sq_ptr.add(self.io_tail as usize), cmd);
        self.iosq_dma.sync_for_device();

        self.io_tail = (self.io_tail + 1) % QUEUE_DEPTH;
        // IO Queue 1 SQ Tail Doorbell (Index 2)
        mmio_w32(
            self.mmio_base + 0x1000 + 2 * self.db_stride,
            self.io_tail as u32,
        );

        let cq_ptr = self.iocq_dma.virt_ptr() as *const NvmeCompletion;
        let mut spins = 0u32;
        loop {
            self.iocq_dma.sync_for_cpu();
            let cqe = read_volatile(cq_ptr.add(self.io_head as usize));
            let status = cqe.status;
            if (status & 1) == self.io_phase {
                self.io_head += 1;
                if self.io_head >= QUEUE_DEPTH {
                    self.io_head = 0;
                    self.io_phase ^= 1;
                }
                // IO Queue 1 CQ Head Doorbell (Index 3)
                mmio_w32(
                    self.mmio_base + 0x1000 + 3 * self.db_stride,
                    self.io_head as u32,
                );
                if (status >> 1) != 0 {
                    return Err(());
                }
                return Ok(());
            }
            spins += 1;
            if spins > 2_000_000 {
                return Err(());
            }
            core::hint::spin_loop();
        }
    }
}

unsafe fn probe_nvme_controller(mut ep: PciEndpoint, out: &mut Vec<NvmeControllerRuntime>) {
    let Some(bar0) = ep.bar(0).mmio_base() else {
        return;
    };
    if bar0 == 0 || bar0 >= 0xFFFF_0000 {
        return;
    }

    ep.enable_resource_decoding(true, false);

    let cap = mmio_r64(bar0 + 0x00);
    let dstrd = ((cap >> 32) & 0x0F) as usize;
    let db_stride = 4usize << dstrd;

    // Controller Disable (CC.EN = 0)
    let mut cc = mmio_r32(bar0 + 0x14);
    if (cc & 1) != 0 {
        mmio_w32(bar0 + 0x14, cc & !1);
    }
    for _ in 0..1_000_000 {
        if (mmio_r32(bar0 + 0x1C) & 1) == 0 {
            break;
        }
        core::hint::spin_loop();
    }

    let Ok(asq_dma) = DmaBuffer::allocate(4096, 4096, DmaMask::Bits64) else { return };
    let Ok(acq_dma) = DmaBuffer::allocate(4096, 4096, DmaMask::Bits64) else { return };
    let Ok(iosq_dma) = DmaBuffer::allocate(4096, 4096, DmaMask::Bits64) else { return };
    let Ok(iocq_dma) = DmaBuffer::allocate(4096, 4096, DmaMask::Bits64) else { return };
    let Ok(bounce_dma) = DmaBuffer::allocate(4096, 4096, DmaMask::Bits64) else { return };

    // AQA, ASQ, ACQ ayarla
    let aqa = (((QUEUE_DEPTH - 1) as u32) << 16) | ((QUEUE_DEPTH - 1) as u32);
    mmio_w32(bar0 + 0x24, aqa);
    mmio_w64(bar0 + 0x28, asq_dma.phys_addr());
    mmio_w64(bar0 + 0x30, acq_dma.phys_addr());

    ep.enable_bus_mastering();

    // CC: IOSQES=6 (64B), IOCQES=4 (16B), EN=1
    cc = (6 << 16) | (4 << 20) | 1;
    mmio_w32(bar0 + 0x14, cc);

    let mut ready = false;
    for _ in 0..1_000_000 {
        let csts = mmio_r32(bar0 + 0x1C);
        if (csts & 2) != 0 {
            return; // CFS (Fatal)
        }
        if (csts & 1) != 0 {
            ready = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !ready {
        return;
    }

    let mut rt = NvmeControllerRuntime {
        mmio_base: bar0,
        db_stride,
        nsid: 1,
        lba_shift: 9,
        info: BlockDeviceInfo::EMPTY,
        asq_dma,
        acq_dma,
        iosq_dma,
        iocq_dma,
        bounce_dma,
        admin_tail: 0,
        admin_head: 0,
        admin_phase: 1,
        io_tail: 0,
        io_head: 0,
        io_phase: 1,
        next_cid: 1,
    };

    // 1. Identify Controller (CNS = 1)
    let id_ctrl_cmd = NvmeCommand {
        opcode: 0x06,
        flags: 0,
        cid: 0,
        nsid: 0,
        cdw2: 0,
        cdw3: 0,
        mptr: 0,
        prp1: rt.bounce_dma.phys_addr(),
        prp2: 0,
        cdw10: 1,
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };
    if rt.submit_admin_cmd(id_ctrl_cmd).is_err() {
        return;
    }
    rt.bounce_dma.sync_for_cpu();
    let mut model = [0u8; 40];
    model.copy_from_slice(&rt.bounce_dma.as_slice()[24..64]);
    let mut model_len = 40;
    while model_len > 0 && (model[model_len - 1] == b' ' || model[model_len - 1] == 0) {
        model_len -= 1;
    }

    // 2. Identify Namespace 1 (CNS = 0)
    rt.bounce_dma.as_mut_slice().fill(0);
    let id_ns_cmd = NvmeCommand {
        opcode: 0x06,
        nsid: 1,
        prp1: rt.bounce_dma.phys_addr(),
        cdw10: 0,
        ..id_ctrl_cmd
    };
    if rt.submit_admin_cmd(id_ns_cmd).is_err() {
        return;
    }
    rt.bounce_dma.sync_for_cpu();
    let ns_bytes = rt.bounce_dma.as_slice();
    let nsze = u64::from_le_bytes(ns_bytes[0..8].try_into().unwrap_or([0; 8]));
    let flbas = (ns_bytes[26] & 0x0F) as usize;
    let lbads = ns_bytes[128 + flbas * 4 + 2];
    if nsze == 0 || !(9..=12).contains(&lbads) {
        return;
    }
    rt.lba_shift = lbads;
    let sectors_512 = nsze << (lbads - 9);

    // 3. Create I/O Completion Queue (Opcode 0x05, QID=1)
    let create_cq = NvmeCommand {
        opcode: 0x05,
        prp1: rt.iocq_dma.phys_addr(),
        cdw10: (((QUEUE_DEPTH - 1) as u32) << 16) | 1,
        cdw11: 1, // PC (Physically Contiguous) = 1
        ..id_ctrl_cmd
    };
    if rt.submit_admin_cmd(create_cq).is_err() {
        return;
    }

    // 4. Create I/O Submission Queue (Opcode 0x01, QID=1, CQID=1)
    let create_sq = NvmeCommand {
        opcode: 0x01,
        prp1: rt.iosq_dma.phys_addr(),
        cdw10: (((QUEUE_DEPTH - 1) as u32) << 16) | 1,
        cdw11: (1 << 16) | 1, // CQID=1, PC=1
        ..id_ctrl_cmd
    };
    if rt.submit_admin_cmd(create_sq).is_err() {
        return;
    }

    let ch = out.len() as u8;
    rt.info = BlockDeviceInfo {
        channel: ch,
        model,
        model_len,
        block_count: sectors_512,
        block_size: 512,
        device_type: DeviceType::HardDisk,
    };
    out.push(rt);
}

fn ensure_initialized() {
    if NVME_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    let Ok(_guard) = NvmeGuard::acquire() else {
        return;
    };
    if NVME_INIT_DONE.load(Ordering::Relaxed) {
        return;
    }

    let mut devs = Vec::new();
    for ep in enumerate_pci_bus() {
        // PCI Class 01 (Storage), Subclass 08 (NVMe)
        if ep.class_code == 0x01 && ep.subclass == 0x08 && devs.len() < 4 {
            unsafe {
                probe_nvme_controller(ep, &mut devs);
            }
        }
    }
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(NVME_DEVS), Some(devs));
    }
    NVME_INIT_DONE.store(true, Ordering::Release);
}

pub fn probe_nvme(channel: u8) -> Option<BlockDeviceInfo> {
    ensure_initialized();
    let _guard = NvmeGuard::acquire().ok()?;
    unsafe {
        let list = (*core::ptr::addr_of!(NVME_DEVS)).as_ref()?;
        list.get(channel as usize).map(|d| d.info)
    }
}

pub fn read_sector(channel: u8, lba512: u64, buf: &mut [u8]) -> Result<(), ()> {
    if buf.len() < 512 {
        return Err(());
    }
    ensure_initialized();
    let _guard = NvmeGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(NVME_DEVS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        if lba512 >= rt.info.block_count {
            return Err(());
        }
        let scale = (rt.lba_shift - 9) as u64;
        let nvme_lba = lba512 >> scale;
        let sub_off = ((lba512 & ((1 << scale) - 1)) as usize) * 512;

        let cmd = NvmeCommand {
            opcode: 0x02, // NVM Read
            flags: 0,
            cid: 0,
            nsid: rt.nsid,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            prp1: rt.bounce_dma.phys_addr(),
            prp2: 0,
            cdw10: (nvme_lba & 0xFFFF_FFFF) as u32,
            cdw11: (nvme_lba >> 32) as u32,
            cdw12: 0, // 0 = 1 mantıksal blok
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        rt.submit_io_cmd(cmd)?;
        rt.bounce_dma.sync_for_cpu();
        buf[..512].copy_from_slice(&rt.bounce_dma.as_slice()[sub_off..sub_off + 512]);
        Ok(())
    }
}

pub fn write_sector(channel: u8, lba512: u64, buf: &[u8]) -> Result<(), ()> {
    if buf.len() < 512 {
        return Err(());
    }
    ensure_initialized();
    let _guard = NvmeGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(NVME_DEVS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        if lba512 >= rt.info.block_count {
            return Err(());
        }
        let scale = (rt.lba_shift - 9) as u64;
        let nvme_lba = lba512 >> scale;
        let sub_off = ((lba512 & ((1 << scale) - 1)) as usize) * 512;

        // 4Kn ise önce mevcut bloğu oku, ilgili 512B dilimini güncelle (Read-Modify-Write)
        if scale > 0 {
            let r_cmd = NvmeCommand {
                opcode: 0x02,
                flags: 0,
                cid: 0,
                nsid: rt.nsid,
                cdw2: 0,
                cdw3: 0,
                mptr: 0,
                prp1: rt.bounce_dma.phys_addr(),
                prp2: 0,
                cdw10: (nvme_lba & 0xFFFF_FFFF) as u32,
                cdw11: (nvme_lba >> 32) as u32,
                cdw12: 0,
                cdw13: 0,
                cdw14: 0,
                cdw15: 0,
            };
            rt.submit_io_cmd(r_cmd)?;
        }
        rt.bounce_dma.as_mut_slice()[sub_off..sub_off + 512].copy_from_slice(&buf[..512]);

        let w_cmd = NvmeCommand {
            opcode: 0x01, // NVM Write
            flags: 0,
            cid: 0,
            nsid: rt.nsid,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            prp1: rt.bounce_dma.phys_addr(),
            prp2: 0,
            cdw10: (nvme_lba & 0xFFFF_FFFF) as u32,
            cdw11: (nvme_lba >> 32) as u32,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        rt.submit_io_cmd(w_cmd)
    }
}

pub fn flush_nvme(channel: u8) -> Result<(), ()> {
    ensure_initialized();
    let _guard = NvmeGuard::acquire()?;
    unsafe {
        let list = (*core::ptr::addr_of_mut!(NVME_DEVS)).as_mut().ok_or(())?;
        let rt = list.get_mut(channel as usize).ok_or(())?;
        let f_cmd = NvmeCommand {
            opcode: 0x00, // NVM Flush
            flags: 0,
            cid: 0,
            nsid: rt.nsid,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            prp1: 0,
            prp2: 0,
            cdw10: 0,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        rt.submit_io_cmd(f_cmd)
    }
}
