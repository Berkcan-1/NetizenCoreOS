use crate::dma::{DmaBuffer, DmaMask};
use crate::pci::{enumerate_pci_bus, PciBar, PciEcam, PciEndpoint};
use nc_arch::{inb, inl, inw, outb, outl, outw};
use nc_hal::{AudioError, AudioOutput, AudioStream, PcmFormat};

const PCM_BUFFER_BYTES: usize = 16 * 1024;
const BDL_BYTES: usize = 8;
const AC97_MAX_RATE: u32 = 48_000;
const BLOCKING_WAIT: usize = 400_000;
const READY_WAIT: usize = 60_000;
const RING_SLOTS: usize = 32;
const SLOT_FRAMES: usize = 512;
const SLOT_BYTES: usize = SLOT_FRAMES * 4;
const SLOT_SAMPLES: usize = SLOT_FRAMES * 2;
const PREROLL_SLOTS: u32 = 2;

const REG_PO_BDBAR: u16 = 0x10;
const REG_PO_CIV: u16 = 0x14;
const REG_PO_LVI: u16 = 0x15;
const REG_PO_SR: u16 = 0x16;
const REG_PO_CR: u16 = 0x1B;
const REG_GLOB_CNT: u16 = 0x2C;
const REG_GLOB_STA: u16 = 0x30;

const SR_DCH: u16 = 0x0001;
const SR_CLEAR: u16 = 0x001C;
const CR_RPBM: u8 = 0x01;
const CR_RR: u8 = 0x02;

const NAM_RESET: u16 = 0x00;
const NAM_MASTER_VOLUME: u16 = 0x02;
const NAM_PCM_VOLUME: u16 = 0x18;
const NAM_POWER_STATUS: u16 = 0x26;
const NAM_EXT_ID: u16 = 0x28;
const NAM_EXT_CONTROL: u16 = 0x2A;
const NAM_FRONT_RATE: u16 = 0x2C;

struct Ring {
    pcm: DmaBuffer,
    #[allow(dead_code)]
    bdl: DmaBuffer,
    bdl_phys: u32,
    head: u32,
    played: u32,
    last_civ: u8,
    running: bool,
    halted: bool,
}

pub struct Ac97Output {
    endpoint: PciEndpoint,
    mixer_base: u16,
    bus_master_base: u16,
    pcm: DmaBuffer,
    bdl: DmaBuffer,
    format: Option<PcmFormat>,
    bus_master_enabled: bool,
    ring: Option<Ring>,
}

fn io_delay(rounds: usize) {
    for _ in 0..rounds {
        unsafe {
            let _ = inb(0x80);
        }
    }
}

impl Ac97Output {
    pub fn probe() -> Result<Self, AudioError> {
        Self::probe_from_endpoints(enumerate_pci_bus())
    }

    pub fn probe_with_ecam(ecam: &PciEcam) -> Result<Self, AudioError> {
        Self::probe_from_endpoints(ecam.enumerate())
    }

    fn probe_from_endpoints<I>(endpoints: I) -> Result<Self, AudioError>
    where
        I: IntoIterator<Item = PciEndpoint>,
    {
        let mut endpoint = endpoints
            .into_iter()
            .find(|device| {
                device.class_code == 0x04
                    && device.subclass == 0x01
                    && device.prog_if == 0x00
                    && matches!(device.bars[0], PciBar::IoPort { size, .. } if size >= 0x40)
                    && matches!(device.bars[1], PciBar::IoPort { size, .. } if size >= 0x34)
            })
            .ok_or(AudioError::DeviceNotReady)?;

        let (mixer_base, mixer_size) = match endpoint.bars[0] {
            PciBar::IoPort { base, size } => (base, size),
            _ => return Err(AudioError::DeviceNotReady),
        };
        let (bus_master_base, bus_master_size) = match endpoint.bars[1] {
            PciBar::IoPort { base, size } => (base, size),
            _ => return Err(AudioError::DeviceNotReady),
        };
        if mixer_base == 0 || bus_master_base == 0 || mixer_size < 0x40 || bus_master_size < 0x34 {
            return Err(AudioError::DeviceNotReady);
        }
        if mixer_base.checked_add(0x3E).is_none() || bus_master_base.checked_add(0x32).is_none() {
            return Err(AudioError::DeviceNotReady);
        }

        let pcm = DmaBuffer::allocate(PCM_BUFFER_BYTES, 4096, DmaMask::Bits32)
            .map_err(|_| AudioError::DeviceNotReady)?;
        let bdl = DmaBuffer::allocate(BDL_BYTES, 16, DmaMask::Bits32)
            .map_err(|_| AudioError::DeviceNotReady)?;
        let pcm_phys = pcm.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
        let bdl_phys = bdl.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
        if (bdl_phys & 0x0F) != 0 || (pcm_phys & 0x0F) != 0 {
            return Err(AudioError::DeviceNotReady);
        }

        endpoint.enable_resource_decoding(false, true);
        if !matches!(endpoint.state, crate::pci::DeviceLifecycleState::ResourcesValidated) {
            return Err(AudioError::DeviceNotReady);
        }

        let mut device = Self {
            endpoint,
            mixer_base,
            bus_master_base,
            pcm,
            bdl,
            format: None,
            bus_master_enabled: false,
            ring: None,
        };
        device.initialize_codec()?;
        device.write_bdl(pcm_phys, 0);
        Ok(device)
    }

