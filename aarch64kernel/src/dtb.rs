const MAGIC: u32 = 0xD00D_FEED;
const TOKEN_BEGIN: u32 = 1;
const TOKEN_END_NODE: u32 = 2;
const TOKEN_PROP: u32 = 3;
const TOKEN_NOP: u32 = 4;
const MAX_RAM: usize = 8;
pub const MAX_RESERVED: usize = 16;
const MAX_DEPTH: usize = 12;
const MAX_DTB_BYTES: usize = 8 << 20;

#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub base: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

#[derive(Clone, Copy)]
pub struct DtbInfo {
    pub address: u64,
    pub size: u64,
    pub ram: [(u64, u64); MAX_RAM],
    pub ram_count: usize,
    pub reserved: [(u64, u64); MAX_RESERVED],
    pub reserved_count: usize,
    pub simple_fb: Option<Framebuffer>,
    pub lfb_base: u64,
    pub lfb_size: u64,
    pub timer_hz: u32,
}

impl DtbInfo {
    const fn empty(address: u64, size: u64) -> DtbInfo {
        DtbInfo {
            address,
            size,
            ram: [(0, 0); MAX_RAM],
            ram_count: 0,
            reserved: [(0, 0); MAX_RESERVED],
            reserved_count: 0,
            simple_fb: None,
            lfb_base: 0,
            lfb_size: 0,
            timer_hz: 0,
        }
    }

    fn add_reserved(&mut self, base: u64, size: u64) {
        if size > 0 && self.reserved_count < MAX_RESERVED {
            self.reserved[self.reserved_count] = (base, size);
            self.reserved_count += 1;
        }
    }
}

