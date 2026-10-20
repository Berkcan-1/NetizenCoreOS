extern crate alloc;

use alloc::vec::Vec;
use crate::css::{parse_css_color, SafeClamp};
use crate::dom::{DomTree, NodeType};

#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    pub fn from_bytes(data: &[u8], max_side: u32) -> Option<Self> {
        if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
            return decode_png(data).map(|img| fit_image(img, max_side));
        }
        if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            return decode_jpeg(data).map(|img| fit_image(img, max_side));
        }
        if data.len() >= 2 && &data[0..2] == b"BM" {
            return decode_bmp(data).map(|img| fit_image(img, max_side));
        }
        if let Ok(text) = core::str::from_utf8(data) {
            if text.contains("<svg") {
                let dom = crate::html::HtmlParser::parse(text);
                return rasterize_svg_tree(&dom, dom.root, 64, 64);
            }
        }
        None
    }

    pub fn from_data_uri(uri: &str, max_side: u32) -> Option<Self> {
        let rest = uri.strip_prefix("data:")?;
        let comma = rest.find(',')?;
        let meta = &rest[..comma];
        let payload = &rest[comma + 1..];

        if meta.contains("image/svg+xml") {
            let svg_text = if meta.contains(";base64") {
                let bytes = decode_base64(payload)?;
                alloc::string::String::from_utf8(bytes).ok()?
            } else {
                percent_decode(payload)
            };
            let dom = crate::html::HtmlParser::parse(&svg_text);
            return rasterize_svg_tree(&dom, dom.root, 64, 64);
        }

        if meta.contains(";base64") {
            let bytes = decode_base64(payload)?;
            return Self::from_bytes(&bytes, max_side);
        }
        None
    }
}

pub fn rasterize_svg_tree(tree: &DomTree, svg_node_id: usize, default_w: u32, default_h: u32) -> Option<DecodedImage> {
    let w = tree
        .attr(svg_node_id, "width")
        .and_then(|s| s.trim_end_matches("px").parse::<u32>().ok())
        .unwrap_or(default_w)
        .clamp(12, 240);
    let h = tree
        .attr(svg_node_id, "height")
        .and_then(|s| s.trim_end_matches("px").parse::<u32>().ok())
        .unwrap_or(default_h)
        .clamp(12, 160);

    let mut vb_w = w as i32;
    let mut vb_h = h as i32;
    if let Some(vb) = tree.attr(svg_node_id, "viewbox") {
        let parts: Vec<i32> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter_map(|p| p.split('.').next()?.parse::<i32>().ok())
            .collect();
        if parts.len() == 4 && parts[2] > 0 && parts[3] > 0 {
            vb_w = parts[2];
            vb_h = parts[3];
        }
    }

    let mut rgba = alloc::vec![0u8; (w * h * 4) as usize];
    let root_fill = tree
        .attr(svg_node_id, "fill")
        .and_then(parse_css_color)
        .unwrap_or(0x0025_63EB);

    paint_svg_children(tree, svg_node_id, &mut rgba, w as i32, h as i32, vb_w, vb_h, root_fill);
    Some(DecodedImage { width: w, height: h, rgba })
}

