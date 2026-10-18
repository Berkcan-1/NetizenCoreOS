#![no_std]

use nc_math::{cos_milli, integer_sqrt, sin_milli};

const MAX_SEGMENTS: usize = 64;
const SPACE_WIDTH: i32 = 340;

#[derive(Clone, Copy)]
pub struct Style {
    pub cap_height: i32,
    pub stroke_sixteenths: i32,
    pub tracking: i32,
}

#[derive(Clone, Copy)]
struct Segment {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

const EMPTY: Segment = Segment { x0: 0, y0: 0, x1: 0, y1: 0 };

struct Path {
    segments: [Segment; MAX_SEGMENTS],
    count: usize,
    width: i32,
}

fn arc_point(center_x: i32, center_y: i32, radius_x: i32, radius_y: i32, degrees: i32) -> (i32, i32) {
    (
        center_x + radius_x * cos_milli(degrees) / 1000,
        center_y - radius_y * sin_milli(degrees) / 1000,
    )
}

impl Path {
    fn new(width: i32) -> Path {
        Path { segments: [EMPTY; MAX_SEGMENTS], count: 0, width }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        if self.count < MAX_SEGMENTS {
            self.segments[self.count] = Segment { x0, y0, x1, y1 };
            self.count += 1;
        }
    }

