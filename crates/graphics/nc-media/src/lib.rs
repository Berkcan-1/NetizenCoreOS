#![no_std]

//! Bounded media inspection and decode pipelines for NetizenCore.
//!
//! PNG/JPEG decoding and MP3/WAV-to-PCM streaming are implemented here. MP4
//! currently has bounded metadata parsing only; sample demux, video/audio codecs,
//! and presentation synchronization are intentionally not claimed.

mod resample;

pub use resample::{compose_screen, resample_window, FitMode, RowFn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Bmp,
    Png,
    Jpeg,
    Mp3,
    Wav,
    Mp4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaError {
    Empty,
    UnknownFormat,
    Truncated,
    InvalidHeader,
    UnsupportedEncoding,
    InvalidDimensions,
    InvalidContainer,
    ResourceLimit,
    DecodeFailed,
    IoFailure,
    AudioOutputFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bits_per_sample: Option<u16>,
    pub bitrate_kbps: Option<u16>,
    pub duration_ms: Option<u64>,
    pub codec: &'static str,
    pub progressive: bool,
}

impl MediaInfo {
    const fn new(kind: MediaKind, codec: &'static str) -> Self {
        Self {
            kind,
            width: None,
            height: None,
            sample_rate: None,
            channels: None,
            bits_per_sample: None,
            bitrate_kbps: None,
            duration_ms: None,
            codec,
            progressive: false,
        }
    }
}

/// Inspect a media file by its signature. The extension is intentionally not used
/// as proof of format; callers must not trust a filename supplied by the user.
pub fn inspect(data: &[u8]) -> Result<MediaInfo, MediaError> {
    if data.is_empty() {
        return Err(MediaError::Empty);
    }

    if data.len() >= 2 && &data[..2] == b"BM" {
        return inspect_bmp(data);
    }
    if data.len() >= 8 && &data[..8] == &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return inspect_png(data);
    }
    if data.len() >= 2 && &data[..2] == &[0xFF, 0xD8] {
        return inspect_jpeg(data);
    }
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return inspect_wav(data);
    }
    if data.len() >= 8 && looks_like_mp4(data) {
        return inspect_mp4(data);
    }
    if let Some(result) = inspect_mp3(data) {
        return result;
    }

    Err(MediaError::UnknownFormat)
}

fn le_u16(data: &[u8], offset: usize) -> Option<u16> {
    let s = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn le_u32(data: &[u8], offset: usize) -> Option<u32> {
    let s = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let s = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let s = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn dimensions_ok(width: u32, height: u32) -> bool {
    width != 0
        && height != 0
        && width <= 32_768
        && height <= 32_768
        && (width as u64).checked_mul(height as u64).map_or(false, |n| n <= 268_435_456)
}

fn inspect_bmp(data: &[u8]) -> Result<MediaInfo, MediaError> {
    if data.len() < 54 {
        return Err(MediaError::Truncated);
    }
    let declared_size = le_u32(data, 2).ok_or(MediaError::Truncated)? as usize;
    let pixel_offset = le_u32(data, 10).ok_or(MediaError::Truncated)? as usize;
    let dib_size = le_u32(data, 14).ok_or(MediaError::Truncated)? as usize;
    if dib_size < 40 || 14usize.checked_add(dib_size).map_or(true, |end| end > data.len()) {
        return Err(MediaError::InvalidHeader);
    }
    let signed_width = le_u32(data, 18).ok_or(MediaError::Truncated)? as i32;
    let signed_height = le_u32(data, 22).ok_or(MediaError::Truncated)? as i32;
    let planes = le_u16(data, 26).ok_or(MediaError::Truncated)?;
    let bpp = le_u16(data, 28).ok_or(MediaError::Truncated)?;
    let compression = le_u32(data, 30).ok_or(MediaError::Truncated)?;
    if signed_width <= 0 || signed_height == 0 || signed_height == i32::MIN || planes != 1 {
        return Err(MediaError::InvalidDimensions);
    }
    if !matches!(bpp, 24 | 32) || compression != 0 {
        return Err(MediaError::UnsupportedEncoding);
    }
    let width = signed_width as u32;
    let height = signed_height.unsigned_abs();
    if !dimensions_ok(width, height) {
        return Err(MediaError::InvalidDimensions);
    }
    let bytes_per_pixel = (bpp / 8) as usize;
    let row_bytes = (width as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or(MediaError::InvalidDimensions)?;
    let stride = row_bytes.checked_add(3).ok_or(MediaError::InvalidDimensions)? & !3;
    let image_bytes = stride.checked_mul(height as usize).ok_or(MediaError::InvalidDimensions)?;
    let end = pixel_offset.checked_add(image_bytes).ok_or(MediaError::InvalidContainer)?;
    if pixel_offset < 14 + dib_size
        || end > data.len()
        || (declared_size != 0 && (declared_size > data.len() || declared_size < end))
    {
        return Err(MediaError::Truncated);
    }
    let mut info = MediaInfo::new(MediaKind::Bmp, if bpp == 24 { "BMP BGR24" } else { "BMP BGRX32" });
    info.width = Some(width);
    info.height = Some(height);
    info.bits_per_sample = Some(bpp);
    Ok(info)
}

fn inspect_png(data: &[u8]) -> Result<MediaInfo, MediaError> {
    if data.len() < 33 {
        return Err(MediaError::Truncated);
    }
    if be_u32(data, 8) != Some(13) || &data[12..16] != b"IHDR" {
        return Err(MediaError::InvalidHeader);
    }
    let width = be_u32(data, 16).ok_or(MediaError::Truncated)?;
    let height = be_u32(data, 20).ok_or(MediaError::Truncated)?;
    let bit_depth = data[24];
    let color_type = data[25];
    if !dimensions_ok(width, height) {
        return Err(MediaError::InvalidDimensions);
    }
    let valid_depth = match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        4 | 6 => matches!(bit_depth, 8 | 16),
        _ => false,
    };
    if !valid_depth || data[26] != 0 || data[27] != 0 || data[28] > 1 {
        return Err(MediaError::UnsupportedEncoding);
    }

    // Animated PNG decoding can expand a composited canvas to RGBA16 regardless
    // of the source IHDR pixel format. This preview path budgets the static image
    // layout before calling the codec, so APNG is rejected explicitly until a
    // frame-aware budget/decode path is wired in.
    let mut chunk_pos = 8usize;
    while chunk_pos.checked_add(8).map_or(false, |end| end <= data.len()) {
        let chunk_len = be_u32(data, chunk_pos).ok_or(MediaError::Truncated)? as usize;
        let chunk_type = data.get(chunk_pos + 4..chunk_pos + 8).ok_or(MediaError::Truncated)?;
        if chunk_type == b"acTL" {
            return Err(MediaError::UnsupportedEncoding);
        }
        if chunk_type == b"IDAT" {
            break;
        }
        let next = chunk_pos
            .checked_add(12)
            .and_then(|base| base.checked_add(chunk_len))
            .ok_or(MediaError::InvalidContainer)?;
        if next > data.len() {
            break;
        }
        chunk_pos = next;
    }

    let mut info = MediaInfo::new(MediaKind::Png, "PNG");
    info.width = Some(width);
    info.height = Some(height);
    info.bits_per_sample = Some(bit_depth as u16);
    info.progressive = data[28] == 1; // PNG interlace flag.
    Ok(info)
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(marker, 0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF)
}

fn inspect_jpeg(data: &[u8]) -> Result<MediaInfo, MediaError> {
    let mut pos = 2usize;
    while pos < data.len() {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            return Err(MediaError::Truncated);
        }
        let marker = data[pos];
        pos += 1;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let segment_len = be_u16(data, pos).ok_or(MediaError::Truncated)? as usize;
        if segment_len < 2 {
            return Err(MediaError::InvalidContainer);
        }
        let segment_end = pos.checked_add(segment_len).ok_or(MediaError::InvalidContainer)?;
        if segment_end > data.len() {
            return Err(MediaError::Truncated);
        }
        if is_jpeg_sof(marker) {
            if segment_len < 8 {
                return Err(MediaError::InvalidHeader);
            }
            let precision = data[pos + 2];
            let height = be_u16(data, pos + 3).ok_or(MediaError::Truncated)? as u32;
            let width = be_u16(data, pos + 5).ok_or(MediaError::Truncated)? as u32;
            let components = data[pos + 7];
            if precision == 0 || components == 0 || !dimensions_ok(width, height) {
                return Err(MediaError::InvalidDimensions);
            }
            if segment_len < 8usize.saturating_add(components as usize * 3) {
                return Err(MediaError::InvalidHeader);
            }
            let mut info = MediaInfo::new(MediaKind::Jpeg, if marker == 0xC2 { "JPEG progressive" } else { "JPEG DCT" });
            info.width = Some(width);
            info.height = Some(height);
            info.bits_per_sample = Some(precision as u16);
            info.progressive = marker == 0xC2;
            return Ok(info);
        }
        pos = segment_end;
    }
    Err(MediaError::InvalidHeader)
}

fn inspect_wav(data: &[u8]) -> Result<MediaInfo, MediaError> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(MediaError::InvalidHeader);
    }
    let riff_size = le_u32(data, 4).ok_or(MediaError::Truncated)? as usize;
    let riff_end = 8usize.checked_add(riff_size).ok_or(MediaError::InvalidContainer)?;
    if riff_end > data.len() || riff_end < 12 {
        return Err(MediaError::Truncated);
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u32, u16, u16)> = None;
    let mut data_len: Option<u32> = None;
    while pos.checked_add(8).map_or(false, |end| end <= riff_end) {
        let size = le_u32(data, pos + 4).ok_or(MediaError::Truncated)? as usize;
        let payload = pos + 8;
        let end = payload.checked_add(size).ok_or(MediaError::InvalidContainer)?;
        if end > riff_end {
            return Err(MediaError::Truncated);
        }
        let chunk_id = &data[pos..pos + 4];
        if chunk_id == b"fmt " {
            if size < 16 {
                return Err(MediaError::InvalidHeader);
            }
            fmt = Some((
                le_u16(data, payload).ok_or(MediaError::Truncated)?,
                le_u16(data, payload + 2).ok_or(MediaError::Truncated)?,
                le_u32(data, payload + 4).ok_or(MediaError::Truncated)?,
                le_u32(data, payload + 8).ok_or(MediaError::Truncated)?,
                le_u16(data, payload + 12).ok_or(MediaError::Truncated)?,
                le_u16(data, payload + 14).ok_or(MediaError::Truncated)?,
            ));
        } else if chunk_id == b"data" {
            data_len = Some(size as u32);
        }
        pos = end.checked_add(size & 1).ok_or(MediaError::InvalidContainer)?;
        if pos > riff_end {
            return Err(MediaError::Truncated);
        }
    }
    if pos != riff_end {
        return Err(MediaError::Truncated);
    }
    let (format, channels, rate, byte_rate, block_align, bits) = fmt.ok_or(MediaError::InvalidHeader)?;
    if !matches!(format, 1 | 3) || channels == 0 || channels > 8 || !(8_000..=384_000).contains(&rate)
        || !matches!(bits, 8 | 16 | 24 | 32)
        || block_align as u32 != channels as u32 * (bits as u32 / 8)
        || byte_rate != rate.saturating_mul(block_align as u32)
    {
        return Err(MediaError::UnsupportedEncoding);
    }
    let data_len = data_len.ok_or(MediaError::InvalidHeader)? as u64;
    let duration_ms = data_len.checked_mul(1000).ok_or(MediaError::InvalidContainer)? / byte_rate as u64;
    let mut info = MediaInfo::new(MediaKind::Wav, if format == 1 { "WAVE PCM" } else { "WAVE IEEE float" });
    info.sample_rate = Some(rate);
    info.channels = Some(channels);
    info.bits_per_sample = Some(bits);
    info.duration_ms = Some(duration_ms);
    Ok(info)
}