fn paint_svg_children(
    tree: &DomTree,
    node_id: usize,
    rgba: &mut [u8],
    w: i32,
    h: i32,
    vb_w: i32,
    vb_h: i32,
    inherited_color: u32,
) {
    let Some(node) = tree.nodes.get(node_id) else {
        return;
    };
    let color = tree
        .attr(node_id, "fill")
        .and_then(parse_css_color)
        .or_else(|| tree.attr(node_id, "stroke").and_then(parse_css_color))
        .unwrap_or(inherited_color);

    if let NodeType::Element { ref tag, .. } = node.node_type {
        match tag.as_str() {
            "rect" => {
                let rx = parse_svg_coord(tree.attr(node_id, "x"), vb_w, w);
                let ry = parse_svg_coord(tree.attr(node_id, "y"), vb_h, h);
                let rw = parse_svg_coord(tree.attr(node_id, "width"), vb_w, w).max(1);
                let rh = parse_svg_coord(tree.attr(node_id, "height"), vb_h, h).max(1);
                fill_rgba_rect(rgba, w, h, rx, ry, rw, rh, color);
            }
            "circle" => {
                let cx = parse_svg_coord(tree.attr(node_id, "cx"), vb_w, w);
                let cy = parse_svg_coord(tree.attr(node_id, "cy"), vb_h, h);
                let r = parse_svg_coord(tree.attr(node_id, "r"), vb_w, w).max(2);
                fill_rgba_circle(rgba, w, h, cx, cy, r, color);
            }
            "line" => {
                let x1 = parse_svg_coord(tree.attr(node_id, "x1"), vb_w, w);
                let y1 = parse_svg_coord(tree.attr(node_id, "y1"), vb_h, h);
                let x2 = parse_svg_coord(tree.attr(node_id, "x2"), vb_w, w);
                let y2 = parse_svg_coord(tree.attr(node_id, "y2"), vb_h, h);
                draw_rgba_line(rgba, w, h, x1, y1, x2, y2, color);
            }
            "path" => {
                if let Some(d) = tree.attr(node_id, "d") {
                    rasterize_simple_svg_path(d, rgba, w, h, vb_w, vb_h, color);
                }
            }
            _ => {}
        }
    }

    for &child in &node.children {
        paint_svg_children(tree, child, rgba, w, h, vb_w, vb_h, color);
    }
}

fn parse_svg_coord(val: Option<&str>, vb_dim: i32, target_dim: i32) -> i32 {
    let Some(s) = val else {
        return 0;
    };
    let clean = s.trim().trim_end_matches("px").split('.').next().unwrap_or("0");
    let raw = clean.parse::<i32>().unwrap_or(0);
    (raw * target_dim) / vb_dim.max(1)
}

fn put_rgba_pixel(rgba: &mut [u8], w: i32, h: i32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let idx = ((y * w + x) * 4) as usize;
    rgba[idx] = ((color >> 16) & 0xFF) as u8;
    rgba[idx + 1] = ((color >> 8) & 0xFF) as u8;
    rgba[idx + 2] = (color & 0xFF) as u8;
    rgba[idx + 3] = 255;
}

fn fill_rgba_rect(rgba: &mut [u8], w: i32, h: i32, rx: i32, ry: i32, rw: i32, rh: i32, color: u32) {
    for y in ry..(ry + rh) {
        for x in rx..(rx + rw) {
            put_rgba_pixel(rgba, w, h, x, y, color);
        }
    }
}

fn fill_rgba_circle(rgba: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, color: u32) {
    let r2 = r * r;
    for y in (cy - r)..=(cy + r) {
        for x in (cx - r)..=(cx + r) {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                put_rgba_pixel(rgba, w, h, x, y, color);
            }
        }
    }
}

fn draw_rgba_line(rgba: &mut [u8], w: i32, h: i32, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for s in 0..=steps {
        let x = x0 + (x1 - x0) * s / steps;
        let y = y0 + (y1 - y0) * s / steps;
        put_rgba_pixel(rgba, w, h, x, y, color);
        put_rgba_pixel(rgba, w, h, x + 1, y, color);
    }
}

