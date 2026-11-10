use super::*;

const POLICY: Policy = Policy { min_vaddr: 0x80_0100_0000, max_vaddr: 0x80_0800_0000, max_pages: 8192 };
const REAL: &[u8] = include_bytes!("../../../kernel/faz6/faz6_app.elf");

#[derive(Clone, Copy)]
struct Ph {
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

fn load(flags: u32, offset: u64, vaddr: u64, filesz: u64, memsz: u64) -> Ph {
    Ph { p_type: PT_LOAD, flags, offset, vaddr, filesz, memsz }
}

fn base_phs() -> [Ph; 3] {
    [
        load(PF_R | PF_X, 0x1000, 0x80_0100_0000, 0x100, 0x100),
        load(PF_R, 0x2000, 0x80_0100_1000, 0x80, 0x80),
        load(PF_R | PF_W, 0x3000, 0x80_0100_2000, 4, 0x50),
    ]
}

const LEN: usize = 0x3004;

fn build(entry: u64, phs: &[Ph]) -> [u8; 0x4000] {
    let mut b = [0u8; 0x4000];
    b[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    b[4] = 2;
    b[5] = 1;
    b[6] = 1;
    b[16..18].copy_from_slice(&2u16.to_le_bytes());
    b[18..20].copy_from_slice(&62u16.to_le_bytes());
    b[20..24].copy_from_slice(&1u32.to_le_bytes());
    b[24..32].copy_from_slice(&entry.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[52..54].copy_from_slice(&64u16.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&(phs.len() as u16).to_le_bytes());
    for (i, p) in phs.iter().enumerate() {
        let o = 64 + i * 56;
        b[o..o + 4].copy_from_slice(&p.p_type.to_le_bytes());
        b[o + 4..o + 8].copy_from_slice(&p.flags.to_le_bytes());
        b[o + 8..o + 16].copy_from_slice(&p.offset.to_le_bytes());
        b[o + 16..o + 24].copy_from_slice(&p.vaddr.to_le_bytes());
        b[o + 24..o + 32].copy_from_slice(&p.vaddr.to_le_bytes());
        b[o + 32..o + 40].copy_from_slice(&p.filesz.to_le_bytes());
        b[o + 40..o + 48].copy_from_slice(&p.memsz.to_le_bytes());
        b[o + 48..o + 56].copy_from_slice(&0x1000u64.to_le_bytes());
    }
    b
}

fn parse_with(entry: u64, phs: &[Ph]) -> Result<LoadPlan, ElfError> {
    let elf = build(entry, phs);
    parse(&elf[..LEN], &POLICY)
}

const ENTRY: u64 = 0x80_0100_0000;

#[test]
fn accepts_valid_image() {
    let plan = parse_with(ENTRY, &base_phs()).unwrap();
    assert_eq!(plan.count, 3);
    assert_eq!(plan.entry, ENTRY);
    assert_eq!(plan.total_pages, 3);
    assert_eq!(plan.segments()[2].mem_size, 0x50);
    assert!(plan.segments()[0].executable() && !plan.segments()[0].writable());
}

#[test]
fn sorts_segments_by_address() {
    let [a, b, c] = base_phs();
    let plan = parse_with(ENTRY, &[c, a, b]).unwrap();
    let addrs = [plan.segments()[0].vaddr, plan.segments()[1].vaddr, plan.segments()[2].vaddr];
    assert!(addrs[0] < addrs[1] && addrs[1] < addrs[2]);
}

#[test]
fn rejects_bad_headers() {
    let good = build(ENTRY, &base_phs());
    let check = |mutate: &dyn Fn(&mut [u8; 0x4000]), expected: ElfError| {
        let mut b = good;
        mutate(&mut b);
        assert_eq!(parse(&b[..LEN], &POLICY), Err(expected));
    };
    check(&|b| b[0] = 0, ElfError::BadMagic);
    check(&|b| b[4] = 1, ElfError::NotElf64);
    check(&|b| b[5] = 2, ElfError::NotLittleEndian);
    check(&|b| b[6] = 2, ElfError::BadVersion);
    check(&|b| b[16] = 3, ElfError::NotExec);
    check(&|b| b[18] = 3, ElfError::NotX86_64);
    check(&|b| b[52] = 60, ElfError::BadHeaderSize);
    check(&|b| b[54] = 32, ElfError::BadHeaderSize);
    check(&|b| b[56] = 0, ElfError::BadPhdrTable);
    check(&|b| b[56] = 17, ElfError::BadPhdrTable);
    check(&|b| b[32..40].copy_from_slice(&u64::MAX.to_le_bytes()), ElfError::BadPhdrTable);
    check(&|b| b[32..40].copy_from_slice(&(LEN as u64 - 10).to_le_bytes()), ElfError::BadPhdrTable);
    check(&|b| b[32..40].copy_from_slice(&8u64.to_le_bytes()), ElfError::BadPhdrTable);
    assert_eq!(parse(&good[..63], &POLICY), Err(ElfError::TooShort));
    assert_eq!(parse(&[], &POLICY), Err(ElfError::TooShort));
}

#[test]
fn rejects_unsafe_segments() {
    let [text, ro, data] = base_phs();
    let mut wx = text;
    wx.flags = PF_R | PF_W | PF_X;
    assert_eq!(parse_with(ENTRY, &[wx, ro, data]), Err(ElfError::WritableExecutable));

    let mut noflags = ro;
    noflags.flags = 0;
    assert_eq!(parse_with(ENTRY, &[text, noflags, data]), Err(ElfError::BadFlags));
    let mut junk = ro;
    junk.flags = PF_R | 0x0100_0000;
    assert_eq!(parse_with(ENTRY, &[text, junk, data]), Err(ElfError::BadFlags));

    let mut big_file = data;
    big_file.filesz = 0x60;
    assert_eq!(parse_with(ENTRY, &[text, ro, big_file]), Err(ElfError::SizeMismatch));

    let mut past_eof = data;
    past_eof.offset = 0x3000;
    past_eof.filesz = 0x1000;
    past_eof.memsz = 0x1000;
    assert_eq!(parse_with(ENTRY, &[text, ro, past_eof]), Err(ElfError::FileRange));
    let mut wrap = data;
    wrap.offset = u64::MAX - 1;
    wrap.filesz = 8;
    assert_eq!(parse_with(ENTRY, &[text, ro, wrap]), Err(ElfError::FileRange));

    let mut skew = ro;
    skew.offset = 0x2010;
    assert_eq!(parse_with(ENTRY, &[text, skew, data]), Err(ElfError::Misaligned));
}

#[test]
fn bss_only_segment_may_have_any_file_offset() {
    let [text, ro, data] = base_phs();
    // Yalnızca .bss: filesz = 0, ofset sayfa-uyumsuz olabilir ama dosya içinde kalmalı.
    let bss_only = load(PF_R | PF_W, 0x3003, 0x80_0100_2000, 0, 0x4000);
    let plan = parse_with(ENTRY, &[text, ro, bss_only]).unwrap();
    assert_eq!(plan.segments()[2].file_size, 0);
    assert_eq!(plan.total_pages, 1 + 1 + 4);
    // Dosya-destekli veri varken uyumsuzluk hâlâ reddedilir.
    let mut skew = data;
    skew.offset = 0x3001;
    skew.filesz = 2;
    assert_eq!(parse_with(ENTRY, &[text, ro, skew]), Err(ElfError::Misaligned));
    // Ofset dosyanın dışında olamaz.
    let mut outside = bss_only;
    outside.offset = LEN as u64 + 1;
    assert_eq!(parse_with(ENTRY, &[text, ro, outside]), Err(ElfError::FileRange));
}

#[test]
fn rejects_addresses_outside_policy() {
    let [text, ro, data] = base_phs();
    for bad in [0u64, 0x1000, 0x80_0000_0000, 0x80_00FF_F000, 0xFFFF_8000_0000_0000, 0x80_0800_0000] {
        let mut t = text;
        t.vaddr = bad;
        t.offset = 0x1000 + (bad & 0xFFF);
        assert_eq!(parse_with(bad, &[t, ro, data]), Err(ElfError::OutOfRange), "vaddr {:#x}", bad);
    }
    let mut spill = data;
    spill.vaddr = 0x80_07FF_F000;
    spill.memsz = 0x2000;
    spill.filesz = 4;
    assert_eq!(parse_with(ENTRY, &[text, ro, spill]), Err(ElfError::OutOfRange));
    let mut wrap = data;
    wrap.vaddr = u64::MAX - 0xFFF;
    wrap.offset = 0x3000 + 0;
    wrap.memsz = 0x2000;
    wrap.vaddr &= !0xFFF;
    assert_eq!(parse_with(ENTRY, &[text, ro, wrap]), Err(ElfError::OutOfRange));
    let mut huge = data;
    huge.memsz = u64::MAX;
    assert_eq!(parse_with(ENTRY, &[text, ro, huge]), Err(ElfError::OutOfRange));
}

#[test]
fn rejects_overlap_and_oversize() {
    let [text, ro, data] = base_phs();
    let mut same_page = ro;
    same_page.vaddr = 0x80_0100_0800;
    same_page.offset = 0x2800;
    assert_eq!(parse_with(ENTRY, &[text, same_page, data]), Err(ElfError::Overlap));
    let mut dup = data;
    dup.vaddr = text.vaddr;
    dup.offset = 0x1000;
    assert_eq!(parse_with(ENTRY, &[text, ro, dup]), Err(ElfError::Overlap));

    // 8193 sayfalık .bss: aralığa sığar ama toplam sayfa sınırını (8192) aşar.
    let mut big = data;
    big.memsz = 8193 * 4096;
    assert_eq!(parse_with(ENTRY, &[text, ro, big]), Err(ElfError::TooLarge));
}

#[test]
fn rejects_bad_entry_and_segment_kinds() {
    let [text, ro, data] = base_phs();
    assert_eq!(parse_with(ro.vaddr, &base_phs()), Err(ElfError::BadEntry));
    assert_eq!(parse_with(data.vaddr, &base_phs()), Err(ElfError::BadEntry));
    assert_eq!(parse_with(text.vaddr + 0x100, &base_phs()), Err(ElfError::BadEntry));
    assert_eq!(parse_with(0, &base_phs()), Err(ElfError::BadEntry));
    assert_eq!(parse_with(text.vaddr + 0xFF, &base_phs()).map(|p| p.count), Ok(3));

    let none = Ph { p_type: PT_NULL, flags: 0, offset: 0, vaddr: 0, filesz: 0, memsz: 0 };
    assert_eq!(parse_with(ENTRY, &[none]), Err(ElfError::NoLoadSegments));
    let empty_load = load(PF_R, 0x2000, 0x80_0100_5000, 0, 0);
    assert_eq!(parse_with(ENTRY, &[empty_load]), Err(ElfError::NoLoadSegments));

    for kind in [2u32 /*DYNAMIC*/, 3 /*INTERP*/, 7 /*TLS*/, 0x6474_E553, 0x7000_0000] {
        let weird = Ph { p_type: kind, flags: PF_R, offset: 0, vaddr: 0, filesz: 0, memsz: 0 };
        assert_eq!(parse_with(ENTRY, &[text, ro, data, weird]), Err(ElfError::UnsupportedSegment), "tur {:#x}", kind);
    }
    let exec_stack = Ph { p_type: PT_GNU_STACK, flags: PF_R | PF_W | PF_X, offset: 0, vaddr: 0, filesz: 0, memsz: 0 };
    assert_eq!(parse_with(ENTRY, &[text, ro, data, exec_stack]), Err(ElfError::ExecutableStack));
    let ok_stack = Ph { p_type: PT_GNU_STACK, flags: PF_R | PF_W, offset: 0, vaddr: 0, filesz: 0, memsz: 0 };
    assert!(parse_with(ENTRY, &[text, ro, data, ok_stack]).is_ok());
    let note = Ph { p_type: PT_NOTE, flags: PF_R, offset: 0x100, vaddr: 0, filesz: 8, memsz: 8 };
    assert!(parse_with(ENTRY, &[text, ro, data, note]).is_ok());
}

#[test]
fn rejects_too_many_segments() {
    let mut phs = [load(PF_R, 0x2000, 0x80_0100_1000, 8, 8); 9];
    for (i, p) in phs.iter_mut().enumerate() {
        p.vaddr = 0x80_0100_1000 + (i as u64) * 0x1000;
        p.offset = 0x2000;
        if i == 0 {
            p.flags = PF_R | PF_X;
        }
    }
    assert_eq!(parse_with(0x80_0100_1000, &phs), Err(ElfError::TooManySegments));
    assert!(parse_with(0x80_0100_1000, &phs[..8]).is_ok());
}

#[test]
fn accepts_real_linker_output() {
    let plan = parse(REAL, &POLICY).expect("gercek ELF kabul edilmeli");
    assert_eq!(plan.entry, 0x80_0100_0000);
    assert_eq!(plan.count, 3);
    let s = plan.segments();
    assert!(s[0].executable() && !s[0].writable() && s[0].vaddr == 0x80_0100_0000);
    assert!(!s[1].executable() && !s[1].writable() && s[1].vaddr == 0x80_0100_1000);
    assert!(s[2].writable() && !s[2].executable() && s[2].vaddr == 0x80_0100_2000);
    assert!(s[2].mem_size > s[2].file_size, ".bss bellek boyutunu buyutmeli");
    assert_eq!(plan.total_pages, 3);
}

#[test]
fn mutation_fuzz_never_panics_and_keeps_invariants() {
    let mut state: u64 = 0xD1B5_4A32_D192_ED03;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let header_len = 64 + 3 * 56;
    let mut accepted = 0u32;
    for _ in 0..60_000 {
        let mut b = [0u8; 16384];
        b[..REAL.len()].copy_from_slice(REAL);
        let mut len = REAL.len();
        for _ in 0..(1 + next() % 4) {
            let pos = (next() as usize) % header_len;
            b[pos] = (next() & 0xFF) as u8;
        }
        if next() % 5 == 0 {
            len = (next() as usize) % (REAL.len() + 1);
        }
        if let Ok(plan) = parse(&b[..len], &POLICY) {
            accepted += 1;
            assert!(plan.count >= 1 && plan.count <= MAX_SEGMENTS);
            let mut pages = 0u64;
            for (i, s) in plan.segments().iter().enumerate() {
                assert!(s.flags & (PF_W | PF_X) != (PF_W | PF_X));
                assert!(s.page_start() >= POLICY.min_vaddr && s.page_end() <= POLICY.max_vaddr);
                assert!(s.file_size <= s.mem_size);
                assert!(s.file_offset + s.file_size <= len as u64);
                if s.file_size > 0 {
                    assert_eq!(s.vaddr % PAGE, s.file_offset % PAGE);
                }
                pages += s.page_count();
                for o in &plan.segments()[i + 1..] {
                    assert!(s.page_end() <= o.page_start() || o.page_end() <= s.page_start());
                }
            }
            assert_eq!(pages, plan.total_pages);
            assert!(plan.segments().iter().any(|s| s.executable() && plan.entry >= s.vaddr && plan.entry < s.vaddr + s.file_size));
        }
    }
    assert!(accepted > 0, "fuzz hic gecerli girdi uretmedi (test gecersiz)");
}