    fn codec_ready(&self) -> bool {
        unsafe {
            let status = inl(self.bus_master_base + REG_GLOB_STA);
            if status != 0xFFFF_FFFF && (status & 0x0000_0100) != 0 {
                return true;
            }
            let power = inw(self.mixer_base + NAM_POWER_STATUS);
            power != 0xFFFF && (power & 0x000F) == 0x000F
        }
    }

    fn wait_codec_ready(&self) -> bool {
        for _ in 0..READY_WAIT {
            if self.codec_ready() {
                return true;
            }
        }
        false
    }

    fn initialize_codec(&mut self) -> Result<(), AudioError> {
        unsafe {
            outl(self.bus_master_base + REG_GLOB_CNT, 0x0000_0002);
            io_delay(2_000);
            if !self.wait_codec_ready() {
                return Err(AudioError::DeviceNotReady);
            }
            outw(self.mixer_base + NAM_RESET, 0x0000);
            io_delay(2_000);
            if !self.wait_codec_ready() {
                return Err(AudioError::DeviceNotReady);
            }
            outw(self.mixer_base + NAM_MASTER_VOLUME, 0x8000);
            outw(self.mixer_base + NAM_PCM_VOLUME, 0x8000);
        }
        Ok(())
    }

    fn write_bdl(&mut self, pcm_phys: u32, sample_words: u16) {
        let bytes = self.bdl.as_mut_slice();
        bytes.fill(0);
        bytes[0..4].copy_from_slice(&pcm_phys.to_le_bytes());
        bytes[4..6].copy_from_slice(&sample_words.to_le_bytes());
        bytes[6..8].copy_from_slice(&0x8000u16.to_le_bytes());
        self.bdl.sync_for_device();
    }

    fn play_buffer(&mut self, sample_words: usize) -> Result<(), AudioError> {
        if sample_words == 0 || sample_words > (PCM_BUFFER_BYTES / 2) || sample_words > u16::MAX as usize {
            return Err(AudioError::BufferTooLarge);
        }
        let pcm_phys = self.pcm.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
        let bdl_phys = self.bdl.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
        self.write_bdl(pcm_phys, sample_words as u16);
        self.pcm.sync_for_device();

        unsafe {
            outb(self.bus_master_base + REG_PO_CR, CR_RR);
            for _ in 0..20_000 {
                if (inb(self.bus_master_base + REG_PO_CR) & CR_RR) == 0 {
                    break;
                }
            }
            outw(self.bus_master_base + REG_PO_SR, SR_CLEAR);
            outl(self.bus_master_base + REG_PO_BDBAR, bdl_phys);
            outb(self.bus_master_base + REG_PO_LVI, 0x00);

            if !self.bus_master_enabled {
                self.endpoint.enable_bus_mastering();
                self.bus_master_enabled = true;
            }
            outb(self.bus_master_base + REG_PO_CR, CR_RPBM);

            let mut completed = false;
            for _ in 0..BLOCKING_WAIT {
                let status = inw(self.bus_master_base + REG_PO_SR);
                if (status & 0x0010) != 0 {
                    outb(self.bus_master_base + REG_PO_CR, 0x00);
                    return Err(AudioError::HardwareFailure);
                }
                if (status & 0x000C) != 0 || (status & SR_DCH) != 0 {
                    completed = true;
                    break;
                }
            }
            outb(self.bus_master_base + REG_PO_CR, 0x00);
            if !completed {
                return Err(AudioError::Timeout);
            }
        }
        Ok(())
    }

