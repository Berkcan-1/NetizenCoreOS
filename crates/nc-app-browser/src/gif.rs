//! GIF decoder (first frame only), pure `alloc` code: host-testable and kernel-safe.
//!
//! Supports GIF87a/89a, global + local palettes, transparency (Graphic Control Extension),
//! interlacing and the variable-width LZW stream. Animation frames after the first are ignored.
extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

/// Largest canvas decoded (pixels). Bigger images are rejected instead of exhausting memory.
const MAX_PIXELS: usize = 4_000_000;

pub struct Gif {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn le16(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*d.get(at)?, *d.get(at + 1)?]) as usize)
}

fn read_palette(d: &[u8], pos: &mut usize, flags: u8) -> Option<Vec<[u8; 3]>> {
    let n = 1usize << ((flags & 7) + 1);
    let end = pos.checked_add(n * 3)?;
    if end > d.len() {
        return None;
    }
    let mut pal = Vec::with_capacity(n);
    for i in 0..n {
        let o = *pos + i * 3;
        pal.push([d[o], d[o + 1], d[o + 2]]);
    }
    *pos = end;
    Some(pal)
}

fn skip_sub_blocks(d: &[u8], pos: &mut usize) -> Option<()> {
    loop {
        let size = *d.get(*pos)? as usize;
        *pos += 1;
        if size == 0 {
            return Some(());
        }
        *pos = pos.checked_add(size)?;
        if *pos > d.len() {
            return None;
        }
    }
}

pub fn decode(d: &[u8]) -> Option<Gif> {
    if d.len() < 13 || !(d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a")) {
        return None;
    }
    let sw = le16(d, 6)?;
    let sh = le16(d, 8)?;
    let flags = d[10];
    if sw == 0 || sh == 0 || sw.checked_mul(sh)? > MAX_PIXELS {
        return None;
    }
    let mut pos = 13usize;
    let global = if flags & 0x80 != 0 { Some(read_palette(d, &mut pos, flags)?) } else { None };
    let mut rgba = vec![0u8; sw * sh * 4];
    let mut transparent: Option<u8> = None;

    loop {
        match *d.get(pos)? {
            0x21 => {
                let label = *d.get(pos + 1)?;
                if label == 0xF9 && *d.get(pos + 2)? >= 4 {
                    // packed bit 0 = transparent colour flag, byte 6 = its index
                    if d.get(pos + 3)? & 1 != 0 {
                        transparent = Some(*d.get(pos + 6)?);
                    } else {
                        transparent = None;
                    }
                }
                pos += 2;
                skip_sub_blocks(d, &mut pos)?;
            }
            0x2C => {
                let left = le16(d, pos + 1)?;
                let top = le16(d, pos + 3)?;
                let w = le16(d, pos + 5)?;
                let h = le16(d, pos + 7)?;
                let packed = *d.get(pos + 9)?;
                pos += 10;
                let local = if packed & 0x80 != 0 { Some(read_palette(d, &mut pos, packed)?) } else { None };
                let palette = local.as_ref().or(global.as_ref())?;
                let interlaced = packed & 0x40 != 0;
                let min_code = *d.get(pos)?;
                pos += 1;
                if !(2..=11).contains(&min_code) || w == 0 || h == 0 || w.checked_mul(h)? > MAX_PIXELS {
                    return None;
                }
                let mut stream: Vec<u8> = Vec::new();
                loop {
                    let size = *d.get(pos)? as usize;
                    pos += 1;
                    if size == 0 {
                        break;
                    }
                    let end = pos.checked_add(size)?;
                    stream.extend_from_slice(d.get(pos..end)?);
                    pos = end;
                }
                let indices = lzw(min_code, &stream, w * h);
                // Row order: interlaced images store rows in four passes.
                let mut rows: Vec<usize> = Vec::with_capacity(h);
                if interlaced {
                    for (start, step) in [(0usize, 8usize), (4, 8), (2, 4), (1, 2)] {
                        let mut y = start;
                        while y < h {
                            rows.push(y);
                            y += step;
                        }
                    }
                } else {
                    rows.extend(0..h);
                }
                for (src_row, &y) in rows.iter().enumerate() {
                    for x in 0..w {
                        let Some(&ix) = indices.get(src_row * w + x) else { break };
                        if transparent == Some(ix) {
                            continue;
                        }
                        let Some(c) = palette.get(ix as usize) else { continue };
                        let (px, py) = (left + x, top + y);
                        if px < sw && py < sh {
                            let o = (py * sw + px) * 4;
                            rgba[o] = c[0];
                            rgba[o + 1] = c[1];
                            rgba[o + 2] = c[2];
                            rgba[o + 3] = 255;
                        }
                    }
                }
                return Some(Gif { width: sw as u32, height: sh as u32, rgba });
            }
            _ => return None, // trailer (0x3B) before any image, or corrupt data
        }
    }
}

