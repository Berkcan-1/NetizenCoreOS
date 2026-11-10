//! Conservative AC'97 PCM output for legacy x86 PCs and VM profiles exposing
//! an Intel-compatible AC'97 controller (e.g. QEMU/VirtualBox configured for it).
//! HDA is detected separately and intentionally not claimed as supported here.

use crate::dma::{DmaBuffer, DmaMask};
use crate::pci::{enumerate_pci_bus, PciBar, PciEcam, PciEndpoint};
use nc_arch::{inb, outb, outl, outw};
use nc_hal::{AudioError, AudioOutput, PcmFormat};

const PCM_BUFFER_BYTES: usize = 16 * 1024;
const BDL_BYTES: usize = 8;
const AC97_MAX_RATE: u32 = 48_000;
const WAIT_LIMIT: usize = 8_000_000;

#[inline]
unsafe fn read_port_u16(port: u16) -> u16 {
    let lo = inb(port) as u16;
    let hi = inb(port.wrapping_add(1)) as u16;
    lo | (hi << 8)
}

#[inline]
unsafe fn write_port_u16(port: u16, value: u16) {
    outw(port, value);
}

/// One-entry AC'97 buffer descriptor list. The bytes are serialized explicitly
/// into a DMA buffer, avoiding assumptions about Rust struct layout.
pub struct Ac97Output {
    endpoint: PciEndpoint,
    mixer_base: u16,
    bus_master_base: u16,
    pcm: DmaBuffer,
    bdl: DmaBuffer,
    format: Option<PcmFormat>,
    bus_master_enabled: bool,
}

impl Ac97Output {
    /// Finds and initializes the first standard PCI multimedia/audio controller
    /// with AC'97 programming interface and valid I/O BARs.
    pub fn probe() -> Result<Self, AudioError> {
        Self::probe_from_endpoints(enumerate_pci_bus())
    }

    /// Probe using endpoints discovered through an ACPI MCFG/ECAM mapping.
    /// The caller remains responsible for discovering and mapping the MCFG window.
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

        // Allocate and validate every DMA byte before granting the controller bus mastering.
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
        };
        device.initialize_codec()?;
        device.write_bdl(pcm_phys, 0);
        let _ = bdl_phys;
        Ok(device)
    }

    fn initialize_codec(&mut self) -> Result<(), AudioError> {
        unsafe {
            // Cold reset is bounded. Never leave the CPU in an unbounded hardware wait.
            write_port_u16(self.bus_master_base + 0x2C, 0x0002);
            for _ in 0..100_000 { let _ = inb(0x80); }
            write_port_u16(self.bus_master_base + 0x2C, 0x0000);
            let mut ready = false;
            for _ in 0..WAIT_LIMIT {
                if (read_port_u16(self.bus_master_base + 0x30) & 0x0001) != 0 {
                    ready = true;
                    break;
                }
            }
            if !ready { return Err(AudioError::DeviceNotReady); }

            // Keep output muted until a real PCM format is configured.
            write_port_u16(self.mixer_base + 0x02, 0x8000);
            write_port_u16(self.mixer_base + 0x18, 0x8000);
        }
        Ok(())
    }

    fn write_bdl(&mut self, pcm_phys: u32, sample_words: u16) {
        let bytes = self.bdl.as_mut_slice();
        bytes.fill(0);
        bytes[0..4].copy_from_slice(&pcm_phys.to_le_bytes());
        bytes[4..6].copy_from_slice(&sample_words.to_le_bytes());
        // IOC: interrupt-on-completion; no loop/cyclic flag is set.
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
            // Stop/reset the PCM-out channel, clear W1C status, program descriptor, then run.
            outb(self.bus_master_base + 0x1B, 0x02);
            for _ in 0..100_000 {
                if (inb(self.bus_master_base + 0x1B) & 0x02) == 0 { break; }
            }
            write_port_u16(self.bus_master_base + 0x16, 0x001C);
            outl(self.bus_master_base + 0x10, bdl_phys);
            outb(self.bus_master_base + 0x15, 0x00);

            if !self.bus_master_enabled {
                self.endpoint.enable_bus_mastering();
                self.bus_master_enabled = true;
            }
            outb(self.bus_master_base + 0x1B, 0x01);

            let mut completed = false;
            for _ in 0..WAIT_LIMIT {
                let status = read_port_u16(self.bus_master_base + 0x16);
                if (status & 0x0010) != 0 {
                    outb(self.bus_master_base + 0x1B, 0x00);
                    self.endpoint.disable_bus_mastering();
                    self.bus_master_enabled = false;
                    return Err(AudioError::HardwareFailure);
                }
                if (status & 0x000C) != 0 {
                    completed = true;
                    break;
                }
            }
            outb(self.bus_master_base + 0x1B, 0x00);
            if !completed {
                self.endpoint.disable_bus_mastering();
                self.bus_master_enabled = false;
                return Err(AudioError::Timeout);
            }
        }
        Ok(())
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
            let extended_id = read_port_u16(self.mixer_base + 0x28);
            if format.sample_rate_hz != 48_000 && (extended_id & 0x0001) == 0 {
                return Err(AudioError::UnsupportedFormat);
            }
            if (extended_id & 0x0001) != 0 {
                let extended_control = read_port_u16(self.mixer_base + 0x2A) | 0x0001;
                write_port_u16(self.mixer_base + 0x2A, extended_control);
                write_port_u16(self.mixer_base + 0x2C, format.sample_rate_hz as u16);
                if read_port_u16(self.mixer_base + 0x2C) != format.sample_rate_hz as u16 {
                    return Err(AudioError::HardwareFailure);
                }
            } else if format.sample_rate_hz != 48_000 {
                return Err(AudioError::UnsupportedFormat);
            }
            // Restore audible-but-moderate level only after a valid format is selected.
            write_port_u16(self.mixer_base + 0x02, 0x0808);
            write_port_u16(self.mixer_base + 0x18, 0x0808);
        }
        self.format = Some(format);
        Ok(())
    }

    fn write_interleaved_i16(&mut self, samples: &[i16]) -> Result<(), AudioError> {
        let format = self.format.ok_or(AudioError::UnsupportedFormat)?;
        let channels = format.channels as usize;
        if samples.len() % channels != 0 { return Err(AudioError::UnsupportedFormat); }

        // The DMA buffer holds 4096 stereo frames. Mono is duplicated to stereo.
        let max_input = if channels == 1 { 4096 } else { 8192 };
        for chunk in samples.chunks(max_input) {
            if chunk.len() % channels != 0 { return Err(AudioError::UnsupportedFormat); }
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
        unsafe { outb(self.bus_master_base + 0x1B, 0x00); }
        Ok(())
    }

    fn set_volume_percent(&mut self, percent: u8) -> Result<(), AudioError> {
        let percent = percent.min(100) as u16;
        let attenuation = if percent == 0 { 31 } else { ((100 - percent) * 31) / 100 };
        let value = if percent == 0 { 0x8000 } else { (attenuation << 8) | attenuation };
        unsafe {
            write_port_u16(self.mixer_base + 0x02, value);
            write_port_u16(self.mixer_base + 0x18, value);
        }
        Ok(())
    }
}

impl Drop for Ac97Output {
    fn drop(&mut self) {
        unsafe { outb(self.bus_master_base + 0x1B, 0x00); }
        self.endpoint.disable_bus_mastering();
    }
}