#[derive(Clone, Copy)]
struct Mp3Header {
    version: u8,
    sample_rate: u32,
    bitrate_kbps: u16,
    frame_len: usize,
    samples_per_frame: u16,
    mono: bool,
}

fn parse_mp3_header(bytes: &[u8]) -> Option<Mp3Header> {
    if bytes.len() < 4 {
        return None;
    }
    let h = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if (h >> 21) & 0x7FF != 0x7FF {
        return None;
    }
    let version = ((h >> 19) & 0x3) as u8;
    let layer = (h >> 17) & 0x3;
    let bitrate_idx = ((h >> 12) & 0xF) as usize;
    let sample_idx = ((h >> 10) & 0x3) as usize;
    let padding = ((h >> 9) & 1) as usize;
    let channel_mode = (h >> 6) & 0x3;
    if version == 1 || layer != 1 || bitrate_idx == 0 || bitrate_idx == 15 || sample_idx == 3 {
        return None;
    }
    const MPEG1_L3: [u16; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
    const MPEG2_L3: [u16; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];
    const BASE_RATES: [u32; 3] = [44_100, 48_000, 32_000];
    let bitrate = if version == 3 { MPEG1_L3[bitrate_idx] } else { MPEG2_L3[bitrate_idx] };
    let divisor = match version { 3 => 1, 2 => 2, _ => 4 };
    let sample_rate = BASE_RATES[sample_idx] / divisor;
    let samples = if version == 3 { 1152 } else { 576 };
    let coefficient = if version == 3 { 144_000u32 } else { 72_000u32 };
    let frame_len = (coefficient * bitrate as u32 / sample_rate) as usize + padding;
    if frame_len < 24 {
        return None;
    }
    Some(Mp3Header {
        version,
        sample_rate,
        bitrate_kbps: bitrate,
        frame_len,
        samples_per_frame: samples,
        mono: channel_mode == 3,
    })
}

fn id3_audio_offset(data: &[u8]) -> usize {
    if data.len() >= 10 && &data[..3] == b"ID3" {
        let size = ((data[6] as usize & 0x7F) << 21)
            | ((data[7] as usize & 0x7F) << 14)
            | ((data[8] as usize & 0x7F) << 7)
            | (data[9] as usize & 0x7F);
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        10usize.saturating_add(size).saturating_add(footer).min(data.len())
    } else {
        0
    }
}

fn inspect_mp3(data: &[u8]) -> Option<Result<MediaInfo, MediaError>> {
    let start = id3_audio_offset(data);
    let search_end = data.len().min(start.saturating_add(65_536));
    let mut first = None;
    let mut first_pos = start;
    for pos in start..search_end.saturating_sub(3) {
        if let Some(header) = parse_mp3_header(&data[pos..]) {
            if pos.checked_add(header.frame_len).map_or(false, |end| end <= data.len()) {
                first = Some(header);
                first_pos = pos;
                break;
            }
        }
    }
    let header = first?;
    let mut frames = 0u64;
    let mut pos = first_pos;
    while pos.checked_add(4).map_or(false, |end| end <= data.len()) {
        let Some(current) = parse_mp3_header(&data[pos..]) else { break };
        if current.version != header.version || current.sample_rate != header.sample_rate
            || pos.checked_add(current.frame_len).map_or(true, |end| end > data.len())
        {
            break;
        }
        frames = frames.saturating_add(1);
        pos += current.frame_len;
    }
    if frames == 0 {
        return Some(Err(MediaError::InvalidContainer));
    }
    let duration_ms = frames
        .saturating_mul(header.samples_per_frame as u64)
        .saturating_mul(1000)
        / header.sample_rate as u64;
    let mut info = MediaInfo::new(MediaKind::Mp3, "MPEG Audio Layer III");
    info.sample_rate = Some(header.sample_rate);
    info.channels = Some(if header.mono { 1 } else { 2 });
    info.bitrate_kbps = Some(header.bitrate_kbps);
    info.duration_ms = Some(duration_ms);
    Some(Ok(info))
}

fn looks_like_mp4(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    &data[4..8] == b"ftyp"
}

#[derive(Clone, Copy)]
struct BoxRef {
    kind: [u8; 4],
    payload_start: usize,
    end: usize,
}

fn next_box(data: &[u8], cursor: &mut usize, limit: usize) -> Result<Option<BoxRef>, MediaError> {
    if *cursor == limit {
        return Ok(None);
    }
    if (*cursor).checked_add(8).map_or(true, |end| end > limit) {
        return Err(MediaError::Truncated);
    }
    let size32 = be_u32(data, *cursor).ok_or(MediaError::Truncated)?;
    let mut header = 8usize;
    let size = if size32 == 1 {
        header = 16;
        let high = be_u32(data, *cursor + 8).ok_or(MediaError::Truncated)? as u64;
        let low = be_u32(data, *cursor + 12).ok_or(MediaError::Truncated)? as u64;
        let wide = (high << 32) | low;
        usize::try_from(wide).map_err(|_| MediaError::InvalidContainer)?
    } else if size32 == 0 {
        limit - *cursor
    } else {
        size32 as usize
    };
    if size < header {
        return Err(MediaError::InvalidContainer);
    }
    let end = cursor.checked_add(size).ok_or(MediaError::InvalidContainer)?;
    if end > limit {
        return Err(MediaError::Truncated);
    }
    let mut kind = [0u8; 4];
    kind.copy_from_slice(&data[*cursor + 4..*cursor + 8]);
    let item = BoxRef { kind, payload_start: *cursor + header, end };
    *cursor = end;
    Ok(Some(item))
}

fn find_child(data: &[u8], mut start: usize, end: usize, wanted: &[u8; 4]) -> Result<Option<BoxRef>, MediaError> {
    while let Some(item) = next_box(data, &mut start, end)? {
        if &item.kind == wanted {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

fn parse_mvhd(data: &[u8], item: BoxRef) -> Option<u64> {
    let p = item.payload_start;
    let version = *data.get(p)?;
    let (timescale, duration) = if version == 0 {
        (be_u32(data, p + 12)? as u64, be_u32(data, p + 16)? as u64)
    } else if version == 1 {
        let timescale = be_u32(data, p + 20)? as u64;
        let high = be_u32(data, p + 24)? as u64;
        let low = be_u32(data, p + 28)? as u64;
        (timescale, (high << 32) | low)
    } else {
        return None;
    };
    if timescale == 0 || duration == u64::MAX {
        return None;
    }
    duration.checked_mul(1000).map(|n| n / timescale)
}

fn track_is_video(data: &[u8], trak: BoxRef) -> Result<bool, MediaError> {
    let mdia = match find_child(data, trak.payload_start, trak.end, b"mdia")? {
        Some(v) => v,
        None => return Ok(false),
    };
    let hdlr = match find_child(data, mdia.payload_start, mdia.end, b"hdlr")? {
        Some(v) => v,
        None => return Ok(false),
    };
    let p = hdlr.payload_start;
    Ok(data.get(p + 8..p + 12) == Some(&b"vide"[..]))
}

fn parse_tkhd_size(data: &[u8], trak: BoxRef) -> Result<Option<(u32, u32)>, MediaError> {
    let Some(tkhd) = find_child(data, trak.payload_start, trak.end, b"tkhd")? else {
        return Ok(None);
    };
    if tkhd.end < tkhd.payload_start + 8 {
        return Err(MediaError::Truncated);
    }
    let w = be_u32(data, tkhd.end - 8).ok_or(MediaError::Truncated)? >> 16;
    let h = be_u32(data, tkhd.end - 4).ok_or(MediaError::Truncated)? >> 16;
    if dimensions_ok(w, h) { Ok(Some((w, h))) } else { Ok(None) }
}

fn codec_from_track(data: &[u8], trak: BoxRef) -> &'static str {
    let Some(mdia) = find_child(data, trak.payload_start, trak.end, b"mdia").ok().flatten() else {
        return "MP4 container";
    };
    let Some(minf) = find_child(data, mdia.payload_start, mdia.end, b"minf").ok().flatten() else {
        return "MP4 container";
    };
    let Some(stbl) = find_child(data, minf.payload_start, minf.end, b"stbl").ok().flatten() else {
        return "MP4 container";
    };
    let Some(stsd) = find_child(data, stbl.payload_start, stbl.end, b"stsd").ok().flatten() else {
        return "MP4 container";
    };
    let start = stsd.payload_start.saturating_add(8);
    let end = stsd.end.min(start.saturating_add(64));
    let sample = data.get(start..end).unwrap_or(&[]);
    if sample.windows(4).any(|w| w == b"avc1" || w == b"avc3") {
        "H.264 / AVC"
    } else if sample.windows(4).any(|w| w == b"hvc1" || w == b"hev1") {
        "H.265 / HEVC"
    } else if sample.windows(4).any(|w| w == b"mp4a") {
        "AAC"
    } else if sample.windows(4).any(|w| w == b"vp09") {
        "VP9"
    } else {
        "MP4 (codec unknown)"
    }
}

fn inspect_mp4(data: &[u8]) -> Result<MediaInfo, MediaError> {
    let mut top = 0usize;
    let mut moov = None;
    let mut ftyp_seen = false;
    while let Some(item) = next_box(data, &mut top, data.len())? {
        if &item.kind == b"ftyp" {
            if item.end.saturating_sub(item.payload_start) < 8 {
                return Err(MediaError::InvalidContainer);
            }
            ftyp_seen = true;
        } else if &item.kind == b"moov" {
            moov = Some(item);
        }
    }
    if !ftyp_seen {
        return Err(MediaError::InvalidContainer);
    }
    let moov = moov.ok_or(MediaError::Truncated)?;
    let mut info = MediaInfo::new(MediaKind::Mp4, "ISO Base Media / MP4");
    if let Some(mvhd) = find_child(data, moov.payload_start, moov.end, b"mvhd")? {
        info.duration_ms = parse_mvhd(data, mvhd);
    }
    let mut cursor = moov.payload_start;
    while let Some(item) = next_box(data, &mut cursor, moov.end)? {
        if &item.kind == b"trak" && track_is_video(data, item)? {
            if let Some((w, h)) = parse_tkhd_size(data, item)? {
                info.width = Some(w);
                info.height = Some(h);
            }
            info.codec = codec_from_track(data, item);
            break;
        }
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec;
    use std::vec::Vec;

    fn push_be32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn png_header_is_checked_and_dimensions_are_real() {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        push_be32(&mut bytes, 13);
        bytes.extend_from_slice(b"IHDR");
        push_be32(&mut bytes, 1920);
        push_be32(&mut bytes, 1080);
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        push_be32(&mut bytes, 0); // CRC is checked by the decoder, not this metadata parser.
        let info = inspect(&bytes).unwrap();
        assert_eq!(info.kind, MediaKind::Png);
        assert_eq!(info.width, Some(1920));
        assert_eq!(info.height, Some(1080));
    }

    #[test]
    fn jpeg_dimensions_come_from_sof_marker() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 8, 0x02, 0xD0, 0x05, 0x00, 3,
            1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0,
        ];
        let info = inspect(&bytes).unwrap();
        assert_eq!(info.kind, MediaKind::Jpeg);
        assert_eq!(info.width, Some(1280));
        assert_eq!(info.height, Some(720));
    }

    #[test]
    fn mp3_duration_uses_valid_frame_count() {
        let mut bytes = Vec::new();
        // MPEG-1, Layer III, 128 kbps, 44.1 kHz, stereo; 417-byte frames.
        let header = [0xFF, 0xFB, 0x90, 0x64];
        for _ in 0..10 {
            bytes.extend_from_slice(&header);
            bytes.resize(bytes.len() + 413, 0);
        }
        let info = inspect(&bytes).unwrap();
        assert_eq!(info.kind, MediaKind::Mp3);
        assert_eq!(info.sample_rate, Some(44_100));
        assert_eq!(info.bitrate_kbps, Some(128));
        assert_eq!(info.duration_ms, Some(261));
    }

    #[test]
    fn wav_duration_uses_data_size_and_byte_rate() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        push_le32(&mut bytes, 36 + 17640);
        bytes.extend_from_slice(b"WAVEfmt ");
        push_le32(&mut bytes, 16);
        push_le16(&mut bytes, 1);
        push_le16(&mut bytes, 2);
        push_le32(&mut bytes, 44_100);
        push_le32(&mut bytes, 176_400);
        push_le16(&mut bytes, 4);
        push_le16(&mut bytes, 16);
        bytes.extend_from_slice(b"data");
        push_le32(&mut bytes, 17_640);
        bytes.resize(44 + 17_640, 0);
        let info = inspect(&bytes).unwrap();
        assert_eq!(info.kind, MediaKind::Wav);
        assert_eq!(info.duration_ms, Some(100));
    }

    fn push_le16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_le32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn truncated_bmp_is_rejected() {
        assert_eq!(inspect(b"BM\x00\x00"), Err(MediaError::Truncated));
    }

    #[test]
    fn png_decoder_rejects_invalid_chunk_crc() {
        // Structurally valid IHDR, deliberately invalid CRC/IDAT payload.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        push_be32(&mut bytes, 13);
        bytes.extend_from_slice(b"IHDR");
        push_be32(&mut bytes, 1);
        push_be32(&mut bytes, 1);
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        push_be32(&mut bytes, 0);
        assert!(decode_png_rgba(&bytes).is_err());
    }
}


extern crate alloc;
#[cfg(test)]
extern crate std;

use alloc::vec::Vec;
pub use nc_hal::{AudioOutput, AudioError, PcmFormat};

/// Encoded image files are read with a separate cap before they reach a decoder.
pub const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Hard ceiling for the decoder's temporary full-resolution pixel buffer.
/// The cap is in bytes because PNG pixel formats range from 1 to 8 bytes per pixel.
pub const MAX_DECODED_SOURCE_BYTES: usize = 64 * 1024 * 1024;
/// Absolute source-pixel ceiling, including formats that use one byte per sample.
pub const MAX_DECODED_SOURCE_PIXELS: u64 = 20_000_000;
/// Default file-manager preview bounds. The original image dimensions are retained
/// separately by the caller; only this much raster data is stored for display.
pub const MAX_PREVIEW_WIDTH: u32 = 1280;
pub const MAX_PREVIEW_HEIGHT: u32 = 800;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedRaster {
    /// Width of the returned preview raster, not necessarily the source image.
    pub width: u32,
    /// Height of the returned preview raster, not necessarily the source image.
    pub height: u32,
    /// Pixels are packed as 0xAARRGGBB.
    pub pixels_argb: Vec<u32>,
}

fn preview_dimensions(
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> Result<(u32, u32), MediaError> {
    if width == 0 || height == 0 || max_width == 0 || max_height == 0 {
        return Err(MediaError::InvalidDimensions);
    }
    let max_width = max_width.min(MAX_PREVIEW_WIDTH);
    let max_height = max_height.min(MAX_PREVIEW_HEIGHT);
    let w = width as u64;
    let h = height as u64;
    let mw = max_width as u64;
    let mh = max_height as u64;

    if w <= mw && h <= mh {
        return Ok((width, height));
    }

    // Choose the limiting axis without floating point arithmetic or overflow.
    let (out_w, out_h) = if w.saturating_mul(mh) >= h.saturating_mul(mw) {
        (mw, (h.saturating_mul(mw) / w).max(1))
    } else {
        ((w.saturating_mul(mh) / h).max(1), mh)
    };
    Ok((out_w as u32, out_h as u32))
}


fn sample_png_argb(
    format: &nopng::PixelFormat,
    data: &[u8],
    pixel_index: usize,
) -> Result<u32, MediaError> {
    use nopng::PixelFormat;

    let scaled_gray = |value: u8, max: u8| -> u8 {
        if max == 0 { 0 } else { ((value as u16 * 255) / max as u16) as u8 }
    };
    let (r, g, b, a) = match format {
        PixelFormat::Gray1 => {
            let v = scaled_gray(*data.get(pixel_index).ok_or(MediaError::Truncated)?, 1);
            (v, v, v, 255)
        }
        PixelFormat::Gray2 => {
            let v = scaled_gray(*data.get(pixel_index).ok_or(MediaError::Truncated)?, 3);
            (v, v, v, 255)
        }
        PixelFormat::Gray4 => {
            let v = scaled_gray(*data.get(pixel_index).ok_or(MediaError::Truncated)?, 15);
            (v, v, v, 255)
        }
        PixelFormat::Gray8 => {
            let v = *data.get(pixel_index).ok_or(MediaError::Truncated)?;
            (v, v, v, 255)
        }
        PixelFormat::Gray16Be => {
            let base = pixel_index.checked_mul(2).ok_or(MediaError::ResourceLimit)?;
            let v = *data.get(base).ok_or(MediaError::Truncated)?;
            (v, v, v, 255)
        }
        PixelFormat::GrayAlpha8 => {
            let base = pixel_index.checked_mul(2).ok_or(MediaError::ResourceLimit)?;
            let v = *data.get(base).ok_or(MediaError::Truncated)?;
            let a = *data.get(base + 1).ok_or(MediaError::Truncated)?;
            (v, v, v, a)
        }
        PixelFormat::GrayAlpha16Be => {
            let base = pixel_index.checked_mul(4).ok_or(MediaError::ResourceLimit)?;
            let v = *data.get(base).ok_or(MediaError::Truncated)?;
            let a = *data.get(base + 2).ok_or(MediaError::Truncated)?;
            (v, v, v, a)
        }
        PixelFormat::Rgb8 => {
            let base = pixel_index.checked_mul(3).ok_or(MediaError::ResourceLimit)?;
            (
                *data.get(base).ok_or(MediaError::Truncated)?,
                *data.get(base + 1).ok_or(MediaError::Truncated)?,
                *data.get(base + 2).ok_or(MediaError::Truncated)?,
                255,
            )
        }
        PixelFormat::Rgb16Be => {
            let base = pixel_index.checked_mul(6).ok_or(MediaError::ResourceLimit)?;
            (
                *data.get(base).ok_or(MediaError::Truncated)?,
                *data.get(base + 2).ok_or(MediaError::Truncated)?,
                *data.get(base + 4).ok_or(MediaError::Truncated)?,
                255,
            )
        }
        PixelFormat::Rgba8 => {
            let base = pixel_index.checked_mul(4).ok_or(MediaError::ResourceLimit)?;
            (
                *data.get(base).ok_or(MediaError::Truncated)?,
                *data.get(base + 1).ok_or(MediaError::Truncated)?,
                *data.get(base + 2).ok_or(MediaError::Truncated)?,
                *data.get(base + 3).ok_or(MediaError::Truncated)?,
            )
        }
        PixelFormat::Rgba16Be => {
            let base = pixel_index.checked_mul(8).ok_or(MediaError::ResourceLimit)?;
            (
                *data.get(base).ok_or(MediaError::Truncated)?,
                *data.get(base + 2).ok_or(MediaError::Truncated)?,
                *data.get(base + 4).ok_or(MediaError::Truncated)?,
                *data.get(base + 6).ok_or(MediaError::Truncated)?,
            )
        }
        PixelFormat::Indexed1 { palette, trns }
        | PixelFormat::Indexed2 { palette, trns }
        | PixelFormat::Indexed4 { palette, trns }
        | PixelFormat::Indexed8 { palette, trns } => {
            let index = *data.get(pixel_index).ok_or(MediaError::Truncated)? as usize;
            let base = index.checked_mul(3).ok_or(MediaError::ResourceLimit)?;
            let rgb = palette.get(base..base + 3).ok_or(MediaError::InvalidContainer)?;
            let a = trns.as_ref().and_then(|values| values.get(index)).copied().unwrap_or(255);
            (rgb[0], rgb[1], rgb[2], a)
        }
    };

    Ok(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

fn png_source_bytes_per_pixel(data: &[u8]) -> Result<usize, MediaError> {
    if data.len() < 26 { return Err(MediaError::Truncated); }
    let depth = data[24];
    let color_type = data[25];
    let bytes = match (color_type, depth) {
        (0, 1 | 2 | 4 | 8) => 1,
        (0, 16) => 2,
        (2, 8) => 3,
        (2, 16) => 6,
        (3, 1 | 2 | 4 | 8) => 1,
        (4, 8) => 2,
        (4, 16) => 4,
        (6, 8) => 4,
        (6, 16) => 8,
        _ => return Err(MediaError::UnsupportedEncoding),
    };
    Ok(bytes)
}

fn with_png_rows<R>(
    data: &[u8],
    run: impl FnOnce(u32, u32, RowFn<'_>) -> Result<R, MediaError>,
) -> Result<R, MediaError> {
    if data.len() > MAX_ENCODED_IMAGE_BYTES { return Err(MediaError::ResourceLimit); }
    let info = inspect(data)?;
    if info.kind != MediaKind::Png { return Err(MediaError::UnknownFormat); }
    let width = info.width.ok_or(MediaError::InvalidHeader)?;
    let height = info.height.ok_or(MediaError::InvalidHeader)?;
    let count = (width as u64).checked_mul(height as u64).ok_or(MediaError::ResourceLimit)?;
    let source_bpp = png_source_bytes_per_pixel(data)? as u64;
    let decoded_bytes = count.checked_mul(source_bpp).ok_or(MediaError::ResourceLimit)?;
    if count == 0 || count > MAX_DECODED_SOURCE_PIXELS || decoded_bytes > MAX_DECODED_SOURCE_BYTES as u64 {
        return Err(MediaError::ResourceLimit);
    }

    let image = nopng::decode_image(data).map_err(|_| MediaError::DecodeFailed)?;
    if image.width.get() != width || image.height.get() != height {
        return Err(MediaError::InvalidContainer);
    }
    let expected_source_bytes = count
        .checked_mul(source_bpp)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(MediaError::ResourceLimit)?;
    if image.pixels.len() != expected_source_bytes {
        return Err(MediaError::InvalidContainer);
    }

    let format = &image.pixel_format;
    let pixels: &[u8] = &image.pixels;
    let mut rows = |y: u32, x0: u32, out: &mut [u32]| -> Result<(), MediaError> {
        let base = (y as usize)
            .checked_mul(width as usize)
            .and_then(|n| n.checked_add(x0 as usize))
            .ok_or(MediaError::ResourceLimit)?;
        for (offset, slot) in out.iter_mut().enumerate() {
            *slot = sample_png_argb(format, pixels, base + offset)?;
        }
        Ok(())
    };
    run(width, height, &mut rows)
}

/// Decodes a PNG into a display-sized preview. Downscaling uses an area-aware
/// tent filter, so photos stay smooth instead of aliasing like point sampling.
/// `max_width`/`max_height` are hard-clamped to safe UI caps.
pub fn decode_png_preview(
    data: &[u8],
    max_width: u32,
    max_height: u32,
) -> Result<DecodedRaster, MediaError> {
    with_png_rows(data, |width, height, rows| {
        let (preview_width, preview_height) = preview_dimensions(width, height, max_width, max_height)?;
        let pixels = resample_window(rows, 0, 0, width, height, preview_width, preview_height)?;
        Ok(DecodedRaster { width: preview_width, height: preview_height, pixels_argb: pixels })
    })
}

/// Decodes a PNG and fits it to an exact screen size without stretching it.
pub fn decode_png_wallpaper(
    data: &[u8],
    screen_width: u32,
    screen_height: u32,
    mode: FitMode,
) -> Result<DecodedRaster, MediaError> {
    with_png_rows(data, |width, height, rows| {
        let pixels = compose_screen(rows, width, height, screen_width, screen_height, mode)?;
        Ok(DecodedRaster { width: screen_width, height: screen_height, pixels_argb: pixels })
    })
}

/// Compatibility entry point. It now returns a display-sized preview rather than
/// allocating a full-resolution ARGB copy.
pub fn decode_png_rgba(data: &[u8]) -> Result<DecodedRaster, MediaError> {
    decode_png_preview(data, MAX_PREVIEW_WIDTH, MAX_PREVIEW_HEIGHT)
}

fn with_jpeg_rows<R>(
    data: &[u8],
    run: impl FnOnce(u32, u32, RowFn<'_>) -> Result<R, MediaError>,
) -> Result<R, MediaError> {
    if data.len() > MAX_ENCODED_IMAGE_BYTES { return Err(MediaError::ResourceLimit); }
    let info = inspect(data)?;
    if info.kind != MediaKind::Jpeg { return Err(MediaError::UnknownFormat); }
    let width = info.width.ok_or(MediaError::InvalidHeader)?;
    let height = info.height.ok_or(MediaError::InvalidHeader)?;
    let count = (width as u64).checked_mul(height as u64).ok_or(MediaError::ResourceLimit)?;
    let expected_source_bytes = count.checked_mul(3).ok_or(MediaError::ResourceLimit)?;
    if count == 0 || count > MAX_DECODED_SOURCE_PIXELS || expected_source_bytes > MAX_DECODED_SOURCE_BYTES as u64 {
        return Err(MediaError::ResourceLimit);
    }

    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;

    let options = DecoderOptions::new_safe()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(width as usize)
        .set_max_height(height as usize)
        .jpeg_set_max_scans(100);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().map_err(|_| MediaError::DecodeFailed)?;
    let (decoded_width, decoded_height) = decoder.dimensions().ok_or(MediaError::InvalidHeader)?;
    if decoded_width != width as usize || decoded_height != height as usize {
        return Err(MediaError::InvalidContainer);
    }

    let decoded_len = decoder.output_buffer_size().ok_or(MediaError::ResourceLimit)?;
    if decoded_len != expected_source_bytes as usize || decoded_len > MAX_DECODED_SOURCE_BYTES {
        return Err(MediaError::ResourceLimit);
    }
    let mut rgb = Vec::new();
    rgb.try_reserve_exact(decoded_len).map_err(|_| MediaError::ResourceLimit)?;
    rgb.resize(decoded_len, 0);
    decoder.decode_into(&mut rgb).map_err(|_| MediaError::DecodeFailed)?;

    let mut rows = |y: u32, x0: u32, out: &mut [u32]| -> Result<(), MediaError> {
        let base = (y as usize)
            .checked_mul(width as usize)
            .and_then(|n| n.checked_add(x0 as usize))
            .and_then(|n| n.checked_mul(3))
            .ok_or(MediaError::ResourceLimit)?;
        let bytes = rgb.get(base..base + out.len() * 3).ok_or(MediaError::Truncated)?;
        for (slot, px) in out.iter_mut().zip(bytes.chunks_exact(3)) {
            *slot = 0xFF00_0000 | ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
        }
        Ok(())
    };
    run(width, height, &mut rows)
}

/// Decodes a JPEG into a display-sized preview using the same area-aware filter as PNG.
pub fn decode_jpeg_preview(
    data: &[u8],
    max_width: u32,
    max_height: u32,
) -> Result<DecodedRaster, MediaError> {
    with_jpeg_rows(data, |width, height, rows| {
        let (preview_width, preview_height) = preview_dimensions(width, height, max_width, max_height)?;
        let pixels = resample_window(rows, 0, 0, width, height, preview_width, preview_height)?;
        Ok(DecodedRaster { width: preview_width, height: preview_height, pixels_argb: pixels })
    })
}

/// Decodes a JPEG and fits it to an exact screen size without stretching it.
pub fn decode_jpeg_wallpaper(
    data: &[u8],
    screen_width: u32,
    screen_height: u32,
    mode: FitMode,
) -> Result<DecodedRaster, MediaError> {
    with_jpeg_rows(data, |width, height, rows| {
        let pixels = compose_screen(rows, width, height, screen_width, screen_height, mode)?;
        Ok(DecodedRaster { width: screen_width, height: screen_height, pixels_argb: pixels })
    })
}

/// Compatibility entry point returning a bounded display preview.
pub fn decode_jpeg_rgba(data: &[u8]) -> Result<DecodedRaster, MediaError> {
    decode_jpeg_preview(data, MAX_PREVIEW_WIDTH, MAX_PREVIEW_HEIGHT)
}

/// A bounded random-access source. A VFS adapter can implement this with
/// `VfsManager::read_file_range_path`, avoiding whole-file allocations for audio.
pub trait MediaSource {
    fn len(&self) -> u64;
    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<usize, MediaError>;
}

fn read_exact_at<S: MediaSource>(source: &mut S, offset: u64, output: &mut [u8]) -> Result<(), MediaError> {
    let mut done = 0usize;
    while done < output.len() {
        let at = offset.checked_add(done as u64).ok_or(MediaError::InvalidContainer)?;
        let read = source.read_at(at, &mut output[done..])?;
        if read == 0 || read > output.len() - done {
            return Err(MediaError::Truncated);
        }
        done += read;
    }
    Ok(())
}


/// Maximum amount of audio/video metadata read while opening a file.
/// Media payloads (MP3 frames, WAV samples, MP4 mdat) are skipped, not copied.
pub const MAX_MEDIA_PROBE_BYTES: usize = 64 * 1024;
/// MP4 metadata is allowed a separate bounded allocation; video samples are never loaded.
pub const MAX_MP4_MOOV_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MP4_TOP_LEVEL_BOXES: usize = 4096;

/// Inspect audio/video metadata from a random-access source without reading the
/// complete media file. This is intended for file-manager open/preview paths.
/// MP3 duration is estimated from the first valid frame bitrate (VBR may vary).
pub fn inspect_source<S: MediaSource>(source: &mut S) -> Result<MediaInfo, MediaError> {
    let file_len = source.len();
    if file_len < 4 {
        return Err(MediaError::Empty);
    }

    let mut signature = [0u8; 12];
    let signature_len = usize::try_from(file_len.min(signature.len() as u64))
        .map_err(|_| MediaError::InvalidContainer)?;
    read_exact_at(source, 0, &mut signature[..signature_len])?;

    if signature_len >= 12 && &signature[..4] == b"RIFF" && &signature[8..12] == b"WAVE" {
        return inspect_wav_source(source);
    }
    if signature_len >= 8 && &signature[4..8] == b"ftyp" {
        return inspect_mp4_source(source);
    }

    // ID3 is optional. The bounded probe starts after it, so a large audio tag
    // is skipped by offset instead of being copied into a temporary Vec.
    let audio_offset = mp3_audio_offset(source)?;
    let remaining = file_len.checked_sub(audio_offset).ok_or(MediaError::InvalidContainer)?;
    let probe_len = usize::try_from(remaining.min(MAX_MEDIA_PROBE_BYTES as u64))
        .map_err(|_| MediaError::InvalidContainer)?;
    if probe_len < 4 {
        return Err(MediaError::UnknownFormat);
    }
    let mut probe = Vec::new();
    probe.try_reserve_exact(probe_len).map_err(|_| MediaError::ResourceLimit)?;
    probe.resize(probe_len, 0);
    read_exact_at(source, audio_offset, &mut probe)?;

    let result = inspect_mp3(&probe).ok_or(MediaError::UnknownFormat)??;
    let mut info = result;
    if let Some(bitrate_kbps) = info.bitrate_kbps.filter(|value| *value > 0) {
        let audio_bytes = remaining;
        // bytes * 8 / kbit/s yields milliseconds; saturate rather than overflow.
        info.duration_ms = Some(
            audio_bytes
                .saturating_mul(8)
                .checked_div(bitrate_kbps as u64)
                .unwrap_or(u64::MAX),
        );
    }
    Ok(info)
}

fn inspect_wav_source<S: MediaSource>(source: &mut S) -> Result<MediaInfo, MediaError> {
    if source.len() < 12 {
        return Err(MediaError::Truncated);
    }
    let mut riff = [0u8; 12];
    read_exact_at(source, 0, &mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(MediaError::InvalidHeader);
    }
    let riff_end = 8u64
        .checked_add(u32::from_le_bytes([riff[4], riff[5], riff[6], riff[7]]) as u64)
        .ok_or(MediaError::InvalidContainer)?;
    if riff_end > source.len() || riff_end < 12 {
        return Err(MediaError::Truncated);
    }

    let mut position = 12u64;
    let mut format: Option<(u16, u16, u32, u32, u16, u16)> = None;
    let mut data_len: Option<u64> = None;
    let mut chunks = 0usize;
    while position.checked_add(8).ok_or(MediaError::InvalidContainer)? <= riff_end {
        chunks += 1;
        if chunks > MAX_MP4_TOP_LEVEL_BOXES {
            return Err(MediaError::ResourceLimit);
        }
        let mut header = [0u8; 8];
        read_exact_at(source, position, &mut header)?;
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let payload = position.checked_add(8).ok_or(MediaError::InvalidContainer)?;
        let end = payload.checked_add(size).ok_or(MediaError::InvalidContainer)?;
        if end > riff_end {
            return Err(MediaError::Truncated);
        }
        if &header[..4] == b"fmt " {
            if size < 16 {
                return Err(MediaError::InvalidHeader);
            }
            let mut fmt = [0u8; 16];
            read_exact_at(source, payload, &mut fmt)?;
            format = Some((
                u16::from_le_bytes([fmt[0], fmt[1]]),
                u16::from_le_bytes([fmt[2], fmt[3]]),
                u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]),
                u32::from_le_bytes([fmt[8], fmt[9], fmt[10], fmt[11]]),
                u16::from_le_bytes([fmt[12], fmt[13]]),
                u16::from_le_bytes([fmt[14], fmt[15]]),
            ));
        } else if &header[..4] == b"data" {
            data_len = Some(size);
        }
        position = end
            .checked_add(size & 1)
            .ok_or(MediaError::InvalidContainer)?;
        if position > riff_end {
            return Err(MediaError::Truncated);
        }
    }
    if position != riff_end {
        return Err(MediaError::Truncated);
    }

    let (tag, channels, rate, byte_rate, align, bits) = format.ok_or(MediaError::InvalidHeader)?;
    if !matches!(tag, 1 | 3)
        || channels == 0
        || channels > 8
        || !(8_000..=384_000).contains(&rate)
        || !matches!(bits, 8 | 16 | 24 | 32)
        || align as u32 != channels as u32 * (bits as u32 / 8)
        || byte_rate != rate.saturating_mul(align as u32)
        || byte_rate == 0
    {
        return Err(MediaError::UnsupportedEncoding);
    }
    let data_len = data_len.ok_or(MediaError::InvalidHeader)?;
    let duration_ms = data_len
        .checked_mul(1000)
        .ok_or(MediaError::InvalidContainer)?
        / byte_rate as u64;
    let mut info = MediaInfo::new(MediaKind::Wav, if tag == 1 { "WAVE PCM" } else { "WAVE IEEE float" });
    info.sample_rate = Some(rate);
    info.channels = Some(channels);
    info.bits_per_sample = Some(bits);
    info.duration_ms = Some(duration_ms);
    Ok(info)
}

fn inspect_mp4_source<S: MediaSource>(source: &mut S) -> Result<MediaInfo, MediaError> {
    let file_len = source.len();
    let mut position = 0u64;
    let mut found_ftyp = false;
    let mut moov_range: Option<(u64, usize)> = None;
    let mut boxes_seen = 0usize;

    while position.checked_add(8).ok_or(MediaError::InvalidContainer)? <= file_len {
        boxes_seen += 1;
        if boxes_seen > MAX_MP4_TOP_LEVEL_BOXES {
            return Err(MediaError::ResourceLimit);
        }
        let mut header = [0u8; 16];
        read_exact_at(source, position, &mut header[..8])?;
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let kind = [header[4], header[5], header[6], header[7]];
        let (header_len, box_size) = if size32 == 1 {
            if position.checked_add(16).ok_or(MediaError::InvalidContainer)? > file_len {
                return Err(MediaError::Truncated);
            }
            read_exact_at(source, position + 8, &mut header[8..16])?;
            let high = u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as u64;
            let low = u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as u64;
            (16u64, (high << 32) | low)
        } else if size32 == 0 {
            (8u64, file_len - position)
        } else {
            (8u64, size32 as u64)
        };
        if box_size < header_len {
            return Err(MediaError::InvalidContainer);
        }
        let end = position.checked_add(box_size).ok_or(MediaError::InvalidContainer)?;
        if end > file_len {
            return Err(MediaError::Truncated);
        }

        if &kind == b"ftyp" {
            if box_size < header_len + 8 {
                return Err(MediaError::InvalidContainer);
            }
            found_ftyp = true;
        } else if &kind == b"moov" {
            if box_size > MAX_MP4_MOOV_BYTES {
                return Err(MediaError::ResourceLimit);
            }
            let moov_size = usize::try_from(box_size).map_err(|_| MediaError::ResourceLimit)?;
            moov_range = Some((position, moov_size));
            break;
        }
        if end <= position {
            return Err(MediaError::InvalidContainer);
        }
        position = end;
    }

    if !found_ftyp {
        return Err(MediaError::InvalidContainer);
    }
    let (moov_offset, moov_size) = moov_range.ok_or(MediaError::Truncated)?;
    let total = 16usize.checked_add(moov_size).ok_or(MediaError::ResourceLimit)?;
    let mut container = Vec::new();
    container.try_reserve_exact(total).map_err(|_| MediaError::ResourceLimit)?;
    // A small synthetic ftyp is sufficient: the real ftyp was validated above.
    container.extend_from_slice(&16u32.to_be_bytes());
    container.extend_from_slice(b"ftypisom\0\0\0\0");
    let old_len = container.len();
    container.resize(total, 0);
    read_exact_at(source, moov_offset, &mut container[old_len..])?;
    inspect_mp4(&container)
}

fn mp3_audio_offset<S: MediaSource>(source: &mut S) -> Result<u64, MediaError> {
    if source.len() < 10 { return Ok(0); }
    let mut header = [0u8; 10];
    read_exact_at(source, 0, &mut header)?;
    if &header[..3] != b"ID3" { return Ok(0); }
    if header[6..10].iter().any(|v| (v & 0x80) != 0) {
        return Err(MediaError::InvalidHeader);
    }
    let size = ((header[6] as u64) << 21)
        | ((header[7] as u64) << 14)
        | ((header[8] as u64) << 7)
        | header[9] as u64;
    let footer = if header[5] & 0x10 != 0 { 10 } else { 0 };
    let end = 10u64.checked_add(size).and_then(|v| v.checked_add(footer)).ok_or(MediaError::InvalidContainer)?;
    if end > source.len() { return Err(MediaError::Truncated); }
    Ok(end)
}

/// Decode an MP3 from a streaming source and submit actual PCM frames to an audio
/// backend. Input and PCM buffers are fixed-size; files are never copied wholesale.
pub fn play_mp3<S: MediaSource, A: AudioOutput>(
    source: &mut S,
    output: &mut A,
) -> Result<u64, MediaError> {
    const INPUT_WINDOW: usize = 16 * 1024;
    let file_len = source.len();
    if file_len == 0 { return Err(MediaError::Empty); }
    let mut read_offset = mp3_audio_offset(source)?;
    let mut input = [0u8; INPUT_WINDOW];
    let mut available = 0usize;
    let mut decoder = mbop3::Decoder::new_boxed();
    let mut pcm = [0i16; mbop3::MAX_SAMPLES_PER_FRAME];
    let mut configured: Option<PcmFormat> = None;
    let mut decoded_frames = 0u64;
    let mut total_samples = 0u64;

    loop {
        if available < INPUT_WINDOW && read_offset < file_len {
            let read = source.read_at(read_offset, &mut input[available..])?;
            if read == 0 || read > INPUT_WINDOW - available {
                return Err(MediaError::IoFailure);
            }
            available += read;
            read_offset = read_offset.checked_add(read as u64).ok_or(MediaError::InvalidContainer)?;
        }

        let (samples_per_channel, frame) = decoder.decode_frame(&input[..available], Some(&mut pcm));
        if frame.frame_bytes > 0 {
            let consumed = usize::try_from(frame.frame_bytes).map_err(|_| MediaError::InvalidContainer)?;
            if consumed > available { return Err(MediaError::InvalidContainer); }
            if frame.layer != 3 {
                return Err(MediaError::UnsupportedEncoding);
            }
            let channels = u8::try_from(frame.channels).map_err(|_| MediaError::UnsupportedEncoding)?;
            let rate = u32::try_from(frame.hz).map_err(|_| MediaError::UnsupportedEncoding)?;
            if !matches!(channels, 1 | 2) || !(8_000..=192_000).contains(&rate) {
                return Err(MediaError::UnsupportedEncoding);
            }
            let format = PcmFormat { sample_rate_hz: rate, channels };
            if configured != Some(format) {
                output.configure(format).map_err(|_| MediaError::AudioOutputFailure)?;
                configured = Some(format);
            }
            let sample_count = samples_per_channel.checked_mul(channels as usize).ok_or(MediaError::InvalidContainer)?;
            if sample_count > pcm.len() { return Err(MediaError::InvalidContainer); }
            if sample_count != 0 {
                output.write_interleaved_i16(&pcm[..sample_count]).map_err(|_| MediaError::AudioOutputFailure)?;
                total_samples = total_samples.saturating_add(samples_per_channel as u64);
            }
            input.copy_within(consumed..available, 0);
            available -= consumed;
            decoded_frames = decoded_frames.saturating_add(1);
            continue;
        }

        if read_offset >= file_len {
            let tail = &input[..available];
            if tail.is_empty() || tail.iter().all(|byte| *byte == 0) || (tail.len() <= 128 && tail.starts_with(b"TAG")) {
                break;
            }
            return Err(MediaError::Truncated);
        }
        if available == INPUT_WINDOW {
            return Err(MediaError::InvalidContainer);
        }
    }

    if decoded_frames == 0 { return Err(MediaError::InvalidHeader); }
    output.drain().map_err(|_| MediaError::AudioOutputFailure)?;
    Ok(total_samples)
}

/// Stream a conventional RIFF/WAVE PCM16 file to a hardware-independent audio sink.
/// Other WAV sample encodings are rejected explicitly instead of being misinterpreted.
pub fn play_wav_pcm16<S: MediaSource, A: AudioOutput>(
    source: &mut S,
    output: &mut A,
) -> Result<u64, MediaError> {
    if source.len() < 12 { return Err(MediaError::Truncated); }
    let mut riff = [0u8; 12];
    read_exact_at(source, 0, &mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..12] != b"WAVE" { return Err(MediaError::InvalidHeader); }
    let riff_end = 8u64.checked_add(u32::from_le_bytes([riff[4], riff[5], riff[6], riff[7]]) as u64)
        .ok_or(MediaError::InvalidContainer)?;
    if riff_end > source.len() || riff_end < 12 { return Err(MediaError::Truncated); }

    let mut position = 12u64;
    let mut format: Option<(u16, u16, u32, u32, u16, u16)> = None;
    let mut data_chunk: Option<(u64, u64)> = None;
    while position.checked_add(8).ok_or(MediaError::InvalidContainer)? <= riff_end {
        let mut chunk = [0u8; 8];
        read_exact_at(source, position, &mut chunk)?;
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
        let payload = position.checked_add(8).ok_or(MediaError::InvalidContainer)?;
        let end = payload.checked_add(size).ok_or(MediaError::InvalidContainer)?;
        if end > riff_end { return Err(MediaError::Truncated); }
        if &chunk[..4] == b"fmt " {
            if size < 16 { return Err(MediaError::InvalidHeader); }
            let mut fmt = [0u8; 16];
            read_exact_at(source, payload, &mut fmt)?;
            format = Some((
                u16::from_le_bytes([fmt[0], fmt[1]]),
                u16::from_le_bytes([fmt[2], fmt[3]]),
                u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]),
                u32::from_le_bytes([fmt[8], fmt[9], fmt[10], fmt[11]]),
                u16::from_le_bytes([fmt[12], fmt[13]]),
                u16::from_le_bytes([fmt[14], fmt[15]]),
            ));
        } else if &chunk[..4] == b"data" {
            data_chunk = Some((payload, size));
        }
        position = end.checked_add(size & 1).ok_or(MediaError::InvalidContainer)?;
    }
    if position != riff_end {
        return Err(MediaError::Truncated);
    }

    let (tag, channels, rate, byte_rate, align, bits) = format.ok_or(MediaError::InvalidHeader)?;
    let (data_offset, data_size) = data_chunk.ok_or(MediaError::InvalidHeader)?;
    if tag != 1 || !matches!(channels, 1 | 2) || !(8_000..=192_000).contains(&rate)
        || bits != 16 || align != channels * 2 || byte_rate != rate.saturating_mul(align as u32)
        || data_size % align as u64 != 0
    {
        return Err(MediaError::UnsupportedEncoding);
    }
    output.configure(PcmFormat { sample_rate_hz: rate, channels: channels as u8 })
        .map_err(|_| MediaError::AudioOutputFailure)?;

    let mut offset = 0u64;
    let mut raw = [0u8; 4096];
    let mut samples = [0i16; 2048];
    while offset < data_size {
        let remaining = data_size - offset;
        let request = raw.len().min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let request = request - (request % align as usize);
        if request == 0 { break; }
        read_exact_at(source, data_offset.checked_add(offset).ok_or(MediaError::InvalidContainer)?, &mut raw[..request])?;
        let sample_count = request / 2;
        for index in 0..sample_count {
            samples[index] = i16::from_le_bytes([raw[index * 2], raw[index * 2 + 1]]);
        }
        output.write_interleaved_i16(&samples[..sample_count]).map_err(|_| MediaError::AudioOutputFailure)?;
        offset = offset.checked_add(request as u64).ok_or(MediaError::InvalidContainer)?;
    }
    output.drain().map_err(|_| MediaError::AudioOutputFailure)?;
    Ok(data_size / align as u64)
}


#[cfg(test)]
mod preview_decode_tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn full_hd_is_inside_preview_source_budget() {
        let pixels = 1920u64 * 1080u64;
        assert!(pixels <= MAX_DECODED_SOURCE_PIXELS);
        assert!(pixels * 4 <= MAX_DECODED_SOURCE_BYTES as u64);
        assert_eq!(preview_dimensions(1920, 1080, 1280, 800), Ok((1280, 720)));
    }

    #[test]
    fn png_rgba8_decodes_to_exact_argb_pixels() {
        let image = nopng::Image {
            width: core::num::NonZeroU32::new(2).unwrap(),
            height: core::num::NonZeroU32::new(2).unwrap(),
            pixel_format: nopng::PixelFormat::Rgba8,
            pixels: vec![
                255, 0, 0, 255,   0, 255, 0, 255,
                0, 0, 255, 255,   255, 255, 255, 128,
            ],
        };
        let encoded = nopng::encode_image(&image).unwrap();
        let preview = decode_png_preview(&encoded, 2, 2).unwrap();
        assert_eq!(preview.width, 2);
        assert_eq!(preview.height, 2);
        assert_eq!(preview.pixels_argb, vec![
            0xFFFF_0000, 0xFF00_FF00,
            0xFF00_00FF, 0x80FF_FFFF,
        ]);
    }

    #[test]
    fn indexed_png_respects_palette_alpha() {
        let image = nopng::Image {
            width: core::num::NonZeroU32::new(2).unwrap(),
            height: core::num::NonZeroU32::new(1).unwrap(),
            pixel_format: nopng::PixelFormat::Indexed8 {
                palette: vec![255, 0, 0, 0, 0, 255],
                trns: Some(vec![255, 64]),
            },
            pixels: vec![0, 1],
        };
        let encoded = nopng::encode_image(&image).unwrap();
        let preview = decode_png_preview(&encoded, 2, 1).unwrap();
        assert_eq!(preview.pixels_argb, vec![0xFFFF_0000, 0x4000_00FF]);
    }
}

#[cfg(test)]
mod resample_tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn solid(width: u32, height: u32, color: u32) -> Vec<u32> {
        vec![color; (width * height) as usize]
    }

    fn rows_of<'a>(pixels: &'a [u32], width: u32) -> impl FnMut(u32, u32, &mut [u32]) -> Result<(), MediaError> + 'a {
        move |y, x0, out| {
            let start = (y * width + x0) as usize;
            out.copy_from_slice(&pixels[start..start + out.len()]);
            Ok(())
        }
    }

    #[test]
    fn solid_colors_survive_scaling() {
        let src = solid(64, 48, 0xFF33_6699);
        let mut rows = rows_of(&src, 64);
        let down = resample_window(&mut rows, 0, 0, 64, 48, 17, 11).unwrap();
        assert!(down.iter().all(|p| *p == 0xFF33_6699));
        let mut rows = rows_of(&src, 64);
        let up = resample_window(&mut rows, 0, 0, 64, 48, 200, 150).unwrap();
        assert!(up.iter().all(|p| *p == 0xFF33_6699));
    }

    #[test]
    fn downscale_averages_instead_of_picking_one_pixel() {
        let mut src = Vec::new();
        for y in 0..8u32 {
            for x in 0..8u32 {
                src.push(if (x + y) % 2 == 0 { 0xFF00_0000 } else { 0xFFFF_FFFF });
            }
        }
        let mut rows = rows_of(&src, 8);
        let out = resample_window(&mut rows, 0, 0, 8, 8, 2, 2).unwrap();
        for p in out {
            let v = p & 255;
            assert!(v > 100 && v < 155, "checkerboard should average to gray, got {}", v);
        }
    }

    #[test]
    fn wallpaper_fill_and_fit_produce_exact_screen_size() {
        let wide = solid(400, 100, 0xFF20_8040);
        let mut rows = rows_of(&wide, 400);
        let filled = compose_screen(&mut rows, 400, 100, 160, 90, FitMode::Fill).unwrap();
        assert_eq!(filled.len(), 160 * 90);
        let mut rows = rows_of(&wide, 400);
        let fitted = compose_screen(&mut rows, 400, 100, 160, 90, FitMode::Auto).unwrap();
        assert_eq!(fitted.len(), 160 * 90);
        assert_ne!(fitted[0] & 0x00FF_FFFF, 0, "letterbox bars use a darkened blur, not pure black");
        assert!(fitted.iter().all(|p| *p >> 24 == 255));
    }

    #[test]
    fn similar_aspect_uses_fill_without_bars() {
        let src = solid(1920, 1000, 0xFFAA_BBCC);
        let mut rows = rows_of(&src, 1920);
        let out = compose_screen(&mut rows, 1920, 1000, 192, 100, FitMode::Auto).unwrap();
        assert!(out.iter().all(|p| *p == 0xFFAA_BBCC));
    }
}

