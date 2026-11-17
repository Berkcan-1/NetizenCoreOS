const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73,
    80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494,
    544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499,
    2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

pub const HEADER_BYTES: usize = 24;

fn decode_nibble(predictor: &mut i32, index: &mut i32, nibble: u8) -> i32 {
    let step = STEP_TABLE[*index as usize];
    let mut diff = step >> 3;
    if nibble & 4 != 0 {
        diff += step;
    }
    if nibble & 2 != 0 {
        diff += step >> 1;
    }
    if nibble & 1 != 0 {
        diff += step >> 2;
    }
    if nibble & 8 != 0 {
        *predictor -= diff;
    } else {
        *predictor += diff;
    }
    *predictor = (*predictor).clamp(-32768, 32767);
    *index = (*index + INDEX_TABLE[nibble as usize]).clamp(0, 88);
    *predictor
}

#[derive(Clone, Copy)]
pub struct AdpcmReader {
    data: &'static [u8],
    cursor: usize,
    frames_left: u32,
    rate: u32,
    predictor: [i32; 2],
    index: [i32; 2],
}

impl AdpcmReader {
    pub fn new(blob: &'static [u8]) -> Option<Self> {
        if blob.len() < HEADER_BYTES || &blob[0..4] != b"NCSN" || blob[4] != 1 || blob[5] != 2 {
            return None;
        }
        let rate = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
        let frames = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        if !(8_000..=96_000).contains(&rate) || frames == 0 {
            return None;
        }
        let payload = blob.len() - HEADER_BYTES;
        if (frames as usize) > payload {
            return None;
        }
        let predictor_l = i16::from_le_bytes([blob[16], blob[17]]) as i32;
        let index_l = (blob[18] as i32).min(88);
        let predictor_r = i16::from_le_bytes([blob[20], blob[21]]) as i32;
        let index_r = (blob[22] as i32).min(88);
        Some(Self {
            data: blob,
            cursor: HEADER_BYTES,
            frames_left: frames,
            rate,
            predictor: [predictor_l, predictor_r],
            index: [index_l, index_r],
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn next_frame(&mut self) -> Option<(i32, i32)> {
        if self.frames_left == 0 {
            return None;
        }
        let byte = *self.data.get(self.cursor)?;
        self.cursor += 1;
        self.frames_left -= 1;
        let left = decode_nibble(&mut self.predictor[0], &mut self.index[0], byte & 0x0F);
        let right = decode_nibble(&mut self.predictor[1], &mut self.index[1], byte >> 4);
        Some((left, right))
    }
}
