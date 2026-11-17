use alloc::boxed::Box;
use alloc::vec::Vec;
use nc_hal::BlockDevice;
use nc_storage::{parse_mbr_partitions, read_initial_locale, scan_channel};

const MIN_SPLASH_MS: u32 = 900; // Sadece gecis animasyonunun puruzsuz gorunmesi icin kisa alt sinir

pub struct Preparation {
    started: u32,
    finished: bool,
    step: u8,
    disks: Vec<Box<dyn BlockDevice>>,
    pub locale: Option<(u8, u8)>,
}

impl Preparation {
    pub fn new(now: u32) -> Preparation {
        Preparation {
            started: now,
            finished: false,
            step: 0,
            disks: Vec::new(),
            locale: None,
        }
    }

    pub fn poll(&mut self, now: u32) -> bool {
        let elapsed = now.wrapping_sub(self.started);

        match self.step {
            // Adim 0..3: 4 ATA kanalini gercek zamanli tara
            0..=3 => {
                if let Some(mut dev) = scan_channel(self.step) {
                    let _ = parse_mbr_partitions(&mut dev);
                    self.disks.push(Box::new(dev));
                }
                self.step += 1;
            }
            // Adim 4: Kayitli dil ve klavye duzenini MBR'den oku
            4 => {
                self.locale = read_initial_locale();
                if let Some((_lang, kbd)) = self.locale {
                    nc_input::set_keyboard_layout(kbd);
                }
                self.step += 1;
            }
            // Adim 5: Ag yiginini ilk kez yokla
            5 => {
                nc_net::global().poll();
                self.step += 1;
            }
            _ => {
                if elapsed >= MIN_SPLASH_MS {
                    self.finished = true;
                }
            }
        }

        self.finished
    }

    pub fn take_disks(&mut self) -> Vec<Box<dyn BlockDevice>> {
        core::mem::take(&mut self.disks)
    }
}
