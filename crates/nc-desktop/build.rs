use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let minlogo_path = manifest_dir.join("../../assets/logo/minlogo.png");
    let out_bin = out_dir.join("minlogo.bin");

    println!("cargo:rerun-if-changed={}", minlogo_path.display());

    if minlogo_path.exists() {
        // Sistemdeki python3 (standart kutuphane veya PIL) ile PNG -> [w:u32, h:u32, rgba...] cevirimi
        let py_script = r#"
import sys, struct, zlib

data = open(sys.argv[1], 'rb').read()
if data[:8] != b'\x89PNG\r\n\x1a\n':
    sys.exit(1)

pos = 8
w = h = bit_depth = color_type = 0
idat = bytearray()
plte = b''
trns = b''

while pos < len(data):
    length = struct.unpack('>I', data[pos:pos+4])[0]
    ctype = data[pos+4:pos+8]
    chunk = data[pos+8:pos+8+length]
    pos += 12 + length
    if ctype == b'IHDR':
        w, h, bit_depth, color_type, _, _, _ = struct.unpack('>IIBBBBB', chunk)
    elif ctype == b'PLTE':
        plte = chunk
    elif ctype == b'tRNS':
        trns = chunk
    elif ctype == b'IDAT':
        idat.extend(chunk)
    elif ctype == b'IEND':
        break

if bit_depth != 8 or color_type not in (2, 3, 6):
    sys.exit(1)

bpp = 4 if color_type == 6 else (3 if color_type == 2 else 1)
raw = zlib.decompress(bytes(idat))
stride = w * bpp
recon = bytearray(h * stride)

def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)

src = 0
for y in range(h):
    f = raw[src]
    src += 1
    row_off = y * stride
    for x in range(stride):
        val = raw[src]
        src += 1
        a = recon[row_off + x - bpp] if x >= bpp else 0
        b = recon[row_off - stride + x] if y > 0 else 0
        c = recon[row_off - stride + x - bpp] if (x >= bpp and y > 0) else 0
        if f == 1: val = (val + a) & 0xFF
        elif f == 2: val = (val + b) & 0xFF
        elif f == 3: val = (val + ((a + b) >> 1)) & 0xFF
        elif f == 4: val = (val + paeth(a, b, c)) & 0xFF
        recon[row_off + x] = val

out = bytearray(struct.pack('<II', w, h))
for y in range(h):
    for x in range(w):
        idx = y * stride + x * bpp
        if color_type == 6:
            out.extend(recon[idx:idx+4])
        elif color_type == 2:
            out.extend((recon[idx], recon[idx+1], recon[idx+2], 255))
        elif color_type == 3:
            p = recon[idx]
            r, g, b = plte[p*3:p*3+3]
            a = trns[p] if p < len(trns) else 255
            out.extend((r, g, b, a))

open(sys.argv[2], 'wb').write(out)
"#;
        let status = Command::new("python3")
            .arg("-c")
            .arg(py_script)
            .arg(&minlogo_path)
            .arg(&out_bin)
            .status();

        if let Ok(s) = status {
            if s.success() && out_bin.exists() {
                return;
            }
        }
    }

    // minlogo.png yoksa 0x0 boyutlu baslik yaz (nc_assets::logo() yedek olarak kullanilir)
    let _ = fs::write(&out_bin, [0u8; 8]);
}