unsafe fn be32(address: usize) -> u32 {
    let b0 = core::ptr::read_volatile(address as *const u8) as u32;
    let b1 = core::ptr::read_volatile((address + 1) as *const u8) as u32;
    let b2 = core::ptr::read_volatile((address + 2) as *const u8) as u32;
    let b3 = core::ptr::read_volatile((address + 3) as *const u8) as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

unsafe fn cells(address: usize, count: u32) -> u64 {
    let mut value = 0u64;
    for index in 0..count as usize {
        value = (value << 32) | be32(address + index * 4) as u64;
    }
    value
}

unsafe fn cstr<'a>(address: usize, limit: usize) -> Option<&'a [u8]> {
    let mut length = 0usize;
    loop {
        if address + length >= limit || length >= 128 {
            return None;
        }
        if core::ptr::read_volatile((address + length) as *const u8) == 0 {
            break;
        }
        length += 1;
    }
    Some(core::slice::from_raw_parts(address as *const u8, length))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    for start in 0..=(haystack.len() - needle.len()) {
        if &haystack[start..start + needle.len()] == needle {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy)]
struct Pending {
    is_simple_fb: bool,
    reg: Option<(u64, u64)>,
    width: u32,
    height: u32,
    stride: u32,
}

impl Pending {
    const EMPTY: Pending = Pending { is_simple_fb: false, reg: None, width: 0, height: 0, stride: 0 };
}

pub fn parse(address: usize) -> Option<DtbInfo> {
    unsafe {
        if address == 0 || address & 7 != 0 || be32(address) != MAGIC {
            return None;
        }
        let total = be32(address + 4) as usize;
        if total < 40 || total > MAX_DTB_BYTES {
            return None;
        }
        let end = address.checked_add(total)?;
        let struct_offset = be32(address + 8) as usize;
        let strings_offset = be32(address + 12) as usize;
        let rsvmap_offset = be32(address + 16) as usize;
        if struct_offset >= total || strings_offset >= total || rsvmap_offset >= total {
            return None;
        }
        let structure = address + struct_offset;
        let strings = address + strings_offset;

        let mut info = DtbInfo::empty(address as u64, total as u64);

        let mut entry = address + rsvmap_offset;
        for _ in 0..MAX_RESERVED {
            if entry + 16 > end {
                break;
            }
            let base = cells(entry, 2);
            let size = cells(entry + 8, 2);
            if base == 0 && size == 0 {
                break;
            }
            info.add_reserved(base, size);
            entry += 16;
        }

        let mut address_cells = 2u32;
        let mut size_cells = 2u32;
        let mut depth = 0usize;
        let mut names: [&[u8]; MAX_DEPTH] = [&[]; MAX_DEPTH];
        let mut pending = [Pending::EMPTY; MAX_DEPTH];
        let mut cursor = structure;

        while cursor + 4 <= end {
            let token = be32(cursor);
            cursor += 4;
            match token {
                TOKEN_BEGIN => {
                    let name = cstr(cursor, end)?;
                    cursor += (name.len() + 1 + 3) & !3;
                    if depth < MAX_DEPTH {
                        names[depth] = name;
                        pending[depth] = Pending::EMPTY;
                    }
                    depth += 1;
                }
                TOKEN_END_NODE => {
                    if depth == 0 {
                        return Some(info);
                    }
                    depth -= 1;
                    if depth < MAX_DEPTH && pending[depth].is_simple_fb {
                        if let Some((base, _size)) = pending[depth].reg {
                            info.simple_fb = Some(Framebuffer {
                                base,
                                width: pending[depth].width,
                                height: pending[depth].height,
                                stride: pending[depth].stride,
                            });
                        }
                    }
                }
                TOKEN_PROP => {
                    if cursor + 8 > end {
                        return Some(info);
                    }
                    let length = be32(cursor) as usize;
                    let name_offset = be32(cursor + 4) as usize;
                    let data = cursor + 8;
                    if length > end - data || strings + name_offset >= end {
                        return Some(info);
                    }
                    cursor = data + ((length + 3) & !3);
                    let name = match cstr(strings + name_offset, end) {
                        Some(name) => name,
                        None => return Some(info),
                    };
                    if depth == 0 || depth > MAX_DEPTH {
                        continue;
                    }
                    let slot = depth - 1;
                    let node = names[slot];
                    if depth == 1 && length >= 4 {
                        if name == b"#address-cells" {
                            address_cells = be32(data);
                        } else if name == b"#size-cells" {
                            size_cells = be32(data);
                        }
                    }
                    let cells_valid = address_cells >= 1 && address_cells <= 2 && size_cells <= 2;
                    let reg_bytes = ((address_cells + size_cells) * 4) as usize;
                    if !cells_valid || reg_bytes == 0 {
                        continue;
                    }
                    if depth == 2 && node.starts_with(b"memory") && name == b"reg" {
                        let mut offset = 0usize;
                        while offset + reg_bytes <= length && info.ram_count < MAX_RAM {
                            let base = cells(data + offset, address_cells);
                            let size = cells(data + offset + (address_cells * 4) as usize, size_cells);
                            if size > 0 {
                                info.ram[info.ram_count] = (base, size);
                                info.ram_count += 1;
                            }
                            offset += reg_bytes;
                        }
                    }
                    if depth == 3 && names[1] == b"reserved-memory" && name == b"reg" {
                        let mut offset = 0usize;
                        while offset + reg_bytes <= length {
                            let base = cells(data + offset, address_cells);
                            let size = cells(data + offset + (address_cells * 4) as usize, size_cells);
                            info.add_reserved(base, size);
                            offset += reg_bytes;
                        }
                    }
                    if depth == 2 && node == b"chosen" {
                        if name == b"atag,videolfb-fb_base_l" && length >= 4 {
                            info.lfb_base |= be32(data) as u64;
                        } else if name == b"atag,videolfb-fb_base_h" && length >= 4 {
                            info.lfb_base |= (be32(data) as u64) << 32;
                        } else if name == b"atag,videolfb-vramSize" && length >= 4 {
                            info.lfb_size = if length >= 8 { cells(data, 2) } else { be32(data) as u64 };
                        }
                    }
                    if depth == 2 && node.starts_with(b"timer") && name == b"clock-frequency" && length >= 4 {
                        info.timer_hz = be32(data);
                    }
                    if name == b"compatible" {
                        let bytes = core::slice::from_raw_parts(data as *const u8, length);
                        if contains(bytes, b"simple-framebuffer") {
                            pending[slot].is_simple_fb = true;
                        }
                    } else if name == b"reg" && length >= reg_bytes {
                        pending[slot].reg = Some((
                            cells(data, address_cells),
                            cells(data + (address_cells * 4) as usize, size_cells),
                        ));
                    } else if name == b"width" && length >= 4 {
                        pending[slot].width = be32(data);
                    } else if name == b"height" && length >= 4 {
                        pending[slot].height = be32(data);
                    } else if name == b"stride" && length >= 4 {
                        pending[slot].stride = be32(data);
                    }
                }
                TOKEN_NOP => {}
                _ => return Some(info),
            }
        }
        Some(info)
    }
}
