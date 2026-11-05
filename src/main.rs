#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod brightness;
mod composition;
mod theme;
mod mixer;
mod monitors;
mod autostart;
mod crashlog;
mod gfx;
mod keyboard;
mod lyrics;
mod media;
mod modules;
mod network;
mod occlusion;
mod privacy;
mod power;
mod screenshots;
mod settings;
mod system;
mod tables;
mod time;
mod tray;
mod uninstall;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use anim::Spring;
use gfx::{Align, Color, Gfx, Rect, DIM};
use modules::{
    clipboard, clipboard::Clipboard, clock::ClockModule, media::MediaModule, schedule::Schedule,
    settings_page::SettingsPage, Poll, osd::Osd, pomodoro::Pomodoro, shelf, shelf::Shelf, todo::Todo,
    water::Water, Event, Module,
};
use settings::Settings;

/// Ukuran kanvas jendela (DIP). Notch digambar di tengah atasnya; sisanya transparan.
const CANVAS_W: f32 = 760.0;
const CANVAS_H: f32 = 800.0;
/// Tinggi baris ikon halaman di bawah notch yang terbuka.
const TAB_H: f32 = theme::NAV_ROW;
const MIN_OPEN_W: f32 = 360.0;
const BADGE_W: f32 = 64.0;
/// Lebar per titik indikator mic/kamera di notch tertutup.
const DOT_W: f32 = 12.0;
const MIC: Color = Color(1.0, 0.62, 0.2, 1.0);
const CAM: Color = Color(0.3, 0.85, 0.45, 1.0);
/// Tinggi garis notch dalam mode minimal.
const LINE_H: f32 = 5.0;

const WM_APP_MEDIA: u32 = WM_APP + 1;
const WM_APP_LYRICS: u32 = WM_APP + 2;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_ANIM: usize = 1;
const TIMER_TICK: usize = 2;
const TIMER_POLL: usize = 3;
const TIMER_ALERT: usize = 4;
/// Saat ada jendela di posisi notch: cek posisi kursor untuk menyingkirkan notch.
const TIMER_DODGE: usize = 6;
/// Status jaringan dibaca setelah event berhenti beruntun (bisa naik-turun sebentar).
const TIMER_NET: usize = 7;
const NET_SETTLE_MS: u32 = 2000;
const LOW_BATTERY: [u32; 2] = [20, 10];
/// Proses clipboard sedikit setelah berubah, supaya aplikasi penyalin selesai menulis
/// dulu dan tidak terganggu oleh Amora yang ikut membuka clipboard.
const TIMER_CLIP: usize = 8;
const CLIP_SETTLE_MS: u32 = 150;
const DODGE_MS: u32 = 80;
/// Area (DIP) di sekitar notch: kursor di sini berarti mengarah ke tab di belakang notch.
const DODGE_MARGIN_X: f32 = EAR_R + 4.0;
const DODGE_MARGIN_Y: f32 = 4.0;
/// Lama notch terbuka sendiri saat ada pengingat.
const ALERT_MS: u32 = 7000;
/// Kursor harus diam sebentar di atas notch sebelum ia terbuka, supaya tidak
/// terbuka sendiri saat kursor hanya lewat.
const TIMER_HOVER: usize = 9;
const TIMER_NAV_HOVER: usize = 12;
const HOVER_DELAY_MS: u32 = 150;
/// Jari-jari "telinga" cekung yang menyambungkan notch ke tepi atas layar.
const EAR_R: f32 = 8.0;
const GRIP_H: f32 = 12.0;

struct NotchDrag {
    origin: POINT,
    anchor: POINT,
}

struct App {
    monitor: Option<monitors::Monitor>,
    notch_drag: Option<NotchDrag>,
    pointer_page: Option<usize>,
    suppress_up: bool,
    hwnd: HWND,
    g: Gfx,
    origin: POINT,
    modules: Vec<Box<dyn Module>>,
    /// Modul yang tampil saat notch tertutup.
    current: usize,
    /// Halaman yang tampil saat notch terbuka.
    page: usize,
    hover: bool,
    /// Kursor baru masuk; notch dibuka bila masih di sana setelah `HOVER_DELAY_MS`.
    hover_pending: bool,
    /// Previous panel bounds keep a navigation click alive while the panel shrinks.
    navigation_hover: Option<Rect>,
    /// Notch dibuka sendiri oleh pengingat.
    alert: bool,
    /// Keyboard sedang diarahkan ke halaman (mis. mengetik tugas).
    editing: bool,
    tab_hits: Vec<(Rect, usize)>,
    /// File sedang diseret dari luar ke atas notch.
    drop_hover: bool,
    /// Titik tombol kiri ditekan (DIP), untuk mendeteksi seret keluar.
    press: Option<(f32, f32)>,
    /// File yang akan diseret keluar setelah `handle` selesai (di luar pinjaman APP).
    pending_drag: Option<PathBuf>,
    audio: Option<system::AudioWatch>,
    /// Notifikasi kecerahan pertama hanyalah nilai awal, bukan perubahan.
    brightness_ready: bool,
    /// Status terakhir untuk notifikasi: hanya perubahan yang ditampilkan.
    charger: Option<bool>,
    battery: Option<u32>,
    /// Batas baterai lemah terendah yang sudah diperingatkan sejak terakhir dicas.
    battery_warned: u32,
    net: network::NetState,
    settings: settings::SharedSettings,
    /// Sedang menerima WM_CLIPBOARDUPDATE.
    clip_listening: bool,
    /// Aplikasi yang sedang memakai mikrofon/kamera.
    in_use: privacy::InUse,
    auto_hide: bool,
    /// Ada jendela lain di posisi notch.
    covered: bool,
    /// Notch minggir karena kursor mengarah ke jendela di belakangnya.
    dodge: bool,
    w: Spring,
    h: Spring,
    last: Instant,
    animating: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Sedang menyeret file keluar dari notch; jangan terima sebagai drop ke diri sendiri.
    static SELF_DRAG: Cell<bool> = const { Cell::new(false) };
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|a| {
        if let Ok(mut a) = a.try_borrow_mut() {
            if let Some(app) = a.as_mut() {
                f(app);
            }
        }
    });
}

impl App {
    fn active_index(&self) -> usize {
        self.modules.iter().position(|m| m.active()).unwrap_or(self.modules.len() - 1)
    }

    fn tabs(&self) -> Vec<usize> {
        (0..self.modules.len()).filter(|&i| self.modules[i].tab().is_some()).collect()
    }

    fn is_open(&self) -> bool {
        self.notch_drag.is_some() || self.pointer_page.is_some() || self.navigation_hover.is_some()
            || self.hover || self.alert || self.editing || self.drop_hover || force_open()
    }

    /// Info kecil dari modul lain untuk notch tertutup.
    fn badge(&self) -> Option<(usize, String, Color)> {
        (0..self.modules.len())
            .filter(|&i| i != self.current)
            .find_map(|i| self.modules[i].badge().map(|(s, c)| (i, s, c)))
    }

