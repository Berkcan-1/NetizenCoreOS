//! Katı ELF64 yükleme planlayıcısı (Ring 3 uygulamaları için).
//!
//! Bu crate hiçbir şeyi BELLEĞE YÜKLEMEZ; yalnızca güvenilmeyen baytları doğrular ve çekirdeğin
//! uygulayacağı bir `LoadPlan` üretir. Kabul kuralları bilerek dardır:
//!   - ELF64, little-endian, x86-64, ET_EXEC (konuma bağımlı; yeniden konumlandırma yok)
//!   - yalnızca PT_LOAD (+ zararsız NOTE/PHDR/GNU_*); INTERP/DYNAMIC/TLS reddedilir
//!   - W^X: hiçbir segment hem yazılabilir hem yürütülebilir olamaz; GNU_STACK yürütülebilir olamaz
//!   - tüm segmentler `Policy` aralığında, sayfa düzeyinde çakışmasız
//!   - giriş noktası yürütülebilir bir segmentin dosya-destekli kısmında
#![no_std]

pub const PAGE: u64 = 4096;
pub const MAX_SEGMENTS: usize = 8;
pub const MAX_PHDRS: u16 = 16;

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
const PT_NULL: u32 = 0;
const PT_LOAD: u32 = 1;
const PT_NOTE: u32 = 4;
const PT_PHDR: u32 = 6;
const PT_GNU_EH_FRAME: u32 = 0x6474_E550;
const PT_GNU_STACK: u32 = 0x6474_E551;
const PT_GNU_RELRO: u32 = 0x6474_E552;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub min_vaddr: u64,
    pub max_vaddr: u64,
    pub max_pages: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub vaddr: u64,
    pub mem_size: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub flags: u32,
}

impl Segment {
    pub const EMPTY: Segment = Segment { vaddr: 0, mem_size: 0, file_offset: 0, file_size: 0, flags: 0 };

    pub fn page_start(&self) -> u64 {
        self.vaddr & !(PAGE - 1)
    }

    /// Sayfa sınırına yuvarlanmış bitiş (bu değer `Policy` doğrulamasında taşmadan geçmiştir).
    pub fn page_end(&self) -> u64 {
        (self.vaddr + self.mem_size + PAGE - 1) & !(PAGE - 1)
    }

    pub fn page_count(&self) -> u64 {
        (self.page_end() - self.page_start()) / PAGE
    }

    pub fn writable(&self) -> bool {
        self.flags & PF_W != 0
    }

    pub fn executable(&self) -> bool {
        self.flags & PF_X != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadPlan {
    pub entry: u64,
    pub segments: [Segment; MAX_SEGMENTS],
    pub count: usize,
    pub total_pages: u64,
}

impl LoadPlan {
    pub fn segments(&self) -> &[Segment] {
        &self.segments[..self.count]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    TooShort,
    BadMagic,
    NotElf64,
    NotLittleEndian,
    BadVersion,
    NotExec,
    NotX86_64,
    BadHeaderSize,
    BadPhdrTable,
    UnsupportedSegment,
    ExecutableStack,
    NoLoadSegments,
    TooManySegments,
    BadFlags,
    WritableExecutable,
    SizeMismatch,
    FileRange,
    Misaligned,
    OutOfRange,
    Overlap,
    TooLarge,
    BadEntry,
}

impl ElfError {
    pub fn as_str(self) -> &'static str {
        match self {
            ElfError::TooShort => "ELF cok kisa",
            ElfError::BadMagic => "ELF sihirli sayisi gecersiz",
            ElfError::NotElf64 => "ELF64 degil",
            ElfError::NotLittleEndian => "little-endian degil",
            ElfError::BadVersion => "ELF surumu gecersiz",
            ElfError::NotExec => "ET_EXEC degil",
            ElfError::NotX86_64 => "x86-64 degil",
            ElfError::BadHeaderSize => "baslik boyutu gecersiz",
            ElfError::BadPhdrTable => "program baslik tablosu gecersiz",
            ElfError::UnsupportedSegment => "desteklenmeyen segment turu",
            ElfError::ExecutableStack => "yurutulebilir yigin istegi",
            ElfError::NoLoadSegments => "yuklenecek segment yok",
            ElfError::TooManySegments => "cok fazla segment",
            ElfError::BadFlags => "segment izinleri gecersiz",
            ElfError::WritableExecutable => "segment hem yazilabilir hem yurutulebilir",
            ElfError::SizeMismatch => "dosya boyutu bellek boyutundan buyuk",
            ElfError::FileRange => "segment dosya araligi disinda",
            ElfError::Misaligned => "segment ofset/adres uyumsuz",
            ElfError::OutOfRange => "segment izinli adres araliginin disinda",
            ElfError::Overlap => "segmentler sayfa duzeyinde cakisiyor",
            ElfError::TooLarge => "imaj cok buyuk",
            ElfError::BadEntry => "giris noktasi yurutulebilir segmentte degil",
        }
    }
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3], b[off + 4], b[off + 5], b[off + 6], b[off + 7]])
}