#[cfg(test)]
mod streaming_inspection_tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    struct CountingSource {
        bytes: Vec<u8>,
        bytes_read: u64,
    }

    impl MediaSource for CountingSource {
        fn len(&self) -> u64 {
            self.bytes.len() as u64
        }

        fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<usize, MediaError> {
            let start = usize::try_from(offset).map_err(|_| MediaError::InvalidContainer)?;
            if start >= self.bytes.len() || output.is_empty() {
                return Ok(0);
            }
            let count = output.len().min(self.bytes.len() - start);
            output[..count].copy_from_slice(&self.bytes[start..start + count]);
            self.bytes_read = self.bytes_read.saturating_add(count as u64);
            Ok(count)
        }
    }

    #[test]
    fn mp3_inspection_does_not_read_the_full_audio_file() {
        let mut bytes = Vec::new();
        let frame = [0xFF, 0xFB, 0x90, 0x64];
        for _ in 0..300 {
            bytes.extend_from_slice(&frame);
            bytes.resize(bytes.len() + 413, 0);
        }
        let total_len = bytes.len() as u64;
        let mut source = CountingSource { bytes, bytes_read: 0 };
        let info = inspect_source(&mut source).unwrap();
        assert_eq!(info.kind, MediaKind::Mp3);
        assert!(info.duration_ms.unwrap_or(0) > 0);
        assert!(source.bytes_read < total_len);
        assert!(source.bytes_read <= (MAX_MEDIA_PROBE_BYTES as u64) + 32);
    }

    #[test]
    fn mp4_inspection_skips_large_mdat_payload() {
        let mdat_payload = 2 * 1024 * 1024usize;
        let moov_offset = 16 + 8 + mdat_payload;
        let total_len = moov_offset + 8;
        let mut bytes = vec![0u8; total_len];
        bytes[0..4].copy_from_slice(&16u32.to_be_bytes());
        bytes[4..8].copy_from_slice(b"ftyp");
        bytes[8..12].copy_from_slice(b"isom");
        bytes[12..16].copy_from_slice(&0u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&((mdat_payload as u32) + 8).to_be_bytes());
        bytes[20..24].copy_from_slice(b"mdat");
        bytes[moov_offset..moov_offset + 4].copy_from_slice(&8u32.to_be_bytes());
        bytes[moov_offset + 4..moov_offset + 8].copy_from_slice(b"moov");

        let mut source = CountingSource { bytes, bytes_read: 0 };
        let info = inspect_source(&mut source).unwrap();
        assert_eq!(info.kind, MediaKind::Mp4);
        assert!(source.bytes_read < 128);
    }
}
