#![no_std]

use nc_assets::{logo, Logo};
use nc_font::{text_width, Style};
use nc_gfx::blur::box_blur;
use nc_gfx::compose::compose_ink;
use nc_gfx::{invert, lerp_color, rgb, Canvas, Rect};
use nc_math::{ratio256, sin_milli, smoothstep256};
use nc_mem::Arena;
use nc_ui::{
    draw_dot, draw_text_color, text_style, ACCENT_DARK, ACCENT_LIGHT, ACCENT_LINE, AQUA,
    TEXT_MUTED, TEXT_PRIMARY,
};

const TITLE: &str = "NetizenCore";

const INTRO_MS: u32 = 1600;
const OUTRO_MS: u32 = 800;
const FADE_IN_MS: u32 = 1000;
const RAY_CONVERGE_MS: u32 = 1150;
const RAY_PULSE_PERIOD_MS: u32 = 1800;
const SPINNER_PERIOD_MS: u32 = 1050;
const DOT_PERIOD_MS: u32 = 1400;

const LEVEL_COUNT: usize = 4;
const BLUR_RADII: [usize; LEVEL_COUNT] = [0, 4, 10, 22];
const STAGE_MARGIN: i32 = 64;
const SILHOUETTE_FLOOR: u8 = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Intro,
    Idle,
    Outro,
    Done,
}

pub struct SplashOptions<'a> {
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

    fn one(first: Rect) -> Update {
        Update {
            rects: [first, Rect::EMPTY],
            count: 1,
        }
    }

    fn two(first: Rect, second: Rect) -> Update {
        Update {
            rects: [first, second],
            count: 2,
        }
    }
}

struct Layout {
    stage: Rect,
    center_x: i32,
    logo_x: i32,
    logo_y: i32,
    title_style: Style,
    title_x: i32,
    title_y: i32,
    title_width: i32,
    rays_y: i32,
    rays_half_span: i32,
    spinner_y: i32,
    spinner_radius: i32,
    spinner_dot_radius: i32,
    has_subtitle: bool,
    subtitle_style: Style,
    subtitle_x: i32,
    subtitle_y: i32,
    dots_x: i32,
    dots_y: i32,
    dot_gap: i32,
    dot_radius: i32,
}

fn compute_layout(
    width: i32,
    height: i32,
    logo_width: i32,
    logo_height: i32,
    subtitle: Option<&str>,
) -> Layout {
    let cap = (height / 15).max(20);
    let title_style = text_style(cap, 36);
    let title_width = text_width(TITLE, &title_style);

    let subtitle_cap = (cap * 36 / 100).max(12);
    let subtitle_style = text_style(subtitle_cap, 70);
    let has_subtitle = subtitle.is_some();
    let subtitle_width = match subtitle {
        Some(text) => text_width(text, &subtitle_style),
        None => 0,
    };

    let dot_radius = (subtitle_cap / 3).max(3);
    let dot_gap = subtitle_cap * 12 / 10;
    let spacer = subtitle_cap * 12 / 10;
    let subtitle_total = if has_subtitle {
        subtitle_width + spacer + dot_gap * 2
    } else {
        0
    };

    let spinner_radius = (cap * 52 / 100).clamp(13, 24);
    let spinner_dot_radius = (spinner_radius / 5).clamp(2, 4);

    let gap_logo = if logo_height > 0 { cap * 85 / 100 } else { 0 };
    let title_offset = logo_height + gap_logo;
    let rays_offset = title_offset + cap + cap * 45 / 100;
    let spinner_offset = rays_offset + spinner_radius + cap * 40 / 100;
    let subtitle_offset = spinner_offset + spinner_radius + cap * 65 / 100;

    let block_height = if has_subtitle {
        subtitle_offset + subtitle_cap
    } else {
        spinner_offset + spinner_radius + 8
    };

    let top = ((height - block_height) / 2 - height / 28).max(8);
    let center_x = width / 2;
    let subtitle_x = center_x - subtitle_total / 2;

    let rays_half_span = (title_width * 9 / 10).max(160);
    let widest = logo_width
        .max(title_width)
        .max(subtitle_total)
        .max(rays_half_span * 2);

    let stage_left = (center_x - widest / 2 - STAGE_MARGIN).max(0);
    let stage_right = (center_x + widest / 2 + STAGE_MARGIN).min(width);
    let stage_top = (top - STAGE_MARGIN).max(0);
    let stage_bottom = (top + block_height + STAGE_MARGIN).min(height);

    Layout {
        stage: Rect::new(
            stage_left,
            stage_top,
            stage_right - stage_left,
            stage_bottom - stage_top,
        ),
        center_x,
        logo_x: center_x - logo_width / 2,
        logo_y: top,
        title_style,
        title_x: center_x - title_width / 2,
        title_y: top + title_offset,
        title_width,
        rays_y: top + rays_offset,
        rays_half_span,
        spinner_y: top + spinner_offset,
        spinner_radius,
        spinner_dot_radius,
        has_subtitle,
        subtitle_style,
        subtitle_x,
        subtitle_y: top + subtitle_offset,
        dots_x: subtitle_x + subtitle_width + spacer,
        dots_y: top + subtitle_offset + subtitle_cap * 9 / 10,
        dot_gap,
        dot_radius,
    }
}