    fn ring_reset(&mut self) {
        let base = self.bus_master_base;
        let Some(ring) = self.ring.as_mut() else { return; };
        unsafe {
            outb(base + REG_PO_CR, 0x00);
            outb(base + REG_PO_CR, CR_RR);
            for _ in 0..20_000 {
                if (inb(base + REG_PO_CR) & CR_RR) == 0 {
                    break;
                }
            }
            outw(base + REG_PO_SR, SR_CLEAR);
            outl(base + REG_PO_BDBAR, ring.bdl_phys);
            outb(base + REG_PO_LVI, 0x00);
        }
        ring.head = 0;
        ring.played = 0;
        ring.last_civ = 0;
        ring.running = false;
    }
}

impl AudioOutput for Ac97Output {
    fn configure(&mut self, format: PcmFormat) -> Result<(), AudioError> {
        if !matches!(format.channels, 1 | 2)
            || !(8_000..=AC97_MAX_RATE).contains(&format.sample_rate_hz)
        {
            return Err(AudioError::UnsupportedFormat);
        }
        unsafe {
            let extended_id = inw(self.mixer_base + NAM_EXT_ID);
            let variable_rate = extended_id != 0xFFFF && (extended_id & 0x0001) != 0;
            if variable_rate {
                let extended_control = inw(self.mixer_base + NAM_EXT_CONTROL) | 0x0001;
                outw(self.mixer_base + NAM_EXT_CONTROL, extended_control);
                outw(self.mixer_base + NAM_FRONT_RATE, format.sample_rate_hz as u16);
                if inw(self.mixer_base + NAM_FRONT_RATE) != format.sample_rate_hz as u16 {
                    return Err(AudioError::HardwareFailure);
                }
            } else if format.sample_rate_hz != 48_000 {
                return Err(AudioError::UnsupportedFormat);
            }
            outw(self.mixer_base + NAM_MASTER_VOLUME, 0x0000);
            outw(self.mixer_base + NAM_PCM_VOLUME, 0x0808);
        }
        self.format = Some(format);
        Ok(())
    }

    fn write_interleaved_i16(&mut self, samples: &[i16]) -> Result<(), AudioError> {
        let format = self.format.ok_or(AudioError::UnsupportedFormat)?;
        let channels = format.channels as usize;
        if samples.len() % channels != 0 {
            return Err(AudioError::UnsupportedFormat);
        }
        let max_input = if channels == 1 { 4096 } else { 8192 };
        for chunk in samples.chunks(max_input) {
            if chunk.len() % channels != 0 {
                return Err(AudioError::UnsupportedFormat);
            }
            let dma = self.pcm.as_mut_slice();
            let words = if channels == 2 {
                let bytes_len = chunk.len() * 2;
                for (dst, sample) in dma[..bytes_len].chunks_exact_mut(2).zip(chunk.iter()) {
                    dst.copy_from_slice(&sample.to_le_bytes());
                }
                chunk.len()
            } else {
                for (index, sample) in chunk.iter().enumerate() {
                    let bytes = sample.to_le_bytes();
                    let out = index * 4;
                    dma[out..out + 2].copy_from_slice(&bytes);
                    dma[out + 2..out + 4].copy_from_slice(&bytes);
                }
                chunk.len() * 2
            };
            self.play_buffer(words)?;
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), AudioError> {
        unsafe {
            outb(self.bus_master_base + REG_PO_CR, 0x00);
        }
        Ok(())
    }

    fn set_volume_percent(&mut self, percent: u8) -> Result<(), AudioError> {
        let percent = percent.min(100) as u16;
        let attenuation = if percent == 0 { 31 } else { ((100 - percent) * 31) / 100 };
        let value = if percent == 0 { 0x8000 } else { (attenuation << 8) | attenuation };
        unsafe {
            outw(self.mixer_base + NAM_MASTER_VOLUME, value);
            outw(self.mixer_base + NAM_PCM_VOLUME, value);
        }
        Ok(())
    }
}

impl AudioStream for Ac97Output {
    fn stream_open(&mut self, sample_rate_hz: u32) -> Result<(), AudioError> {
        self.configure(PcmFormat { sample_rate_hz, channels: 2 })?;
        if self.ring.is_none() {
            let pcm = DmaBuffer::allocate(RING_SLOTS * SLOT_BYTES, 4096, DmaMask::Bits32)
                .map_err(|_| AudioError::DeviceNotReady)?;
            let mut bdl = DmaBuffer::allocate(RING_SLOTS * 8, 16, DmaMask::Bits32)
                .map_err(|_| AudioError::DeviceNotReady)?;
            let pcm_phys = pcm.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
            let bdl_phys = bdl.phys_addr_u32().map_err(|_| AudioError::DeviceNotReady)?;
            if (pcm_phys & 0x0F) != 0 || (bdl_phys & 0x07) != 0 {
                return Err(AudioError::DeviceNotReady);
            }
            let bytes = bdl.as_mut_slice();
            for slot in 0..RING_SLOTS {
                let entry = &mut bytes[slot * 8..slot * 8 + 8];
                let address = pcm_phys + (slot * SLOT_BYTES) as u32;
                entry[0..4].copy_from_slice(&address.to_le_bytes());
                entry[4..6].copy_from_slice(&(SLOT_SAMPLES as u16).to_le_bytes());
                entry[6..8].copy_from_slice(&0u16.to_le_bytes());
            }
            bdl.sync_for_device();
            self.ring = Some(Ring {
                pcm,
                bdl,
                bdl_phys,
                head: 0,
                played: 0,
                last_civ: 0,
                running: false,
                halted: false,
            });
        }
        if !self.bus_master_enabled {
            self.endpoint.enable_bus_mastering();
            self.bus_master_enabled = true;
        }
        self.ring_reset();
        Ok(())
    }

