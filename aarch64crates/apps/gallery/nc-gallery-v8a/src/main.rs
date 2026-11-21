//! NetizenCore Independent Ring 3 / EL0 System App: Gallery (`nc.gallery`).
//!
//! - Follows live NetizenHome OS theme (light/dark, accent color, TR/EN localization).
//! - Requests `media.photos.read` before enumerating or opening protected media.
//! - Uses kernel-enforced scoped Media service (`nc_app_sdk::media`); never requires broad FS caps.
//! - Persists favorites and recently viewed history inside its private `/data/appdata/nc.gallery/` sandbox.
//! - Image-led minimal navigation (Photos, Favorites, Recent, Search), metadata sorting,
//!   multi-select batch favorite, and an immersive full-screen viewer with real image details.
#![no_std]
#![no_main]

use nc_abi::draw::ICON_GALLERY;
use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_ANIMATING, RET_CLOSE,
    RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::perm::{PermState, PERM_MEDIA_PHOTOS_READ};
use nc_app_sdk::media::{self, MediaPhoto};
use nc_app_sdk::{dispatch, ime, perm, pointer, status, storage, sys, Gfx, Line, RingApp};
use nc_mobile_ui::{
    draw_bottom_nav, draw_btn, draw_icon_btn, draw_search_bar, draw_state_view, draw_toast,
    draw_top_bar, draw_ui_icon, hit_bottom_nav, truncate_to_px, BtnKind, MobileTheme,
    SurfaceTokens, ToastState, TopBarGeo, Tween, UiIcon, UiRect, Viewport, AMBER, CORAL, ONE,
    SPEC_FADE, SPEC_SHEET, WHITE,
};

const MAX_PHOTOS: usize = 48;
const MAX_PAGE_TILES: usize = 9; // up to 3x3 portrait or 4x2 landscape
const THUMB_SIDE: usize = 10;
const THUMB_RGB_LEN: usize = THUMB_SIDE * THUMB_SIDE * 3;
const MAX_FAVS: usize = 32;
const MAX_RECENT: usize = 16;
const FAV_PATH: &str = "/data/appdata/nc.gallery/favorites.cfg";
const RECENT_PATH: &str = "/data/appdata/nc.gallery/recent.cfg";

#[derive(Clone, Copy, PartialEq, Eq)]
enum NavTab {
    Photos,
    Favorites,
    Recent,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    NameAsc,
    NameDesc,
    SizeDesc,
    Format,
}

impl SortMode {
    fn next(self) -> Self {
        match self {
            Self::NameAsc => Self::NameDesc,
            Self::NameDesc => Self::SizeDesc,
            Self::SizeDesc => Self::Format,
            Self::Format => Self::NameAsc,
        }
    }

    fn label(self, en: bool) -> &'static str {
        match self {
            Self::NameAsc => if en { "A-Z" } else { "A-Z" },
            Self::NameDesc => if en { "Z-A" } else { "Z-A" },
            Self::SizeDesc => if en { "Size" } else { "Boyut" },
            Self::Format => if en { "Type" } else { "Tür" },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Grid,
    Viewer,
}

struct GalleryApp {
    view: View,
    tab: NavTab,
    sort: SortMode,
    perm_state: PermState,
    hw_avail: bool,
    prompted: bool,
    load_error: bool,

    photos: [MediaPhoto; MAX_PHOTOS],
    photo_count: usize,
    // Filtered & sorted indices into `photos`
    filtered: [u8; MAX_PHOTOS],
    filtered_count: usize,
    page: usize,
    // Index inside `filtered` when in Viewer
    viewer_pos: usize,
    chrome_visible: bool,
    info_open: bool,

    // Multi-select mode (indices into `photos`)
    select_mode: bool,
    selected_mask: u64,

    // Search
    search_active: bool,
    search: [u8; 32],
    search_len: usize,

    // Cached 10x10 RGB thumbnails for currently visible page slots
    thumbs: [[u8; THUMB_RGB_LEN]; MAX_PAGE_TILES],
    thumb_valid: [bool; MAX_PAGE_TILES],
    thumb_cache_key: u64,

    // Persisted Favorites & Recently Viewed
    favs: [[u8; 44]; MAX_FAVS],
    fav_lens: [u8; MAX_FAVS],
    fav_count: usize,
    recent: [[u8; 44]; MAX_RECENT],
    recent_lens: [u8; MAX_RECENT],
    recent_count: usize,
    sandbox_loaded: bool,

