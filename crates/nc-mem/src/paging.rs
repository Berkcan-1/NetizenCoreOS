// crates/nc-mem/src/paging.rs
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

pub const PAGE_SIZE: usize = 4096;
// 512 GiB sınırı: PML4 Index 1'in tam başlangıcı (PML4[0] çekirdeğe aittir)
pub const USER_SPACE_START: u64 = 0x0000_0080_0000_0000;
pub const USER_SPACE_END: u64   = 0x0000_7FFF_FFFF_F000;

pub const PTE_PRESENT: u64  = 1 << 0;
pub const PTE_WRITABLE: u64 = 1 << 1;
pub const PTE_USER: u64     = 1 << 2;
pub const PTE_PWT: u64      = 1 << 3;
pub const PTE_PCD: u64      = 1 << 4;
pub const PTE_HUGE: u64     = 1 << 7;
pub const PTE_NO_EXEC: u64  = 1 << 63;

pub const ADDR_MASK_4K: u64 = 0x000F_FFFF_FFFF_F000;
pub const ADDR_MASK_2M: u64 = 0x000F_FFFF_FFE0_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PagePermissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user: bool,
}

impl PagePermissions {
    pub const KERNEL_RO: Self = Self { readable: true, writable: false, executable: false, user: false };
    pub const KERNEL_RW: Self = Self { readable: true, writable: true,  executable: false, user: false };
    pub const KERNEL_RX: Self = Self { readable: true, writable: false, executable: true,  user: false };
    pub const USER_RO: Self   = Self { readable: true, writable: false, executable: false, user: true };
    pub const USER_RW: Self   = Self { readable: true, writable: true,  executable: false, user: true };
    pub const USER_RX: Self   = Self { readable: true, writable: false, executable: true,  user: true };