    fn stream_slot_frames(&self) -> usize {
        SLOT_FRAMES
    }

    fn stream_slot_count(&self) -> usize {
        RING_SLOTS
    }

    fn stream_queued_slots(&mut self) -> usize {
        let base = self.bus_master_base;
        let Some(ring) = self.ring.as_mut() else { return 0; };
        if !ring.running {
            return ring.head.wrapping_sub(ring.played) as usize;
        }
        let (status, civ) = unsafe { (inw(base + REG_PO_SR), inb(base + REG_PO_CIV)) };
        if status == 0xFFFF {
            ring.halted = true;
            self.ring_reset();
            return 0;
        }
        if (status & SR_DCH) != 0 {
            ring.halted = true;
            self.ring_reset();
            return 0;
        }
        let delta = civ.wrapping_sub(ring.last_civ) & 0x1F;
        ring.played = ring.played.wrapping_add(delta as u32);
        ring.last_civ = civ;
        ring.head.wrapping_sub(ring.played) as usize
    }

    fn stream_push_slot(&mut self, interleaved_stereo: &[i16]) -> Result<(), AudioError> {
        let base = self.bus_master_base;
        let Some(ring) = self.ring.as_mut() else { return Err(AudioError::DeviceNotReady); };
        if interleaved_stereo.len() != SLOT_SAMPLES {
            return Err(AudioError::UnsupportedFormat);
        }
        if ring.head.wrapping_sub(ring.played) as usize >= RING_SLOTS - 2 {
            return Err(AudioError::BufferTooLarge);
        }
        let index = (ring.head as usize) % RING_SLOTS;
        let target = &mut ring.pcm.as_mut_slice()[index * SLOT_BYTES..(index + 1) * SLOT_BYTES];
        for (dst, sample) in target.chunks_exact_mut(2).zip(interleaved_stereo.iter()) {
            dst.copy_from_slice(&sample.to_le_bytes());
        }
        ring.pcm.sync_for_device();
        ring.head = ring.head.wrapping_add(1);
        unsafe {
            outb(base + REG_PO_LVI, index as u8);
            if !ring.running && ring.head >= PREROLL_SLOTS {
                outb(base + REG_PO_CR, CR_RPBM);
                ring.running = true;
                ring.last_civ = 0;
            }
        }
        Ok(())
    }

    fn stream_take_halt(&mut self) -> bool {
        match self.ring.as_mut() {
            Some(ring) => core::mem::replace(&mut ring.halted, false),
            None => false,
        }
    }

    fn stream_kick(&mut self) {
        let base = self.bus_master_base;
        let Some(ring) = self.ring.as_mut() else { return; };
        if !ring.running && ring.head > 0 {
            unsafe {
                outb(base + REG_PO_CR, CR_RPBM);
            }
            ring.running = true;
            ring.last_civ = 0;
        }
    }

    fn stream_stop(&mut self) {
        self.ring_reset();
    }
}

impl Drop for Ac97Output {
    fn drop(&mut self) {
        unsafe {
            outb(self.bus_master_base + REG_PO_CR, 0x00);
        }
        self.endpoint.disable_bus_mastering();
    }
}