    // Motion & interaction
    reduced_motion: bool,
    chrome_tween: Tween,
    info_tween: Tween,
    pressed_nav: Option<usize>,
    pressed_id: u8,
    toast: ToastState,
    last_flags: u32,
    last_accent: u32,
    last_lang: u32,
}

static mut RESP_BUF: [u8; 4096] = [0; 4096];
static mut THUMB_BUF: [u8; 512] = [0; 512];
static mut COMMANDS: [u8; 16384] = [0; 16384];

fn copy_str(dst: &mut [u8], s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len().min(dst.len());
    dst[..n].copy_from_slice(&b[..n]);
    n
}

fn s_of(buf: &[u8], len: usize) -> &str {
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

fn push_char(buf: &mut [u8], len: &mut usize, c: char) {
    let mut tmp = [0u8; 4];
    let enc = c.encode_utf8(&mut tmp);
    if *len + enc.len() <= buf.len() {
        buf[*len..*len + enc.len()].copy_from_slice(enc.as_bytes());
        *len += enc.len();
    }
}

fn pop_char(buf: &[u8], len: &mut usize) {
    if *len == 0 {
        return;
    }
    let mut nl = *len - 1;
    while nl > 0 && (buf[nl] & 0xC0) == 0x80 {
        nl -= 1;
    }
    *len = nl;
}

fn contains_ci_ascii(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hb = hay.as_bytes();
    let nb = needle.as_bytes();
    if nb.len() > hb.len() {
        return false;
    }
    for i in 0..=hb.len() - nb.len() {
        let mut ok = true;
        for j in 0..nb.len() {
            if hb[i + j].to_ascii_lowercase() != nb[j].to_ascii_lowercase() {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

fn grid_metrics(vp: &Viewport, has_subbar: bool) -> (usize, usize, UiRect, i32, i32, i32) {
    let top = TopBarGeo::compute(vp, 36);
    let nav_h = vp.bottom_nav_h();
    let sub_h = if has_subbar { vp.d(44).clamp(38, 50) } else { vp.d(8) };
    let pager_h = vp.d(36).clamp(30, 42);
    let grid_y = top.bar.bottom() + sub_h;
    let grid_h = (vp.area_h - nav_h - pager_h - vp.d(12) - grid_y).max(vp.d(120));
    let cols: usize = if vp.is_landscape { 4 } else { 3 };
    let rows: usize = if vp.is_landscape || vp.is_compact_h { 2 } else { 3 };
    let gap = vp.d(6).clamp(4, 10);
    let grid_w = if vp.is_landscape { vp.area_w - vp.margin_x * 2 } else { vp.content_w };
    let grid_x = (vp.area_w - grid_w) / 2;
    let tile_w = (grid_w - gap * (cols as i32 - 1)) / cols as i32;
    let tile_h = ((grid_h - gap * (rows as i32 - 1)) / rows as i32).min(tile_w + vp.d(18)).max(vp.d(56));
    (cols, rows, UiRect::new(grid_x, grid_y, grid_w, grid_h), tile_w, tile_h, gap)
}

fn viewer_stage_rect(vp: &Viewport, chrome_visible: bool) -> UiRect {
    let top_h = if chrome_visible { vp.header_h() } else { 0 };
    let bot_h = if chrome_visible { vp.d(64).clamp(52, 72) } else { 0 };
    UiRect::new(0, top_h, vp.area_w, (vp.area_h - top_h - bot_h).max(120))
}

impl GalleryApp {
    const fn new() -> Self {
        Self {
            view: View::Grid,
            tab: NavTab::Photos,
            sort: SortMode::NameAsc,
            perm_state: PermState::Ask,
            hw_avail: true,
            prompted: false,
            load_error: false,
            photos: [MediaPhoto::EMPTY; MAX_PHOTOS],
            photo_count: 0,
            filtered: [0; MAX_PHOTOS],
            filtered_count: 0,
            page: 0,
            viewer_pos: 0,
            chrome_visible: true,
            info_open: false,
            select_mode: false,
            selected_mask: 0,
            search_active: false,
            search: [0; 32],
            search_len: 0,
            thumbs: [[0; THUMB_RGB_LEN]; MAX_PAGE_TILES],
            thumb_valid: [false; MAX_PAGE_TILES],
            thumb_cache_key: 0,
            favs: [[0; 44]; MAX_FAVS],
            fav_lens: [0; MAX_FAVS],
            fav_count: 0,
            recent: [[0; 44]; MAX_RECENT],
            recent_lens: [0; MAX_RECENT],
            recent_count: 0,
            sandbox_loaded: false,
            reduced_motion: false,
            chrome_tween: Tween::with_value(ONE),
            info_tween: Tween::new(),
            pressed_nav: None,
            pressed_id: 0,
            toast: ToastState::new(),
            last_flags: 0,
            last_accent: 0,
            last_lang: 0,
        }
    }

    fn load_sandbox(&mut self) {
        if self.sandbox_loaded {
            return;
        }
        self.sandbox_loaded = true;
        self.fav_count = 0;
        let mut buf = [0u8; 2048];
        if let Some(n) = storage::read_file(FAV_PATH, &mut buf) {
            if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                for line in text.lines() {
                    let t = line.trim();
                    if !t.is_empty() && self.fav_count < MAX_FAVS {
                        let slot = self.fav_count;
                        self.fav_lens[slot] = copy_str(&mut self.favs[slot], t) as u8;
                        self.fav_count += 1;
                    }
                }
            }
        }
        self.recent_count = 0;
        if let Some(n) = storage::read_file(RECENT_PATH, &mut buf) {
            if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                for line in text.lines() {
                    let t = line.trim();
                    if !t.is_empty() && self.recent_count < MAX_RECENT {
                        let slot = self.recent_count;
                        self.recent_lens[slot] = copy_str(&mut self.recent[slot], t) as u8;
                        self.recent_count += 1;
                    }
                }
            }
        }
    }

    fn save_list(path: &str, items: &[[u8; 44]], lens: &[u8], count: usize) {
        let _ = storage::mkdir("/data/appdata/nc.gallery");
        let mut out = [0u8; 2048];
        let mut pos = 0usize;
        for i in 0..count {
            let len = lens[i] as usize;
            if pos + len + 1 > out.len() {
                break;
            }
            out[pos..pos + len].copy_from_slice(&items[i][..len]);
            pos += len;
            out[pos] = b'\n';
            pos += 1;
        }
        let _ = storage::write_file(path, &out[..pos]);
    }

    fn is_favorite(&self, name: &str) -> bool {
        for i in 0..self.fav_count {
            if s_of(&self.favs[i], self.fav_lens[i] as usize) == name {
                return true;
            }
        }
        false
    }

    fn set_favorite(&mut self, name: &str, fav: bool) -> bool {
        let exists = (0..self.fav_count).position(|i| s_of(&self.favs[i], self.fav_lens[i] as usize) == name);
        if fav {
            if exists.is_some() || self.fav_count >= MAX_FAVS {
                return false;
            }
            let slot = self.fav_count;
            self.fav_lens[slot] = copy_str(&mut self.favs[slot], name) as u8;
            self.fav_count += 1;
            Self::save_list(FAV_PATH, &self.favs, &self.fav_lens, self.fav_count);
            true
        } else if let Some(idx) = exists {
            for j in idx..self.fav_count - 1 {
                self.favs[j] = self.favs[j + 1];
                self.fav_lens[j] = self.fav_lens[j + 1];
            }
            self.fav_count -= 1;
            Self::save_list(FAV_PATH, &self.favs, &self.fav_lens, self.fav_count);
            true
        } else {
            false
        }
    }

    fn record_recent(&mut self, name: &str) {
        if let Some(pos) = (0..self.recent_count).position(|i| s_of(&self.recent[i], self.recent_lens[i] as usize) == name) {
            for j in (1..=pos).rev() {
                self.recent[j] = self.recent[j - 1];
                self.recent_lens[j] = self.recent_lens[j - 1];
            }
        } else {
            let lim = self.recent_count.min(MAX_RECENT - 1);
            for j in (1..=lim).rev() {
                self.recent[j] = self.recent[j - 1];
                self.recent_lens[j] = self.recent_lens[j - 1];
            }
            if self.recent_count < MAX_RECENT {
                self.recent_count += 1;
            }
        }
        self.recent_lens[0] = copy_str(&mut self.recent[0], name) as u8;
        Self::save_list(RECENT_PATH, &self.recent, &self.recent_lens, self.recent_count);
    }

    fn rebuild_filtered(&mut self) {
        self.filtered_count = 0;
        match self.tab {
            NavTab::Recent => {
                for r in 0..self.recent_count {
                    let rname = s_of(&self.recent[r], self.recent_lens[r] as usize);
                    if let Some(pidx) = (0..self.photo_count).position(|i| self.photos[i].name_str() == rname) {
                        if self.filtered_count < MAX_PHOTOS {
                            self.filtered[self.filtered_count] = pidx as u8;
                            self.filtered_count += 1;
                        }
                    }
                }
            }
            _ => {
                let q = s_of(&self.search, self.search_len);
                for i in 0..self.photo_count {
                    let p = &self.photos[i];
                    let include = match self.tab {
                        NavTab::Photos => true,
                        NavTab::Favorites => self.is_favorite(p.name_str()),
                        NavTab::Search => {
                            q.is_empty()
                                || contains_ci_ascii(p.name_str(), q)
                                || contains_ci_ascii(p.format_str(), q)
                        }
                        NavTab::Recent => false,
                    };
                    if include && self.filtered_count < MAX_PHOTOS {
                        self.filtered[self.filtered_count] = i as u8;
                        self.filtered_count += 1;
                    }
                }
                // Simple in-place insertion sort by active real metadata sort mode
                for i in 1..self.filtered_count {
                    let key = self.filtered[i];
                    let mut j = i;
                    while j > 0 && self.comes_before(key as usize, self.filtered[j - 1] as usize) {
                        self.filtered[j] = self.filtered[j - 1];
                        j -= 1;
                    }
                    self.filtered[j] = key;
                }
            }
        }
        self.page = 0;
        self.thumb_cache_key = 0;
    }

    fn comes_before(&self, a: usize, b: usize) -> bool {
        let pa = &self.photos[a];
        let pb = &self.photos[b];
        match self.sort {
            SortMode::NameAsc => pa.name_str() < pb.name_str(),
            SortMode::NameDesc => pa.name_str() > pb.name_str(),
            SortMode::SizeDesc => pa.size_bytes > pb.size_bytes,
            SortMode::Format => (pa.format_str(), pa.name_str()) < (pb.format_str(), pb.name_str()),
        }
    }

    fn sync_permission(&mut self, request_if_ask: bool, page_cap: usize) {
        let st = perm::check(PERM_MEDIA_PHOTOS_READ);
        self.perm_state = st.state;
        self.hw_avail = st.hw_available;
        if request_if_ask && self.perm_state == PermState::Ask && !self.prompted && !st.pending {
            self.prompted = true;
            let _ = perm::request(PERM_MEDIA_PHOTOS_READ);
        }
        if self.perm_state == PermState::Granted {
            self.reload_photos(page_cap);
        } else {
            self.photo_count = 0;
            self.filtered_count = 0;
            media::clear_preview();
            self.view = View::Grid;
        }
    }

    fn reload_photos(&mut self, page_cap: usize) {
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(RESP_BUF) };
        match media::list_photos(buf, &mut self.photos) {
            Ok(cnt) => {
                self.photo_count = cnt;
                self.load_error = false;
                self.rebuild_filtered();
                self.load_page_thumbs(page_cap);
            }
            Err(_) => {
                self.photo_count = 0;
                self.filtered_count = 0;
                self.load_error = true;
            }
        }
    }

    fn cache_key(&self, page_cap: usize) -> u64 {
        let mut h = ((self.tab as u64) << 40)
            ^ ((self.sort as u64) << 32)
            ^ ((self.page as u64) << 16)
            ^ (page_cap as u64)
            ^ ((self.filtered_count as u64) << 8)
            ^ 0x1357_9BDF;
        let start = self.page * page_cap;
        for slot in 0..page_cap.min(MAX_PAGE_TILES) {
            if let Some(&idx) = self.filtered.get(start + slot) {
                h = h.rotate_left(7) ^ (idx as u64 + 1);
            }
        }
        h
    }

    fn load_page_thumbs(&mut self, page_cap: usize) {
        let cap = page_cap.clamp(1, MAX_PAGE_TILES);
        let key = self.cache_key(cap);
        if self.thumb_cache_key == key {
            return;
        }
        self.thumb_cache_key = key;
        self.thumb_valid = [false; MAX_PAGE_TILES];
        let start = self.page * cap;
        let tbuf = unsafe { &mut *core::ptr::addr_of_mut!(THUMB_BUF) };
        for slot in 0..cap {
            let fpos = start + slot;
            if fpos >= self.filtered_count {
                break;
            }
            let pidx = self.filtered[fpos] as usize;
            let mut nbuf = [0u8; 44];
            let nlen = copy_str(&mut nbuf, self.photos[pidx].name_str());
            let name = s_of(&nbuf, nlen);
            if let Ok((w, h, need)) = media::thumbnail(name, tbuf) {
                if w as usize == THUMB_SIDE && h as usize == THUMB_SIDE && need == THUMB_RGB_LEN {
                    let off = nc_abi::sys::RESP_HEADER + 2;
                    if off + need <= tbuf.len() {
                        self.thumbs[slot].copy_from_slice(&tbuf[off..off + need]);
                        self.thumb_valid[slot] = true;
                    }
                }
            }
        }
    }

    fn update_viewer_preview(&mut self, ev: &EventBlock) {
        if self.view != View::Viewer || self.info_open || self.viewer_pos >= self.filtered_count {
            media::clear_preview();
            return;
        }
        let vp = Viewport::from_event(ev);
        let stage = viewer_stage_rect(&vp, self.chrome_visible);
        let pidx = self.filtered[self.viewer_pos] as usize;
        let mut nbuf = [0u8; 44];
        let nlen = copy_str(&mut nbuf, self.photos[pidx].name_str());
        let name = s_of(&nbuf, nlen);
        let pad = vp.d(8);
        let _ = media::set_preview(
            name,
            stage.x + pad,
            stage.y + pad,
            (stage.w - pad * 2).max(48),
            (stage.h - pad * 2).max(48),
        );
    }

    fn open_viewer_at(&mut self, fpos: usize, ev: &EventBlock) {
        if fpos >= self.filtered_count {
            return;
        }
        self.viewer_pos = fpos;
        self.view = View::Viewer;
        self.chrome_visible = true;
        self.info_open = false;
        self.chrome_tween.set(ONE);
        self.info_tween.set(0);
        let pidx = self.filtered[fpos] as usize;
        let mut nbuf = [0u8; 44];
        let nlen = copy_str(&mut nbuf, self.photos[pidx].name_str());
        self.record_recent(s_of(&nbuf, nlen));
        self.update_viewer_preview(ev);
    }

    fn step_animations(&mut self, now_ms: u32) -> bool {
        self.chrome_tween.step(
            now_ms,
            if self.chrome_visible { ONE } else { 0 },
            &SPEC_FADE,
            self.reduced_motion,
        );
        self.info_tween.step(
            now_ms,
            if self.info_open { ONE } else { 0 },
            &SPEC_SHEET,
            self.reduced_motion,
        );
        !self.chrome_tween.settled() || !self.info_tween.settled()
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let th = MobileTheme::from_event(ev);
        let tok = th.tokens();
        let vp = Viewport::from_event(ev);

        if self.view == View::Viewer && self.perm_state == PermState::Granted {
            self.draw_viewer(gfx, &vp, &th, &tok);
            return;
        }

        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, tok.bg);
        let top = TopBarGeo::compute(&vp, 36);
        let title = match self.tab {
            NavTab::Photos => th.tr("Galeri", "Gallery"),
            NavTab::Favorites => th.tr("Favoriler", "Favorites"),
            NavTab::Recent => th.tr("Son Bakılan", "Recent"),
            NavTab::Search => th.tr("Fotoğraf Ara", "Search Photos"),
        };
        draw_top_bar(gfx, &vp, &tok, &top, title, false, false, Some(ICON_GALLERY));

        if self.perm_state != PermState::Granted {
            let area = UiRect::new(
                vp.content_x,
                top.bar.bottom() + vp.d(12),
                vp.content_w,
                (vp.area_h - top.bar.bottom() - vp.d(24)).max(160),
            );
            let denied = self.perm_state == PermState::Denied;
            draw_state_view(
                gfx,
                &vp,
                &tok,
                area,
                UiIcon::Photo,
                if denied { CORAL } else { tok.accent },
                if denied {
                    th.tr("Fotoğraf izni reddedildi", "Photos permission denied")
                } else {
                    th.tr("Fotoğraf erişimi gerekli", "Photos access required")
                },
                if denied {
                    th.tr(
                        "Fotoğraflarınızı görüntülemek için Ayarlar > Uygulama İzinleri bölümünden izin verin.",
                        "Enable Photos access in Settings > App Permissions to browse your library.",
                    )
                } else {
                    th.tr(
                        "Galeri yalnızca kapsamlı fotoğraf izniyle /data/media görsellerini gösterir.",
                        "Gallery uses scoped Photos permission to browse images inside /data/media.",
                    )
                },
                Some(if denied {
                    th.tr("Ayarları Aç", "Open Settings")
                } else {
                    th.tr("İzin İste", "Allow Photos")
                }),
                self.pressed_id == 99,
            );
            return;
        }

        if !self.hw_avail {
            let area = UiRect::new(
                vp.content_x,
                top.bar.bottom() + vp.d(12),
                vp.content_w,
                (vp.area_h - top.bar.bottom() - vp.d(24)).max(160),
            );
            draw_state_view(
                gfx,
                &vp,
                &tok,
                area,
                UiIcon::Photo,
                AMBER,
                th.tr("Depolama kullanılamıyor", "Storage unavailable"),
                th.tr("/data birimi bağlı değil. Depolama bağlandığında yeniden deneyin.", "/data volume is not mounted. Retry once storage is available."),
                Some(th.tr("Yeniden Dene", "Retry")),
                self.pressed_id == 98,
            );
            return;
        }

        // Trailing header buttons: Select mode & Sort/Refresh
        draw_icon_btn(gfx, &tok, top.trailing_btn, UiIcon::Select, self.select_mode, self.filtered_count == 0, self.pressed_id == 1);
        let sort_btn = UiRect::new(
            top.trailing_btn.x - vp.d(56),
            top.trailing_btn.y,
            vp.d(50),
            top.trailing_btn.h,
        );
        if self.tab != NavTab::Recent {
            draw_btn(gfx, &tok, sort_btn, BtnKind::Quiet, self.pressed_id == 2, vp.d(7).max(7), self.sort.label(th.en));
        }

        // Subbar: Search input on Search tab, or concise count/refresh strip on Photos/Favorites/Recent
        let has_subbar = true;
        let (cols, rows, grid_box, tile_w, tile_h, gap) = grid_metrics(&vp, has_subbar);
        let sub_y = top.bar.bottom() + vp.d(6);
        let sub_h = vp.d(32).clamp(28, 38);
        if self.tab == NavTab::Search {
            let sr = UiRect::new(vp.content_x, sub_y, vp.content_w, sub_h);
            draw_search_bar(
                gfx,
                &vp,
                &tok,
                sr,
                s_of(&self.search, self.search_len),
                th.tr("Dosya adı veya PNG/JPEG ara...", "Search filename or PNG/JPEG..."),
                self.search_active,
            );
        } else {
            let mut cnt = Line::new();
            if self.select_mode {
                cnt.number(self.selected_mask.count_ones(), 1).text(th.tr(" seçildi", " selected"));
            } else {
                cnt.number(self.filtered_count as u32, 1).text(th.tr(" öğe", " items"));
            }
            gfx.text(vp.content_x, sub_y + (sub_h - vp.d(8).max(8)) / 2, vp.d(8).max(8), tok.muted, 1, cnt.as_str());
            let ref_r = UiRect::new(vp.content_x + vp.content_w - vp.d(68), sub_y + vp.d(2), vp.d(68), sub_h - vp.d(4));
            draw_btn(gfx, &tok, ref_r, BtnKind::Quiet, self.pressed_id == 3, vp.d(7).max(7), th.tr("Yenile", "Refresh"));
        }

        // Empty / error states
        if self.load_error {
            draw_state_view(
                gfx,
                &vp,
                &tok,
                grid_box,
                UiIcon::Refresh,
                AMBER,
                th.tr("Fotoğraflar okunamadı", "Could not load photos"),
                th.tr("Medya servisi yanıt vermedi. Yenilemek için dokunun.", "Media service returned an error. Tap to retry."),
                Some(th.tr("Yenile", "Refresh")),
                self.pressed_id == 3,
            );
        } else if self.filtered_count == 0 {
            let (et, eb) = match self.tab {
                NavTab::Photos => (
                    th.tr("Henüz fotoğraf yok", "No photos yet"),
                    th.tr("/data/media klasörüne PNG veya JPEG ekleyin.", "Add PNG or JPEG images to /data/media."),
                ),
                NavTab::Favorites => (
                    th.tr("Favori fotoğraf yok", "No favorites yet"),
                    th.tr("Bir fotoğrafı açıp yıldız simgesine dokunarak favorilere ekleyin.", "Open any photo and tap the star to add it to favorites."),
                ),
                NavTab::Recent => (
                    th.tr("Son bakılan fotoğraf yok", "No recent photos"),
                    th.tr("Açtığınız fotoğraflar burada hızlı erişim için listelenir.", "Photos you open appear here for quick access."),
                ),
                NavTab::Search => (
                    th.tr("Eşleşen sonuç yok", "No matching photos"),
                    th.tr("Farklı bir dosya adı veya biçim (PNG/JPEG) deneyin.", "Try a different filename or format (PNG/JPEG)."),
                ),
            };
            draw_state_view(
                gfx,
                &vp,
                &tok,
                grid_box,
                UiIcon::Photo,
                tok.muted,
                et,
                eb,
                if self.tab == NavTab::Photos { Some(th.tr("Yenile", "Refresh")) } else { None },
                self.pressed_id == 3,
            );
        } else {
            let page_cap = (cols * rows).min(MAX_PAGE_TILES);
            let start = self.page * page_cap;
            let rad = vp.d(10).clamp(6, 14);
            for slot in 0..page_cap {
                let fpos = start + slot;
                if fpos >= self.filtered_count {
                    break;
                }
                let pidx = self.filtered[fpos] as usize;
                let photo = &self.photos[pidx];
                let col = (slot % cols) as i32;
                let row = (slot / cols) as i32;
                let tx = grid_box.x + col * (tile_w + gap);
                let ty = grid_box.y + row * (tile_h + gap);
                let img_h = (tile_h - vp.d(18)).max(vp.d(42));

                let is_sel = self.select_mode && ((self.selected_mask >> pidx) & 1) != 0;
                gfx.panel(
                    tx,
                    ty,
                    tile_w,
                    tile_h,
                    rad,
                    tok.surface,
                    255,
                    if is_sel { tok.accent } else { tok.border },
                    if is_sel { 240 } else { tok.border_alpha },
                );

                // Downsampled 4x4 RGB mosaic preview (16 fill_rect per tile -> fast & well within 512 cmd budget!)
                let pad_i = 3;
                let px0 = tx + pad_i;
                let py0 = ty + pad_i;
                let pw = (tile_w - pad_i * 2).max(16);
                let ph = (img_h - pad_i).max(16);
                if self.thumb_valid[slot] {
                    let cw = (pw / 4).max(1);
                    let ch = (ph / 4).max(1);
                    for gy in 0..4usize {
                        for gx in 0..4usize {
                            let sy = (gy * 3).min(THUMB_SIDE - 1);
                            let sx = (gx * 3).min(THUMB_SIDE - 1);
                            let si = (sy * THUMB_SIDE + sx) * 3;
                            let r = self.thumbs[slot][si] as u32;
                            let g = self.thumbs[slot][si + 1] as u32;
                            let b = self.thumbs[slot][si + 2] as u32;
                            let rw = if gx == 3 { pw - cw * 3 } else { cw };
                            let rh = if gy == 3 { ph - ch * 3 } else { ch };
                            gfx.fill_rect(
                                px0 + gx as i32 * cw,
                                py0 + gy as i32 * ch,
                                rw,
                                rh,
                                (r << 16) | (g << 8) | b,
                            );
                        }
                    }
                } else {
                    gfx.panel(px0, py0, pw, ph, (rad - 2).max(4), tok.surface_subtle, 255, 0, 0);
                    gfx.text_centered(px0 + pw / 2, py0 + ph / 2 - vp.d(4), vp.d(8).max(7), tok.muted, 2, photo.format_str());
                }

                // Favorite indicator or selection checkmark
                if self.select_mode {
                    let br = vp.d(9).clamp(7, 11);
                    let bcx = tx + tile_w - br - 5;
                    let bcy = ty + br + 5;
                    gfx.dot(bcx, bcy, br, if is_sel { tok.accent } else { tok.surface }, 1024);
                    if is_sel {
                        draw_ui_icon(gfx, UiIcon::Check, bcx, bcy, br * 6 / 10, WHITE, tok.accent);
                    }
                } else if self.is_favorite(photo.name_str()) {
                    gfx.dot(tx + tile_w - vp.d(10), ty + vp.d(10), vp.d(4).max(3), AMBER, 1024);
                }

                // Crisp single-line truncated filename below image
                let mut nb = [0u8; 24];
                let lbl_cap = vp.d(7).max(7);
                let short = truncate_to_px(photo.name_str(), tile_w - vp.d(10), lbl_cap, &mut nb);
                gfx.text(tx + vp.d(5), ty + img_h + vp.d(4), lbl_cap, tok.ink, 1, short);
            }

            // Compact pager when multiple pages exist
            let total_pages = (self.filtered_count + page_cap - 1) / page_cap;
            if total_pages > 1 {
                let py = grid_box.bottom() + vp.d(4);
                let ph = vp.d(30).clamp(26, 36);
                let bw = vp.d(84).clamp(72, 100);
                draw_btn(
                    gfx,
                    &tok,
                    UiRect::new(grid_box.x, py, bw, ph),
                    if self.page > 0 { BtnKind::Secondary } else { BtnKind::Disabled },
                    self.pressed_id == 4,
                    vp.d(8).max(7),
                    th.tr("Önceki", "Prev"),
                );
                let mut pg = Line::new();
                pg.number(self.page as u32 + 1, 1).text(" / ").number(total_pages as u32, 1);
                gfx.text_centered(vp.area_w / 2, py + (ph - vp.d(8).max(8)) / 2, vp.d(8).max(8), tok.muted, 1, pg.as_str());
                draw_btn(
                    gfx,
                    &tok,
                    UiRect::new(grid_box.right() - bw, py, bw, ph),
                    if self.page + 1 < total_pages { BtnKind::Secondary } else { BtnKind::Disabled },
                    self.pressed_id == 5,
                    vp.d(8).max(7),
                    th.tr("Sonraki", "Next"),
                );
            }
        }

        // Bottom bar: either multi-select batch bar OR quiet 4-tab bottom navigation
        if self.select_mode {
            let h = vp.bottom_nav_h();
            let y = vp.area_h - h;
            gfx.fill_rect(0, y, vp.area_w, h, tok.surface);
            gfx.fill_rect(0, y, vp.area_w, 1, tok.divider);
            let bh = vp.d(36).clamp(30, h - vp.d(10));
            let by = y + (h - bh) / 2;
            let gap = vp.d(8);
            let bw = (vp.content_w - gap * 2) / 3;
            let cap = vp.d(8).max(7);
            draw_btn(gfx, &tok, UiRect::new(vp.content_x, by, bw, bh), BtnKind::Primary, self.pressed_id == 6, cap, th.tr("Favorile", "Favorite"));
            draw_btn(gfx, &tok, UiRect::new(vp.content_x + bw + gap, by, bw, bh), BtnKind::Secondary, self.pressed_id == 7, cap, th.tr("Kaldır", "Unfav"));
            draw_btn(gfx, &tok, UiRect::new(vp.content_x + (bw + gap) * 2, by, bw, bh), BtnKind::Quiet, self.pressed_id == 8, cap, th.tr("Bitti", "Done"));
        } else if !self.search_active {
            let nav_items: [(UiIcon, &str); 4] = [
                (UiIcon::Photo, th.tr("Fotoğraflar", "Photos")),
                (UiIcon::Star, th.tr("Favoriler", "Favorites")),
                (UiIcon::Clock, th.tr("Son", "Recent")),
                (UiIcon::Search, th.tr("Ara", "Search")),
            ];
            let active_i = match self.tab {
                NavTab::Photos => 0,
                NavTab::Favorites => 1,
                NavTab::Recent => 2,
                NavTab::Search => 3,
            };
            draw_bottom_nav(gfx, &vp, &tok, &nav_items, active_i, self.pressed_nav);
        }

        if self.toast.active {
            draw_toast(gfx, &vp, &tok, top.bar.bottom() + vp.d(8), self.toast.color, self.toast.text());
        }
    }

    fn draw_viewer(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens) {
        // Immersive pure-dark viewer stage so photo colors read accurately
        let dark_tok = SurfaceTokens::camera_black(tok.accent);
        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, 0x0005_070A);

        let Some(&pidx) = self.filtered.get(self.viewer_pos) else { return };
        let photo = &self.photos[pidx as usize];
        let fav = self.is_favorite(photo.name_str());

        let cp = self.chrome_tween.value().clamp(0, ONE);
        if cp > 16 {
            let top = TopBarGeo::compute(vp, 36);
            let slide_top = (ONE - cp) * top.bar.h / ONE;
            let bar_y = -slide_top;
            gfx.panel(0, bar_y, vp.area_w, top.bar.h, 0, 0x000A_0D12, (230 * cp / ONE) as u32, 0, 0);
            let back_r = UiRect::new(top.back_btn.x, top.back_btn.y + bar_y, top.back_btn.w, top.back_btn.h);
            let info_r = UiRect::new(top.trailing_btn.x, top.trailing_btn.y + bar_y, top.trailing_btn.w, top.trailing_btn.h);
            draw_icon_btn(gfx, &dark_tok, back_r, UiIcon::Back, false, false, self.pressed_id == 20);
            draw_icon_btn(gfx, &dark_tok, info_r, UiIcon::Info, self.info_open, false, self.pressed_id == 21);

            let cap = vp.d(11).clamp(9, 14);
            let mut nb = [0u8; 40];
            let max_w = (info_r.x - back_r.right() - vp.d(16)).max(48);
            let shown = truncate_to_px(photo.name_str(), max_w, cap, &mut nb);
            gfx.text(back_r.right() + vp.d(10), bar_y + (top.bar.h - cap) / 2, cap, WHITE, 2, shown);

            // Bottom viewer toolbar (Prev, Favorite, Index, Next)
            let bot_h = vp.d(60).clamp(50, 68);
            let slide_bot = (ONE - cp) * bot_h / ONE;
            let by = vp.area_h - bot_h + slide_bot;
            gfx.panel(0, by, vp.area_w, bot_h, 0, 0x000A_0D12, (230 * cp / ONE) as u32, 0, 0);
            let bs = vp.d(40).clamp(34, 46);
            let cy = by + (bot_h - bs) / 2;
            let prev_r = UiRect::new(vp.content_x, cy, bs, bs);
            let fav_r = UiRect::new(vp.area_w / 2 - bs / 2, cy, bs, bs);
            let next_r = UiRect::new(vp.content_x + vp.content_w - bs, cy, bs, bs);
            draw_icon_btn(gfx, &dark_tok, prev_r, UiIcon::ChevronLeft, false, self.viewer_pos == 0, self.pressed_id == 22);
            draw_icon_btn(
                gfx,
                &dark_tok,
                fav_r,
                if fav { UiIcon::StarFilled } else { UiIcon::Star },
                false,
                false,
                self.pressed_id == 23,
            );
            draw_icon_btn(
                gfx,
                &dark_tok,
                next_r,
                UiIcon::ChevronRight,
                false,
                self.viewer_pos + 1 >= self.filtered_count,
                self.pressed_id == 24,
            );
        }

        // Animated Image Info sheet (real metadata only: name, format, dimensions, file size, favorite)
        let ip = self.info_tween.value().clamp(0, ONE);
        if ip > 8 {
            let sheet_w = vp.content_w.min(vp.d(360));
            let sheet_x = (vp.area_w - sheet_w) / 2;
            let sheet_h = vp.d(214).clamp(186, vp.area_h - vp.d(40));
            let slide = (ONE - ip) * (sheet_h + vp.d(20)) / ONE;
            let sy = vp.area_h - sheet_h - vp.d(12) + slide;
            gfx.panel(0, 0, vp.area_w, vp.area_h, 0, 0x0000_0000, (140 * ip / ONE) as u32, 0, 0);
            gfx.panel(sheet_x, sy, sheet_w, sheet_h, vp.d(20), tok.elevated, (252 * ip / ONE) as u32, tok.border, tok.border_alpha);

            let pad = vp.d(18);
            let cap = vp.d(11).clamp(10, 14);
            let row_cap = vp.d(9).clamp(8, 11);
            gfx.text(sheet_x + pad, sy + pad, cap, tok.ink, 2, th.tr("Görsel Bilgileri", "Image Details"));
            let close_s = vp.d(30).clamp(26, 36);
            let close_r = UiRect::new(sheet_x + sheet_w - pad - close_s, sy + pad - vp.d(4), close_s, close_s);
            draw_icon_btn(gfx, tok, close_r, UiIcon::Close, false, false, self.pressed_id == 25);

            let mut dim_l = Line::new();
            if photo.width > 0 && photo.height > 0 {
                dim_l.number(photo.width, 1).text(" × ").number(photo.height, 1).text(" px");
            } else {
                dim_l.text("-");
            }
            let mut size_l = Line::new();
            let kb = (photo.size_bytes / 1024).max(1);
            if kb >= 1024 {
                size_l.number(kb / 1024, 1).text(".").number((kb % 1024) * 10 / 1024, 1).text(" MB");
            } else {
                size_l.number(kb, 1).text(" KB");
            }
            let mut name_buf = [0u8; 36];
            let short_name = truncate_to_px(photo.name_str(), sheet_w / 2 - pad, row_cap, &mut name_buf);

            let rows: [(&str, &str); 5] = [
                (th.tr("Dosya", "Filename"), short_name),
                (th.tr("Biçim", "Format"), photo.format_str()),
                (th.tr("Çözünürlük", "Dimensions"), dim_l.as_str()),
                (th.tr("Boyut", "File Size"), size_l.as_str()),
                (th.tr("Favori", "Favorite"), if fav { th.tr("Evet", "Yes") } else { th.tr("Hayır", "No") }),
            ];
            let mut ry = sy + pad + cap + vp.d(16);
            let step = vp.d(28).clamp(22, 32);
            for (lbl, val) in rows {
                gfx.text(sheet_x + pad, ry, row_cap, tok.muted, 1, lbl);
                gfx.text(sheet_x + sheet_w * 44 / 100, ry, row_cap, tok.ink, 2, val);
                ry += step;
            }
        }

        if self.toast.active {
            draw_toast(gfx, vp, &dark_tok, vp.header_h() + vp.d(8), self.toast.color, self.toast.text());
        }
    }

    fn handle_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> u32 {
        let vp = Viewport::from_event(ev);
        let en = ev.lang == 1;
        let pad = vp.d(6);

        if self.perm_state != PermState::Granted {
            if self.perm_state == PermState::Denied {
                let _ = perm::open_settings();
            } else {
                let _ = perm::request(PERM_MEDIA_PHOTOS_READ);
            }
            return RET_REDRAW;
        }
        if !self.hw_avail {
            let (cols, rows, _, _, _, _) = grid_metrics(&vp, true);
            self.sync_permission(false, cols * rows);
            return RET_REDRAW;
        }

        if self.view == View::Viewer {
            if self.info_open {
                let sheet_w = vp.content_w.min(vp.d(360));
                let sheet_x = (vp.area_w - sheet_w) / 2;
                let sheet_h = vp.d(214).clamp(186, vp.area_h - vp.d(40));
                let sy = vp.area_h - sheet_h - vp.d(12);
                let close_s = vp.d(30).clamp(26, 36);
                let close_r = UiRect::new(sheet_x + sheet_w - vp.d(18) - close_s, sy + vp.d(14), close_s, close_s);
                if close_r.expand(pad).contains(x, y) || y < sy {
                    self.info_open = false;
                    self.update_viewer_preview(ev);
                    return RET_REDRAW | RET_ANIMATING;
                }
                return RET_NONE;
            }

            if self.chrome_visible {
                let top = TopBarGeo::compute(&vp, 36);
                if top.back_btn.expand(pad).contains(x, y) {
                    media::clear_preview();
                    self.view = View::Grid;
                    return RET_REDRAW;
                }
                if top.trailing_btn.expand(pad).contains(x, y) {
                    self.info_open = true;
                    media::clear_preview(); // Keep sheet crisp above stage
                    return RET_REDRAW | RET_ANIMATING;
                }
                let bot_h = vp.d(60).clamp(50, 68);
                let by = vp.area_h - bot_h;
                let bs = vp.d(40).clamp(34, 46);
                let cy = by + (bot_h - bs) / 2;
                let prev_r = UiRect::new(vp.content_x, cy, bs, bs);
                let fav_r = UiRect::new(vp.area_w / 2 - bs / 2, cy, bs, bs);
                let next_r = UiRect::new(vp.content_x + vp.content_w - bs, cy, bs, bs);
                if prev_r.expand(pad).contains(x, y) && self.viewer_pos > 0 {
                    let next_pos = self.viewer_pos - 1;
                    self.open_viewer_at(next_pos, ev);
                    return RET_REDRAW;
                }
                if next_r.expand(pad).contains(x, y) && self.viewer_pos + 1 < self.filtered_count {
                    let next_pos = self.viewer_pos + 1;
                    self.open_viewer_at(next_pos, ev);
                    return RET_REDRAW;
                }
                if fav_r.expand(pad).contains(x, y) {
                    if let Some(&pidx) = self.filtered.get(self.viewer_pos) {
                        let mut nb = [0u8; 44];
                        let nl = copy_str(&mut nb, self.photos[pidx as usize].name_str());
                        let name = s_of(&nb, nl);
                        let now_fav = !self.is_favorite(name);
                        self.set_favorite(name, now_fav);
                        self.toast.show(
                            ev.uptime_ms,
                            if now_fav { AMBER } else { 0x0096_A1B2 },
                            if now_fav {
                                if en { "Added to Favorites" } else { "Favorilere eklendi" }
                            } else if en {
                                "Removed from Favorites"
                            } else {
                                "Favorilerden çıkarıldı"
                            },
                        );
                        return RET_REDRAW;
                    }
                }
            }

            // Tapping the photo stage toggles minimal overlay chrome
            self.chrome_visible = !self.chrome_visible;
            self.update_viewer_preview(ev);
            return RET_REDRAW | RET_ANIMATING;
        }

        // Grid view clicks
        let top = TopBarGeo::compute(&vp, 36);
        let (cols, rows, grid_box, tile_w, tile_h, gap) = grid_metrics(&vp, true);
        let page_cap = (cols * rows).min(MAX_PAGE_TILES);

        if top.trailing_btn.expand(pad).contains(x, y) && self.filtered_count > 0 {
            self.select_mode = !self.select_mode;
            self.selected_mask = 0;
            return RET_REDRAW;
        }
        let sort_btn = UiRect::new(top.trailing_btn.x - vp.d(56), top.trailing_btn.y, vp.d(50), top.trailing_btn.h);
        if self.tab != NavTab::Recent && sort_btn.expand(pad).contains(x, y) {
            self.sort = self.sort.next();
            self.rebuild_filtered();
            self.load_page_thumbs(page_cap);
            return RET_REDRAW;
        }

        // Subbar clicks (Search or Refresh)
        let sub_y = top.bar.bottom() + vp.d(6);
        let sub_h = vp.d(32).clamp(28, 38);
        if self.tab == NavTab::Search {
            let sr = UiRect::new(vp.content_x, sub_y, vp.content_w, sub_h);
            let clear_r = UiRect::new(sr.right() - sr.h, sr.y, sr.h, sr.h);
            if self.search_len > 0 && clear_r.expand(pad).contains(x, y) {
                self.search_len = 0;
                self.search_active = false;
                let _ = ime::hide();
                self.rebuild_filtered();
                self.load_page_thumbs(page_cap);
                return RET_REDRAW;
            }
            if sr.contains(x, y) {
                self.search_active = true;
                let _ = ime::show();
                return RET_REDRAW;
            }
        } else {
            let ref_r = UiRect::new(vp.content_x + vp.content_w - vp.d(68), sub_y + vp.d(2), vp.d(68), sub_h - vp.d(4));
            if ref_r.expand(pad).contains(x, y) {
                self.reload_photos(page_cap);
                self.toast.show(ev.uptime_ms, 0x0010_B981, if en { "Library refreshed" } else { "Galeri yenilendi" });
                return RET_REDRAW;
            }
        }

        // Bottom bar: batch selection OR 4-tab nav
        if self.select_mode {
            let h = vp.bottom_nav_h();
            let by = vp.area_h - h;
            if y >= by {
                let bh = vp.d(36).clamp(30, h - vp.d(10));
                let btop = by + (h - bh) / 2;
                let g = vp.d(8);
                let bw = (vp.content_w - g * 2) / 3;
                if UiRect::new(vp.content_x, btop, bw, bh).expand(pad).contains(x, y) {
                    let mut changed = 0u32;
                    for i in 0..self.photo_count {
                        if ((self.selected_mask >> i) & 1) != 0 {
                            let mut nb = [0u8; 44];
                            let nl = copy_str(&mut nb, self.photos[i].name_str());
                            if self.set_favorite(s_of(&nb, nl), true) {
                                changed += 1;
                            }
                        }
                    }
                    self.select_mode = false;
                    self.selected_mask = 0;
                    if changed > 0 {
                        self.toast.show(ev.uptime_ms, AMBER, if en { "Added to Favorites" } else { "Favorilere eklendi" });
                    }
                    return RET_REDRAW;
                }
                if UiRect::new(vp.content_x + bw + g, btop, bw, bh).expand(pad).contains(x, y) {
                    for i in 0..self.photo_count {
                        if ((self.selected_mask >> i) & 1) != 0 {
                            let mut nb = [0u8; 44];
                            let nl = copy_str(&mut nb, self.photos[i].name_str());
                            self.set_favorite(s_of(&nb, nl), false);
                        }
                    }
                    self.select_mode = false;
                    self.selected_mask = 0;
                    self.rebuild_filtered();
                    self.load_page_thumbs(page_cap);
                    self.toast.show(ev.uptime_ms, 0x0096_A1B2, if en { "Updated Favorites" } else { "Favoriler güncellendi" });
                    return RET_REDRAW;
                }
                self.select_mode = false;
                self.selected_mask = 0;
                return RET_REDRAW;
            }
        } else if !self.search_active {
            if let Some(idx) = hit_bottom_nav(&vp, 4, x, y) {
                let next_tab = match idx {
                    0 => NavTab::Photos,
                    1 => NavTab::Favorites,
                    2 => NavTab::Recent,
                    _ => NavTab::Search,
                };
                if next_tab != self.tab {
                    let _ = ime::hide();
                    self.search_active = false;
                    self.tab = next_tab;
                    self.rebuild_filtered();
                    self.load_page_thumbs(page_cap);
                    return RET_REDRAW;
                }
                return RET_NONE;
            }
        }

        // Empty state Refresh action
        if self.filtered_count == 0 {
            if (self.load_error || self.tab == NavTab::Photos) && grid_box.contains(x, y) {
                self.reload_photos(page_cap);
                return RET_REDRAW;
            }
            return RET_NONE;
        }

        // Photo tiles
        let start = self.page * page_cap;
        for slot in 0..page_cap {
            let fpos = start + slot;
            if fpos >= self.filtered_count {
                break;
            }
            let col = (slot % cols) as i32;
            let row = (slot / cols) as i32;
            let r = UiRect::new(
                grid_box.x + col * (tile_w + gap),
                grid_box.y + row * (tile_h + gap),
                tile_w,
                tile_h,
            );
            if r.contains(x, y) {
                let pidx = self.filtered[fpos] as usize;
                if self.select_mode {
                    self.selected_mask ^= 1u64 << pidx;
                } else {
                    let _ = ime::hide();
                    self.search_active = false;
                    self.open_viewer_at(fpos, ev);
                }
                return RET_REDRAW;
            }
        }

        // Pager
        let total_pages = (self.filtered_count + page_cap - 1) / page_cap;
        if total_pages > 1 {
            let py = grid_box.bottom() + vp.d(4);
            let ph = vp.d(30).clamp(26, 36);
            let bw = vp.d(84).clamp(72, 100);
            if UiRect::new(grid_box.x, py, bw, ph).expand(pad).contains(x, y) && self.page > 0 {
                self.page -= 1;
                self.load_page_thumbs(page_cap);
                return RET_REDRAW;
            }
            if UiRect::new(grid_box.right() - bw, py, bw, ph).expand(pad).contains(x, y) && self.page + 1 < total_pages {
                self.page += 1;
                self.load_page_thumbs(page_cap);
                return RET_REDRAW;
            }
        }

        RET_NONE
    }
}

impl RingApp for GalleryApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.last_accent = ev.accent;
                self.last_lang = ev.lang;
                if let Some(st) = status::query() {
                    self.reduced_motion = st.reduced_motion();
                }
                self.load_sandbox();
                let vp = Viewport::from_event(ev);
                let (cols, rows, _, _, _, _) = grid_metrics(&vp, true);
                self.sync_permission(true, cols * rows);
                if self.view == View::Viewer && self.perm_state == PermState::Granted {
                    self.update_viewer_preview(ev);
                }
                RET_REDRAW
            }
            OP_PAUSE => {
                media::clear_preview();
                let _ = ime::hide();
                self.search_active = false;
                RET_NONE
            }
            OP_UPDATE => {
                let mut ret = RET_NONE;
                if ev.flags != self.last_flags || ev.accent != self.last_accent || ev.lang != self.last_lang {
                    self.last_flags = ev.flags;
                    self.last_accent = ev.accent;
                    self.last_lang = ev.lang;
                    ret |= RET_REDRAW;
                }
                if self.toast.tick(ev.uptime_ms) {
                    ret |= RET_REDRAW;
                }
                let prev = self.perm_state;
                let st = perm::check(PERM_MEDIA_PHOTOS_READ);
                if st.state != prev || st.hw_available != self.hw_avail {
                    let vp = Viewport::from_event(ev);
                    let (cols, rows, _, _, _, _) = grid_metrics(&vp, true);
                    self.sync_permission(false, cols * rows);
                    ret |= RET_REDRAW;
                }

                // Keyboard input (Escape + Search typing + Viewer Left/Right arrows)
                if ev.key == key::ESCAPE {
                    let _ = ime::hide();
                    self.search_active = false;
                    if self.info_open {
                        self.info_open = false;
                        self.update_viewer_preview(ev);
                        return RET_REDRAW | RET_ANIMATING;
                    }
                    if self.view == View::Viewer {
                        media::clear_preview();
                        self.view = View::Grid;
                        return RET_REDRAW;
                    }
                    if self.select_mode {
                        self.select_mode = false;
                        self.selected_mask = 0;
                        return RET_REDRAW;
                    }
                    media::clear_preview();
                    return RET_CLOSE;
                }
                if self.view == View::Grid && self.tab == NavTab::Search && self.search_active {
                    let vp = Viewport::from_event(ev);
                    let (cols, rows, _, _, _, _) = grid_metrics(&vp, true);
                    if ev.key == key::BACKSPACE {
                        pop_char(&self.search, &mut self.search_len);
                        self.rebuild_filtered();
                        self.load_page_thumbs(cols * rows);
                        return RET_REDRAW;
                    }
                    if ev.key == key::ENTER {
                        self.search_active = false;
                        let _ = ime::hide();
                        return RET_REDRAW;
                    }
                    if let Some(c) = key::as_char(ev.key) {
                        if !c.is_control() {
                            push_char(&mut self.search, &mut self.search_len, c);
                            self.rebuild_filtered();
                            self.load_page_thumbs(cols * rows);
                            return RET_REDRAW;
                        }
                    }
                }
                if let Some(p) = pointer(ev) {
                    let vp = Viewport::from_event(ev);
                    let new_nav = if p.down && self.view == View::Grid && !self.select_mode && !self.search_active {
                        hit_bottom_nav(&vp, 4, p.x, p.y)
                    } else {
                        None
                    };
                    if new_nav != self.pressed_nav {
                        self.pressed_nav = new_nav;
                        ret |= RET_REDRAW;
                    }
                    if p.clicked {
                        self.pressed_nav = None;
                        self.pressed_id = 0;
                        ret |= self.handle_click(ev, p.x, p.y);
                    }
                }
                if self.step_animations(ev.uptime_ms) {
                    ret |= RET_REDRAW | RET_ANIMATING;
                }
                ret
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: GalleryApp = GalleryApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
