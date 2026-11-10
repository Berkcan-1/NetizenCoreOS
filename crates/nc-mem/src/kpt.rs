// crates/nc-mem/src/kpt.rs
//
// Çekirdeğin KENDİ sayfa tabloları (Ring 3 yol haritası, Adım 1; bkz. docs/RING3_YOL_HARITASI.md).
//
// Neden: Çekirdek bugüne kadar firmware'in (BIOS stage2 veya OVMF) bıraktığı sayfa tablolarıyla
// çalışıyordu. UEFI'de bu tablolar EfiBootServicesData içinde olabilir; çekirdek aynı bölgeyi
// "kullanılabilir" sayıp PMM/heap'e veriyordu. Burada tablolar çekirdek imajının .bss'inde
// (statik, PMM'siz) kurulur, bu yüzden hiçbir ayırıcıya bağlı değildir ve ALLOCATOR/PMM
// başlamadan ÖNCE devreye alınabilir.
//
// Harita (kimlik eşlemesi, davranış eskisiyle aynı + sıkılaştırma):
//   0..4 GiB   2 MiB sayfalar, RW + NX. 0..3 GiB WB, 3..4 GiB UC- (eski stage2 kuralı).
//   çekirdek imajı (0 .. imaj sonu, 2 MiB'a yuvarlı) 4 KiB sayfalar:
//       .text        R+X        .rodata  R+NX        .data/.bss/.stack  RW+NX
//       yığın alt sayfası EŞLENMEZ (guard page).
//   framebuffer  WC (PAT'ta PA1), NX. 4 GiB üstündeyse ayrı bir PD ile eşlenir.
//
// Başarısızlık güvenliği: `prepare` hiçbir şeyi etkinleştirmez. Tablolar kurulur, ardından
// yazılımla (translate_with_flags) taranır; en küçük uyumsuzlukta Err döner ve çağıran firmware
// tablolarıyla devam eder. `activate` yalnızca `prepare` Ok döndüyse çağrılmalıdır.

use core::arch::asm;
use core::arch::x86_64::__cpuid;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::paging::{
    cpu_supports_nx, read_cr3, write_cr3, AddressSpace, VmError, PAGE_SIZE, PTE_HUGE, PTE_NO_EXEC,
    PTE_PCD, PTE_PRESENT, PTE_PWT, PTE_USER, PTE_WRITABLE,
};

const PAGE: u64 = PAGE_SIZE as u64;
const TWO_MIB: u64 = 0x20_0000;
const ONE_GIB: u64 = 0x4000_0000;
const FOUR_GIB: u64 = 4 * ONE_GIB;
const MMIO_START: u64 = 3 * ONE_GIB;
const PML4_SPAN: u64 = 512 * ONE_GIB;
const LOW_GIBS: usize = 4;
const MAX_FINE_CHUNKS: usize = 8;
const MAX_FB_BYTES: u64 = 64 * 1024 * 1024;

const IA32_PAT: u32 = 0x277;
// PA0=WB PA1=WC PA2=UC- PA3=UC (PA4..7 aynı); stage2.asm ile aynı değer.
const PAT_VALUE: u64 = 0x0007_0106_0007_0106;
const CR0_WP: u64 = 1 << 16;
const CR4_PGE: u64 = 1 << 7;

#[repr(C, align(4096))]
struct Page([u64; 512]);

const ZERO_PAGE: Page = Page([0; 512]);

#[repr(C, align(4096))]
pub struct KernelTables {
    pml4: Page,
    pdpt: Page,
    pd: [Page; LOW_GIBS],
    fine: [Page; MAX_FINE_CHUNKS],
    fb_pd: Page,
}