    fn arc(&mut self, center_x: i32, center_y: i32, radius_x: i32, radius_y: i32, start: i32, end: i32, step: i32) {
        let steps = (end - start) / step;
        let mut previous = arc_point(center_x, center_y, radius_x, radius_y, start);
        for index in 1..=steps {
            let current = arc_point(center_x, center_y, radius_x, radius_y, start + index * step);
            self.line(previous.0, previous.1, current.0, current.1);
            previous = current;
        }
    }
}

// crates/nc-font/src/lib.rs içinde normalize_char fonksiyonunu bununla değiştir:
fn normalize_char(c: char) -> char {
    match c {
        '\u{00E0}' | '\u{00E1}' | '\u{00E2}' | '\u{00E4}' => 'a',
        '\u{00C0}' | '\u{00C1}' | '\u{00C2}' | '\u{00C4}' => 'A',
        '\u{00E8}' | '\u{00E9}' | '\u{00EA}' | '\u{00EB}' => 'e',
        '\u{00C8}' | '\u{00C9}' | '\u{00CA}' | '\u{00CB}' => 'E',
        '\u{00EC}' | '\u{00ED}' | '\u{00EE}' | '\u{00EF}' => 'i',
        '\u{00CC}' | '\u{00CD}' | '\u{00CE}' | '\u{00CF}' => 'I',
        '\u{00F2}' | '\u{00F3}' | '\u{00F4}' => 'o',
        '\u{00D2}' | '\u{00D3}' | '\u{00D4}' => 'O',
        '\u{00F9}' | '\u{00FA}' | '\u{00FB}' => 'u',
        '\u{00D9}' | '\u{00DA}' | '\u{00DB}' => 'U',
        _ => c,
    }
}

fn glyph(character: char) -> Path {
    match normalize_char(character) {
        'A' => {
            let mut path = Path::new(680);
            path.line(0, 1000, 340, 0);
            path.line(340, 0, 680, 1000);
            path.line(126, 650, 554, 650);
            path
        }
        'B' => {
            let mut path = Path::new(580);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 310, 0);
            path.arc(310, 245, 245, 245, 90, -90, -15);
            path.line(310, 490, 0, 490);
            path.line(0, 490, 330, 490);
            path.arc(330, 745, 250, 255, 90, -90, -15);
            path.line(330, 1000, 0, 1000);
            path
        }
        'C' => {
            let mut path = Path::new(720);
            path.arc(400, 500, 380, 500, 45, 315, 15);
            path
        }
        'Ç' => {
            let mut path = Path::new(720);
            path.arc(400, 500, 380, 500, 45, 315, 15);
            path.line(390, 1000, 340, 1200);
            path
        }
        'D' => {
            let mut path = Path::new(680);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 310, 0);
            path.arc(310, 500, 360, 500, 90, -90, -15);
            path.line(310, 1000, 0, 1000);
            path
        }
        'E' => {
            let mut path = Path::new(540);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 540, 0);
            path.line(0, 490, 450, 490);
            path.line(0, 1000, 540, 1000);
            path
        }
        'F' => {
            let mut path = Path::new(510);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 510, 0);
            path.line(0, 480, 430, 480);
            path
        }
        'G' => {
            let mut path = Path::new(780);
            path.arc(410, 500, 390, 500, 45, 360, 15);
            path.line(800, 500, 450, 500);
            path.line(800, 500, 800, 960);
            path
        }
        'Ğ' => {
            let mut path = Path::new(780);
            path.arc(410, 500, 390, 500, 45, 360, 15);
            path.line(800, 500, 450, 500);
            path.line(800, 500, 800, 960);
            path.arc(410, -150, 150, 90, 180, 360, 30);
            path
        }
        'H' => {
            let mut path = Path::new(660);
            path.line(0, 0, 0, 1000);
            path.line(660, 0, 660, 1000);
            path.line(0, 500, 660, 500);
            path
        }
        'I' => {
            let mut path = Path::new(160);
            path.line(80, 0, 80, 1000);
            path.line(10, 0, 150, 0);
            path.line(10, 1000, 150, 1000);
            path
        }
        'İ' => {
            let mut path = Path::new(160);
            path.line(80, 0, 80, 1000);
            path.line(80, -190, 80, -130);
            path
        }
        'J' => {
            let mut path = Path::new(440);
            path.line(420, 0, 420, 680);
            path.arc(210, 680, 210, 320, 0, -165, -15);
            path
        }
        'K' => {
            let mut path = Path::new(620);
            path.line(0, 0, 0, 1000);
            path.line(0, 560, 600, 0);
            path.line(190, 390, 620, 1000);
            path
        }
        'L' => {
            let mut path = Path::new(510);
            path.line(0, 0, 0, 1000);
            path.line(0, 1000, 510, 1000);
            path
        }
        'M' => {
            let mut path = Path::new(820);
            path.line(0, 1000, 0, 0);
            path.line(0, 0, 410, 640);
            path.line(410, 640, 820, 0);
            path.line(820, 0, 820, 1000);
            path
        }
        'N' => {
            let mut path = Path::new(680);
            path.line(0, 1000, 0, 0);
            path.line(0, 0, 680, 1000);
            path.line(680, 1000, 680, 0);
            path
        }
        'O' => {
            let mut path = Path::new(780);
            path.arc(390, 500, 380, 500, 90, 450, 15);
            path
        }
        'Ö' => {
            let mut path = Path::new(780);
            path.arc(390, 500, 380, 500, 90, 450, 15);
            path.line(260, -170, 260, -120);
            path.line(520, -170, 520, -120);
            path
        }
        'P' => {
            let mut path = Path::new(580);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 320, 0);
            path.arc(320, 265, 255, 265, 90, -90, -15);
            path.line(320, 530, 0, 530);
            path
        }
        'Q' => {
            let mut path = Path::new(780);
            path.arc(390, 500, 380, 500, 90, 450, 15);
            path.line(490, 730, 790, 1040);
            path
        }
        'R' => {
            let mut path = Path::new(600);
            path.line(0, 0, 0, 1000);
            path.line(0, 0, 315, 0);
            path.arc(315, 260, 255, 260, 90, -90, -15);
            path.line(315, 520, 0, 520);
            path.line(295, 520, 600, 1000);
            path
        }
        'S' => {
            let mut path = Path::new(580);
            path.arc(290, 250, 270, 250, 30, 270, 15);
            path.arc(290, 750, 270, 250, 90, -150, -15);
            path
        }
        'Ş' => {
            let mut path = Path::new(580);
            path.arc(290, 250, 270, 250, 30, 270, 15);
            path.arc(290, 750, 270, 250, 90, -150, -15);
            path.line(290, 1000, 250, 1200);
            path
        }
        'T' => {
            let mut path = Path::new(600);
            path.line(0, 0, 600, 0);
            path.line(300, 0, 300, 1000);
            path
        }
        'U' => {
            let mut path = Path::new(660);
            path.line(0, 0, 0, 660);
            path.arc(330, 660, 330, 340, 180, 360, 15);
            path.line(660, 660, 660, 0);
            path
        }
        'Ü' => {
            let mut path = Path::new(660);
            path.line(0, 0, 0, 660);
            path.arc(330, 660, 330, 340, 180, 360, 15);
            path.line(660, 660, 660, 0);
            path.line(200, -170, 200, -120);
            path.line(460, -170, 460, -120);
            path
        }
        'V' => {
            let mut path = Path::new(660);
            path.line(0, 0, 330, 1000);
            path.line(330, 1000, 660, 0);
            path
        }
        'W' => {
            let mut path = Path::new(960);
            path.line(0, 0, 240, 1000);
            path.line(240, 1000, 480, 180);
            path.line(480, 180, 720, 1000);
            path.line(720, 1000, 960, 0);
            path
        }
        'X' => {
            let mut path = Path::new(640);
            path.line(0, 0, 640, 1000);
            path.line(640, 0, 0, 1000);
            path
        }
        'Y' => {
            let mut path = Path::new(640);
            path.line(0, 0, 320, 520);
            path.line(640, 0, 320, 520);
            path.line(320, 520, 320, 1000);
            path
        }
        'Z' => {
            let mut path = Path::new(580);
            path.line(0, 0, 580, 0);
            path.line(580, 0, 0, 1000);
            path.line(0, 1000, 580, 1000);
            path
        }
        'a' => {
            let mut path = Path::new(520);
            path.arc(250, 470, 230, 175, 150, 0, -15);
            path.line(480, 470, 480, 1000);
            path.line(480, 610, 230, 630);
            path.arc(240, 815, 230, 185, 90, 345, 15);
            path
        }
        'b' => {
            let mut path = Path::new(530);
            path.line(20, 0, 20, 1000);
            path.arc(265, 650, 255, 350, 150, -150, -15);
            path
        }
        'c' => {
            let mut path = Path::new(480);
            path.arc(265, 650, 245, 350, 45, 315, 15);
            path
        }
        'ç' => {
            let mut path = Path::new(480);
            path.arc(265, 650, 245, 350, 45, 315, 15);
            path.line(255, 1000, 215, 1210);
            path
        }
        'd' => {
            let mut path = Path::new(530);
            path.line(510, 0, 510, 1000);
            path.arc(265, 650, 255, 350, 30, 330, 15);
            path
        }
        'e' => {
            let mut path = Path::new(520);
            path.line(15, 640, 505, 640);
            path.arc(260, 650, 245, 350, 0, 315, 15);
            path
        }
        'f' => {
            let mut path = Path::new(340);
            path.arc(245, 160, 130, 160, 30, 180, 15);
            path.line(115, 160, 115, 1000);
            path.line(0, 330, 310, 330);
            path
        }
        'g' => {
            let mut path = Path::new(530);
            path.arc(260, 645, 245, 345, 30, 330, 15);
            path.line(505, 300, 505, 1010);
            path.arc(255, 1010, 250, 240, 0, -150, -15);
            path
        }
        'ğ' => {
            let mut path = Path::new(530);
            path.arc(260, 645, 245, 345, 30, 330, 15);
            path.line(505, 300, 505, 1010);
            path.arc(255, 1010, 250, 240, 0, -150, -15);
            path.arc(260, 110, 130, 75, 180, 360, 30);
            path
        }
        'h' => {
            let mut path = Path::new(510);
            path.line(15, 0, 15, 1000);
            path.arc(255, 535, 240, 235, 165, 0, -15);
            path.line(495, 535, 495, 1000);
            path
        }
        'i' => {
            let mut path = Path::new(150);
            path.line(75, 300, 75, 1000);
            path.line(75, 65, 75, 125);
            path
        }
        'ı' => {
            let mut path = Path::new(150);
            path.line(75, 300, 75, 1000);
            path
        }
        'j' => {
            let mut path = Path::new(220);
            path.line(170, 65, 170, 125);
            path.line(170, 300, 170, 1030);
            path.arc(65, 1030, 105, 210, 0, -150, -15);
            path
        }
        'k' => {
            let mut path = Path::new(480);
            path.line(15, 0, 15, 1000);
            path.line(15, 680, 450, 300);
            path.line(165, 550, 480, 1000);
            path
        }
        'l' => {
            let mut path = Path::new(160);
            path.line(65, 0, 65, 890);
            path.arc(135, 890, 70, 110, 180, 285, 15);
            path
        }
        'm' => {
            let mut path = Path::new(780);
            path.line(10, 300, 10, 1000);
            path.arc(200, 520, 190, 220, 165, 0, -15);
            path.line(390, 520, 390, 1000);
            path.arc(580, 520, 190, 220, 165, 0, -15);
            path.line(770, 520, 770, 1000);
            path
        }
        'n' => {
            let mut path = Path::new(510);
            path.line(15, 300, 15, 1000);
            path.arc(255, 535, 240, 235, 165, 0, -15);
            path.line(495, 535, 495, 1000);
            path
        }
        'o' => {
            let mut path = Path::new(540);
            path.arc(270, 650, 255, 350, 90, 450, 15);
            path
        }
        'ö' => {
            let mut path = Path::new(540);
            path.arc(270, 650, 255, 350, 90, 450, 15);
            path.line(175, 100, 175, 150);
            path.line(365, 100, 365, 150);
            path
        }
        'p' => {
            let mut path = Path::new(530);
            path.line(20, 300, 20, 1250);
            path.arc(265, 650, 255, 350, 150, -150, -15);
            path
        }
        'q' => {
            let mut path = Path::new(530);
            path.line(510, 300, 510, 1250);
            path.arc(265, 650, 255, 350, 30, 330, 15);
            path
        }
        'r' => {
            let mut path = Path::new(340);
            path.line(15, 300, 15, 1000);
            path.arc(215, 520, 200, 220, 165, 30, -15);
            path
        }
        's' => {
            let mut path = Path::new(450);
            path.arc(225, 475, 205, 175, 30, 270, 15);
            path.arc(225, 825, 205, 175, 90, -150, -15);
            path
        }
        'ş' => {
            let mut path = Path::new(450);
            path.arc(225, 475, 205, 175, 30, 270, 15);
            path.arc(225, 825, 205, 175, 90, -150, -15);
            path.line(225, 1000, 185, 1210);
            path
        }
        't' => {
            let mut path = Path::new(330);
            path.line(130, 110, 130, 860);
            path.arc(235, 860, 105, 140, 180, 300, 15);
            path.line(10, 320, 310, 320);
            path
        }
        'u' => {
            let mut path = Path::new(510);
            path.line(15, 300, 15, 765);
            path.arc(255, 765, 240, 235, 180, 360, 15);
            path.line(495, 300, 495, 1000);
            path
        }
        'ü' => {
            let mut path = Path::new(510);
            path.line(15, 300, 15, 765);
            path.arc(255, 765, 240, 235, 180, 360, 15);
            path.line(495, 300, 495, 1000);
            path.line(160, 100, 160, 150);
            path.line(350, 100, 350, 150);
            path
        }
        'v' => {
            let mut path = Path::new(480);
            path.line(0, 300, 240, 1000);
            path.line(240, 1000, 480, 300);
            path
        }
        'w' => {
            let mut path = Path::new(740);
            path.line(0, 300, 185, 1000);
            path.line(185, 1000, 370, 450);
            path.line(370, 450, 555, 1000);
            path.line(555, 1000, 740, 300);
            path
        }
        'x' => {
            let mut path = Path::new(460);
            path.line(10, 300, 450, 1000);
            path.line(450, 300, 10, 1000);
            path
        }
        'y' => {
            let mut path = Path::new(480);
            path.line(10, 300, 245, 1000);
            path.line(470, 300, 130, 1250);
            path
        }
        'z' => {
            let mut path = Path::new(450);
            path.line(10, 300, 440, 300);
            path.line(440, 300, 10, 1000);
            path.line(10, 1000, 440, 1000);
            path
        }
        '0' => {
            let mut path = Path::new(600);
            path.arc(300, 500, 290, 500, 90, 450, 15);
            path.line(140, 820, 460, 180);
            path
        }
        '1' => {
            let mut path = Path::new(340);
            path.line(40, 210, 200, 0);
            path.line(200, 0, 200, 1000);
            path.line(40, 1000, 340, 1000);
            path
        }
        '2' => {
            let mut path = Path::new(580);
            path.arc(290, 290, 275, 290, 165, -30, -15);
            path.line(528, 435, 0, 1000);
            path.line(0, 1000, 580, 1000);
            path
        }
        '3' => {
            let mut path = Path::new(560);
            path.arc(275, 250, 255, 250, 150, -90, -15);
            path.arc(275, 750, 275, 250, 90, -150, -15);
            path
        }
        '4' => {
            let mut path = Path::new(600);
            path.line(420, 1000, 420, 0);
            path.line(420, 0, 0, 680);
            path.line(0, 680, 600, 680);
            path
        }
        '5' => {
            let mut path = Path::new(570);
            path.line(530, 0, 50, 0);
            path.line(50, 0, 20, 460);
            path.line(20, 460, 104, 496);
            path.arc(285, 720, 280, 280, 135, -135, -15);
            path
        }
        '6' => {
            let mut path = Path::new(590);
            path.arc(295, 700, 285, 300, 90, 450, 15);
            path.arc(600, 700, 590, 700, 180, 105, -15);
            path
        }
        '7' => {
            let mut path = Path::new(580);
            path.line(0, 0, 580, 0);
            path.line(580, 0, 190, 1000);
            path
        }
        '8' => {
            let mut path = Path::new(590);
            path.arc(295, 255, 240, 255, 90, 450, 15);
            path.arc(295, 745, 285, 255, 90, 450, 15);
            path
        }
        '9' => {
            let mut path = Path::new(590);
            path.arc(295, 300, 285, 300, 90, 450, 15);
            path.arc(-10, 300, 590, 700, 0, -75, -15);
            path
        }
        '.' => {
            let mut path = Path::new(140);
            path.line(70, 920, 70, 1000);
            path
        }
        ',' => {
            let mut path = Path::new(140);
            path.line(80, 920, 80, 1000);
            path.line(80, 1000, 30, 1150);
            path
        }
        ';' => {
            let mut path = Path::new(140);
            path.line(80, 420, 80, 480);
            path.line(80, 920, 80, 1000);
            path.line(80, 1000, 30, 1150);
            path
        }
        ':' => {
            let mut path = Path::new(140);
            path.line(70, 400, 70, 460);
            path.line(70, 920, 70, 1000);
            path
        }
        '-' => {
            let mut path = Path::new(380);
            path.line(20, 530, 360, 530);
            path
        }
        '_' => {
            let mut path = Path::new(460);
            path.line(0, 1040, 460, 1040);
            path
        }
        '+' => {
            let mut path = Path::new(460);
            path.line(20, 520, 440, 520);
            path.line(230, 300, 230, 740);
            path
        }
        '=' => {
            let mut path = Path::new(460);
            path.line(20, 390, 440, 390);
            path.line(20, 650, 440, 650);
            path
        }
        '/' => {
            let mut path = Path::new(380);
            path.line(20, 1020, 360, -20);
            path
        }
        '\\' => {
            let mut path = Path::new(380);
            path.line(20, -20, 360, 1020);
            path
        }
        '?' => {
            let mut path = Path::new(500);
            path.arc(250, 250, 230, 250, 160, -60, -20);
            path.line(250, 500, 250, 720);
            path.line(250, 940, 250, 1000);
            path
        }
        '%' => {
            let mut path = Path::new(640);
            path.line(80, 1000, 560, 0);
            path.arc(160, 220, 130, 180, 90, 450, 30);
            path.arc(480, 780, 130, 180, 90, 450, 30);
            path
        }
        '&' => {
            let mut path = Path::new(620);
            path.arc(280, 240, 200, 220, 0, 360, 30);
            path.arc(280, 720, 260, 280, 90, 330, 20);
            path.line(120, 360, 620, 1000);
            path
        }
        '(' => {
            let mut path = Path::new(260);
            path.arc(240, 500, 180, 560, 105, 255, 15);
            path
        }
        ')' => {
            let mut path = Path::new(260);
            path.arc(20, 500, 180, 560, 75, -75, -15);
            path
        }
        '[' | '{' => {
            let mut path = Path::new(260);
            path.line(220, -20, 70, -20);
            path.line(70, -20, 70, 1040);
            path.line(70, 1040, 220, 1040);
            path
        }
        ']' | '}' => {
            let mut path = Path::new(260);
            path.line(40, -20, 190, -20);
            path.line(190, -20, 190, 1040);
            path.line(190, 1040, 40, 1040);
            path
        }
        '<' => {
            let mut path = Path::new(440);
            path.line(390, 220, 50, 520);
            path.line(50, 520, 390, 820);
            path
        }
        '>' => {
            let mut path = Path::new(440);
            path.line(50, 220, 390, 520);
            path.line(390, 520, 50, 820);
            path
        }
        '*' | '#' | '@' => {
            let mut path = Path::new(460);
            path.line(60, 250, 400, 750);
            path.line(400, 250, 60, 750);
            path.line(10, 500, 450, 500);
            path
        }
        '\'' => {
            let mut path = Path::new(130);
            path.line(65, 0, 55, 270);
            path
        }
        '"' => {
            let mut path = Path::new(240);
            path.line(65, 0, 55, 270);
            path.line(175, 0, 165, 270);
            path
        }
        '!' => {
            let mut path = Path::new(140);
            path.line(70, 0, 70, 680);
            path.line(70, 920, 70, 1000);
            path
        }
        '|' => {
            let mut path = Path::new(140);
            path.line(70, -40, 70, 1080);
            path
        }
        _ => Path::new(SPACE_WIDTH),
    }
}