    /// Notch disembunyikan: ada jendela di belakangnya, kursor mengarah ke sana,
    /// dan tidak ada yang perlu ditampilkan.
    fn hidden(&self) -> bool {
        self.auto_hide && self.covered && self.dodge && !self.is_open() && !self.modules[self.current].urgent()
    }

    /// Kotak notch tertutup di layar (piksel).
    fn compact_screen_rect(&self, margin_x: f32, margin_y: f32) -> RECT {
        let (w, h) = self.compact_size();
        let s = self.g.scale;
        let left = self.origin.x + (((CANVAS_W - w) / 2.0 - margin_x) * s) as i32;
        RECT {
            left,
            top: self.origin.y,
            right: left + ((w + margin_x * 2.0) * s) as i32,
            bottom: self.origin.y + ((h + margin_y) * s) as i32,
        }
    }

    fn recheck_covered(&mut self) {
        let covered = self.auto_hide && occlusion::covered(self.hwnd, self.compact_screen_rect(0.0, 0.0));
        // Minimal mode needs pointer polling even on an empty desktop: its
        // transparent reveal band cannot receive WM_MOUSEMOVE itself.
        unsafe {
            if covered || self.settings.borrow().minimal {
                SetTimer(self.hwnd, TIMER_DODGE, DODGE_MS, None);
            } else {
                let _ = KillTimer(self.hwnd, TIMER_DODGE);
            }
        }
        if covered == self.covered {
            return;
        }
        self.covered = covered;
        if self.hover { self.track_leave(); }
        self.dodge = false;
        self.check_dodge();
        self.retarget();
    }

    /// Kursor berada di area notch tapi tidak di tepi paling atas layar:
    /// pengguna mengarah ke tab/jendela di belakang notch.
    fn cursor_wants_behind(&self) -> bool {
        let p = occlusion::cursor();
        let z = self.compact_screen_rect(DODGE_MARGIN_X, DODGE_MARGIN_Y);
        let inside = p.x >= z.left && p.x < z.right && p.y >= z.top && p.y < z.bottom;
        inside && !self.cursor_over_notch()
    }

    fn cursor_over_notch(&self) -> bool {
        let r = self.island();
        let s = self.g.scale;
        let visible = RECT { left: self.origin.x + (r.x * s).round() as i32,
            top: self.origin.y + (r.y * s).round() as i32,
            right: self.origin.x + ((r.x + r.w) * s).round() as i32,
            bottom: self.origin.y + ((r.y + r.h) * s).round() as i32 };
        let edge_only = self.as_line() || (self.covered && !self.is_open());
        occlusion::opening_target(occlusion::cursor(), visible, self.compact_screen_rect(0.0, 0.0), edge_only)
    }

    fn queue_hover(&mut self) {
        if !self.hover && !self.hover_pending {
            self.hover_pending = true;
            self.dodge = false;
            self.track_leave();
            unsafe { SetTimer(self.hwnd, TIMER_HOVER, HOVER_DELAY_MS, None); }
        }
    }

    fn navigate_to(&mut self, page: usize) {
        if self.page == page { return; }
        self.navigation_hover = Some(self.island());
        self.page = page;
        unsafe { SetTimer(self.hwnd, TIMER_NAV_HOVER, DODGE_MS, None); }
    }

    fn check_navigation_hover(&mut self) {
        let Some(previous) = self.navigation_hover else { return; };
        let cursor = occlusion::cursor();
        let x = (cursor.x - self.origin.x) as f32 / self.g.scale;
        let y = (cursor.y - self.origin.y) as f32 / self.g.scale;
        match occlusion::navigation_hover(previous, self.island(), x, y, self.animating) {
            occlusion::NavigationHover::Hold => return,
            occlusion::NavigationHover::Entered => self.start_hover(),
            occlusion::NavigationHover::Left => self.hover = false,
        }
        self.navigation_hover = None;
        unsafe { let _ = KillTimer(self.hwnd, TIMER_NAV_HOVER); }
        self.retarget();
        self.render();
    }

    /// Selama ada jendela di belakang notch: minggir saat kursor mengarah ke sana.
    fn check_dodge(&mut self) {
        if (self.covered || self.settings.borrow().minimal) && self.hover && !self.cursor_over_notch()
            && self.navigation_hover.is_none() && self.notch_drag.is_none() && self.pointer_page.is_none() {
            self.hover = false;
            self.alert = false;
            self.retarget();
        }
        if self.hover_pending && !self.cursor_over_notch() {
            self.hover_pending = false;
            unsafe { let _ = KillTimer(self.hwnd, TIMER_HOVER); }
        }
        // When covered, only the top edge expresses intent to open Amora.
        // Everywhere else in the compact slot belongs to the app behind it.
        if (self.covered || self.settings.borrow().minimal) && !self.is_open() && self.cursor_over_notch() { self.queue_hover(); }
        let dodge = self.covered && !self.is_open() && !self.hover_pending && self.cursor_wants_behind();
        if dodge != self.dodge {
            self.dodge = dodge;
            self.retarget();
        }
    }

    /// Mode minimal: notch tertutup hanya garis tipis, kecuali ada lirik atau notifikasi.
    fn as_line(&self) -> bool {
        let m = &self.modules[self.current];
        self.settings.borrow().minimal && !self.is_open() && !m.urgent() && !m.wants_space()
    }

    fn dots(&self) -> Vec<Color> {
        let mut d = Vec::new();
        if !self.in_use.mic.is_empty() {
            d.push(MIC);
        }
        if !self.in_use.cam.is_empty() {
            d.push(CAM);
        }
        d
    }

    fn dots_w(&self) -> f32 {
        let n = self.dots().len();
        if n == 0 { 0.0 } else { 6.0 + n as f32 * DOT_W }
    }

    fn compact_size(&self) -> (f32, f32) {
        let (w, h) = self.modules[self.current].compact_size();
        if self.as_line() {
            return (w, LINE_H);
        }
        (w + if self.badge().is_some() { BADGE_W } else { 0.0 } + self.dots_w(), h)
    }

    fn open_size(&self) -> (f32, f32) {
        let (w, h) = self.modules[self.page].expanded_size();
        let max_w = self.monitor.as_ref().map_or(CANVAS_W - 16.0, |m| (m.rect.right - m.rect.left) as f32 / self.g.scale - 16.0);
        let max_h = self.g.px_h as f32 / self.g.scale - 4.0;
        let nav_width = self.tabs().len() as f32 * theme::NAV_CELL + 24.0;
        let width = w.max(MIN_OPEN_W).max(456.0).max(nav_width).min(max_w).min(CANVAS_W - 16.0);
        let nav = theme::nav_rows(width, self.tabs().len()) as f32 * TAB_H + 10.0 + theme::NAV_GAP;
        (width, (h + nav + GRIP_H).min(max_h))
    }

    fn retarget(&mut self) {
        self.current = self.active_index();
        let (w, h) = if self.is_open() {
            self.open_size()
        } else if self.hidden() {
            // Naik keluar layar.
            (self.compact_size().0, 0.0)
        } else {
            self.compact_size()
        };
        self.w.set(w);
        self.h.set(h);
        if !self.animating {
            self.animating = true;
            self.last = Instant::now();
            unsafe { SetTimer(self.hwnd, TIMER_ANIM, 15, None) };
        }
    }

