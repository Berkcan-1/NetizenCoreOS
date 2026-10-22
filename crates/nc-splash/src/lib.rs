#![no_std]

use nc_assets::{logo, Logo};
use nc_font::{text_width, Style};
use nc_gfx::{lerp_color, rgb, Canvas, Rect};
use nc_math::{ratio256, sin_milli, smoothstep256};
use nc_mem::Arena;
use nc_ui::{draw_dot, draw_text_color, text_style, ACCENT_DARK, ACCENT_LIGHT};

const INTRO_MS: u32 = 420;
const OUTRO_MS: u32 = 320;
const DOT_PERIOD_MS: u32 = 1050;
const MAX_SUBTITLE_BYTES: usize = 128;
const SILHOUETTE_FLOOR: u8 = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Intro,
    Idle,
    Outro,
    Done,
}

pub struct SplashOptions<'a> {
    /// Optional text shown below the loading animation (for example, setup status).
    pub subtitle: Option<&'a str>,
}

pub struct Update {
    pub rects: [Rect; 2],
    pub count: usize,
}

impl Update {
    fn none() -> Update {
        Update {
            rects: [Rect::EMPTY; 2],
            count: 0,
        }
    }

    fn one(rect: Rect) -> Update {
        Update {
            rects: [rect, Rect::EMPTY],
            count: 1,
        }
    }

    fn two(first: Rect, second: Rect) -> Update {
        if first.width <= 0 || first.height <= 0 {
            return Update::one(second);
        }
        if second.width <= 0 || second.height <= 0 {
            return Update::one(first);
        }
        Update {
            rects: [first, second],
            count: 2,
        }
    }
}

struct Layout {
    screen: Rect,
    width: i32,
    height: i32,
    center_x: i32,
    logo_x: i32,
    logo_y: i32,
    logo_rect: Rect,
    footer_rect: Rect,
    subtitle_style: Style,
    subtitle_width: i32,
    subtitle_y: i32,
    has_subtitle: bool,
    dot_y: i32,
    dot_radius: i32,
    dot_gap: i32,
    dot_amplitude: i32,
}

fn clipped_rect(x: i32, y: i32, width: i32, height: i32, screen_w: i32, screen_h: i32) -> Rect {
    let left = x.max(0);
    let top = y.max(0);
    let right = (x + width).min(screen_w);
    let bottom = (y + height).min(screen_h);
    if right <= left || bottom <= top {
        Rect::EMPTY
    } else {
        Rect::new(left, top, right - left, bottom - top)
    }
}

fn compute_layout(
    width: i32,
    height: i32,
    logo_width: i32,
    logo_height: i32,
    subtitle: Option<&str>,
) -> Layout {
    let width = width.max(1);
    let height = height.max(1);
    let center_x = width / 2;
    let logo_x = center_x - logo_width / 2;
    let logo_y = height / 2 - logo_height / 2;
    let logo_rect = clipped_rect(logo_x, logo_y, logo_width, logo_height, width, height);

    let has_subtitle = subtitle.map(|text| !text.is_empty()).unwrap_or(false);
    let subtitle_size = (height / 45).clamp(12, 18);
    let subtitle_style = text_style(subtitle_size, 70);
    let subtitle_width = subtitle
        .filter(|text| !text.is_empty())
        .map(|text| text_width(text, &subtitle_style))
        .unwrap_or(0);
    let subtitle_y = if has_subtitle {
        height - subtitle_size - 8
    } else {
        height
    };

    let dot_radius = (height / 150).clamp(3, 6);
    let dot_gap = dot_radius * 4;
    let dot_amplitude = dot_radius * 2;
    let dot_y = if has_subtitle {
        subtitle_y - dot_radius * 3 - 4
    } else {
        height - dot_radius * 4 - 12
    };
    let dots_width = dot_gap * 2 + dot_radius * 2 + 8;
    let footer_width = dots_width.max(subtitle_width + 8).min(width).max(1);
    let footer_top = (dot_y - dot_amplitude - dot_radius - 4).clamp(0, height - 1);
    let footer_rect = Rect::new(
        (width - footer_width) / 2,
        footer_top,
        footer_width,
        height - footer_top,
    );

    Layout {
        screen: Rect::new(0, 0, width, height),
        width,
        height,
        center_x,
        logo_x,
        logo_y,
        logo_rect,
        footer_rect,
        subtitle_style,
        subtitle_width,
        subtitle_y,
        has_subtitle,
        dot_y,
        dot_radius,
        dot_gap,
        dot_amplitude,
    }
}

