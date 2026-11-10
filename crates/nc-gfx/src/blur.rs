use crate::Canvas;

fn pack(red: u32, green: u32, blue: u32, reciprocal: u32) -> u32 {
    (((red * reciprocal) >> 16) << 16) | (((green * reciprocal) >> 16) << 8) | ((blue * reciprocal) >> 16)
}

fn horizontal(source: &[u32], target: &mut [u32], width: usize, height: usize, radius: usize) {
    let window = (radius * 2 + 1) as u32;
    let reciprocal = (65536 + window - 1) / window;
    for y in 0..height {
        let row = y * width;
        let mut red = 0u32;
        let mut green = 0u32;
        let mut blue = 0u32;
        for x in 0..=radius.min(width - 1) {
            let pixel = source[row + x];
            red += (pixel >> 16) & 0xFF;
            green += (pixel >> 8) & 0xFF;
            blue += pixel & 0xFF;
        }
        for x in 0..width {
            target[row + x] = pack(red, green, blue, reciprocal);
            let incoming = x + radius + 1;
            if incoming < width {
                let pixel = source[row + incoming];
                red += (pixel >> 16) & 0xFF;
                green += (pixel >> 8) & 0xFF;
                blue += pixel & 0xFF;
            }
            if x >= radius {
                let pixel = source[row + x - radius];
                red -= (pixel >> 16) & 0xFF;
                green -= (pixel >> 8) & 0xFF;
                blue -= pixel & 0xFF;
            }
        }
    }
}

fn vertical(source: &[u32], target: &mut [u32], width: usize, height: usize, radius: usize) {
    let window = (radius * 2 + 1) as u32;
    let reciprocal = (65536 + window - 1) / window;
    for x in 0..width {
        let mut red = 0u32;
        let mut green = 0u32;
        let mut blue = 0u32;
        for y in 0..=radius.min(height - 1) {
            let pixel = source[y * width + x];
            red += (pixel >> 16) & 0xFF;
            green += (pixel >> 8) & 0xFF;
            blue += pixel & 0xFF;
        }
        for y in 0..height {
            target[y * width + x] = pack(red, green, blue, reciprocal);
            let incoming = y + radius + 1;
            if incoming < height {
                let pixel = source[incoming * width + x];
                red += (pixel >> 16) & 0xFF;
                green += (pixel >> 8) & 0xFF;
                blue += pixel & 0xFF;
            }
            if y >= radius {
                let pixel = source[(y - radius) * width + x];
                red -= (pixel >> 16) & 0xFF;
                green -= (pixel >> 8) & 0xFF;
                blue -= pixel & 0xFF;
            }
        }
    }
}

pub fn box_blur(source: &Canvas<'_>, target: &mut Canvas<'_>, temp: &mut Canvas<'_>, radius: usize) {
    let width = source.width();
    let height = source.height();
    if radius == 0 {
        target.pixels_mut().copy_from_slice(source.pixels());
        return;
    }
    horizontal(source.pixels(), temp.pixels_mut(), width, height, radius);
    vertical(temp.pixels(), target.pixels_mut(), width, height, radius);
    horizontal(target.pixels(), temp.pixels_mut(), width, height, radius);
    vertical(temp.pixels(), target.pixels_mut(), width, height, radius);
}
