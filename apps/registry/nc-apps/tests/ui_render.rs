use nc_desktop::{draw, draw_wallpaper, DesktopState};
use nc_gfx::Canvas;
use nc_hal::MouseState;

fn write_ppm(path: &str, pixels: &[u32], w: usize, h: usize) {
    let mut out = format!("P6\n{} {}\n255\n", w, h).into_bytes();
    for p in pixels {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    std::fs::write(path, out).unwrap();
}

fn render(w: usize, h: usize, dark: bool, scale: u8, wallpaper: usize, open: &[usize]) -> Vec<u32> {
    let mut state = DesktopState::new("TEST", 1024, nc_apps::builtin());
    state.preview_configure(dark, scale, wallpaper);
    let mouse = MouseState { x: (w / 2) as i32, y: (h / 2) as i32, left_down: false, left_clicked: false, right_down: false, right_clicked: false };
    let mut now = 1000u32;
    for _ in 0..6 {
        now += 40;
        state.update(&mouse, None, w as i32, h as i32, now);
    }
    for app in open {
        state.preview_open_app(*app, w as i32, h as i32);
    }
    for _ in 0..40 {
        now += 40;
        state.update(&mouse, None, w as i32, h as i32, now);
    }
    let mut wall = vec![0u32; w * h];
    let mut wc = Canvas::new(&mut wall, w, h).unwrap();
    draw_wallpaper(&mut wc, state.wallpaper_index());
    let mut screen = vec![0u32; w * h];
    let mut canvas = Canvas::new(&mut screen, w, h).unwrap();
    draw(&mut canvas, &wc, &state);
    screen
}

#[test]
fn desktop_renders_without_panic_and_is_not_blank() {
    let screen = render(1280, 720, false, 2, 0, &[0, 1]);
    let distinct = screen.iter().step_by(97).map(|p| p & 0x00F8_F8F8).collect::<std::collections::HashSet<_>>().len();
    assert!(distinct > 200, "ekran bos gorunuyor: {}", distinct);
}

#[test]
fn dump_desktop_when_requested() {
    let Ok(dir) = std::env::var("NC_UI_DUMP") else { return; };
    std::fs::create_dir_all(&dir).unwrap();
    let cases: [(&str, usize, usize, bool, u8, usize, &[usize]); 4] = [
        ("light_720", 1280, 720, false, 2, 0, &[0, 1]),
        ("light_1080", 1920, 1080, false, 2, 1, &[2]),
        ("dark_720", 1280, 720, true, 2, 5, &[1, 3]),
        ("scale150_720", 1280, 720, false, 5, 0, &[1]),
    ];
    for (name, w, h, dark, scale, wp, open) in cases {
        let screen = render(w, h, dark, scale, wp, open);
        write_ppm(&format!("{}/{}.ppm", dir, name), &screen, w, h);
    }
}

static NOTIFY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn pump(state: &mut DesktopState, mouse: &MouseState, now: &mut u32, frames: u32, w: usize, h: usize) {
    for _ in 0..frames {
        *now += 40;
        state.update(mouse, None, w as i32, h as i32, *now);
    }
}

fn idle(w: usize, h: usize) -> MouseState {
    MouseState { x: (w / 2) as i32, y: (h / 2) as i32, left_down: false, left_clicked: false, right_down: false, right_clicked: false }
}

fn click(x: i32, y: i32) -> MouseState {
    MouseState { x, y, left_down: false, left_clicked: true, right_down: false, right_clicked: false }
}

fn post(app: &str, title: &str, body: &str, level: nc_app::notify::Level) {
    assert!(nc_app::notify::post(nc_app::notify::Notification {
        app_id: app.into(),
        title: title.into(),
        body: body.into(),
        level,
    }));
}

fn shot(state: &DesktopState, w: usize, h: usize) -> Vec<u32> {
    let mut wall = vec![0u32; w * h];
    let mut wc = Canvas::new(&mut wall, w, h).unwrap();
    draw_wallpaper(&mut wc, state.wallpaper_index());
    let mut screen = vec![0u32; w * h];
    let mut canvas = Canvas::new(&mut screen, w, h).unwrap();
    draw(&mut canvas, &wc, state);
    screen
}

#[test]
fn permission_prompt_allow_flow_shows_held_notifications() {
    let _g = NOTIFY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (w, h) = (1280usize, 720usize);
    nc_app::notify::drain();
    nc_app::notify::forget_permission("nc.stopwatch");
    let mut state = DesktopState::new("TEST", 1024, nc_apps::builtin());
    let mut now = 1000u32;
    pump(&mut state, &idle(w, h), &mut now, 4, w, h);
    post("nc.stopwatch", "Kronometre", "1 dakika geçti", nc_app::notify::Level::Info);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    assert_eq!(state.preview_notification_state(), (0, 0, true), "izin sorusu bekleniyordu");
    assert_eq!(nc_app::notify::permission("nc.stopwatch"), nc_app::notify::Permission::Ask);

    if let Ok(dir) = std::env::var("NC_UI_DUMP") {
        std::fs::create_dir_all(&dir).unwrap();
        write_ppm(&format!("{}/notify_prompt.ppm", dir), &shot(&state, w, h), w, h);
    }

    let (x, y) = state.preview_prompt_button(true, w as i32, h as i32).unwrap();
    pump(&mut state, &click(x, y), &mut now, 1, w, h);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    assert_eq!(nc_app::notify::permission("nc.stopwatch"), nc_app::notify::Permission::Allowed);
    assert_eq!(state.preview_notification_state(), (1, 1, false));

    post("nc.stopwatch", "Tur kaydedildi", "Tur 3 · 00:42.7", nc_app::notify::Level::Success);
    post("nc.files", "Kopyalama bitti", "12 öğe kopyalandı", nc_app::notify::Level::Warning);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    nc_app::notify::set_permission("nc.files", nc_app::notify::Permission::Allowed);
    assert!(state.preview_notification_state().1 >= 2);
    if let Ok(dir) = std::env::var("NC_UI_DUMP") {
        write_ppm(&format!("{}/notify_toasts.ppm", dir), &shot(&state, w, h), w, h);
    }

    state.preview_toggle_notification_panel();
    pump(&mut state, &idle(w, h), &mut now, 14, w, h);
    if let Ok(dir) = std::env::var("NC_UI_DUMP") {
        write_ppm(&format!("{}/notify_panel.ppm", dir), &shot(&state, w, h), w, h);
    }

    for _ in 0..170 {
        pump(&mut state, &idle(w, h), &mut now, 1, w, h);
    }
    nc_app::notify::set_permission("nc.stopwatch", nc_app::notify::Permission::Ask);
}

#[test]
fn permission_prompt_deny_flow_drops_and_blocks_future() {
    let _g = NOTIFY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (w, h) = (1280usize, 720usize);
    nc_app::notify::drain();
    nc_app::notify::forget_permission("nc.browser");
    let mut state = DesktopState::new("TEST", 1024, nc_apps::builtin());
    let mut now = 1000u32;
    pump(&mut state, &idle(w, h), &mut now, 4, w, h);
    post("nc.browser", "Yükleme", "tamamlandı", nc_app::notify::Level::Info);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    let (x, y) = state.preview_prompt_button(false, w as i32, h as i32).unwrap();
    pump(&mut state, &click(x, y), &mut now, 1, w, h);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    assert_eq!(nc_app::notify::permission("nc.browser"), nc_app::notify::Permission::Denied);
    assert_eq!(state.preview_notification_state(), (0, 0, false));
    post("nc.browser", "Yine", "x", nc_app::notify::Level::Info);
    pump(&mut state, &idle(w, h), &mut now, 12, w, h);
    assert_eq!(state.preview_notification_state(), (0, 0, false), "reddedilen uygulama gösterilmemeli");
    nc_app::notify::forget_permission("nc.browser");
}