fn rasterize_simple_svg_path(d: &str, rgba: &mut [u8], w: i32, h: i32, vb_w: i32, vb_h: i32, color: u32) {
    let mut nums = Vec::new();
    let mut cur = alloc::string::String::new();
    for ch in d.chars() {
        if ch.is_ascii_digit() || ch == '-' || ch == '.' {
            if ch == '-' && !cur.is_empty() {
                if let Some(v) = cur.split('.').next().and_then(|s| s.parse::<i32>().ok()) {
                    nums.push(v);
                }
                cur.clear();
            }
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Some(v) = cur.split('.').next().and_then(|s| s.parse::<i32>().ok()) {
                nums.push(v);
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Some(v) = cur.split('.').next().and_then(|s| s.parse::<i32>().ok()) {
            nums.push(v);
        }
    }

    if nums.len() >= 4 {
        let mut prev_x = (nums[0] * w) / vb_w.max(1);
        let mut prev_y = (nums[1] * h) / vb_h.max(1);
        let mut i = 2;
        while i + 1 < nums.len() {
            let nx = ((nums[i] * w) / vb_w.max(1)).sclamp(0, w - 1);
            let ny = ((nums[i + 1] * h) / vb_h.max(1)).sclamp(0, h - 1);
            draw_rgba_line(rgba, w, h, prev_x, prev_y, nx, ny, color);
            prev_x = nx;
            prev_y = ny;
            i += 2;
        }
    } else {
        fill_rgba_circle(rgba, w, h, w / 2, h / 2, w.min(h) / 3, color);
    }
}

fn decode_bmp(data: &[u8]) -> Option<DecodedImage> {
    if data.len() < 54 {
        return None;
    }
    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);

    if width <= 0 || height_raw == 0 || (bpp != 24 && bpp != 32) {
        return None;
    }
    let top_down = height_raw < 0;
    let w = width as usize;
    let h = height_raw.unsigned_abs() as usize;
    if w > 2048 || h > 2048 {
        return None;
    }

    let bytes_per_pixel = (bpp / 8) as usize;
    let row_stride = (w * bytes_per_pixel + 3) & !3;
    let mut rgba = alloc::vec![0u8; w * h * 4];

    for row in 0..h {
        let src_row = if top_down { row } else { h - 1 - row };
        let row_start = pixel_offset + src_row * row_stride;
        if row_start + w * bytes_per_pixel > data.len() {
            break;
        }
        for col in 0..w {
            let p = row_start + col * bytes_per_pixel;
            let dst = (row * w + col) * 4;
            rgba[dst] = data[p + 2];
            rgba[dst + 1] = data[p + 1];
            rgba[dst + 2] = data[p];
            rgba[dst + 3] = if bytes_per_pixel == 4 { data[p + 3] } else { 255 };
        }
    }

    Some(DecodedImage {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Clone)]
struct JpegHuff {
    counts: [u8; 16],
    symbols: [u8; 256],
    valid: bool,
}

impl JpegHuff {
    const fn empty() -> Self {
        JpegHuff {
            counts: [0; 16],
            symbols: [0; 256],
            valid: false,
        }
    }

    fn decode(&self, reader: &mut JpegBitReader<'_>) -> Option<u8> {
        if !self.valid {
            return None;
        }
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for i in 0..16 {
            let bit = reader.read_bit()? as i32;
            code = (code << 1) | bit;
            let count = self.counts[i] as i32;
            if code - count < first {
                let sym_idx = (index + (code - first)) as usize;
                return self.symbols.get(sym_idx).copied();
            }
            index += count;
            first = (first + count) << 1;
        }
        None
    }
}

struct JpegBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> JpegBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        JpegBitReader {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn read_bit(&mut self) -> Option<u8> {
        if self.bit_count == 0 {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            if b == 0xFF {
                while let Some(&next_b) = self.data.get(self.pos) {
                    if next_b == 0xFF {
                        self.pos += 1;
                    } else if next_b == 0x00 {
                        self.pos += 1;
                        break;
                    } else if (0xD0..=0xD7).contains(&next_b) {
                        self.pos += 1;
                        let b2 = *self.data.get(self.pos)?;
                        self.pos += 1;
                        self.bit_buf = b2 as u32;
                        self.bit_count = 7;
                        return Some((b2 >> 7) & 1);
                    } else {
                        return None;
                    }
                }
            }
            self.bit_buf = b as u32;
            self.bit_count = 8;
        }
        self.bit_count -= 1;
        Some(((self.bit_buf >> self.bit_count) & 1) as u8)
    }

    fn read_bits(&mut self, n: u8) -> Option<i32> {
        let mut val = 0i32;
        for _ in 0..n {
            val = (val << 1) | (self.read_bit()? as i32);
        }
        Some(val)
    }

    fn receive_extend(&mut self, n: u8) -> Option<i32> {
        if n == 0 {
            return Some(0);
        }
        let v = self.read_bits(n)?;
        let half = 1i32 << (n - 1);
        if v < half {
            Some(v - ((1i32 << n) - 1))
        } else {
            Some(v)
        }
    }
}

fn decode_jpeg(data: &[u8]) -> Option<DecodedImage> {
    let mut pos = 2usize;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut num_comp = 0usize;
    let mut qtables = [[1i32; 64]; 4];
    let mut dc_huff = [
        JpegHuff::empty(),
        JpegHuff::empty(),
        JpegHuff::empty(),
        JpegHuff::empty(),
    ];
    let mut ac_huff = [
        JpegHuff::empty(),
        JpegHuff::empty(),
        JpegHuff::empty(),
        JpegHuff::empty(),
    ];
    let mut comp_q = [0usize; 3];
    let mut comp_dc = [0usize; 3];
    let mut comp_ac = [0usize; 3];
    let mut comp_h = [1usize; 3];
    let mut comp_v = [1usize; 3];

    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }
        let marker = data[pos];
        pos += 1;
        if marker == 0xD9 || marker == 0x00 {
            break;
        }
        if (0xD0..=0xD8).contains(&marker) {
            continue;
        }
        if pos + 2 > data.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        if seg_len < 2 || pos + seg_len > data.len() {
            break;
        }
        let seg = &data[pos + 2..pos + seg_len];
        pos += seg_len;

        match marker {
            0xDB => {
                let mut sp = 0;
                while sp + 65 <= seg.len() {
                    let info = seg[sp];
                    let tq = (info & 0x0F) as usize;
                    sp += 1;
                    if tq < 4 {
                        for i in 0..64 {
                            qtables[tq][ZIGZAG[i]] = seg[sp + i] as i32;
                        }
                    }
                    sp += 64;
                }
            }
            0xC0 => {
                if seg.len() >= 6 {
                    height = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                    width = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                    num_comp = (seg[5] as usize).min(3);
                    for c in 0..num_comp {
                        if 6 + c * 3 + 2 < seg.len() {
                            let samp = seg[6 + c * 3 + 1];
                            comp_h[c] = ((samp >> 4) & 0x0F).max(1) as usize;
                            comp_v[c] = (samp & 0x0F).max(1) as usize;
                            comp_q[c] = (seg[6 + c * 3 + 2] as usize).min(3);
                        }
                    }
                }
            }
            0xC4 => {
                let mut sp = 0;
                while sp + 17 <= seg.len() {
                    let info = seg[sp];
                    let tc = (info >> 4) & 0x0F;
                    let th = (info & 0x0F) as usize;
                    sp += 1;
                    let mut counts = [0u8; 16];
                    let mut total = 0usize;
                    for i in 0..16 {
                        counts[i] = seg[sp + i];
                        total += counts[i] as usize;
                    }
                    sp += 16;
                    if sp + total > seg.len() || total > 256 {
                        break;
                    }
                    let mut symbols = [0u8; 256];
                    symbols[..total].copy_from_slice(&seg[sp..sp + total]);
                    sp += total;
                    if th < 4 {
                        let table = JpegHuff {
                            counts,
                            symbols,
                            valid: true,
                        };
                        if tc == 0 {
                            dc_huff[th] = table;
                        } else {
                            ac_huff[th] = table;
                        }
                    }
                }
            }
            0xDA => {
                if width == 0 || height == 0 || width > 1024 || height > 1024 || num_comp == 0 {
                    return None;
                }
                let ns = (seg[0] as usize).min(num_comp);
                for c in 0..ns {
                    if 1 + c * 2 + 1 < seg.len() {
                        let td_ta = seg[1 + c * 2 + 1];
                        comp_dc[c] = ((td_ta >> 4) & 0x0F).min(3) as usize;
                        comp_ac[c] = (td_ta & 0x0F).min(3) as usize;
                    }
                }

                let mut reader = JpegBitReader::new(&data[pos..]);
                let mut rgba = alloc::vec![255u8; width * height * 4];
                let mut dc_pred = [0i32; 3];
                let max_h = comp_h[0].max(1);
                let max_v = comp_v[0].max(1);
                let mcu_w = max_h * 8;
                let mcu_h = max_v * 8;
                let mcus_x = (width + mcu_w - 1) / mcu_w;
                let mcus_y = (height + mcu_h - 1) / mcu_h;

                for my in 0..mcus_y {
                    for mx in 0..mcus_x {
                        let mut y_blocks = [[0i32; 64]; 4];
                        let mut cb_block = [0i32; 64];
                        let mut cr_block = [0i32; 64];

                        let y_count = (max_h * max_v).min(4);
                        for b_idx in 0..y_count {
                            decode_jpeg_block(
                                &mut reader,
                                &dc_huff[comp_dc[0]],
                                &ac_huff[comp_ac[0]],
                                &qtables[comp_q[0]],
                                &mut dc_pred[0],
                                &mut y_blocks[b_idx],
                            );
                        }
                        if num_comp >= 3 {
                            decode_jpeg_block(
                                &mut reader,
                                &dc_huff[comp_dc[1]],
                                &ac_huff[comp_ac[1]],
                                &qtables[comp_q[1]],
                                &mut dc_pred[1],
                                &mut cb_block,
                            );
                            decode_jpeg_block(
                                &mut reader,
                                &dc_huff[comp_dc[2]],
                                &ac_huff[comp_ac[2]],
                                &qtables[comp_q[2]],
                                &mut dc_pred[2],
                                &mut cr_block,
                            );
                        }

                        for py in 0..mcu_h {
                            let img_y = my * mcu_h + py;
                            if img_y >= height {
                                continue;
                            }
                            for px in 0..mcu_w {
                                let img_x = mx * mcu_w + px;
                                if img_x >= width {
                                    continue;
                                }
                                let by = (py / 8).min(max_v - 1);
                                let bx = (px / 8).min(max_h - 1);
                                let b_idx = (by * max_h + bx).min(3);
                                let y_val = y_blocks[b_idx][(py % 8) * 8 + (px % 8)] + 128;
                                let c_idx = (py / max_v) * 8 + (px / max_h);
                                let cb_val = if num_comp >= 3 { cb_block[c_idx.min(63)] } else { 0 };
                                let cr_val = if num_comp >= 3 { cr_block[c_idx.min(63)] } else { 0 };

                                let r = (y_val + ((359 * cr_val) >> 8)).clamp(0, 255) as u8;
                                let g = (y_val - ((88 * cb_val + 183 * cr_val) >> 8)).clamp(0, 255) as u8;
                                let b = (y_val + ((454 * cb_val) >> 8)).clamp(0, 255) as u8;

                                let dst = (img_y * width + img_x) * 4;
                                rgba[dst] = r;
                                rgba[dst + 1] = g;
                                rgba[dst + 2] = b;
                                rgba[dst + 3] = 255;
                            }
                        }
                    }
                }

                return Some(DecodedImage {
                    width: width as u32,
                    height: height as u32,
                    rgba,
                });
            }
            _ => {}
        }
    }
    None
}