    #[inline]
    pub fn validate_wx(&self) -> Result<(), VmError> {
        if self.writable && self.executable {
            Err(VmError::WxViolation)
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn to_pte_flags(&self, nx_supported: bool) -> u64 {
        let mut flags = PTE_PRESENT;
        if self.writable {
            flags |= PTE_WRITABLE;
        }
        if self.user {
            flags |= PTE_USER;
        }
        if !self.executable && nx_supported {
            flags |= PTE_NO_EXEC;
        }
        flags
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VmError {
    UnalignedAddress,
    NonCanonicalAddress,
    OutOfMemory,
    WxViolation,
    AlreadyMapped,
    NotMapped,
    SupervisorOverlap,
    AddressOverflow,
}

// 4 GiB'a kadar 4 KiB fiziksel sayfa çerçevelerini izleyen bitmap (1M frame = 128 KiB)
const MAX_FRAMES: usize = 1024 * 1024;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

pub struct FrameAllocator {
    bitmap: [u64; BITMAP_WORDS],
    total_free: usize,
    nx_supported: bool,
}

static PMM_LOCK: AtomicBool = AtomicBool::new(false);
static mut GLOBAL_PMM: FrameAllocator = FrameAllocator {
    bitmap: [0; BITMAP_WORDS], // 0 = Rezerve/Kullanımda, 1 = Boş (Varsayılan güvenli: tümü rezerve)
    total_free: 0,
    nx_supported: false,
};

fn lock_pmm() {
    while PMM_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn unlock_pmm() {
    PMM_LOCK.store(false, Ordering::Release);
}

#[inline]
pub fn cpu_supports_nx() -> bool {
    let edx: u32;
    unsafe {
        asm!(
            "mov eax, 0x80000001",
            "cpuid",
            out("eax") _,
            out("ecx") _,
            out("edx") edx,
            options(nomem, nostack, preserves_flags)
        );
    }
    (edx & (1 << 20)) != 0
}

pub unsafe fn init_pmm(free_phys_start: u64, free_phys_end: u64) {
    lock_pmm();
    GLOBAL_PMM.nx_supported = cpu_supports_nx();
    let mut start_frame = ((free_phys_start + 0xFFF) / 4096) as usize;
    let mut end_frame = (free_phys_end / 4096) as usize;

    // Eğer parametreler geçersizse veya 16 MiB altındaysa, garantili 32 MiB .. 48 MiB aralığına yerleş
    if end_frame <= start_frame || end_frame < 4096 {
        start_frame = 8192;  // 32 MiB
        end_frame = 12288;  // 48 MiB (4096 frame)
    }

    let safe_start = start_frame.max(4096); // 16 MiB altı (kernel/boot) asla PMM'e verilmez
    let safe_end = end_frame.min(MAX_FRAMES);

    if safe_end > safe_start {
        for frame in safe_start..safe_end {
            let word = frame / 64;
            let bit = frame % 64;
            if (GLOBAL_PMM.bitmap[word] & (1u64 << bit)) == 0 {
                GLOBAL_PMM.bitmap[word] |= 1u64 << bit;
                GLOBAL_PMM.total_free += 1;
            }
        }
    }
    unlock_pmm();
}

pub fn alloc_frame() -> Option<u64> {
    lock_pmm();
    let mut found: Option<u64> = None;
    unsafe {
        for word_idx in 64..BITMAP_WORDS {
            let w = GLOBAL_PMM.bitmap[word_idx];
            if w != 0 {
                let bit = w.trailing_zeros() as usize;
                GLOBAL_PMM.bitmap[word_idx] &= !(1u64 << bit);
                GLOBAL_PMM.total_free = GLOBAL_PMM.total_free.saturating_sub(1);
                let phys = ((word_idx * 64 + bit) * PAGE_SIZE) as u64;
                core::ptr::write_bytes(phys as *mut u8, 0, PAGE_SIZE);
                found = Some(phys);
                break;
            }
        }
    }
    unlock_pmm();
    found
}

pub unsafe fn free_frame(phys_addr: u64) {
    if (phys_addr & 0xFFF) != 0 || phys_addr < 0x0100_0000 {
        return;
    }
    let frame = (phys_addr as usize) / PAGE_SIZE;
    if frame >= MAX_FRAMES {
        return;
    }
    lock_pmm();
    let word = frame / 64;
    let bit = frame % 64;
    if (GLOBAL_PMM.bitmap[word] & (1u64 << bit)) == 0 {
        GLOBAL_PMM.bitmap[word] |= 1u64 << bit;
        GLOBAL_PMM.total_free += 1;
    }
    unlock_pmm();
}

pub fn free_frame_count() -> usize {
    lock_pmm();
    let count = unsafe { GLOBAL_PMM.total_free };
    unlock_pmm();
    count
}

#[inline]
pub fn is_canonical(vaddr: u64) -> bool {
    let sign_ext = ((vaddr as i64) << 16) >> 16;
    (sign_ext as u64) == vaddr
}

#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3 & ADDR_MASK_4K
}

#[inline]
pub unsafe fn write_cr3(phys_pml4: u64) {
    asm!("mov cr3, {}", in(reg) (phys_pml4 & ADDR_MASK_4K), options(nostack, preserves_flags));
}

#[inline]
pub fn flush_tlb_page(vaddr: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) vaddr, options(nostack, preserves_flags));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AddressSpace {
    pub pml4_phys: u64,
}

impl AddressSpace {
    #[inline]
    pub fn current() -> Self {
        Self { pml4_phys: read_cr3() }
    }

    /// Aktif çekirdek eşlemelerini kopyalar ancak çekirdeğin kendi tablolarına asla yazmaz
    pub fn create_user_space() -> Result<Self, VmError> {
        let new_pml4 = alloc_frame().ok_or(VmError::OutOfMemory)?;
        let cur_pml4 = read_cr3();
        unsafe {
            let src = cur_pml4 as *const u64;
            let dst = new_pml4 as *mut u64;

            // 512 PML4 girişinin tamamını temizle
            core::ptr::write_bytes(dst, 0, 512);

            // PML4[0] (0..512 GiB Çekirdek, MMIO, Framebuffer kimlik haritası)
            // U/S=0 (Supervisor Only) olarak güvenli şekilde kopyalanır
            let kernel_entry = core::ptr::read_volatile(src.add(0)) & !PTE_USER;
            core::ptr::write_volatile(dst.add(0), kernel_entry);
        }
        Ok(Self { pml4_phys: new_pml4 })
    }

    pub fn map_4k(
        &mut self,
        vaddr: u64,
        paddr: u64,
        perms: PagePermissions,
    ) -> Result<(), VmError> {
        if (vaddr & 0xFFF) != 0 || (paddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        if !is_canonical(vaddr) {
            return Err(VmError::NonCanonicalAddress);
        }
        perms.validate_wx()?;

        if perms.user && vaddr < USER_SPACE_START {
            return Err(VmError::SupervisorOverlap);
        }

        let nx = unsafe { GLOBAL_PMM.nx_supported };
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx   = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx   = ((vaddr >> 12) & 0x1FF) as usize;

        let dir_flags = PTE_PRESENT | PTE_WRITABLE | if perms.user { PTE_USER } else { 0 };

        unsafe {
            let pml4 = self.pml4_phys as *mut u64;
            let pdpt_phys = Self::ensure_next_table(pml4.add(pml4_idx), dir_flags)?;

            let pdpt = pdpt_phys as *mut u64;
            let pd_phys = Self::ensure_next_table(pdpt.add(pdpt_idx), dir_flags)?;

            let pd = pd_phys as *mut u64;
            let pde_ptr = pd.add(pd_idx);
            let pde = core::ptr::read_volatile(pde_ptr);

            let pt_phys = if (pde & PTE_PRESENT) != 0 && (pde & PTE_HUGE) != 0 {
                Self::split_2m_huge_page(pde_ptr, pde)?
            } else {
                Self::ensure_next_table(pde_ptr, dir_flags)?
            };

            let pt = pt_phys as *mut u64;
            let pte_ptr = pt.add(pt_idx);
            let old_pte = core::ptr::read_volatile(pte_ptr);
            if (old_pte & PTE_PRESENT) != 0 {
                return Err(VmError::AlreadyMapped);
            }

            let leaf_flags = perms.to_pte_flags(nx);
            core::ptr::write_volatile(pte_ptr, (paddr & ADDR_MASK_4K) | leaf_flags);
        }

        flush_tlb_page(vaddr);
        Ok(())
    }

    pub fn protect_4k(&mut self, vaddr: u64, perms: PagePermissions) -> Result<(), VmError> {
        if (vaddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        perms.validate_wx()?;
        if perms.user && vaddr < USER_SPACE_START {
            return Err(VmError::SupervisorOverlap);
        }
        let nx = unsafe { GLOBAL_PMM.nx_supported };
        let pte_ptr = unsafe { self.walk_to_pte(vaddr)? };
        unsafe {
            let old = core::ptr::read_volatile(pte_ptr);
            if (old & PTE_PRESENT) == 0 {
                return Err(VmError::NotMapped);
            }
            let phys = old & ADDR_MASK_4K;
            let new_flags = perms.to_pte_flags(nx);
            core::ptr::write_volatile(pte_ptr, phys | new_flags);
        }
        flush_tlb_page(vaddr);
        Ok(())
    }

    pub fn unmap_4k(&mut self, vaddr: u64) -> Result<u64, VmError> {
        if (vaddr & 0xFFF) != 0 {
            return Err(VmError::UnalignedAddress);
        }
        let pte_ptr = unsafe { self.walk_to_pte(vaddr)? };
        let phys = unsafe {
            let old = core::ptr::read_volatile(pte_ptr);
            if (old & PTE_PRESENT) == 0 {
                return Err(VmError::NotMapped);
            }
            core::ptr::write_volatile(pte_ptr, 0);
            old & ADDR_MASK_4K
        };
        flush_tlb_page(vaddr);
        Ok(phys)
    }

    pub fn translate_with_flags(&self, vaddr: u64) -> Option<(u64, u64)> {
        if !is_canonical(vaddr) {
            return None;
        }
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx   = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx   = ((vaddr >> 12) & 0x1FF) as usize;

        unsafe {
            let pml4e = core::ptr::read_volatile((self.pml4_phys as *const u64).add(pml4_idx));
            if (pml4e & PTE_PRESENT) == 0 {
                return None;
            }
            let pdpte = core::ptr::read_volatile(((pml4e & ADDR_MASK_4K) as *const u64).add(pdpt_idx));
            if (pdpte & PTE_PRESENT) == 0 {
                return None;
            }
            let pde = core::ptr::read_volatile(((pdpte & ADDR_MASK_4K) as *const u64).add(pd_idx));
            if (pde & PTE_PRESENT) == 0 {
                return None;
            }

            let dir_user = (pml4e & PTE_USER) != 0 && (pdpte & PTE_USER) != 0 && (pde & PTE_USER) != 0;
            let dir_write = (pml4e & PTE_WRITABLE) != 0 && (pdpte & PTE_WRITABLE) != 0 && (pde & PTE_WRITABLE) != 0;
            let dir_nx = (pml4e & PTE_NO_EXEC) != 0 || (pdpte & PTE_NO_EXEC) != 0 || (pde & PTE_NO_EXEC) != 0;

            if (pde & PTE_HUGE) != 0 {
                let paddr = (pde & ADDR_MASK_2M) | (vaddr & 0x1F_FFFF);
                let mut flags = PTE_PRESENT | PTE_HUGE;
                if dir_user { flags |= PTE_USER; }
                if dir_write { flags |= PTE_WRITABLE; }
                if dir_nx { flags |= PTE_NO_EXEC; }
                return Some((paddr, flags));
            }

            let pte = core::ptr::read_volatile(((pde & ADDR_MASK_4K) as *const u64).add(pt_idx));
            if (pte & PTE_PRESENT) == 0 {
                return None;
            }

            let eff_user = dir_user && ((pte & PTE_USER) != 0);
            let eff_write = dir_write && ((pte & PTE_WRITABLE) != 0);
            let eff_nx = dir_nx || ((pte & PTE_NO_EXEC) != 0);

            let paddr = (pte & ADDR_MASK_4K) | (vaddr & 0xFFF);
            let mut flags = PTE_PRESENT;
            if eff_user { flags |= PTE_USER; }
            if eff_write { flags |= PTE_WRITABLE; }
            if eff_nx { flags |= PTE_NO_EXEC; }

            Some((paddr, flags))
        }
    }

    unsafe fn ensure_next_table(entry_ptr: *mut u64, req_flags: u64) -> Result<u64, VmError> {
        let cur = core::ptr::read_volatile(entry_ptr);
        if (cur & PTE_PRESENT) != 0 {
            let need_user = (req_flags & PTE_USER) != 0 && (cur & PTE_USER) == 0;
            let need_write = (req_flags & PTE_WRITABLE) != 0 && (cur & PTE_WRITABLE) == 0;
            if need_user || need_write {
                let phys = cur & ADDR_MASK_4K;
                // Yalnızca PMM tarafından yönetilen dinamik RAM tablolarına izin güncellemesi yap
                if phys >= 0x0100_0000 && phys < 0x2000_0000 {
                    core::ptr::write_volatile(entry_ptr, cur | (req_flags & (PTE_USER | PTE_WRITABLE)));
                }
            }
            return Ok(cur & ADDR_MASK_4K);
        }
        let frame = alloc_frame().ok_or(VmError::OutOfMemory)?;
        core::ptr::write_volatile(entry_ptr, frame | req_flags);
        Ok(frame)
    }


    unsafe fn split_2m_huge_page(pde_ptr: *mut u64, old_pde: u64) -> Result<u64, VmError> {
        let new_pt = alloc_frame().ok_or(VmError::OutOfMemory)?;
        let base_phys = old_pde & ADDR_MASK_2M;
        let inherited_flags = old_pde & (PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_PWT | PTE_PCD | PTE_NO_EXEC);
        let pt_ptr = new_pt as *mut u64;
        for i in 0..512usize {
            let page_phys = base_phys + (i as u64) * 4096;
            core::ptr::write_volatile(pt_ptr.add(i), page_phys | inherited_flags);
        }
        let dir_flags = PTE_PRESENT | PTE_WRITABLE | (old_pde & PTE_USER);
        core::ptr::write_volatile(pde_ptr, new_pt | dir_flags);
        Ok(new_pt)
    }

    unsafe fn walk_to_pte(&self, vaddr: u64) -> Result<*mut u64, VmError> {
        if !is_canonical(vaddr) {
            return Err(VmError::NonCanonicalAddress);
        }
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx   = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx   = ((vaddr >> 12) & 0x1FF) as usize;

        let pml4e = core::ptr::read_volatile((self.pml4_phys as *const u64).add(pml4_idx));
        if (pml4e & PTE_PRESENT) == 0 {
            return Err(VmError::NotMapped);
        }
        let pdpte = core::ptr::read_volatile(((pml4e & ADDR_MASK_4K) as *const u64).add(pdpt_idx));
        if (pdpte & PTE_PRESENT) == 0 {
            return Err(VmError::NotMapped);
        }
        let pde = core::ptr::read_volatile(((pdpte & ADDR_MASK_4K) as *const u64).add(pd_idx));
        if (pde & PTE_PRESENT) == 0 || (pde & PTE_HUGE) != 0 {
            return Err(VmError::NotMapped);
        }
        Ok(((pde & ADDR_MASK_4K) as *mut u64).add(pt_idx))
    }

    pub fn validate_user_range(
        &self,
        user_addr: u64,
        len: usize,
        require_writable: bool,
    ) -> Result<(), VmError> {
        if len == 0 {
            return Ok(());
        }
        if user_addr < USER_SPACE_START || user_addr > USER_SPACE_END {
            return Err(VmError::SupervisorOverlap);
        }
        let end = user_addr
            .checked_add((len - 1) as u64)
            .ok_or(VmError::AddressOverflow)?;
        if end > USER_SPACE_END {
            return Err(VmError::SupervisorOverlap);
        }

        let start_page = user_addr & ADDR_MASK_4K;
        let end_page = end & ADDR_MASK_4K;
        let mut page = start_page;
        loop {
            let (_, flags) = self.translate_with_flags(page).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            if require_writable && (flags & PTE_WRITABLE) == 0 {
                return Err(VmError::WxViolation);
            }
            if page >= end_page {
                break;
            }
            page = match page.checked_add(PAGE_SIZE as u64) {
                Some(next) => next,
                None => break,
            };
        }
        Ok(())
    }

    /// Kullanıcı adres alanından çekirdek tamponuna sayfa-sayfa sınır denetimli kopyalama
    pub fn copy_from_user(&self, user_src: u64, kernel_dst: &mut [u8]) -> Result<(), VmError> {
        let len = kernel_dst.len();
        if len == 0 {
            return Ok(());
        }
        if len > 64 * 1024 {
            return Err(VmError::AddressOverflow);
        }
        self.validate_user_range(user_src, len, false)?;

        let mut copied = 0usize;
        while copied < len {
            let cur_vaddr = user_src + (copied as u64);
            let page_off = (cur_vaddr & 0xFFF) as usize;
            let chunk = (PAGE_SIZE - page_off).min(len - copied);
            let (paddr, flags) = self.translate_with_flags(cur_vaddr).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    paddr as *const u8,
                    kernel_dst.as_mut_ptr().add(copied),
                    chunk,
                );
            }
            copied += chunk;
        }
        Ok(())
    }

    /// Çekirdek tamponundan kullanıcı adres alanına sayfa-sayfa sınır ve yazma izni denetimli kopyalama
    pub fn copy_to_user(&self, user_dst: u64, kernel_src: &[u8]) -> Result<(), VmError> {
        let len = kernel_src.len();
        if len == 0 {
            return Ok(());
        }
        if len > 64 * 1024 {
            return Err(VmError::AddressOverflow);
        }
        self.validate_user_range(user_dst, len, true)?;

        let mut copied = 0usize;
        while copied < len {
            let cur_vaddr = user_dst + (copied as u64);
            let page_off = (cur_vaddr & 0xFFF) as usize;
            let chunk = (PAGE_SIZE - page_off).min(len - copied);
            let (paddr, flags) = self.translate_with_flags(cur_vaddr).ok_or(VmError::NotMapped)?;
            if (flags & PTE_USER) == 0 || (flags & PTE_WRITABLE) == 0 {
                return Err(VmError::SupervisorOverlap);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    kernel_src.as_ptr().add(copied),
                    paddr as *mut u8,
                    chunk,
                );
            }
            copied += chunk;
        }
        Ok(())
    }
}

/// Faz 1 Tamamlanma Testi: W^X, Supervisor Koruması, 4K Map/Unmap ve Kanonik Sınır Doğrulaması
pub fn run_phase1_vm_self_test() -> Result<(), &'static str> {
    let cur_cr3 = read_cr3();

    // 1. Çekirdek kod adresini doğrula (0x100000): U/S=0 olmalı
    let cur = AddressSpace { pml4_phys: cur_cr3 };
    let (_, k_flags) = cur.translate_with_flags(0x0010_0000).ok_or("Kernel code mapping bulunamadi")?;
    if (k_flags & PTE_USER) != 0 {
        return Err("Guvenlik Ihlali: Cekirdek sayfasi U/S=1 (User) olarak isaretlenmis!");
    }

    // 2. Yeni ve izole kullanıcı adres alanı oluştur
    let mut user_space = AddressSpace::create_user_space().map_err(|_| "User address space olusturulamadi")?;

    // 3. W^X Testi
    let test_frame = alloc_frame().ok_or("PMM test frame ayiramadi")?;
    let wx_perms = PagePermissions { readable: true, writable: true, executable: true, user: true };
    if user_space.map_4k(USER_SPACE_START, test_frame, wx_perms) != Err(VmError::WxViolation) {
        unsafe { free_frame(test_frame); }
        return Err("W^X ihlali engellenmedi!");
    }

    // 4. Supervisor Koruma Testi (0x0020_0000 çekirdek bölgesidir, kullanıcı eşlemesi reddedilmeli)
    if user_space.map_4k(0x0020_0000, test_frame, PagePermissions::USER_RW) != Err(VmError::SupervisorOverlap) {
        unsafe { free_frame(test_frame); }
        return Err("Supervisor bolgesine User eslemesi engellenmedi!");
    }

    // 5. 4 KiB Kullanıcı Sayfası Eşle ve Doğrula
    user_space.map_4k(USER_SPACE_START, test_frame, PagePermissions::USER_RW)
        .map_err(|_| "Gecerli 4K User eslemesi basarisiz")?;

    let (mapped_phys, u_flags) = user_space.translate_with_flags(USER_SPACE_START)
        .ok_or("Eslenen 4K User sayfasi translate edilemedi")?;
    if mapped_phys != test_frame || (u_flags & PTE_USER) == 0 {
        return Err("4K User sayfasi fiziksel adresi veya U/S bayragi hatali");
    }

    // Unmap ve serbest bırak
    let unmapped = user_space.unmap_4k(USER_SPACE_START).map_err(|_| "4K unmap basarisiz")?;
    unsafe { 
        free_frame(unmapped);
        free_frame(user_space.pml4_phys);
    }

    Ok(())
}