pub fn parse(bytes: &[u8], policy: &Policy) -> Result<LoadPlan, ElfError> {
    if bytes.len() < EHDR_SIZE {
        return Err(ElfError::TooShort);
    }
    if bytes[0..4] != [0x7F, b'E', b'L', b'F'] {
        return Err(ElfError::BadMagic);
    }
    if bytes[4] != 2 {
        return Err(ElfError::NotElf64);
    }
    if bytes[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }
    if bytes[6] != 1 || u32_at(bytes, 20) != 1 {
        return Err(ElfError::BadVersion);
    }
    if u16_at(bytes, 16) != 2 {
        return Err(ElfError::NotExec);
    }
    if u16_at(bytes, 18) != 62 {
        return Err(ElfError::NotX86_64);
    }
    if u16_at(bytes, 52) as usize != EHDR_SIZE || u16_at(bytes, 54) as usize != PHDR_SIZE {
        return Err(ElfError::BadHeaderSize);
    }

    let entry = u64_at(bytes, 24);
    let phoff = u64_at(bytes, 32);
    let phnum = u16_at(bytes, 56);
    if phnum == 0 || phnum > MAX_PHDRS {
        return Err(ElfError::BadPhdrTable);
    }
    let table_len = (phnum as u64) * (PHDR_SIZE as u64);
    let table_end = phoff.checked_add(table_len).ok_or(ElfError::BadPhdrTable)?;
    if phoff < EHDR_SIZE as u64 || table_end > bytes.len() as u64 {
        return Err(ElfError::BadPhdrTable);
    }

    let mut plan = LoadPlan { entry, segments: [Segment::EMPTY; MAX_SEGMENTS], count: 0, total_pages: 0 };

    for index in 0..phnum as usize {
        let base = phoff as usize + index * PHDR_SIZE;
        let p_type = u32_at(bytes, base);
        let p_flags = u32_at(bytes, base + 4);
        match p_type {
            PT_NULL | PT_NOTE | PT_PHDR | PT_GNU_EH_FRAME | PT_GNU_RELRO => continue,
            PT_GNU_STACK => {
                if p_flags & PF_X != 0 {
                    return Err(ElfError::ExecutableStack);
                }
                continue;
            }
            PT_LOAD => {}
            _ => return Err(ElfError::UnsupportedSegment),
        }

        let offset = u64_at(bytes, base + 8);
        let vaddr = u64_at(bytes, base + 16);
        let filesz = u64_at(bytes, base + 32);
        let memsz = u64_at(bytes, base + 40);

        if memsz == 0 {
            continue;
        }
        if p_flags == 0 || p_flags & !(PF_R | PF_W | PF_X) != 0 {
            return Err(ElfError::BadFlags);
        }
        if p_flags & PF_W != 0 && p_flags & PF_X != 0 {
            return Err(ElfError::WritableExecutable);
        }
        if filesz > memsz {
            return Err(ElfError::SizeMismatch);
        }
        let file_end = offset.checked_add(filesz).ok_or(ElfError::FileRange)?;
        if file_end > bytes.len() as u64 {
            return Err(ElfError::FileRange);
        }
        // Dosya-destekli veri varsa ofset/adres sayfa içinde uyumlu olmalı. Yalnızca .bss içeren
        // segmentte (filesz = 0) ofset anlamsızdır (ELF kuralı) ve bağlayıcılar onu hizalamaz.
        if filesz > 0 && vaddr % PAGE != offset % PAGE {
            return Err(ElfError::Misaligned);
        }
        let mem_end = vaddr.checked_add(memsz).ok_or(ElfError::OutOfRange)?;
        let page_end = mem_end.checked_add(PAGE - 1).ok_or(ElfError::OutOfRange)? & !(PAGE - 1);
        if vaddr < policy.min_vaddr || page_end > policy.max_vaddr {
            return Err(ElfError::OutOfRange);
        }
        if plan.count >= MAX_SEGMENTS {
            return Err(ElfError::TooManySegments);
        }
        plan.segments[plan.count] = Segment { vaddr, mem_size: memsz, file_offset: offset, file_size: filesz, flags: p_flags };
        plan.count += 1;
    }

    if plan.count == 0 {
        return Err(ElfError::NoLoadSegments);
    }

    // Sayfa düzeyinde çakışma: farklı izinli segmentler aynı sayfayı paylaşamaz.
    for i in 0..plan.count {
        for j in i + 1..plan.count {
            let (a, b) = (&plan.segments[i], &plan.segments[j]);
            if a.page_start() < b.page_end() && b.page_start() < a.page_end() {
                return Err(ElfError::Overlap);
            }
        }
    }

    // Adrese göre sırala (ekleme sıralaması; en fazla 8 öğe).
    for i in 1..plan.count {
        let mut j = i;
        while j > 0 && plan.segments[j - 1].vaddr > plan.segments[j].vaddr {
            plan.segments.swap(j - 1, j);
            j -= 1;
        }
    }

    let mut total = 0u64;
    for segment in plan.segments() {
        total = total.checked_add(segment.page_count()).ok_or(ElfError::TooLarge)?;
    }
    if total > policy.max_pages {
        return Err(ElfError::TooLarge);
    }
    plan.total_pages = total;

    let entry_ok = plan.segments().iter().any(|s| {
        s.executable() && entry >= s.vaddr && entry < s.vaddr.saturating_add(s.file_size)
    });
    if !entry_ok {
        return Err(ElfError::BadEntry);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests;