    /// Jadwalkan pengecekan pengingat berikutnya dari semua modul.
    fn schedule_poll(&self) {
        let next = self.modules.iter().filter_map(|m| m.poll_in()).min();
        unsafe {
            match next {
                Some(ms) => {
                    SetTimer(self.hwnd, TIMER_POLL, ms.max(50), None);
                }
                None => {
                    let _ = KillTimer(self.hwnd, TIMER_POLL);
                }
            }
        }
    }

    fn poll(&mut self) {
        // Do not replace the page underneath a captured slider or monitor drag.
        if self.pointer_page.is_some() || self.notch_drag.is_some() {
            unsafe { SetTimer(self.hwnd, TIMER_POLL, 200, None); }
            return;
        }
        let mut changed = false;
        for i in 0..self.modules.len() {
            match self.modules[i].poll() {
                Poll::Alert if self.modules[i].tab().is_some() => {
                    self.page = i;
                    self.alert = true;
                    unsafe { SetTimer(self.hwnd, TIMER_ALERT, ALERT_MS, None) };
                    changed = true;
                }
                Poll::Idle => {}
                _ => changed = true,
            }
        }
        self.schedule_poll();
        if changed {
            self.retarget();
            self.render();
        }
    }

    /// Resolve the saved device identity again on every display topology change.
    fn place(&mut self) {
        let list = monitors::all();
        let wanted = self.settings.borrow().monitor.clone();
        let Some(monitor) = monitors::choose(&list, wanted.as_deref()).cloned() else { return };
        {
            let scale = monitor.scale;
            let pw = (CANVAS_W * scale).round() as i32;
            let ph = monitor.rect.bottom - monitor.rect.top;
            self.origin = monitors::top_center(monitor.rect, pw);
            if dev_instance() {
                // `AMORA_DEV_X=<piksel>` menggeser instance uji ke tempat lain (layar sempit).
                let dx = std::env::var("AMORA_DEV_X").ok().and_then(|v| v.parse().ok());
                self.origin.x += dx.unwrap_or(pw * 3 / 4);
            }
            if pw != self.g.px_w || ph != self.g.px_h || scale != self.g.scale {
                let _ = self.g.resize(pw, ph, scale);
            }
        }
        self.monitor = Some(monitor.clone());
        for module in &mut self.modules { module.on_event(&Event::MonitorChanged(monitor.clone())); }
    }

    fn grip(&self) -> Rect {
        let island = self.island();
        Rect::new(island.x + island.w / 2.0 - 28.0, 0.0, 56.0, GRIP_H)
    }

    fn finish_notch_drag(&mut self, commit: bool) {
        let Some(drag) = self.notch_drag.take() else { return };
        self.suppress_up = !commit;
        if commit {
            let mut pt = POINT::default();
            unsafe { let _ = GetCursorPos(&mut pt); }
            if let Some(monitor) = monitors::at_point(&monitors::all(), pt) {
                let mut settings = self.settings.borrow_mut();
                settings.monitor = Some(monitor.id.clone());
                settings.save();
            }
            self.place();
        } else { self.origin = drag.origin; }
        self.set_editing(false);
        unsafe { let _ = ReleaseCapture(); }
        self.sync_hover_after_capture();
        self.recheck_covered();
        self.retarget();
        self.render();
    }

    fn sync_hover_after_capture(&mut self) {
        let mut pt = POINT::default();
        unsafe { let _ = GetCursorPos(&mut pt); }
        self.hover_pending = false;
        self.hover = self.island().contains((pt.x - self.origin.x) as f32 / self.g.scale,
            (pt.y - self.origin.y) as f32 / self.g.scale);
        // WM_MOUSELEAVE may already have fired (and been ignored) during capture.
        if self.hover { self.track_leave(); }
    }

    fn island(&self) -> Rect {
        let (w, h) = (self.w.value, self.h.value);
        let compact = self.compact_size().1;
        let progress = anim::expansion_progress(h, compact, self.open_size().1, self.navigation_hover.is_some());
        // The compact player floats as one black capsule, then rejoins the top
        // edge while opening. A hidden/minimal notch still reaches the edge.
        let gap = 6.0 * (1.0 - progress * 2.0).max(0.0)
            * ((h - LINE_H) / (compact - LINE_H).max(1.0)).clamp(0.0, 1.0);
        Rect::new((CANVAS_W - w) / 2.0, gap, w, h)
    }

    fn content(&self) -> Rect {
        let i = self.island();
        let nav = theme::nav_rows(self.open_size().0, self.tabs().len()) as f32 * TAB_H + 10.0 + theme::NAV_GAP;
        Rect::new(i.x, i.y + GRIP_H, i.w, (i.h - nav - GRIP_H).max(0.0))
    }