fn decode_jpeg_block(
    reader: &mut JpegBitReader<'_>,
    dc_table: &JpegHuff,
    ac_table: &JpegHuff,
    qtable: &[i32; 64],
    dc_pred: &mut i32,
    out: &mut [i32; 64],
) {
    let mut coef = [0i32; 64];
    if let Some(s) = dc_table.decode(reader) {
        if let Some(diff) = reader.receive_extend(s & 0x0F) {
            *dc_pred += diff;
        }
    }
    coef[0] = *dc_pred * qtable[0];

    let mut k = 1usize;
    while k < 64 {
        let Some(rs) = ac_table.decode(reader) else {
            break;
        };
        let r = (rs >> 4) as usize;
        let s = rs & 0x0F;
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            } else {
                break;
            }
        }
        k += r;
        if k >= 64 {
            break;
        }
        if let Some(v) = reader.receive_extend(s) {
            let zz = ZIGZAG[k];
            coef[zz] = v * qtable[zz];
        }
        k += 1;
    }

    idct_8x8(&coef, out);
}

fn idct_8x8(src: &[i32; 64], dst: &mut [i32; 64]) {
    const COS: [[i32; 8]; 8] = [
        [256,  256,  256,  256,  256,  256,  256,  256],
        [355,  301,  201,   71,  -71, -201, -301, -355],
        [334,  139, -139, -334, -334, -139,  139,  334],
        [301,  -71, -355, -201,  201,  355,   71, -301],
        [256, -256, -256,  256,  256, -256, -256,  256],
        [201, -355,   71,  301, -301,  -71,  355, -201],
        [139, -334,  334, -139, -139,  334, -334,  139],
        [ 71, -201,  301, -355,  355, -301,  201,  -71],
    ];
    let mut tmp = [0i32; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0i32;
            for u in 0..8 {
                sum += src[y * 8 + u] * COS[u][x];
            }
            tmp[y * 8 + x] = sum >> 8;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut sum = 0i32;
            for v in 0..8 {
                sum += tmp[v * 8 + x] * COS[v][y];
            }
            dst[y * 8 + x] = sum >> 10;
        }
    }
}