fn new_layer(arena: &mut Arena, width: usize, height: usize) -> Option<Canvas<'static>> {
    let pixels = arena.alloc_pixels(width.checked_mul(height)?.max(1))?;
    Canvas::new(pixels, width.max(1), height.max(1))
}

fn is_silhouette(logo: &Logo) -> bool {
    logo.pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .all(|pixel| {
            pixel[0] >= SILHOUETTE_FLOOR
                && pixel[1] >= SILHOUETTE_FLOOR
                && pixel[2] >= SILHOUETTE_FLOOR
        })
}

/// Draw the supplied logo in normal foreground colors for a black background.
fn draw_logo(canvas: &mut Canvas<'_>, x: i32, y: i32, logo: &Logo, opacity: u32) {
    let width = logo.width as usize;
    let height = logo.height as usize;
    let tinted = is_silhouette(logo);

    for row in 0..height {
        let tint = lerp_color(
            ACCENT_LIGHT,
            ACCENT_DARK,
            (row * 255 / height.max(1)) as u32,
        );
        for column in 0..width {
            let index = (row * width + column) * 4;
            let source_alpha = logo.pixels[index + 3] as u32;
            if source_alpha == 0 || opacity == 0 {
                continue;
            }
            let color = if tinted {
                tint
            } else {
                rgb(
                    logo.pixels[index],
                    logo.pixels[index + 1],
                    logo.pixels[index + 2],
                )
            };
            let alpha = (source_alpha * opacity / 256).min(255) as u8;
            if alpha > 0 {
                canvas.blend_pixel(x + column as i32, y + row as i32, color, alpha);
            }
        }
    }
}

fn clear_rect_black(canvas: &mut Canvas<'_>, rect: Rect) {
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = (rect.x + rect.width).min(canvas.width() as i32);
    let bottom = (rect.y + rect.height).min(canvas.height() as i32);
    for y in top..bottom {
        for x in left..right {
            canvas.blend_pixel(x, y, rgb(0, 0, 0), 255);
        }
    }
}

fn copy_subtitle(
    source: Option<&str>,
    output: &mut [u8; MAX_SUBTITLE_BYTES],
) -> usize {
    let Some(text) = source else {
        return 0;
    };
    let mut length = 0;
    for character in text.chars() {
        let mut encoded = [0u8; 4];
        let bytes = character.encode_utf8(&mut encoded).as_bytes();
        if length + bytes.len() > output.len() {
            break;
        }
        output[length..length + bytes.len()].copy_from_slice(bytes);
        length += bytes.len();
    }
    length
}

pub struct Splash {
    layout: Layout,
    base: Canvas<'static>,
    subtitle_bytes: [u8; MAX_SUBTITLE_BYTES],
    subtitle_len: usize,
    started_at: Option<u32>,
    initialized: bool,
    finish_requested: bool,
    finish_at: u32,
    phase: Phase,
}