    fn render(&mut self) {
        if self.is_open() {
            let old_size = self.modules[self.page].expanded_size();
            self.modules[self.page].prepare();
            if old_size != self.modules[self.page].expanded_size()
                && self.pointer_page.is_none() && self.notch_drag.is_none() {
                self.retarget();
            }
        }
        if self.g.begin().is_err() {
            return;
        }
        let island = self.island();
        let appearance = self.settings.borrow().appearance.clone();
        // A one-byte alpha floor retains layered-window hit testing at 100%.
        let material_alpha = appearance.tint_alpha().max(0.004);
        let capability = Event::Material(self.g.backdrop_error.clone());
        self.modules[self.page].on_event(&capability);
        let (_, ch) = self.compact_size();
        let (_, eh) = self.open_size();
        let p = anim::expansion_progress(island.h, ch, eh, self.navigation_hover.is_some());
        let expanded = p >= 0.5;
        // Sudut atas menempel di tepi layar, hanya sudut bawah yang membulat.
        let radius = (island.h * 0.5).min(22.0);
        let surface = Rect::new(island.x, -radius, island.w, island.h + radius);
        if expanded {
            self.g.glass(surface, radius, material_alpha);
            self.g.stroke_rrect(surface, radius, 0.7, Color(0.68, 0.69, 0.84, 0.24));
        } else {
            self.g.fill_rrect(island, radius, Color(0.035, 0.035, 0.04, 1.0));
        }
        // Telinga cekung di kiri-kanan, supaya notch tampak tumbuh dari tepi layar.
        // Mengecil bersama notch (garis tipis, bersembunyi).
        let ear = (island.h * 0.35).min(EAR_R);
        if expanded && ear >= 1.0 {
            let edge = Color(0.125, 0.13, 0.15, material_alpha);
            self.g.ear(island.x, ear, true, edge);
            self.g.ear(island.x + island.w, ear, false, edge);
        }

        let badge = self.badge();
        self.g.push_clip(island);
        self.tab_hits.clear();
        if !expanded {
            // Memudar saat notch naik bersembunyi.
            // Memudar saat menyusut jadi garis atau naik bersembunyi.
            let full_h = self.modules[self.current].compact_size().1;
            self.g.alpha = (1.0 - p * 2.0) * ((island.h - LINE_H) / (full_h - LINE_H)).clamp(0.0, 1.0);
            let mut r = island;
            // Titik mic (oranye) dan kamera (hijau) di ujung kanan, seperti di iPhone.
            let dots = self.dots();
            if !dots.is_empty() {
                let dw = self.dots_w();
                r.w -= dw;
                for (k, c) in dots.iter().enumerate() {
                    let cx = r.x + r.w + 4.0 + DOT_W * (k as f32 + 0.5);
                    self.g.circle(cx, r.y + r.h / 2.0, 3.5, *c);
                }
            }
            if let Some((_, text, color)) = &badge {
                r.w -= BADGE_W;
                let br = Rect::new(r.x + r.w - 4.0, r.y, BADGE_W - 12.0, r.h);
                self.g.text(text, br, 12.5, 600, *color, Align::Right);
            }
            self.modules[self.current].draw_compact(&mut self.g, r);
        } else {
            self.g.alpha = (p - 0.5) * 2.0;
            let content = self.content();
            // Keep a local contrast surface beneath content, independently of the
            // surrounding glass tint. Foreground opacity never follows the slider.
            self.g.fill_rrect(Rect::new(content.x + 10.0, content.y, content.w - 20.0, content.h),
                16.0, Color(0.035, 0.04, 0.05, 0.72));
            self.g.push_clip(content);
            self.modules[self.page].draw_expanded(&mut self.g, content);
            self.g.pop_clip();
            self.draw_tabs(Rect::new(island.x, content.y + content.h + theme::NAV_GAP,
                island.w, island.h - content.h - GRIP_H - theme::NAV_GAP));
            let grip = self.grip();
            self.g.fill_rrect(Rect::new(grip.x + 12.0, 4.0, grip.w - 24.0, 3.0), 1.5, DIM);
        }
        self.g.alpha = 1.0;
        self.g.pop_clip();
        let _ = self.g.end(self.hwnd, self.origin, island, if expanded { appearance.blur } else { 0 });

        let tick = if expanded {
            self.modules[self.page].tick_ms(true)
        } else {
            let a = self.modules[self.current].tick_ms(false);
            let b = badge.and_then(|(i, _, _)| self.modules[i].tick_ms(false));
            a.into_iter().chain(b).min()
        };
        unsafe {
            match tick {
                Some(ms) => {
                    SetTimer(self.hwnd, TIMER_TICK, ms, None);
                }
                None => {
                    let _ = KillTimer(self.hwnd, TIMER_TICK);
                }
            }
        }
    }

    fn draw_tabs(&mut self, r: Rect) {
        let tabs = self.tabs();
        let width = (tabs.len() as f32 * theme::NAV_CELL).min(r.w - 24.0);
        self.g.fill_rrect(Rect::new(r.x + (r.w - width) / 2.0, r.y, width, r.h - 8.0),
            16.0, Color(0.055, 0.06, 0.075, 0.82));
        let rects = theme::nav_rects(r, tabs.len(), self.open_size().0);
        for (tr, &i) in rects.into_iter().zip(&tabs) {
            let button = Rect::new(tr.x + (tr.w - 32.0) / 2.0, tr.y + (tr.h - 32.0) / 2.0, 32.0, 32.0);
            self.g.fill_rrect(button, 10.0, if i == self.page { theme::SELECTED } else { theme::SURFACE });
            if i == self.page { self.g.stroke_rrect(button, 10.0, 0.7, Color(0.7, 0.69, 0.9, 0.3)); }
            theme::draw_icon(&mut self.g, self.modules[i].tab().unwrap_or(""), tr,
                if i == self.page { theme::ACCENT } else { DIM });
            self.tab_hits.push((tr, i));
        }
    }

