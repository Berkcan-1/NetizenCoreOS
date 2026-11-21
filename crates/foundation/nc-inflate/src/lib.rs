#![no_std]

extern crate alloc;

use alloc::vec::Vec;

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CLORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

pub const MAX_OUTPUT: usize = 24 * 1024 * 1024;

struct Tree {
    table: [u16; 16],
    trans: [u16; 288],
}

impl Tree {
    fn new() -> Tree {
        Tree { table: [0; 16], trans: [0; 288] }
    }

    fn build(&mut self, lengths: &[u8]) -> bool {
        if lengths.len() > 288 {
            return false;
        }
        self.table = [0; 16];
        self.trans = [0; 288];
        for &length in lengths {
            if length > 15 {
                return false;
            }
            self.table[length as usize] += 1;
        }
        self.table[0] = 0;
        let mut offs = [0u16; 16];
        let mut sum = 0u16;
        let mut index = 0usize;
        while index < 16 {
            offs[index] = sum;
            sum = sum.wrapping_add(self.table[index]);
            index += 1;
        }
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let slot = offs[length as usize] as usize;
                if slot >= 288 {
                    return false;
                }
                self.trans[slot] = symbol as u16;
                offs[length as usize] += 1;
            }
        }
        true
    }
}

struct Inflater<'a> {
    src: &'a [u8],
    pos: usize,
    tag: u32,
    count: u32,
    out: Vec<u8>,
}

impl<'a> Inflater<'a> {
    fn bit(&mut self) -> Option<u32> {
        if self.count == 0 {
            let byte = *self.src.get(self.pos)?;
            self.pos += 1;
            self.tag = byte as u32;
            self.count = 8;
        }
        let value = self.tag & 1;
        self.tag >>= 1;
        self.count -= 1;
        Some(value)
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        let mut value = 0u32;
        let mut index = 0u32;
        while index < count {
            value |= self.bit()? << index;
            index += 1;
        }
        Some(value)
    }

    fn symbol(&mut self, tree: &Tree) -> Option<u16> {
        let mut total: i32 = 0;
        let mut cur: i32 = 0;
        let mut length = 0usize;
        loop {
            cur = 2 * cur + self.bit()? as i32;
            length += 1;
            if length > 15 {
                return None;
            }
            total += tree.table[length] as i32;
            cur -= tree.table[length] as i32;
            if cur < 0 {
                break;
            }
        }
        let index = (total + cur) as usize;
        if index >= 288 {
            return None;
        }
        Some(tree.trans[index])
    }

    fn block(&mut self, literals: &Tree, distances: &Tree) -> Option<()> {
        loop {
            let code = self.symbol(literals)? as usize;
            if code < 256 {
                if self.out.len() >= MAX_OUTPUT {
                    return None;
                }
                self.out.push(code as u8);
                continue;
            }
            if code == 256 {
                return Some(());
            }
            let code = code - 257;
            if code >= 29 {
                return None;
            }
            let length = LBASE[code] as usize + self.bits(LEXT[code] as u32)? as usize;
            let dsym = self.symbol(distances)? as usize;
            if dsym >= 30 {
                return None;
            }
            let distance = DBASE[dsym] as usize + self.bits(DEXT[dsym] as u32)? as usize;
            if distance > self.out.len() || self.out.len() + length > MAX_OUTPUT {
                return None;
            }
            let mut copied = 0usize;
            while copied < length {
                let byte = self.out[self.out.len() - distance];
                self.out.push(byte);
                copied += 1;
            }
        }
    }