fn new_layer(arena: &mut Arena, width: usize, height: usize) -> Option<Canvas<'static>> {
    let pixels = arena.alloc_pixels(width * height)?;
    Canvas::new(pixels, width, height)
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

fn draw_logo_ink(layer: &mut Canvas<'_>, x: i32, y: i32, logo: &Logo) {
    let width = logo.width as usize;
    let height = logo.height as usize;
    let tinted = is_silhouette(logo);

    for row in 0..height {
        let tint = lerp_color(ACCENT_LIGHT, ACCENT_DARK, (row * 255 / height.max(1)) as u32);
        for column in 0..width {
            let index = (row * width + column) * 4;
            let alpha = logo.pixels[index + 3];
            if alpha == 0 {
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
            layer.blend_pixel(x + column as i32, y + row as i32, invert(color), alpha);
        }
    }
}

#[inline]
fn cos_milli(deg: i32) -> i32 {
    sin_milli(deg + 90)
}

pub struct Splash {
    layout: Layout,
    base: Canvas<'static>,
    levels: [Canvas<'static>; LEVEL_COUNT],
    started_at: Option<u32>,
    finish_requested: bool,
    finish_at: u32,
    phase: Phase,
}

impl Splash {
    pub fn new(canvas: &Canvas<'_>, arena: &mut Arena, options: SplashOptions<'_>) -> Option<Splash> {
        let width = canvas.width() as i32;
        let height = canvas.height() as i32;
        let logo = logo();
        let (logo_width, logo_height) = match &logo {
            Some(logo) => (logo.width as i32, logo.height as i32),
            None => (0, 0),
        };
        let layout = compute_layout(width, height, logo_width, logo_height, options.subtitle);

        let stage = layout.stage;
        let stage_width = stage.width as usize;
        let stage_height = stage.height as usize;

        let mut base = new_layer(arena, stage_width, stage_height)?;
        base.copy_rect_from(canvas, stage.x, stage.y, 0, 0, stage.width, stage.height);

        let mut sharp = new_layer(arena, stage_width, stage_height)?;
        sharp.fill(0);
        if let Some(logo) = &logo {
            draw_logo_ink(
                &mut sharp,
                layout.logo_x - stage.x,
                layout.logo_y - stage.y,
                logo,
            );
        }
        draw_text_color(
            &mut sharp,
            TITLE,
            layout.title_x - stage.x,
            layout.title_y - stage.y,
            &layout.title_style,
            invert(TEXT_PRIMARY),
        );
        if let Some(text) = options.subtitle {
            draw_text_color(
                &mut sharp,
                text,
                layout.subtitle_x - stage.x,
                layout.subtitle_y - stage.y,
                &layout.subtitle_style,
                invert(TEXT_MUTED),
            );
        }

        let mut level1 = new_layer(arena, stage_width, stage_height)?;
        let mut level2 = new_layer(arena, stage_width, stage_height)?;
        let mut level3 = new_layer(arena, stage_width, stage_height)?;
        let mut temp = new_layer(arena, stage_width, stage_height)?;
        box_blur(&sharp, &mut level1, &mut temp, BLUR_RADII[1]);
        box_blur(&sharp, &mut level2, &mut temp, BLUR_RADII[2]);
        box_blur(&sharp, &mut level3, &mut temp, BLUR_RADII[3]);

        Some(Splash {
            layout,
            base,
            levels: [sharp, level1, level2, level3],
            started_at: None,
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

        if self.phase == Phase::Idle && self.finish_requested {
            self.phase = Phase::Outro;
            self.finish_at = elapsed;
        }

        match self.phase {
            Phase::Intro => self.frame_intro(canvas, elapsed),
            Phase::Idle => self.frame_idle(canvas, elapsed),
            Phase::Outro => self.frame_outro(canvas, elapsed),
            Phase::Done => Update::none(),
        }
    }

    fn full_area(&self) -> Rect {
        Rect::new(0, 0, self.layout.stage.width, self.layout.stage.height)
    }

    fn compose(&self, canvas: &mut Canvas<'_>, area: Rect, blur_position: u32, opacity: u32) {
        let lower = ((blur_position >> 8) as usize).min(LEVEL_COUNT - 1);
        let upper = (lower + 1).min(LEVEL_COUNT - 1);
        let mix = if lower == upper { 0 } else { blur_position & 255 };
        compose_ink(
            canvas,
            self.layout.stage.x,
            self.layout.stage.y,
            &self.base,
            &self.levels[lower],
            &self.levels[upper],
            mix,
            opacity,
            area,
        );
    }

    fn restore(&self, canvas: &mut Canvas<'_>, screen_area: Rect) {
        let local = Rect::new(
            screen_area.x - self.layout.stage.x,
            screen_area.y - self.layout.stage.y,
            screen_area.width,
            screen_area.height,
        );
        self.compose(canvas, local, 0, 256);
    }

    /// Işınların ve yükleme yuvarlağının kapladığı dinamik dikdörtgen
    fn anim_rect(&self) -> Rect {
        let layout = &self.layout;
        let half = layout.rays_half_span + 12;
        let top = layout.rays_y - 10;
        let bottom = layout.spinner_y + layout.spinner_radius + layout.spinner_dot_radius + 8;
        Rect::new(layout.center_x - half, top, half * 2, bottom - top)
    }

    fn dots_rect(&self) -> Rect {
        let layout = &self.layout;
        let radius = layout.dot_radius * 2;
        Rect::new(
            layout.dots_x - radius - 2,
            layout.dots_y - radius - 2,
            layout.dot_gap * 2 + radius * 2 + 4,
            radius * 2 + 4,
        )
    }

    /// Sağdan ve soldan merkeze doğru akan ışık hüzmeleri (Converging Light Rays)
    fn draw_converging_rays(&self, canvas: &mut Canvas<'_>, elapsed: u32, opacity: u32) {
        if opacity == 0 {
            return;
        }
        let layout = &self.layout;
        let span = layout.rays_half_span.max(32);
        let y = layout.rays_y;

        // 1. Intro başlangıcında sağdan ve soldan merkeze hücum eden ana ışın dalgası
        let intro_prog = smoothstep256(ratio256(elapsed, RAY_CONVERGE_MS)) as i32; // 0..256
        let intro_head = span - (span * intro_prog / 256); // span (dış) -> 0 (merkez)

        // 2. Sürekli dıştan merkeze akan ikincil enerji darbeleri (2 fazlı)
        let pulse1_prog = ((elapsed % RAY_PULSE_PERIOD_MS) * 256 / RAY_PULSE_PERIOD_MS) as i32;
        let pulse2_prog = (((elapsed + RAY_PULSE_PERIOD_MS / 2) % RAY_PULSE_PERIOD_MS) * 256
            / RAY_PULSE_PERIOD_MS) as i32;
        let pulse1_pos = span - (span * pulse1_prog / 256);
        let pulse2_pos = span - (span * pulse2_prog / 256);

        let beam_width = (span / 3).max(28);
        let core_half = (layout.title_width / 2).max(40);

        for dist in 0..=span {
            // Dış uçlara doğru doğal sönümlenme zarfı
            let edge_env = 255 - (dist * 210 / span);

            // Merkeze yerleşen kalıcı ışıltı (Intro ışınları merkeze ulaştıkça güçlenir)
            let base_glow = if dist < core_half && intro_prog > 96 {
                let c_fade = 255 - (dist * 255 / core_half);
                (c_fade * (intro_prog - 96) / 160).clamp(0, 140)
            } else {
                0
            };

            // Intro sırasında dıştan merkeze gelen öncü ışın
            let intro_intensity = if intro_prog < 256 {
                let d = (dist - intro_head).abs();
                if d < beam_width {
                    (255 - d * 255 / beam_width) * edge_env / 255
                } else {
                    0
                }
            } else {
                0
            };

            // Sürekli dıştan içe akan 1. ve 2. ışın paketleri
            let d1 = (dist - pulse1_pos).abs();
            let p1 = if d1 < beam_width {
                (220 - d1 * 220 / beam_width) * edge_env / 255
            } else {
                0
            };

            let d2 = (dist - pulse2_pos).abs();
            let p2 = if d2 < beam_width {
                (170 - d2 * 170 / beam_width) * edge_env / 255
            } else {
                0
            };

            let combined = (base_glow + intro_intensity.max(p1).max(p2)).clamp(0, 255) as u32;
            let alpha = (combined * opacity / 256).min(255) as u8;
            if alpha < 4 {
                continue;
            }

            // Merkeze yaklaştıkça ve ışın tepesinde rengi AQUA / ACCENT_LIGHT'a parlat
            let color_mix = ((255 - dist * 255 / span) as u32).max(combined);
            let color = lerp_color(ACCENT_DARK, AQUA, color_mix.min(255));

            let x_left = layout.center_x - dist;
            let x_right = layout.center_x + dist;

            // Merkez çizgi ve dikey ışıma (halo) katmanları
            canvas.blend_pixel(x_left, y, color, alpha);
            canvas.blend_pixel(x_left, y - 1, color, alpha / 2);
            canvas.blend_pixel(x_left, y + 1, color, alpha / 2);
            canvas.blend_pixel(x_left, y - 2, ACCENT_LIGHT, alpha / 5);
            canvas.blend_pixel(x_left, y + 2, ACCENT_LIGHT, alpha / 5);

            if dist > 0 {
                canvas.blend_pixel(x_right, y, color, alpha);
                canvas.blend_pixel(x_right, y - 1, color, alpha / 2);
                canvas.blend_pixel(x_right, y + 1, color, alpha / 2);
                canvas.blend_pixel(x_right, y - 2, ACCENT_LIGHT, alpha / 5);
                canvas.blend_pixel(x_right, y + 2, ACCENT_LIGHT, alpha / 5);
            }
        }

        // Sağ ve sol ışınların tam merkezde çarpışma / birleşme yıldızı (Center Flare)
        if intro_prog > 140 {
            let pulse_wave = (1000 + sin_milli((elapsed % 1200) as i32 * 360 / 1200)) as u32 / 2;
            let flare_strength =
                ((130 + 110 * pulse_wave / 1000) * (intro_prog as u32 - 140) / 116 * opacity / 256)
                    .min(256);
            draw_dot(canvas, layout.center_x, y, 3, AQUA, flare_strength);
        }
    }

    /// NetizenCore yazısının altındaki akıcı dönen yükleme yuvarlağı (Loading Spinner)
    fn draw_spinner(&self, canvas: &mut Canvas<'_>, elapsed: u32, opacity: u32) {
        if opacity == 0 {
            return;
        }
        let layout = &self.layout;
        let cx = layout.center_x;
        let cy = layout.spinner_y;
        let r = layout.spinner_radius;
        let dot_r = layout.spinner_dot_radius;

        let head_angle = ((elapsed % SPINNER_PERIOD_MS) * 360 / SPINNER_PERIOD_MS) as i32;

        // 1. Arka planda çok hafif dairesel yörünge izi (24 nokta)
        for step in 0..24 {
            let deg = step * 15;
            let x = cx + (r * cos_milli(deg) + 500) / 1000;
            let y = cy + (r * sin_milli(deg) + 500) / 1000;
            draw_dot(canvas, x, y, (dot_r - 1).max(1), ACCENT_LINE, 28 * opacity / 256);
        }

        // 2. Dönen parlak kuyruklu yay (Comet Arc - 18 kademeli mikro-segment, 255 derecelik kuyruk)
        const SEGMENTS: i32 = 18;
        for i in 0..SEGMENTS {
            let trail_offset = i * 14; // Kuyruk boyunca açı farkı
            let deg = head_angle - trail_offset;
            let x = cx + (r * cos_milli(deg) + 500) / 1000;
            let y = cy + (r * sin_milli(deg) + 500) / 1000;

            let progress = (SEGMENTS - 1 - i) as u32; // 0 (kuyruk ucu) .. 17 (baş kısım)
            let intensity = (progress * progress * 230) / ((SEGMENTS - 1) * (SEGMENTS - 1)) as u32 + 26;
            let strength = (intensity * opacity / 256).min(256);

            let color_mix = (progress * 255 / (SEGMENTS - 1) as u32).min(255);
            let color = lerp_color(ACCENT_DARK, AQUA, color_mix);

            // Baş kısım tam kalınlıkta, kuyruk ucu hafif incelerek biter
            let seg_radius = if i < 5 {
                dot_r
            } else if i < 13 {
                (dot_r - 1).max(2)
            } else {
                (dot_r - 2).max(1)
            };

            draw_dot(canvas, x, y, seg_radius, color, strength);
        }
    }

    fn draw_dots(&self, canvas: &mut Canvas<'_>, elapsed: u32, opacity: u32) {
        let layout = &self.layout;
        let cycle = (elapsed % DOT_PERIOD_MS) as i32 * 360 / DOT_PERIOD_MS as i32;

        for index in 0..3i32 {
            let wave = (1000 + sin_milli(cycle - index * 110)) as u32 / 2;
            let strength = ((70 + 186 * wave / 1000) * opacity / 256).min(256);
            draw_dot(
                canvas,
                layout.dots_x + index * layout.dot_gap,
                layout.dots_y,
                layout.dot_radius,
                ACCENT_LINE,
                strength,
            );
        }
    }

    fn draw_dynamic(&self, canvas: &mut Canvas<'_>, elapsed: u32, opacity: u32) {
        self.draw_converging_rays(canvas, elapsed, opacity);
        self.draw_spinner(canvas, elapsed, opacity);
        if self.layout.has_subtitle {
            self.draw_dots(canvas, elapsed, opacity);
        }
    }

    fn frame_intro(&mut self, canvas: &mut Canvas<'_>, elapsed: u32) -> Update {
        if elapsed >= INTRO_MS {
            self.phase = Phase::Idle;
            let area = self.full_area();
            self.compose(canvas, area, 0, 256);
            self.draw_dynamic(canvas, elapsed, 256);
            return Update::one(self.layout.stage);
        }

        let progress = smoothstep256(ratio256(elapsed, INTRO_MS));
        let blur_position = (256 - progress) * 3;
        let opacity = smoothstep256(ratio256(elapsed, FADE_IN_MS));

        let area = self.full_area();
        self.compose(canvas, area, blur_position, opacity);

        // Işınlar en baştan itibaren (minimum 110 opaklıkla) sağdan soldan gelmeye başlar
        let ray_opacity = opacity.max(110);
        self.draw_converging_rays(canvas, elapsed, ray_opacity);
        self.draw_spinner(canvas, elapsed, opacity);
        if self.layout.has_subtitle {
            self.draw_dots(canvas, elapsed, opacity);
        }

        Update::one(self.layout.stage)
    }

    fn frame_idle(&mut self, canvas: &mut Canvas<'_>, elapsed: u32) -> Update {
        let anim_rect = self.anim_rect();
        self.restore(canvas, anim_rect);
        self.draw_converging_rays(canvas, elapsed, 256);
        self.draw_spinner(canvas, elapsed, 256);

        if self.layout.has_subtitle {
            let dots_rect = self.dots_rect();
            self.restore(canvas, dots_rect);
            self.draw_dots(canvas, elapsed, 256);
            Update::two(anim_rect, dots_rect)
        } else {
            Update::one(anim_rect)
        }
    }

    fn frame_outro(&mut self, canvas: &mut Canvas<'_>, elapsed: u32) -> Update {
        let since = elapsed.wrapping_sub(self.finish_at);
        let area = self.full_area();
        if since >= OUTRO_MS {
            self.phase = Phase::Done;
            self.compose(canvas, area, 512, 0);
            return Update::one(self.layout.stage);
        }

        let progress = smoothstep256(ratio256(since, OUTRO_MS));
        let blur_position = progress * 2;
        let opacity = 256 - progress;

        self.compose(canvas, area, blur_position, opacity);
        self.draw_dynamic(canvas, elapsed, opacity);
        Update::one(self.layout.stage)
    }
}