fn decode_png(data: &[u8]) -> Option<DecodedImage> {
    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat = Vec::new();

    while pos + 8 <= data.len() {
        let length = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];
        pos += 8;
        if pos + length > data.len() {
            break;
        }
        let chunk_data = &data[pos..pos + length];
        pos += length + 4;

        match chunk_type {
            b"IHDR" if chunk_data.len() >= 13 => {
                width = u32::from_be_bytes([chunk_data[0], chunk_data[1], chunk_data[2], chunk_data[3]]);
                height = u32::from_be_bytes([chunk_data[4], chunk_data[5], chunk_data[6], chunk_data[7]]);
                bit_depth = chunk_data[8];
                color_type = chunk_data[9];
            }
            b"PLTE" => {
                for rgb_chunk in chunk_data.chunks_exact(3) {
                    palette.push([rgb_chunk[0], rgb_chunk[1], rgb_chunk[2]]);
                }
            }
            b"tRNS" => {
                trns.extend_from_slice(chunk_data);
            }
            b"IDAT" => {
                idat.extend_from_slice(chunk_data);
            }
            b"IEND" => break,
            _ => {}
        }
    }

    if width == 0 || height == 0 || width > 2048 || height > 2048 || bit_depth != 8 {
        return None;
    }

    let bpp = match color_type {
        0 => 1usize,
        2 => 3usize,
        3 => 1usize,
        4 => 2usize,
        6 => 4usize,
        _ => return None,
    };

    let raw = inflate_zlib(&idat)?;
    let w = width as usize;
    let h = height as usize;
    let stride = w * bpp;
    if raw.len() < h * (stride + 1) {
        return None;
    }

    let mut recon = alloc::vec![0u8; h * stride];
    for y in 0..h {
        let filter = raw[y * (stride + 1)];
        let src_row = &raw[y * (stride + 1) + 1..y * (stride + 1) + 1 + stride];
        for x in 0..stride {
            let a = if x >= bpp { recon[y * stride + x - bpp] } else { 0 };
            let b = if y > 0 { recon[(y - 1) * stride + x] } else { 0 };
            let c = if y > 0 && x >= bpp { recon[(y - 1) * stride + x - bpp] } else { 0 };
            let val = match filter {
                0 => src_row[x],
                1 => src_row[x].wrapping_add(a),
                2 => src_row[x].wrapping_add(b),
                3 => src_row[x].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => src_row[x].wrapping_add(paeth(a, b, c)),
                _ => src_row[x],
            };
            recon[y * stride + x] = val;
        }
    }

    let mut rgba = alloc::vec![0u8; w * h * 4];
    for i in 0..(w * h) {
        let dst = i * 4;
        match color_type {
            6 => {
                rgba[dst] = recon[i * 4];
                rgba[dst + 1] = recon[i * 4 + 1];
                rgba[dst + 2] = recon[i * 4 + 2];
                rgba[dst + 3] = recon[i * 4 + 3];
            }
            2 => {
                rgba[dst] = recon[i * 3];
                rgba[dst + 1] = recon[i * 3 + 1];
                rgba[dst + 2] = recon[i * 3 + 2];
                rgba[dst + 3] = 255;
            }
            4 => {
                let g = recon[i * 2];
                rgba[dst] = g;
                rgba[dst + 1] = g;
                rgba[dst + 2] = g;
                rgba[dst + 3] = recon[i * 2 + 1];
            }
            0 => {
                let g = recon[i];
                rgba[dst] = g;
                rgba[dst + 1] = g;
                rgba[dst + 2] = g;
                rgba[dst + 3] = 255;
            }
            3 => {
                let idx = recon[i] as usize;
                let rgb_val = palette.get(idx).copied().unwrap_or([0, 0, 0]);
                let alpha = trns.get(idx).copied().unwrap_or(255);
                rgba[dst] = rgb_val[0];
                rgba[dst + 1] = rgb_val[1];
                rgba[dst + 2] = rgb_val[2];
                rgba[dst + 3] = alpha;
            }
            _ => {}
        }
    }

    Some(DecodedImage { width, height, rgba })
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn fit_image(image: DecodedImage, max_side: u32) -> DecodedImage {
    let longest = image.width.max(image.height);
    if longest <= max_side || max_side == 0 {
        return image;
    }
    let new_w = ((image.width as u64 * max_side as u64) / longest as u64).max(1) as u32;
    let new_h = ((image.height as u64 * max_side as u64) / longest as u64).max(1) as u32;
    let mut rgba = alloc::vec![0u8; (new_w * new_h * 4) as usize];

    for dy in 0..new_h {
        let sy = (dy * image.height / new_h).min(image.height - 1);
        for dx in 0..new_w {
            let sx = (dx * image.width / new_w).min(image.width - 1);
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((dy * new_w + dx) * 4) as usize;
            rgba[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    DecodedImage {
        width: new_w,
        height: new_h,
        rgba,
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, byte_pos: 0, bit_pos: 0 }
    }

    fn read_bit(&mut self) -> Option<u16> {
        if self.byte_pos >= self.data.len() {
            return None;
        }
        let bit = ((self.data[self.byte_pos] >> self.bit_pos) & 1) as u16;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Some(bit)
    }

    fn read_bits(&mut self, count: u8) -> Option<u16> {
        let mut val = 0u16;
        for i in 0..count {
            val |= self.read_bit()? << i;
        }
        Some(val)
    }

    fn align_byte(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }
}

struct HuffTable {
    counts: [u16; 16],
    symbols: [u16; 288],
}

impl HuffTable {
    fn build(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if (len as usize) < 16 {
                counts[len as usize] += 1;
            }
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        let mut sum = 0u16;
        for bits in 1..16 {
            offsets[bits] = sum;
            sum += counts[bits];
        }
        let mut symbols = [0u16; 288];
        for (sym, &len) in lengths.iter().enumerate() {
            if len != 0 && (len as usize) < 16 {
                let idx = offsets[len as usize] as usize;
                if idx < symbols.len() {
                    symbols[idx] = sym as u16;
                }
                offsets[len as usize] += 1;
            }
        }
        HuffTable { counts, symbols }
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Option<u16> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= reader.read_bit()? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Some(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

fn inflate_zlib(zlib_data: &[u8]) -> Option<Vec<u8>> {
    if zlib_data.len() < 6 {
        return None;
    }
    let mut reader = BitReader::new(&zlib_data[2..]);
    let mut out = Vec::new();

    const LEN_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
        35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
        3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
        257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
        7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
    ];
    const CL_ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    loop {
        let bfinal = reader.read_bit()?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                reader.align_byte();
                let lo = reader.read_bits(8)?;
                let hi = reader.read_bits(8)?;
                let _nlo = reader.read_bits(8)?;
                let _nhi = reader.read_bits(8)?;
                let len = (lo | (hi << 8)) as usize;
                for _ in 0..len {
                    out.push(reader.read_bits(8)? as u8);
                }
            }
            1 | 2 => {
                let (lit_table, dist_table) = if btype == 1 {
                    let mut lit_lens = [0u8; 288];
                    for i in 0..144 { lit_lens[i] = 8; }
                    for i in 144..256 { lit_lens[i] = 9; }
                    for i in 256..280 { lit_lens[i] = 7; }
                    for i in 280..288 { lit_lens[i] = 8; }
                    let dist_lens = [5u8; 32];
                    (HuffTable::build(&lit_lens), HuffTable::build(&dist_lens))
                } else {
                    let hlit = reader.read_bits(5)? as usize + 257;
                    let hdist = reader.read_bits(5)? as usize + 1;
                    let hclen = reader.read_bits(4)? as usize + 4;
                    let mut cl_lens = [0u8; 19];
                    for i in 0..hclen {
                        cl_lens[CL_ORDER[i]] = reader.read_bits(3)? as u8;
                    }
                    let cl_table = HuffTable::build(&cl_lens);
                    let mut unpacked = alloc::vec![0u8; hlit + hdist];
                    let mut idx = 0;
                    while idx < hlit + hdist {
                        let sym = cl_table.decode(&mut reader)?;
                        match sym {
                            0..=15 => {
                                unpacked[idx] = sym as u8;
                                idx += 1;
                            }
                            16 => {
                                let prev = if idx > 0 { unpacked[idx - 1] } else { 0 };
                                let rep = reader.read_bits(2)? as usize + 3;
                                for _ in 0..rep {
                                    if idx < unpacked.len() {
                                        unpacked[idx] = prev;
                                        idx += 1;
                                    }
                                }
                            }
                            17 => {
                                let rep = reader.read_bits(3)? as usize + 3;
                                for _ in 0..rep {
                                    if idx < unpacked.len() {
                                        unpacked[idx] = 0;
                                        idx += 1;
                                    }
                                }
                            }
                            18 => {
                                let rep = reader.read_bits(7)? as usize + 11;
                                for _ in 0..rep {
                                    if idx < unpacked.len() {
                                        unpacked[idx] = 0;
                                        idx += 1;
                                    }
                                }
                            }
                            _ => return None,
                        }
                    }
                    (
                        HuffTable::build(&unpacked[..hlit]),
                        HuffTable::build(&unpacked[hlit..]),
                    )
                };

                loop {
                    let sym = lit_table.decode(&mut reader)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let len_idx = (sym - 257) as usize;
                        if len_idx >= 29 {
                            return None;
                        }
                        let length = (LEN_BASE[len_idx] + reader.read_bits(LEN_EXTRA[len_idx])?) as usize;
                        let dist_sym = dist_table.decode(&mut reader)? as usize;
                        if dist_sym >= 30 {
                            return None;
                        }
                        let distance = (DIST_BASE[dist_sym] + reader.read_bits(DIST_EXTRA[dist_sym])?) as usize;
                        if distance == 0 || distance > out.len() {
                            return None;
                        }
                        for _ in 0..length {
                            let b = out[out.len() - distance];
                            out.push(b);
                        }
                    }
                }
            }
            _ => return None,
        }

        if bfinal == 1 {
            break;
        }
    }
    Some(out)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in input.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

fn percent_decode(input: &str) -> alloc::string::String {
    let mut bytes = Vec::new();
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex_str) = core::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex_str, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[i]);
        i += 1;
    }
    alloc::string::String::from_utf8_lossy(&bytes).into_owned()
}