impl Splash {
    pub fn new(
        canvas: &Canvas<'_>,
        arena: &mut Arena,
        options: SplashOptions<'_>,
    ) -> Option<Splash> {
        let width = canvas.width() as i32;
        let height = canvas.height() as i32;
        let logo = logo();
        let (logo_width, logo_height) = match &logo {
            Some(image) => (image.width as i32, image.height as i32),
            None => (0, 0),
        };

        let mut subtitle_bytes = [0u8; MAX_SUBTITLE_BYTES];
        let subtitle_len = copy_subtitle(options.subtitle, &mut subtitle_bytes);
        let subtitle = core::str::from_utf8(&subtitle_bytes[..subtitle_len]).ok();
        let layout = compute_layout(width, height, logo_width, logo_height, subtitle);

        let mut base = new_layer(arena, width.max(1) as usize, height.max(1) as usize)?;
        base.copy_rect_from(canvas, 0, 0, 0, 0, width, height);

        Some(Splash {
            layout,
            base,
            subtitle_bytes,
            subtitle_len,
            started_at: None,
            initialized: false,
            finish_requested: false,
            finish_at: 0,
            phase: Phase::Intro,
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn request_finish(&mut self) {
        self.finish_requested = true;
    }

    pub fn frame(&mut self, canvas: &mut Canvas<'_>, now: u32) -> Update {
        let start = match self.started_at {
            Some(value) => value,
            None => {
                self.started_at = Some(now);
                now
            }
        };
        let elapsed = now.wrapping_sub(start);

        if self.phase == Phase::Done {
            return Update::none();
        }

        if self.phase == Phase::Intro && elapsed >= INTRO_MS {
            self.phase = Phase::Idle;
        }
        if self.phase == Phase::Idle && self.finish_requested {
            self.phase = Phase::Outro;
            self.finish_at = elapsed;
        }

        let opacity = match self.phase {
            Phase::Intro => smoothstep256(ratio256(elapsed, INTRO_MS)),
            Phase::Idle => 256,
            Phase::Outro => {
                let since = elapsed.wrapping_sub(self.finish_at);
                if since >= OUTRO_MS {
                    canvas.copy_rect_from(
                        &self.base,
                        0,
                        0,
                        0,
                        0,
                        self.layout.width,
                        self.layout.height,
                    );
                    self.phase = Phase::Done;
                    return Update::one(self.layout.screen);
                }
                256 - smoothstep256(ratio256(since, OUTRO_MS))
            }
            Phase::Done => return Update::none(),
        };

        let full_redraw = !self.initialized;
        if full_redraw {
            canvas.fill(rgb(0, 0, 0));
        } else {
            clear_rect_black(canvas, self.layout.logo_rect);
            clear_rect_black(canvas, self.layout.footer_rect);
        }

        if let Some(image) = logo() {
            draw_logo(
                canvas,
                self.layout.logo_x,
                self.layout.logo_y,
                &image,
                opacity,
            );
        }
        self.draw_loading_dots(canvas, elapsed, opacity);
        self.draw_subtitle(canvas, opacity);
        self.initialized = true;

        if full_redraw {
            Update::one(self.layout.screen)
        } else {
            Update::two(self.layout.logo_rect, self.layout.footer_rect)
        }
    }

    fn draw_loading_dots(&self, canvas: &mut Canvas<'_>, elapsed: u32, opacity: u32) {
        let layout = &self.layout;
        let cycle = (elapsed % DOT_PERIOD_MS) as i32 * 360 / DOT_PERIOD_MS as i32;
        for index in 0..3i32 {
            let phase = cycle - index * 120;
            let wave = (1000 + sin_milli(phase)) as u32 / 2;
            let lift = (wave as i32 * layout.dot_amplitude / 1000) as i32;
            let y = layout.dot_y - lift;
            let radius = layout.dot_radius + if wave > 760 { 1 } else { 0 };
            let strength = ((96 + 160 * wave / 1000) * opacity / 256).min(256);
            draw_dot(
                canvas,
                layout.center_x + (index - 1) * layout.dot_gap,
                y,
                radius,
                rgb(255, 255, 255),
                strength,
            );
        }
    }

    fn draw_subtitle(&self, canvas: &mut Canvas<'_>, opacity: u32) {
        if !self.layout.has_subtitle || self.subtitle_len == 0 || opacity == 0 {
            return;
        }
        let text = match core::str::from_utf8(&self.subtitle_bytes[..self.subtitle_len]) {
            Ok(value) => value,
            Err(_) => return,
        };
        let shade = (255 * opacity / 256).min(255) as u8;
        let color = rgb(shade, shade, shade);
        draw_text_color(
            canvas,
            text,
            self.layout.center_x - self.layout.subtitle_width / 2,
            self.layout.subtitle_y,
            &self.layout.subtitle_style,
            color,
        );
    }
}