/// Variable-width LZW as used by GIF (LSB-first codes). Returns at most `out_len` colour indices.
fn lzw(min_code: u8, data: &[u8], out_len: usize) -> Vec<u8> {
    let clear: u16 = 1 << min_code;
    let eoi: u16 = clear + 1;
    let mut size: u32 = min_code as u32 + 1;
    let mut prefix = [0u16; 4096];
    let mut suffix = [0u8; 4096];
    let mut next: u16 = eoi + 1;
    let mut prev: Option<u16> = None;
    let mut out: Vec<u8> = Vec::with_capacity(out_len);
    let mut stack: Vec<u8> = Vec::new();
    let (mut buf, mut bits, mut idx) = (0u32, 0u32, 0usize);

    while out.len() < out_len {
        while bits < size {
            if idx >= data.len() {
                return out;
            }
            buf |= (data[idx] as u32) << bits;
            bits += 8;
            idx += 1;
        }
        let code = (buf & ((1u32 << size) - 1)) as u16;
        buf >>= size;
        bits -= size;

        if code == clear {
            size = min_code as u32 + 1;
            next = eoi + 1;
            prev = None;
            continue;
        }
        if code == eoi {
            break;
        }
        let p = match prev {
            None => {
                if code >= clear {
                    return out;
                }
                out.push(code as u8);
                prev = Some(code);
                continue;
            }
            Some(p) => p,
        };

        // Expand `code` (or, for the KwKwK case, `prev`) into `stack` in reverse order.
        let kwkwk = code == next;
        if code > next {
            return out;
        }
        let mut c = if kwkwk { p } else { code };
        stack.clear();
        while c >= clear {
            stack.push(suffix[c as usize]);
            c = prefix[c as usize];
        }
        stack.push(c as u8);
        let first = *stack.last().unwrap_or(&0);
        out.extend(stack.iter().rev());
        if kwkwk {
            out.push(first);
        }
        if next < 4096 {
            prefix[next as usize] = p;
            suffix[next as usize] = first;
            next += 1;
            if next as u32 == (1u32 << size) && size < 12 {
                size += 1;
            }
        }
        prev = Some(code);
    }
    out.truncate(out_len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"").is_none());
        assert!(decode(b"GIF89a").is_none());
        assert!(decode(b"PNG....................").is_none());
    }

    fn fnv1a(data: &[u8]) -> u32 {
        let mut h: u32 = 2166136261;
        for b in data {
            h = (h ^ *b as u32).wrapping_mul(16777619);
        }
        h
    }

    #[test]
    fn decodes_fixture_frames() {
        // Fixtures come from Pillow; hashes are over the decoded RGBA (see host-tests/gif_fixtures.rs).
        for (name, bytes, w, h, hash) in crate::gif_fixtures::FIXTURES {
            let g = decode(bytes).unwrap_or_else(|| panic!("{} did not decode", name));
            assert_eq!((g.width, g.height), (*w, *h), "{}", name);
            assert_eq!(fnv1a(&g.rgba), *hash, "{} pixels differ", name);
        }
    }
}