    fn set_editing(&mut self, on: bool) {
        if on == self.editing {
            return;
        }
        self.editing = on;
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let na = WS_EX_NOACTIVATE.0 as isize;
            if on {
                // Jendela notch biasanya tidak pernah aktif; saat mengetik ia perlu fokus keyboard.
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !na);
                let _ = SetForegroundWindow(self.hwnd);
                let _ = SetFocus(self.hwnd);
            } else {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | na);
            }
        }
    }

    fn shelf_index(&self) -> Option<usize> {
        self.modules.iter().position(|m| m.accepts_drop())
    }

    fn set_drop_hover(&mut self, on: bool) {
        let Some(i) = self.shelf_index() else { return };
        if on {
            self.page = i;
        }
        self.drop_hover = on;
        self.modules[i].drop_hover(on);
        self.retarget();
        self.render();
    }

    fn drop_files(&mut self, paths: Vec<PathBuf>) {
        let Some(i) = self.shelf_index() else { return };
        self.modules[i].drop_files(paths);
        self.set_drop_hover(false);
        // Kursor masih di atas notch: anggap hover supaya tetap terbuka sampai ditinggal.
        self.start_hover();
        self.retarget();
        self.render();
    }

    fn start_hover(&mut self) {
        self.hover = true;
        self.track_leave();
    }

    /// Minta WM_MOUSELEAVE saat kursor meninggalkan notch.
    fn track_leave(&self) {
        let mut t = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut t);
        }
    }

    fn broadcast(&mut self, e: Event) {
        for m in &mut self.modules {
            m.on_event(&e);
        }
        self.schedule_poll();
        self.retarget();
        self.render();
    }

    fn notice(&mut self, icon: &'static str, text: impl Into<String>, detail: impl Into<String>) {
        self.broadcast(Event::Notice { icon, text: text.into(), detail: detail.into() });
    }

    fn battery_detail(&self) -> String {
        self.battery.map(|b| format!("{b}%")).unwrap_or_default()
    }

    fn on_power(&mut self, p: system::Power) {
        match p {
            system::Power::Brightness(level) => {
                if self.brightness_ready {
                    self.broadcast(Event::Brightness(level));
                }
                self.brightness_ready = true;
            }
            system::Power::Charger(on) => {
                let changed = self.charger.is_some_and(|c| c != on);
                self.charger = Some(on);
                if on {
                    self.battery_warned = 101;
                }
                // Desktop tanpa baterai tidak perlu diberi tahu soal charger.
                if changed && self.battery.is_some_and(|b| b <= 100) {
                    let d = self.battery_detail();
                    if on {
                        self.notice("🔌", "Mengisi daya", d);
                    } else {
                        self.notice("🔋", "Charger dicabut", d);
                    }
                }
            }
            system::Power::Battery(level) => {
                let first = self.battery.is_none();
                self.battery = Some(level);
                if first || self.charger != Some(false) {
                    return;
                }
                if let Some(&th) = LOW_BATTERY.iter().filter(|&&t| level <= t && self.battery_warned > t).last() {
                    self.battery_warned = th;
                    modules::chime();
                    self.notice("🔋", "Baterai lemah", format!("{level}%"));
                }
            }
        }
    }

    fn on_network(&mut self) {
        let now = network::current();
        if now == self.net {
            return;
        }
        let was_online = self.net.online;
        self.net = now.clone();
        match (was_online, now.online) {
            (_, true) if !now.ssid.is_empty() => self.notice("📶", "Terhubung", now.ssid),
            (false, true) => self.notice("🌐", "Internet tersambung", ""),
            (true, false) => self.notice("⚠️", "Internet terputus", ""),
            _ => {}
        }
    }

    /// Terapkan setelan yang butuh tindakan di luar modul, lalu beri tahu semua modul.
    fn apply_settings(&mut self) {
        let (clip, auto_hide) = {
            let s = self.settings.borrow();
            // Clipboard dipantau untuk riwayat atau untuk merapikan tabel.
            (s.clipboard || s.tidy_tables, s.auto_hide)
        };
        if clip != self.clip_listening {
            clipboard::listen(self.hwnd, clip);
            self.clip_listening = clip;
        }
        if auto_hide != self.auto_hide {
            self.auto_hide = auto_hide;
        }
        self.recheck_covered();
        if self.hover { self.track_leave(); }
        // Halaman yang sedang dibuka bisa hilang (mis. Clipboard dimatikan).
        if self.modules[self.page].tab().is_none() {
            self.page = self.tabs().first().copied().unwrap_or(0);
        }
        self.broadcast(Event::SettingsChanged);
    }

    fn sync_editing(&mut self) {
        let on = self.is_open() && self.modules[self.page].editing();
        self.set_editing(on);
    }

    fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                unsafe { let _ = ValidateRect(self.hwnd, None); }
                self.render();
            }
            WM_MOUSEMOVE if self.notch_drag.is_some() => {
                let mut pt = POINT::default();
                unsafe { let _ = GetCursorPos(&mut pt); }
                let drag = self.notch_drag.as_ref().unwrap();
                self.origin = POINT { x: drag.origin.x + pt.x - drag.anchor.x, y: drag.origin.y + pt.y - drag.anchor.y };
                self.render();
            }
            WM_MOUSEMOVE if self.pointer_page.is_some() => {
                let x = (lp.0 & 0xffff) as i16 as f32 / self.g.scale;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as f32 / self.g.scale;
                self.modules[self.pointer_page.unwrap()].pointer_move(x, y);
                self.render();
            }
            WM_LBUTTONUP if self.notch_drag.is_some() => self.finish_notch_drag(true),
            WM_LBUTTONUP if self.pointer_page.is_some() => {
                if let Some(i) = self.pointer_page.take() { self.modules[i].pointer_up(); }
                self.set_editing(false);
                unsafe { let _ = ReleaseCapture(); }
                self.sync_hover_after_capture();
                self.retarget();
                self.render();
            }
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize && self.notch_drag.is_some() => self.finish_notch_drag(false),
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize && self.pointer_page.is_some() => {
                if let Some(i) = self.pointer_page.take() { self.modules[i].cancel_pointer(); }
                self.suppress_up = true;
                self.set_editing(false);
                unsafe { let _ = ReleaseCapture(); }
                self.sync_hover_after_capture(); self.retarget(); self.render();
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                self.finish_notch_drag(false);
                if let Some(i) = self.pointer_page.take() {
                    self.modules[i].cancel_pointer();
                    self.suppress_up = true;
                    self.set_editing(false);
                    if msg == WM_CANCELMODE { unsafe { let _ = ReleaseCapture(); } }
                    self.hover = false;
                    self.retarget();
                    self.render();
                }
                self.press = None;
            }
            WM_MOUSEMOVE if self.covered && !self.is_open() && self.cursor_wants_behind() => {
                self.check_dodge();
            }
            WM_MOUSEMOVE => {
                if self.is_open() && !self.hover {
                    // Sudah terbuka (pengingat, mengetik): langsung anggap hover.
                    self.hover_pending = false;
                    self.start_hover();
                    self.retarget();
                } else if !self.hover && !self.hover_pending && self.cursor_over_notch() {
                    self.queue_hover();
                }
                // Tombol kiri ditahan dan digeser: mulai seret file keluar.
                if let Some((px, py)) = self.press {
                    let s = self.g.scale;
                    let x = (lp.0 & 0xffff) as i16 as f32 / s;
                    let y = ((lp.0 >> 16) & 0xffff) as i16 as f32 / s;
                    if wp.0 & 1 == 0 {
                        self.press = None;
                    } else if (x - px).hypot(y - py) > 6.0 {
                        self.press = None;
                        if self.is_open() && !self.editing {
                            self.pending_drag = self.modules[self.page].drag_source(px, py);
                        }
                    }
                }
            }
            WM_LBUTTONDOWN => {
                self.suppress_up = false;
                let s = self.g.scale;
                let x = (lp.0 & 0xffff) as i16 as f32 / s;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as f32 / s;
                if self.is_open() && self.grip().contains(x, y) {
                    self.press = None;
                    self.modules[self.page].stop_editing();
                    let mut pt = POINT::default();
                    unsafe { let _ = GetCursorPos(&mut pt); }
                    self.notch_drag = Some(NotchDrag { origin: self.origin, anchor: pt });
                    self.set_editing(true);
                    unsafe { SetCapture(self.hwnd); }
                    self.retarget();
                } else if self.is_open() && {
                    let content = self.content();
                    self.modules[self.page].pointer_down(content, x, y)
                } {
                    self.press = None;
                    self.pointer_page = Some(self.page);
                    self.set_editing(true);
                    unsafe { SetCapture(self.hwnd); }
                    self.render();
                } else { self.press = Some((x, y)); }
            }
            WM_MOUSELEAVE if self.notch_drag.is_some() || self.pointer_page.is_some() => {}
            // Keep a top-edge reveal alive while the panel grows under the
            // cursor. The pointer timer also owns leave detection in this mode.
            WM_MOUSELEAVE if (self.hover_pending || self.covered || self.settings.borrow().minimal)
                && self.cursor_over_notch() => {}
            WM_MOUSELEAVE => {
                if std::mem::take(&mut self.hover_pending) {
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_HOVER);
                    }
                }
                self.hover = false;
                self.alert = false;
                self.retarget();
            }
            WM_LBUTTONUP if self.suppress_up => { self.suppress_up = false; }
            WM_LBUTTONUP => {
                self.press = None;
                let s = self.g.scale;
                let x = (lp.0 & 0xffff) as i16 as f32 / s;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as f32 / s;
                if !self.is_open() {
                    return Some(LRESULT(0));
                }
                self.alert = false;
                if let Some(&(_, i)) = self.tab_hits.iter().find(|(r, _)| r.contains(x, y)) {
                    if self.editing {
                        self.modules[self.page].stop_editing();
                    }
                    self.navigate_to(i);
                } else {
                    let content = self.content();
                    if !self.modules[self.page].click(content, x, y) {
                        return Some(LRESULT(0));
                    }
                    if self.modules[self.page].tab() == Some("Setelan") {
                        self.apply_settings();
                    }
                }
                self.sync_editing();
                self.schedule_poll();
                // Klik bisa mengubah ukuran (mis. lirik dinyalakan, tugas ditambah).
                self.retarget();
                self.render();
            }
            WM_MOUSEWHEEL if self.is_open() && !self.editing => {
                if self.pointer_page.is_some() || self.notch_drag.is_some() { return Some(LRESULT(0)); }
                let mut pt = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
                unsafe { let _ = ScreenToClient(self.hwnd, &mut pt); }
                if self.modules[self.page].wheel(pt.x as f32 / self.g.scale, pt.y as f32 / self.g.scale, (wp.0 >> 16) as i16) {
                    self.render();
                    return Some(LRESULT(0));
                }
                let tabs = self.tabs();
                let k = tabs.iter().position(|&i| i == self.page).unwrap_or(0);
                let up = (wp.0 >> 16) as i16 > 0;
                let n = tabs.len();
                self.navigate_to(tabs[if up { (k + n - 1) % n } else { (k + 1) % n }]);
                self.retarget();
                self.render();
            }
            WM_CHAR if self.editing => {
                if let Some(c) = char::from_u32(wp.0 as u32) {
                    self.modules[self.page].key_char(c);
                }
                self.sync_editing();
                self.schedule_poll();
                self.retarget();
                self.render();
            }
            WM_ACTIVATE if self.notch_drag.is_some() && (wp.0 & 0xffff) as u32 == WA_INACTIVE => {
                self.finish_notch_drag(false);
            }
            WM_ACTIVATE if self.pointer_page.is_some() && (wp.0 & 0xffff) as u32 == WA_INACTIVE => {
                if let Some(i) = self.pointer_page.take() { self.modules[i].cancel_pointer(); }
                self.suppress_up = true;
                self.set_editing(false);
                unsafe { let _ = ReleaseCapture(); }
                self.hover = false;
                self.retarget();
            }
            WM_ACTIVATE if self.editing && (wp.0 & 0xffff) as u32 == WA_INACTIVE => {
                // Klik di luar notch: simpan ketikan.
                self.modules[self.page].stop_editing();
                self.set_editing(false);
                self.retarget();
                self.render();
            }
            WM_MOUSEACTIVATE if !self.editing => return Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_TIMER if wp.0 == TIMER_ANIM => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32();
                self.last = now;
                let a = self.w.step(dt);
                let b = self.h.step(dt);
                self.render();
                if !(a || b) {
                    self.animating = false;
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_ANIM);
                    }
                }
            }
            WM_TIMER if wp.0 == TIMER_TICK => self.render(),
            WM_TIMER if wp.0 == TIMER_POLL => self.poll(),
            WM_TIMER if wp.0 == occlusion::TIMER_RECHECK => {
                unsafe {
                    let _ = KillTimer(self.hwnd, occlusion::TIMER_RECHECK);
                }
                self.recheck_covered();
            }
            WM_TIMER if wp.0 == TIMER_DODGE => self.check_dodge(),
            WM_TIMER if wp.0 == TIMER_NAV_HOVER => self.check_navigation_hover(),
            WM_TIMER if wp.0 == TIMER_HOVER => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_HOVER);
                }
                // Masih di atas notch (WM_MOUSELEAVE belum datang) dan tidak sedang minggir.
                if std::mem::take(&mut self.hover_pending) && !self.hover && self.cursor_over_notch() {
                    self.dodge = false;
                    if !self.is_open() && self.modules[self.current].tab().is_some() {
                        // Buka di halaman yang sedang tampil di notch tertutup.
                        self.page = self.current;
                    }
                    self.start_hover();
                    self.retarget();
                } else {
                    self.check_dodge();
                }
            }
            WM_TIMER if wp.0 == TIMER_ALERT => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_ALERT);
                }
                self.alert = false;
                self.retarget();
            }
            WM_APP_MEDIA => self.broadcast(Event::MediaChanged),
            WM_APP_LYRICS => self.broadcast(Event::LyricsChanged),
            system::WM_APP_VOLUME => {
                self.broadcast(Event::Volume { level: wp.0 as u32, muted: lp.0 != 0 });
            }
            system::WM_APP_AUDIO_DEVICE => {
                let device = self.audio.as_mut().and_then(|a| a.rebind());
                if let Some((name, headphones)) = device {
                    let icon = if headphones { "🎧" } else { "🔊" };
                    self.notice(icon, name, "Output audio");
                }
            }
            WM_POWERBROADCAST => {
                if let Some(p) = system::power_from(wp, lp) {
                    self.on_power(p);
                }
                return Some(LRESULT(1));
            }
            privacy::WM_APP_PRIVACY => {
                let now = privacy::current();
                let new_mic: Vec<String> = now.mic.iter().filter(|a| !self.in_use.mic.contains(a)).cloned().collect();
                let new_cam: Vec<String> = now.cam.iter().filter(|a| !self.in_use.cam.contains(a)).cloned().collect();
                self.in_use = now;
                if let Some(app) = new_cam.first() {
                    self.notice("📷", "Kamera dipakai", app.clone());
                } else if let Some(app) = new_mic.first() {
                    self.notice("🎙️", "Mikrofon dipakai", app.clone());
                } else {
                    self.retarget();
                    self.render();
                }
            }
            WM_CLIPBOARDUPDATE => unsafe {
                SetTimer(self.hwnd, TIMER_CLIP, CLIP_SETTLE_MS, None);
            },
            WM_TIMER if wp.0 == TIMER_CLIP => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_CLIP);
                }
                let tidy = self.settings.borrow().tidy_tables;
                if tidy {
                    if let Some((rows, cols)) = clipboard::tidy_table(self.hwnd) {
                        self.notice("📋", "Tabel siap ditempel", format!("{rows} × {cols}"));
                    }
                }
                self.broadcast(Event::ClipboardChanged);
            }
            screenshots::WM_APP_SCREENSHOT => {
                let paths = screenshots::take();
                let on = self.settings.borrow().screenshot_to_shelf;
                if let (true, Some(i)) = (on && !paths.is_empty(), self.shelf_index()) {
                    self.modules[i].drop_files(paths);
                    self.notice("📸", "Screenshot", "masuk Shelf");
                }
            }
            keyboard::WM_APP_CAPS => {
                self.notice("⇪", "Caps Lock", if wp.0 != 0 { "Nyala" } else { "Mati" });
            }
            keyboard::WM_APP_LANG => {
                let (name, code) = keyboard::language(lp.0);
                self.notice("🌐", name, code);
            }
            network::WM_APP_NET => unsafe {
                SetTimer(self.hwnd, TIMER_NET, NET_SETTLE_MS, None);
            },
            WM_TIMER if wp.0 == TIMER_NET => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_NET);
                }
                self.on_network();
            }
            WM_DPICHANGED | WM_DISPLAYCHANGE => {
                if msg == WM_DPICHANGED && self.notch_drag.is_some() { return Some(LRESULT(0)); }
                self.finish_notch_drag(false);
                if let Some(i) = self.pointer_page.take() {
                    self.modules[i].cancel_pointer();
                    self.suppress_up = true;
                    self.set_editing(false);
                    unsafe { let _ = ReleaseCapture(); }
                }
                self.place();
                // Posisi notch di layar berubah: cek ulang jendela di belakangnya.
                self.recheck_covered();
                self.retarget();
                self.render();
            }
            shelf::WM_APP_THUMB if self.is_open() => self.render(),
            _ => return None,
        }
        Some(LRESULT(0))
    }
}