    fn run(&mut self) -> Option<()> {
        loop {
            let last = self.bit()?;
            let kind = self.bits(2)?;
            match kind {
                0 => {
                    self.count = 0;
                    if self.pos + 4 > self.src.len() {
                        return None;
                    }
                    let len = self.src[self.pos] as usize | ((self.src[self.pos + 1] as usize) << 8);
                    let nlen = self.src[self.pos + 2] as usize | ((self.src[self.pos + 3] as usize) << 8);
                    if len != (!nlen & 0xFFFF) {
                        return None;
                    }
                    self.pos += 4;
                    if self.pos + len > self.src.len() || self.out.len() + len > MAX_OUTPUT {
                        return None;
                    }
                    self.out.extend_from_slice(&self.src[self.pos..self.pos + len]);
                    self.pos += len;
                }
                1 => {
                    let mut literals = Tree::new();
                    let mut distances = Tree::new();
                    let mut lengths = [0u8; 288];
                    let mut index = 0usize;
                    while index < 288 {
                        lengths[index] = if index < 144 {
                            8
                        } else if index < 256 {
                            9
                        } else if index < 280 {
                            7
                        } else {
                            8
                        };
                        index += 1;
                    }
                    if !literals.build(&lengths) || !distances.build(&[5u8; 30]) {
                        return None;
                    }
                    self.block(&literals, &distances)?;
                }
                2 => {
                    let hlit = self.bits(5)? as usize + 257;
                    let hdist = self.bits(5)? as usize + 1;
                    let hclen = self.bits(4)? as usize + 4;
                    let mut code_lengths = [0u8; 19];
                    let mut index = 0usize;
                    while index < hclen {
                        code_lengths[CLORD[index]] = self.bits(3)? as u8;
                        index += 1;
                    }
                    let mut code_tree = Tree::new();
                    if !code_tree.build(&code_lengths) {
                        return None;
                    }
                    let total = hlit + hdist;
                    let mut lens = [0u8; 320];
                    let mut filled = 0usize;
                    while filled < total {
                        let symbol = self.symbol(&code_tree)? as usize;
                        if symbol < 16 {
                            lens[filled] = symbol as u8;
                            filled += 1;
                            continue;
                        }
                        let (value, repeat) = match symbol {
                            16 => {
                                if filled == 0 {
                                    return None;
                                }
                                (lens[filled - 1], 3 + self.bits(2)? as usize)
                            }
                            17 => (0u8, 3 + self.bits(3)? as usize),
                            _ => (0u8, 11 + self.bits(7)? as usize),
                        };
                        if filled + repeat > total {
                            return None;
                        }
                        let mut done = 0usize;
                        while done < repeat {
                            lens[filled] = value;
                            filled += 1;
                            done += 1;
                        }
                    }
                    let mut literals = Tree::new();
                    let mut distances = Tree::new();
                    if !literals.build(&lens[..hlit]) || !distances.build(&lens[hlit..hlit + hdist]) {
                        return None;
                    }
                    self.block(&literals, &distances)?;
                }
                _ => return None,
            }
            if last == 1 {
                return Some(());
            }
        }
    }
}

pub fn inflate_raw(src: &[u8], size_hint: usize) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve_exact(size_hint.min(MAX_OUTPUT)).ok()?;
    let mut state = Inflater { src, pos: 0, tag: 0, count: 0, out };
    state.run()?;
    Some(state.out)
}

pub fn gunzip(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 18 || data[0] != 0x1F || data[1] != 0x8B || data[2] != 8 || data[3] != 0 {
        return None;
    }
    let tail = data.len() - 4;
    let isize = u32::from_le_bytes([data[tail], data[tail + 1], data[tail + 2], data[tail + 3]]) as usize;
    if isize > MAX_OUTPUT {
        return None;
    }
    let out = inflate_raw(&data[10..data.len() - 8], isize)?;
    if out.len() != isize {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn stored_block() {
        let data = [0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3, 1, 3, 0, 0xFC, 0xFF, b'a', b'b', b'c', 0, 0, 0, 0, 3, 0, 0, 0];
        assert_eq!(gunzip(&data).unwrap(), b"abc");
    }

    #[test]
    fn rejects_garbage() {
        assert!(gunzip(&[0u8; 32]).is_none());
        assert!(gunzip(&[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 1, 0, 0, 0]).is_none());
    }
}