#[derive(Clone, Copy, Debug)]
pub struct KernelLayout {
    pub text: (u64, u64),
    pub rodata: (u64, u64),
    pub rw: (u64, u64),
    pub guard_page: u64,
    pub framebuffer: (u64, u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Expect {
    Exec,
    ReadOnly,
    ReadWrite,
    Absent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VerifyFailure {
    pub addr: u64,
    pub expected: Expect,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KptError {
    AlreadyPrepared,
    Build(VmError),
    Verify(VerifyFailure),
}

#[derive(Clone, Copy, Debug)]
pub struct PreparedTables {
    pub pml4_phys: u64,
    pub nx: bool,
    pub wc: bool,
    pub fine_chunks: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Rx,
    Ro,
    Rw,
    Guard,
}

fn round_up(value: u64, align: u64) -> Option<u64> {
    value.checked_add(align - 1).map(|v| v & !(align - 1))
}

fn in_range(addr: u64, range: (u64, u64)) -> bool {
    addr >= range.0 && addr < range.1
}

fn classify(layout: &KernelLayout, addr: u64) -> Kind {
    if layout.guard_page != 0 && addr == layout.guard_page {
        Kind::Guard
    } else if in_range(addr, layout.text) {
        Kind::Rx
    } else if in_range(addr, layout.rodata) {
        Kind::Ro
    } else {
        Kind::Rw
    }
}

fn leaf_4k(kind: Kind, phys: u64, nx: bool) -> u64 {
    let nx_bit = if nx { PTE_NO_EXEC } else { 0 };
    match kind {
        Kind::Rx => phys | PTE_PRESENT,
        Kind::Ro => phys | PTE_PRESENT | nx_bit,
        Kind::Rw => phys | PTE_PRESENT | PTE_WRITABLE | nx_bit,
        Kind::Guard => 0,
    }
}

impl KernelLayout {
    fn validate(&self) -> Result<(), VmError> {
        let bounds = [
            self.text.0, self.text.1, self.rodata.0, self.rodata.1, self.rw.0, self.rw.1,
        ];
        for value in bounds {
            if value & (PAGE - 1) != 0 {
                return Err(VmError::UnalignedAddress);
            }
        }
        if self.guard_page & (PAGE - 1) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        let ordered = self.text.0 < self.text.1
            && self.text.1 <= self.rodata.0
            && self.rodata.0 <= self.rodata.1
            && self.rodata.1 <= self.rw.0
            && self.rw.0 < self.rw.1;
        if !ordered {
            return Err(VmError::InvalidMemoryMap);
        }
        if self.guard_page != 0 && !in_range(self.guard_page, self.rw) {
            return Err(VmError::InvalidMemoryMap);
        }
        if self.rw.1 > FOUR_GIB {
            return Err(VmError::InvalidMemoryMap);
        }
        Ok(())
    }
}

impl KernelTables {
    pub const fn new() -> Self {
        Self {
            pml4: ZERO_PAGE,
            pdpt: ZERO_PAGE,
            pd: [ZERO_PAGE; LOW_GIBS],
            fine: [ZERO_PAGE; MAX_FINE_CHUNKS],
            fb_pd: ZERO_PAGE,
        }
    }

    fn reset(&mut self) {
        self.pml4.0.fill(0);
        self.pdpt.0.fill(0);
        for page in self.pd.iter_mut() {
            page.0.fill(0);
        }
        for page in self.fine.iter_mut() {
            page.0.fill(0);
        }
        self.fb_pd.0.fill(0);
    }

    // Döndürür: (PML4 fiziksel adresi, 4 KiB'a bölünen 2 MiB parça sayısı).
    fn build(&mut self, layout: &KernelLayout, nx: bool, wc: bool) -> Result<(u64, usize), VmError> {
        layout.validate()?;
        self.reset();

        let nx_bit = if nx { PTE_NO_EXEC } else { 0 };
        let fine_end = round_up(layout.rw.1, TWO_MIB).ok_or(VmError::AddressOverflow)?;
        let chunks = (fine_end / TWO_MIB) as usize;
        if chunks > MAX_FINE_CHUNKS {
            return Err(VmError::TransactionCapacity);
        }

        let pdpt_phys = self.pdpt.0.as_ptr() as u64;
        self.pml4.0[0] = pdpt_phys | PTE_PRESENT | PTE_WRITABLE;

        for gib in 0..LOW_GIBS {
            let pd_phys = self.pd[gib].0.as_ptr() as u64;
            self.pdpt.0[gib] = pd_phys | PTE_PRESENT | PTE_WRITABLE;
            for slot in 0..512usize {
                let addr = (gib as u64) * ONE_GIB + (slot as u64) * TWO_MIB;
                let cache = if addr >= MMIO_START { PTE_PCD } else { 0 };
                self.pd[gib].0[slot] = addr | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | cache | nx_bit;
            }
        }

        for chunk in 0..chunks {
            let base = (chunk as u64) * TWO_MIB;
            let pt_phys = self.fine[chunk].0.as_ptr() as u64;
            for slot in 0..512usize {
                let addr = base + (slot as u64) * PAGE;
                self.fine[chunk].0[slot] = leaf_4k(classify(layout, addr), addr, nx);
            }
            self.pd[0].0[chunk] = pt_phys | PTE_PRESENT | PTE_WRITABLE;
        }

        let (fb_addr, fb_len) = layout.framebuffer;
        if fb_len != 0 {
            if fb_len > MAX_FB_BYTES {
                return Err(VmError::InvalidMemoryMap);
            }
            let fb_end = fb_addr.checked_add(fb_len).ok_or(VmError::AddressOverflow)?;
            if fb_end > PML4_SPAN {
                return Err(VmError::InvalidFrame);
            }
            let start = fb_addr & !(TWO_MIB - 1);
            let end = round_up(fb_end, TWO_MIB).ok_or(VmError::AddressOverflow)?;
            let fb_cache = if wc { PTE_PWT } else { PTE_PCD };
            let fb_pd_phys = self.fb_pd.0.as_ptr() as u64;
            let mut fb_gib: Option<usize> = None;
            let mut addr = start;
            while addr < end {
                if addr < FOUR_GIB {
                    if addr >= fine_end {
                        let gib = (addr / ONE_GIB) as usize;
                        let slot = ((addr % ONE_GIB) / TWO_MIB) as usize;
                        self.pd[gib].0[slot] =
                            addr | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | fb_cache | nx_bit;
                    }
                } else {
                    let gib = (addr >> 30) as usize;
                    match fb_gib {
                        None => {
                            if self.pdpt.0[gib] != 0 {
                                return Err(VmError::AlreadyMapped);
                            }
                            self.pdpt.0[gib] = fb_pd_phys | PTE_PRESENT | PTE_WRITABLE;
                            fb_gib = Some(gib);
                        }
                        Some(existing) if existing != gib => {
                            return Err(VmError::InvalidMemoryMap);
                        }
                        Some(_) => {}
                    }
                    let slot = ((addr & (ONE_GIB - 1)) / TWO_MIB) as usize;
                    self.fb_pd.0[slot] =
                        addr | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | fb_cache | nx_bit;
                }
                addr += TWO_MIB;
            }
        }

        Ok((self.pml4.0.as_ptr() as u64, chunks))
    }
}

fn check_one(space: &AddressSpace, addr: u64, expect: Expect, nx: bool) -> bool {
    let hit = space.translate_with_flags(addr);
    if expect == Expect::Absent {
        return hit.is_none();
    }
    let Some((phys, flags)) = hit else {
        return false;
    };
    if phys != addr || (flags & PTE_USER) != 0 {
        return false;
    }
    let writable = (flags & PTE_WRITABLE) != 0;
    let executable = !nx || (flags & PTE_NO_EXEC) == 0;
    match expect {
        Expect::Exec => executable && !writable,
        Expect::ReadOnly => !writable && (!nx || !executable),
        Expect::ReadWrite => writable && (!nx || !executable),
        Expect::Absent => false,
    }
}

fn verify_tables(
    pml4_phys: u64,
    layout: &KernelLayout,
    nx: bool,
    extra: &[(u64, Expect)],
) -> Result<(), VerifyFailure> {
    let space = AddressSpace { pml4_phys };
    let fine_end = round_up(layout.rw.1, TWO_MIB).unwrap_or(layout.rw.1);

    let mut addr = 0u64;
    while addr < fine_end {
        let expected = match classify(layout, addr) {
            Kind::Rx => Expect::Exec,
            Kind::Ro => Expect::ReadOnly,
            Kind::Rw => Expect::ReadWrite,
            Kind::Guard => Expect::Absent,
        };
        if !check_one(&space, addr, expected, nx) {
            return Err(VerifyFailure { addr, expected });
        }
        addr += PAGE;
    }

    let mut addr = fine_end;
    while addr < FOUR_GIB {
        if !check_one(&space, addr, Expect::ReadWrite, nx) {
            return Err(VerifyFailure { addr, expected: Expect::ReadWrite });
        }
        addr += TWO_MIB;
    }

    let (fb_addr, fb_len) = layout.framebuffer;
    if fb_len != 0 {
        let last = fb_addr + fb_len - 1;
        for probe in [fb_addr, last] {
            if !check_one(&space, probe, Expect::ReadWrite, nx) {
                return Err(VerifyFailure { addr: probe, expected: Expect::ReadWrite });
            }
        }
    }

    for &(addr, expected) in extra {
        if !check_one(&space, addr, expected, nx) {
            return Err(VerifyFailure { addr, expected });
        }
    }
    Ok(())
}

struct TablesCell(UnsafeCell<KernelTables>);
unsafe impl Sync for TablesCell {}

static KERNEL_TABLES: TablesCell = TablesCell(UnsafeCell::new(KernelTables::new()));
static PREPARED: AtomicBool = AtomicBool::new(false);

fn pat_supported() -> bool {
    #[allow(unused_unsafe)]
    let edx = unsafe { __cpuid(1).edx };
    (edx & (1 << 16)) != 0
}

/// Çalışan bağlamın (RIP, RSP, GDT, IDT) yeni tablolarda da doğru eşleneceğini sınamak için örnekler.
pub fn live_context_samples(out: &mut [(u64, Expect)]) -> usize {
    let rip: u64;
    let rsp: u64;
    let mut gdtr = [0u8; 10];
    let mut idtr = [0u8; 10];
    unsafe {
        asm!("lea {}, [rip + 0]", out(reg) rip, options(nomem, nostack, preserves_flags));
        asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
        asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr(), options(nostack, preserves_flags));
        asm!("sidt [{}]", in(reg) idtr.as_mut_ptr(), options(nostack, preserves_flags));
    }
    let base = |raw: &[u8; 10]| {
        u64::from_le_bytes([raw[2], raw[3], raw[4], raw[5], raw[6], raw[7], raw[8], raw[9]])
    };
    let candidates = [
        (rip, Expect::Exec),
        (rsp, Expect::ReadWrite),
        (base(&gdtr), Expect::ReadWrite),
        (base(&idtr), Expect::ReadWrite),
    ];
    let mut count = 0usize;
    for (addr, expect) in candidates {
        if addr != 0 && count < out.len() {
            out[count] = (addr, expect);
            count += 1;
        }
    }
    count
}

/// Tabloları kurar ve yazılımla doğrular. CR3'e DOKUNMAZ.
pub fn prepare(layout: &KernelLayout, extra: &[(u64, Expect)]) -> Result<PreparedTables, KptError> {
    if PREPARED.swap(true, Ordering::AcqRel) {
        return Err(KptError::AlreadyPrepared);
    }
    let nx = cpu_supports_nx();
    let wc = pat_supported();
    let tables = unsafe { &mut *KERNEL_TABLES.0.get() };
    let (pml4_phys, fine_chunks) = tables.build(layout, nx, wc).map_err(KptError::Build)?;
    verify_tables(pml4_phys, layout, nx, extra).map_err(KptError::Verify)?;
    Ok(PreparedTables { pml4_phys, nx, wc, fine_chunks })
}

/// CR3'ü yeni tablolara geçirir. Döndürür: eski CR3 (geri dönüş için).
///
/// Sırasıyla: PAT (WC için) -> CR0.WP -> CR4.PGE kapat (global TLB girdilerini de temizler) -> CR3.
pub unsafe fn activate(prepared: &PreparedTables) -> u64 {
    let old_cr3 = read_cr3();

    if prepared.wc {
        asm!("wbinvd", options(nostack, preserves_flags));
        asm!(
            "wrmsr",
            in("ecx") IA32_PAT,
            in("eax") PAT_VALUE as u32,
            in("edx") (PAT_VALUE >> 32) as u32,
            options(nostack, preserves_flags)
        );
    }

    let mut cr0: u64;
    asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
    if cr0 & CR0_WP == 0 {
        cr0 |= CR0_WP;
        asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));
    }

    let cr4: u64;
    asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
    if cr4 & CR4_PGE != 0 {
        asm!("mov cr4, {}", in(reg) cr4 & !CR4_PGE, options(nostack, preserves_flags));
    }

    write_cr3(prepared.pml4_phys);
    old_cr3
}

/// Acil geri dönüş: eski (firmware) CR3'e dön.
pub unsafe fn rollback(old_cr3: u64) {
    write_cr3(old_cr3);
}

#[cfg(test)]
mod tests {
    use super::*;