/// Untuk pengembangan: `AMORA_FORCE_OPEN=1` membuat notch selalu terbuka.
fn dev_instance() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AMORA_DEV").is_some())
}

/// Dibaca sekali saja: `is_open()` dipanggil berkali-kali di setiap frame.
fn force_open() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AMORA_FORCE_OPEN").is_some())
}

const CMD_EXIT: usize = 1;
const CMD_AUTOSTART: usize = 2;
const CMD_ABOUT: usize = 3;
const CMD_MASCOT: usize = 4;
const CMD_MINIMAL: usize = 5;
const CMD_CLIPBOARD: usize = 6;
const CMD_PRIMARY: usize = 7;
const CMD_APPEARANCE: usize = 8;

/// Menu yang sama untuk klik kanan notch dan klik ikon tray.
fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let auto = autostart::is_enabled();
        let (mut mascot, mut minimal, mut clip) = (false, false, false);
        with_app(|a| {
            let s = a.settings.borrow();
            (mascot, minimal, clip) = (s.mascot, s.minimal, s.clipboard);
        });
        let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check(auto), CMD_AUTOSTART, w!("Mulai bersama Windows"));
        let _ = AppendMenuW(menu, MF_STRING | check(mascot), CMD_MASCOT, w!("Maskot Amora"));
        let _ = AppendMenuW(menu, MF_STRING | check(minimal), CMD_MINIMAL, w!("Notch minimal"));
        let _ = AppendMenuW(menu, MF_STRING | check(clip), CMD_CLIPBOARD, w!("Riwayat clipboard"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_PRIMARY, w!("Kembalikan ke monitor utama"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_APPEARANCE, w!("Pulihkan tampilan awal"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, CMD_ABOUT, w!("Tentang Amora"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT, w!("Keluar"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
        // Supaya menu tray menutup dengan benar saat klik di luar.
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        match cmd.0 as usize {
            CMD_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            CMD_AUTOSTART => autostart::set(!auto),
            CMD_APPEARANCE => with_app(|a| {
                { let mut s = a.settings.borrow_mut(); s.appearance = Default::default(); s.save(); }
                a.apply_settings(); a.retarget(); a.render();
            }),
            CMD_PRIMARY => with_app(|a| {
                a.finish_notch_drag(false);
                {
                    let mut settings = a.settings.borrow_mut();
                    settings.monitor = None;
                    settings.save();
                }
                a.place();
                a.recheck_covered();
                a.retarget();
                a.render();
            }),
            CMD_MASCOT | CMD_MINIMAL | CMD_CLIPBOARD => with_app(|a| {
                {
                    let mut s = a.settings.borrow_mut();
                    match cmd.0 as usize {
                        CMD_MASCOT => s.mascot = !s.mascot,
                        CMD_MINIMAL => s.minimal = !s.minimal,
                        _ => s.clipboard = !s.clipboard,
                    }
                    s.save();
                }
                a.apply_settings();
                a.schedule_poll();
                a.retarget();
                a.render();
            }),
            CMD_ABOUT => {
                let text = HSTRING::from(format!(
                    "Amora {}
Notch hidup di tepi atas layar Windows.

Lirik dari LRCLIB (lrclib.net). Hanya judul, artis, dan durasi lagu yang dikirim.",
                    env!("CARGO_PKG_VERSION")
                ));
                MessageBoxW(hwnd, &text, w!("Tentang Amora"), MB_OK | MB_ICONINFORMATION);
            }
            _ => {}
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Menu berjalan dengan loop pesannya sendiri, jadi jangan sambil memegang APP.
        WM_RBUTTONUP => {
            show_menu(hwnd);
            return LRESULT(0);
        }
        tray::WM_TRAY => {
            if tray::wants_menu(lp) {
                show_menu(hwnd);
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            tray::remove(hwnd);
            unsafe {
                let _ = RevokeDragDrop(hwnd);
            }
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        // Explorer restart: ikon tray harus didaftarkan ulang.
        m if tray::is_taskbar_created(m) => {
            tray::add(hwnd);
            return LRESULT(0);
        }
        _ => {}
    }
    let r = APP.with(|a| {
        let mut a = a.try_borrow_mut().ok()?;
        a.as_mut()?.handle(msg, wp, lp)
    });
    // Seret keluar menjalankan loop pesannya sendiri, jadi dilakukan setelah APP dilepas.
    let mut drag = None;
    with_app(|a| drag = a.pending_drag.take());
    if let Some(path) = drag {
        SELF_DRAG.with(|d| d.set(true));
        let dropped = shelf::drag_out(hwnd, &path);
        SELF_DRAG.with(|d| d.set(false));
        if dropped {
            with_app(|a| {
                let page = a.page;
                a.modules[page].dragged_out(&path);
                a.retarget();
                a.render();
            });
        }
    }
    r.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

/// Menerima file yang diseret dari Explorer ke notch.
#[implement(IDropTarget)]
struct DropTarget;

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Option<&IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let ok = !SELF_DRAG.with(|d| d.get()) && data.is_some_and(shelf::has_files);
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if ok {
            with_app(|a| a.set_drop_hover(true));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let mut ok = false;
        with_app(|a| ok = a.drop_hover);
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        with_app(|a| a.set_drop_hover(false));
        Ok(())
    }

    fn Drop(
        &self,
        data: Option<&IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let paths = data.map(shelf::files_from).unwrap_or_default();
        unsafe { *effect = if paths.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        with_app(|a| {
            if paths.is_empty() {
                a.set_drop_hover(false);
            } else {
                a.drop_files(paths);
            }
        });
        Ok(())
    }
}

fn main() -> Result<()> {
    crashlog::install();
    // Diagnostics must use the same physical coordinates/DPI as the real window.
    unsafe { let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2); }
    // Untuk pengembangan: `cargo run -- --probe-lyrics "<judul>" "<artis>" <durasi>`
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--preview-controls" || s == "--preview-prism" || s == "--preview-hover") {
        std::env::set_var("AMORA_DEV", "1");
        if args[1] != "--preview-hover" { std::env::set_var("AMORA_FORCE_OPEN", "1"); }
        let page = if args[1] != "--preview-controls" { args.get(2).map(String::as_str).unwrap_or("Setelan") } else { "Kontrol" };
        std::env::set_var("AMORA_PAGE", page);
        std::env::set_var("AMORA_DEV_X", "0");
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("amora-preview-data"))) {
            std::env::set_var("AMORA_DATA_DIR", dir);
        }
    }
    // Read-only hardware diagnostics; never changes brightness, volume or settings.
    if args.get(1).map(String::as_str) == Some("--probe-controls") {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()?; }
        for monitor in monitors::all() {
            println!("monitor: {} | {} | primary={} | scale={} | rect={:?} | pnp={:?}", monitor.device, monitor.name, monitor.primary, monitor.scale, monitor.rect, monitor.pnp);
            println!("brightness: {:?}", brightness::probe(&monitor));
        }
        println!("master: {:?}", system::master_state());
        match mixer::read() {
            Ok(snapshot) => for app in snapshot.apps { println!("audio: {} | {}% | mute={} | mixed={}", app.name, (app.level * 100.0).round(), app.muted, app.mixed); },
            Err(error) => println!("audio: {error}"),
        }
        unsafe { CoUninitialize(); }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--probe-lyrics") {
        return lyrics::probe(&args[2], &args[3], args[4].parse().unwrap_or(0.0));
    }
    // Untuk pengembangan: `amora --probe-table <file CF_HTML atau teks>` menampilkan hasil urai.
    if args.get(1).map(String::as_str) == Some("--probe-table") {
        let bytes = std::fs::read(&args[2]).unwrap_or_default();
        let rows = tables::cf_html_fragment(&bytes)
            .and_then(|f| tables::from_html(&f))
            .or_else(|| tables::from_markdown(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}')));
        match rows {
            Some(r) => {
                for row in &r {
                    println!("{} {:?}", if row.head { "J" } else { " " }, row.cells);
                }
                println!("lebar kolom %: {:?}", tables::column_widths(&r));
            }
            None => println!("bukan tabel"),
        }
        return Ok(());
    }
    // Untuk pengembangan: `amora --crash-test` memastikan panic tercatat di log.txt.
    if args.get(1).map(String::as_str) == Some("--crash-test") {
        panic!("uji log crash");
    }
    // Dipanggil dari Settings → Apps (lihat installer).
    if args.get(1).map(String::as_str) == Some("--uninstall") {
        let has = |f: &str| args.iter().any(|a| a == f);
        uninstall::run(has("--quiet"), has("--wipe"));
        return Ok(());
    }
    // Untuk pengembangan: `amora --probe-system` mencetak status yang dipakai notifikasi.
    if args.get(1).map(String::as_str) == Some("--probe-system") {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        println!("jaringan: {:?}", network::current());
        println!("output audio: {:?}", system::default_output());
        println!("bahasa keyboard: {:?}", keyboard::language(keyboard::current_layout()));
        return Ok(());
    }
    // Untuk pengembangan: `amora --autostart on|off|status`
    if args.get(1).map(String::as_str) == Some("--autostart") {
        match args.get(2).map(String::as_str) {
            Some("on") => autostart::set(true),
            Some("off") => autostart::set(false),
            _ => {}
        }
        println!("autostart: {}", autostart::is_enabled());
        return Ok(());
    }
    unsafe {
        // Satu instance saja.
        // `AMORA_DEV=1`: instance uji yang boleh jalan berdampingan (digeser ke kanan).
        // Preview instances must not request access to the production mutex.
        let _mutex = if dev_instance() { CreateMutexW(None, true, None)? }
            else { CreateMutexW(None, true, w!("Local\\AmoraNotch"))? };
        if GetLastError() == ERROR_ALREADY_EXISTS && !dev_instance() {
            return Ok(());
        }
        // OLE (bukan COM biasa) supaya drag & drop file bisa dipakai.
        OleInitialize(None)?;

        let hinst = GetModuleHandleW(None)?;
        let class = w!("AmoraNotch");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        // Expose the dedicated preview as a normal app window so UI test tools
        // can discover it; production remains a non-activating tray/tool window.
        let preview = args.get(1).is_some_and(|s| s == "--preview-controls" || s == "--preview-prism" || s == "--preview-hover");
        let flags = WS_EX_LAYERED | WS_EX_TOPMOST | if preview { WS_EX_APPWINDOW } else { WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE };
        let hwnd = CreateWindowExW(
            flags,
            class,
            w!("Amora"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            hinst,
            None,
        )?;

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let g = Gfx::new((CANVAS_W * scale) as i32, (CANVAS_H * scale) as i32, scale)?;
        let (shared, handle) = media::spawn(hwnd, WM_APP_MEDIA);
        let lyrics = lyrics::LyricsService::spawn(hwnd, WM_APP_LYRICS);
        let settings = Settings::load_shared();
        // Urutan = prioritas untuk notch tertutup, dan urutan halaman.
        let modules: Vec<Box<dyn Module>> = vec![
            Box::new(Osd::new()),
            Box::new(MediaModule::new(shared, handle, g.wic.clone(), lyrics, settings.clone())),
            Box::new(Pomodoro::new(settings.clone())),
            Box::new(Water::new(settings.clone())),
            Box::new(Todo::new()),
            Box::new(Schedule::new()),
            Box::new(Shelf::new(g.wic.clone(), hwnd)),
            Box::new(Clipboard::new(hwnd, settings.clone())),
            Box::new(modules::controls::Controls::new()),
            Box::new(SettingsPage::new(settings.clone())),
            Box::new(ClockModule::new(settings.clone())),
        ];

        let mut app = App {
            monitor: None,
            notch_drag: None,
            pointer_page: None,
            suppress_up: false,
            hwnd,
            g,
            origin: POINT::default(),
            modules,
            current: 0,
            page: 0,
            hover: false,
            hover_pending: false,
            navigation_hover: None,
            alert: false,
            editing: false,
            tab_hits: Vec::new(),
            drop_hover: false,
            press: None,
            pending_drag: None,
            audio: system::AudioWatch::new(hwnd),
            brightness_ready: false,
            charger: None,
            battery: None,
            battery_warned: 101,
            net: network::current(),
            settings: settings.clone(),
            clip_listening: settings.borrow().clipboard || settings.borrow().tidy_tables,
            in_use: privacy::InUse::default(),
            auto_hide: settings.borrow().auto_hide,
            covered: false,
            dodge: false,
            w: Spring::new(0.0),
            h: Spring::new(0.0),
            last: Instant::now(),
            animating: false,
        };
        app.current = app.active_index();
        if let Ok(name) = std::env::var("AMORA_PAGE") {
            if let Some(i) = app.modules.iter().position(|m| m.tab() == Some(name.as_str())) {
                app.page = i;
            }
        }
        let (w, h) = if app.is_open() { app.open_size() } else { app.compact_size() };
        app.w = Spring::new(w);
        app.h = Spring::new(h);
        app.schedule_poll();
        app.place();
        app.retarget();
        app.render();
        APP.with(|a| *a.borrow_mut() = Some(app));
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // A hidden launcher STARTUPINFO overrides the first ShowWindow call.
        // Explicit preview mode must still expose its test window.
        if preview {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        with_app(|a| { a.g.enable_backdrop(a.hwnd); a.render(); });
        tray::add(hwnd);
        system::watch_power(hwnd);
        keyboard::spawn(hwnd);
        network::watch(hwnd);
        privacy::watch(hwnd);
        with_app(|a| a.in_use = privacy::current());
        screenshots::watch(hwnd);
        {
            let s = settings.borrow();
            clipboard::listen(hwnd, s.clipboard || s.tidy_tables);
        }
        occlusion::install(hwnd);
        with_app(|a| a.recheck_covered());
        let target: IDropTarget = DropTarget.into();
        let _ = RegisterDragDrop(hwnd, &target);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|a| a.borrow_mut().take());
    }
    Ok(())
}