fn scale(unit: i32, cap_height: i32) -> i32 {
    unit * cap_height * 256 / 1000
}

fn glyph_width_pixels(path: &Path, cap_height: i32) -> i32 {
    let raw = (path.width * cap_height + 500) / 1000;
    raw + (cap_height / 7).max(1)
}

fn distance_squared(px: i64, py: i64, segment: &Segment) -> i64 {
    let ax = segment.x0 as i64;
    let ay = segment.y0 as i64;
    let dx = segment.x1 as i64 - ax;
    let dy = segment.y1 as i64 - ay;
    let length_squared = dx * dx + dy * dy;
    let (qx, qy) = if length_squared == 0 {
        (ax, ay)
    } else {
        let t = (px - ax) * dx + (py - ay) * dy;
        if t <= 0 {
            (ax, ay)
        } else if t >= length_squared {
            (segment.x1 as i64, segment.y1 as i64)
        } else {
            (ax + dx * t / length_squared, ay + dy * t / length_squared)
        }
    };
    let ex = px - qx;
    let ey = py - qy;
    ex * ex + ey * ey
}

fn draw_path(path: &Path, origin_x: i32, origin_y: i32, style: &Style, plot: &mut dyn FnMut(i32, i32, u8)) {
    if path.count == 0 {
        return;
    }
    let cap = style.cap_height;
    let half = style.stroke_sixteenths * 8;
    let mut scaled = [EMPTY; MAX_SEGMENTS];
    let mut left = i32::MAX;
    let mut right = i32::MIN;
    let mut top = i32::MAX;
    let mut bottom = i32::MIN;
    for index in 0..path.count {
        let segment = path.segments[index];
        let moved = Segment {
            x0: origin_x * 256 + scale(segment.x0, cap),
            y0: origin_y * 256 + scale(segment.y0, cap),
            x1: origin_x * 256 + scale(segment.x1, cap),
            y1: origin_y * 256 + scale(segment.y1, cap),
        };
        left = left.min(moved.x0).min(moved.x1);
        right = right.max(moved.x0).max(moved.x1);
        top = top.min(moved.y0).min(moved.y1);
        bottom = bottom.max(moved.y0).max(moved.y1);
        scaled[index] = moved;
    }
    let min_x = (left - half).div_euclid(256) - 2;
    let max_x = (right + half).div_euclid(256) + 2;
    let min_y = (top - half).div_euclid(256) - 2;
    let max_y = (bottom + half).div_euclid(256) + 2;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let sample_x = px as i64 * 256 + 128;
            let sample_y = py as i64 * 256 + 128;
            let mut best = i64::MAX;
            for index in 0..path.count {
                let distance = distance_squared(sample_x, sample_y, &scaled[index]);
                if distance < best {
                    best = distance;
                }
            }
            let distance = integer_sqrt(best as u64) as i32;
            let coverage = half + 128 - distance;
            if coverage > 0 {
                plot(px, py, coverage.min(255) as u8);
            }
        }
    }
}


fn effective_tracking(style: &Style) -> i32 {
    (style.cap_height * 14 / 100).max(2)
}

pub fn text_width(text: &str, style: &Style) -> i32 {
    let mut total = 0;
    let mut first = true;
    let track = effective_tracking(style);
    for character in text.chars() {
        if !first {
            total += track;
        }
        first = false;
        total += glyph_width_pixels(&glyph(character), style.cap_height);
    }
    total
}

pub fn draw_text(text: &str, x: i32, y: i32, style: &Style, plot: &mut dyn FnMut(i32, i32, u8)) {
    let mut cursor = x;
    let track = effective_tracking(style);
    for character in text.chars() {
        let path = glyph(character);
        draw_path(&path, cursor, y, style, plot);
        cursor += glyph_width_pixels(&path, style.cap_height) + track;
    }
}