    static mut TEST_TABLES: KernelTables = KernelTables::new();

    fn base_layout() -> KernelLayout {
        KernelLayout {
            text: (0x10_0000, 0x30_0000),
            rodata: (0x30_0000, 0x40_0000),
            rw: (0x40_0000, 0x70_0000),
            guard_page: 0x6E_0000,
            framebuffer: (0xFD00_0000, 8 * 1024 * 1024),
        }
    }

    fn tables() -> &'static mut KernelTables {
        unsafe { &mut *core::ptr::addr_of_mut!(TEST_TABLES) }
    }

    // Tek test fonksiyonu: tablo statik ve paylaşımlı, testler paralel koşmamalı.
    #[test]
    fn kernel_page_tables_scenarios() {
        // 1) Normal harita kurulur ve taramadan geçer.
        let layout = base_layout();
        let (pml4, chunks) = tables().build(&layout, true, true).expect("build");
        assert_eq!(chunks, 4);
        verify_tables(pml4, &layout, true, &[]).expect("verify");

        let space = AddressSpace { pml4_phys: pml4 };

        // 2) W^X: hiçbir 4 KiB sayfa hem yazılabilir hem yürütülebilir değil; ince bölgede NX ilkeleri.
        let mut addr = 0u64;
        while addr < 0x80_0000 {
            if let Some((_, flags)) = space.translate_with_flags(addr) {
                let writable = flags & PTE_WRITABLE != 0;
                let executable = flags & PTE_NO_EXEC == 0;
                assert!(!(writable && executable), "W^X ihlali: {:#x}", addr);
                assert_eq!(flags & PTE_USER, 0);
            }
            addr += PAGE;
        }
        let (_, text) = space.translate_with_flags(0x10_0000).unwrap();
        assert!(text & PTE_NO_EXEC == 0 && text & PTE_WRITABLE == 0);
        let (_, ro) = space.translate_with_flags(0x30_0000).unwrap();
        assert!(ro & PTE_NO_EXEC != 0 && ro & PTE_WRITABLE == 0);
        let (_, rw) = space.translate_with_flags(0x40_0000).unwrap();
        assert!(rw & PTE_NO_EXEC != 0 && rw & PTE_WRITABLE != 0);
        let (_, low) = space.translate_with_flags(0x900).unwrap();
        assert!(low & PTE_NO_EXEC != 0 && low & PTE_WRITABLE != 0);

        // 3) Guard page eşlenmemiş, komşuları eşli.
        assert!(space.translate_with_flags(0x6E_0000).is_none());
        assert!(space.translate_with_flags(0x6D_F000).is_some());
        assert!(space.translate_with_flags(0x6E_1000).is_some());

        // 4) Bellek ve MMIO: kimlik eşleme, RW+NX; önbellek bitleri doğru.
        let (p, f) = space.translate_with_flags(0x2000_0000).unwrap();
        assert_eq!(p, 0x2000_0000);
        assert!(f & PTE_NO_EXEC != 0 && f & PTE_WRITABLE != 0);
        let pd = &tables().pd;
        let lapic_entry = pd[3].0[((0xFEE0_0000u64 % ONE_GIB) / TWO_MIB) as usize];
        assert!(lapic_entry & PTE_PCD != 0, "MMIO UC- olmalı");
        let ram_entry = pd[1].0[0];
        assert_eq!(ram_entry & (PTE_PCD | PTE_PWT), 0, "RAM WB olmalı");
        let fb_entry = pd[3].0[((0xFD00_0000u64 % ONE_GIB) / TWO_MIB) as usize];
        assert!(fb_entry & PTE_PWT != 0 && fb_entry & PTE_PCD == 0, "framebuffer WC olmalı");

        // 5) 4 GiB üstü framebuffer ayrı PD ile eşlenir.
        let mut high = base_layout();
        high.framebuffer = (0x8_0020_0000, 8 * 1024 * 1024);
        let (pml4, _) = tables().build(&high, true, true).expect("build high fb");
        verify_tables(pml4, &high, true, &[]).expect("verify high fb");
        let space = AddressSpace { pml4_phys: pml4 };
        let (p, f) = space.translate_with_flags(0x8_0020_0000).unwrap();
        assert_eq!(p, 0x8_0020_0000);
        assert!(f & PTE_NO_EXEC != 0);

        // 6) Aynı GiB'a sığmayan 4 GiB üstü framebuffer reddedilir (sessiz kesme yok).
        let mut span = base_layout();
        span.framebuffer = (0x8_3F80_0000, 16 * 1024 * 1024);
        assert_eq!(tables().build(&span, true, true).err(), Some(VmError::InvalidMemoryMap));

        // 7) Hizasız / sıra dışı yerleşim reddedilir.
        let mut bad = base_layout();
        bad.text.1 = 0x30_0001;
        assert_eq!(tables().build(&bad, true, true).err(), Some(VmError::UnalignedAddress));
        let mut bad = base_layout();
        bad.rodata = (0x20_0000, 0x40_0000);
        assert_eq!(tables().build(&bad, true, true).err(), Some(VmError::InvalidMemoryMap));
        let mut bad = base_layout();
        bad.guard_page = 0x20_0000;
        assert_eq!(tables().build(&bad, true, true).err(), Some(VmError::InvalidMemoryMap));

        // 8) NX yoksa bit 63 ASLA yazılmaz (rezerve bit #PF'sini önler).
        let layout = base_layout();
        let (pml4, _) = tables().build(&layout, false, false).expect("build no-nx");
        let space = AddressSpace { pml4_phys: pml4 };
        let (_, f) = space.translate_with_flags(0x40_0000).unwrap();
        assert_eq!(f & PTE_NO_EXEC, 0);
        for gib in 0..LOW_GIBS {
            for slot in 0..512 {
                assert_eq!(tables().pd[gib].0[slot] & PTE_NO_EXEC, 0);
            }
        }

        // 9) Doğrulayıcı gerçek bir hatayı yakalar: RIP/RSP örneği yanlış yerde.
        let layout = base_layout();
        let (pml4, _) = tables().build(&layout, true, true).unwrap();
        let bogus = [(0x40_0000u64, Expect::Exec)];
        assert!(verify_tables(pml4, &layout, true, &bogus).is_err());
        let bogus = [(0x10_0000u64, Expect::ReadWrite)];
        assert!(verify_tables(pml4, &layout, true, &bogus).is_err());
    }
}
