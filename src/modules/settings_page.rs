//! Halaman Setelan: semua pilihan yang sebelumnya harus diubah di settings.json.
//! Perubahan langsung disimpan dan langsung berlaku.

use super::{Event, Module};
use crate::autostart;
use crate::gfx::{Align, Color, Gfx, Rect, DIM, FAINT, WHITE};
use crate::settings::{Settings, SharedSettings};

const ACCENT: Color = crate::theme::ACCENT;
const ROW: f32 = 34.0;

#[derive(Clone, Copy)]
enum Toggle {
    Autostart,
    Lyrics,
    AutoHide,
    Mascot,
    Minimal,
    Screenshot,
    Clipboard,
    TidyTables,
    WaterRemind,
}

#[derive(Clone, Copy)]
enum Step {
    Focus,
    ShortBreak,
    LongBreak,
    WaterGoal,
    WaterEvery,
    WaterStart,
    WaterEnd,
}

#[derive(Clone, Copy)]
enum Hit {
    Toggle(Toggle),
    Step(Step, i32),
    Section(usize),
    Reset,
}

const LEFT: [Toggle; 9] = [
    Toggle::Autostart,
    Toggle::Lyrics,
    Toggle::AutoHide,
    Toggle::Mascot,
    Toggle::Minimal,
    Toggle::Screenshot,
    Toggle::Clipboard,
    Toggle::TidyTables,
    Toggle::WaterRemind,
];
const RIGHT: [Step; 7] = [
    Step::Focus,
    Step::ShortBreak,
    Step::LongBreak,
    Step::WaterGoal,
    Step::WaterEvery,
    Step::WaterStart,
    Step::WaterEnd,
];

fn toggle_label(t: Toggle) -> &'static str {
    match t {
        Toggle::Autostart => "Mulai bersama Windows",
        Toggle::Lyrics => "Lirik lagu",
        Toggle::AutoHide => "Minggir untuk tab di belakang",
        Toggle::Mascot => "Maskot",
        Toggle::Minimal => "Notch minimal",
        Toggle::Screenshot => "Screenshot ke Shelf",
        Toggle::Clipboard => "Riwayat clipboard",
        Toggle::TidyTables => "Rapikan tabel yang disalin",
        Toggle::WaterRemind => "Pengingat minum",
    }
}

fn toggle_get(s: &Settings, t: Toggle) -> bool {
    match t {
        Toggle::Autostart => autostart::is_enabled(),
        Toggle::Lyrics => s.lyrics,
        Toggle::AutoHide => s.auto_hide,
        Toggle::Mascot => s.mascot,
        Toggle::Minimal => s.minimal,
        Toggle::Screenshot => s.screenshot_to_shelf,
        Toggle::Clipboard => s.clipboard,
        Toggle::TidyTables => s.tidy_tables,
        Toggle::WaterRemind => s.water.remind,
    }
}

fn toggle_flip(s: &mut Settings, t: Toggle) {
    match t {
        Toggle::Autostart => autostart::set(!autostart::is_enabled()),
        Toggle::Lyrics => s.lyrics = !s.lyrics,
        Toggle::AutoHide => s.auto_hide = !s.auto_hide,
        Toggle::Mascot => s.mascot = !s.mascot,
        Toggle::Minimal => s.minimal = !s.minimal,
        Toggle::Screenshot => s.screenshot_to_shelf = !s.screenshot_to_shelf,
        Toggle::Clipboard => s.clipboard = !s.clipboard,
        Toggle::TidyTables => s.tidy_tables = !s.tidy_tables,
        Toggle::WaterRemind => s.water.remind = !s.water.remind,
    }
}

/// (label, nilai sekarang, langkah, min, maks, satuan)
fn step_info(s: &Settings, k: Step) -> (&'static str, u32, u32, u32, u32, &'static str) {
    match k {
        Step::Focus => ("Fokus", s.pomodoro.focus_min, 5, 5, 120, "mnt"),
        Step::ShortBreak => ("Istirahat pendek", s.pomodoro.short_break_min, 1, 1, 30, "mnt"),
        Step::LongBreak => ("Istirahat panjang", s.pomodoro.long_break_min, 5, 5, 60, "mnt"),
        Step::WaterGoal => ("Target minum", s.water.goal_ml, 250, 500, 5000, "ml"),
        Step::WaterEvery => ("Ingatkan tiap", s.water.interval_min, 15, 15, 240, "mnt"),
        Step::WaterStart => ("Minum mulai jam", s.water.start_hour, 1, 0, 23, ""),
        Step::WaterEnd => ("Minum sampai jam", s.water.end_hour, 1, 1, 24, ""),
    }
}

fn step_apply(s: &mut Settings, k: Step, dir: i32) {
    let (_, v, step, min, max, _) = step_info(s, k);
    let v = (v as i64 + dir as i64 * step as i64).clamp(min as i64, max as i64) as u32;
    match k {
        Step::Focus => s.pomodoro.focus_min = v,
        Step::ShortBreak => s.pomodoro.short_break_min = v,
        Step::LongBreak => s.pomodoro.long_break_min = v,
        Step::WaterGoal => s.water.goal_ml = v,
        Step::WaterEvery => s.water.interval_min = v,
        // Jam aktif tidak boleh terbalik.
        Step::WaterStart => s.water.start_hour = v.min(s.water.end_hour.saturating_sub(1)),
        Step::WaterEnd => s.water.end_hour = v.max(s.water.start_hour + 1),
    }
}

pub struct SettingsPage {
    settings: SharedSettings,
    hits: Vec<(Rect, Hit)>,
    tracks: Vec<(Rect, bool)>,
    drag: Option<(Rect, bool, crate::settings::Appearance)>,
    section: Option<usize>,
    scroll: f32,
    viewport: Rect,
    material_error: Option<String>,
}

impl SettingsPage {
    pub fn new(settings: SharedSettings) -> Self {
        Self { settings, hits: Vec::new(), tracks: Vec::new(), drag: None, section: None,
            scroll: 0.0, viewport: Rect::new(0.0, 0.0, 0.0, 0.0), material_error: None }
    }

    fn height(&self) -> f32 {
        350.0 + match self.section { Some(0) => LEFT.len() as f32 * ROW,
            Some(1) => 3.0 * ROW, Some(2) => 4.0 * ROW, _ => 0.0 }
    }
    fn set_slider(&mut self, track: Rect, blur: bool, x: f32) {
        let value = (((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0) * if blur { 48.0 } else { 100.0 }).round() as u32;
        let mut s = self.settings.borrow_mut();
        if blur { s.appearance.blur = value; } else { s.appearance.transparency = value; }
    }
}

impl Module for SettingsPage {
    fn tab(&self) -> Option<&'static str> {
        Some("Setelan")
    }

    fn expanded_size(&self) -> (f32, f32) {
        (612.0, self.height())
    }

    fn draw_expanded(&mut self, g: &mut Gfx, r: Rect) {
        self.hits.clear();
        self.tracks.clear();
        self.viewport = r;
        self.scroll = self.scroll.clamp(0.0, (self.height() - r.h).max(0.0));
        g.push_clip(r);
        let top = r.y - self.scroll;
        g.text("Tampilan", Rect::new(r.x + 24.0, top + 12.0, r.w - 48.0, 28.0), 20.0, 600, WHITE, Align::Left);
        let s = self.settings.borrow().clone();
        let width = r.w - 48.0;
        for (i, label, value, max) in [(0, "Transparansi", s.appearance.transparency, 100), (1, "Kekuatan blur", s.appearance.blur, 48)] {
            let y = top + 56.0 + i as f32 * 64.0;
            let enabled = i == 0 || self.material_error.is_none();
            g.text(label, Rect::new(r.x + 24.0, y, width - 65.0, 22.0), 13.0, 400, WHITE, Align::Left);
            let number = if i == 0 { format!("{value}%") } else { value.to_string() };
            g.text(&number, Rect::new(r.x + r.w - 78.0, y, 54.0, 22.0), 12.0, 500, DIM, Align::Right);
            let track = Rect::new(r.x + 24.0, y + 25.0, width, 24.0);
            g.fill_rrect(Rect::new(track.x, track.y + 9.0, track.w, 4.0), 2.0, FAINT);
            if enabled {
                let fill = track.w * value.min(max) as f32 / max as f32;
                g.fill_rrect(Rect::new(track.x, track.y + 9.0, fill, 4.0), 2.0, ACCENT);
                g.circle(track.x + fill, track.y + 11.0, 5.0, WHITE);
                self.tracks.push((track, i == 1));
            }
        }
        let status = self.material_error.as_deref().unwrap_or("Prism Black · teks dan ikon tetap jelas");
        g.text(status, Rect::new(r.x + 24.0, top + 181.0, width, 22.0), 11.0, 400, DIM, Align::Left);
        let reset = Rect::new(r.x + 24.0, top + 210.0, 178.0, 30.0);
        g.button(reset, "Pulihkan tampilan awal", crate::theme::SURFACE, WHITE);
        self.hits.push((reset, Hit::Reset));
        let mut y = top + 248.0;
        for (section, label) in [(0, "Umum"), (1, "Fokus"), (2, "Pengingat minum")] {
            let header = Rect::new(r.x + 24.0, y, width, 34.0);
            g.text(label, Rect::new(header.x, header.y, header.w - 28.0, header.h), 12.0, 600, WHITE, Align::Left);
            g.icon(if self.section == Some(section) { '\u{E70E}' } else { '\u{E70D}' },
                Rect::new(header.x + header.w - 24.0, y, 24.0, 34.0), 11.0, DIM);
            self.hits.push((header, Hit::Section(section)));
            y += 34.0;
            if self.section != Some(section) { continue; }
            if section == 0 {
                for t in LEFT.iter() {
                    let row = Rect::new(r.x + 24.0, y, width, ROW);
                    let on = toggle_get(&s, *t);
                    g.text(toggle_label(*t), Rect::new(row.x, row.y, row.w - 44.0, ROW), 12.5, 400, WHITE, Align::Left);
                    let sw = Rect::new(row.x + row.w - 34.0, row.y + (ROW - 18.0) / 2.0, 34.0, 18.0);
                    g.fill_rrect(sw, 9.0, if on { ACCENT } else { FAINT });
                    let kx = if on { sw.x + sw.w - 9.0 } else { sw.x + 9.0 };
                    g.circle(kx, sw.y + 9.0, 7.0, WHITE);
                    self.hits.push((row, Hit::Toggle(*t)));
                    y += ROW;
                }
            } else {
                let steps = if section == 1 { &RIGHT[..3] } else { &RIGHT[3..] };
                for k in steps {
                    let row = Rect::new(r.x + 24.0, y, width, ROW);
                    let (label, v, _, _, _, unit) = step_info(&s, *k);
                    g.text(label, Rect::new(row.x, row.y, row.w - 110.0, ROW), 12.5, 400, WHITE, Align::Left);
                    let minus = Rect::new(row.x + row.w - 104.0, row.y + 3.0, 22.0, ROW - 6.0);
                    let plus = Rect::new(row.x + row.w - 22.0, row.y + 3.0, 22.0, ROW - 6.0);
                    g.fill_rrect(minus, 6.0, FAINT);
                    g.fill_rrect(plus, 6.0, FAINT);
                    g.text("−", minus, 13.0, 600, WHITE, Align::Center);
                    g.text("+", plus, 13.0, 600, WHITE, Align::Center);
                    let value = if unit.is_empty() { format!("{v:02}.00") } else { format!("{v} {unit}") };
                    let vr = Rect::new(minus.x + minus.w, row.y, plus.x - minus.x - minus.w, ROW);
                    g.text(&value, vr, 12.0, 600, DIM, Align::Center);
                    self.hits.push((minus, Hit::Step(*k, -1)));
                    self.hits.push((plus, Hit::Step(*k, 1)));
                    y += ROW;
                }
            }
        }
        g.pop_clip();
    }

    fn click(&mut self, _r: Rect, x: f32, y: f32) -> bool {
        if !self.viewport.contains(x, y) { return false; }
        let Some(hit) = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, hit)| *hit) else { return false };
        if let Hit::Section(section) = hit {
            self.section = if self.section == Some(section) { None } else { Some(section) };
            self.scroll = 0.0;
            return true;
        }
        let mut s = self.settings.borrow_mut();
        match hit {
            Hit::Toggle(t) => toggle_flip(&mut s, t),
            Hit::Step(k, d) => step_apply(&mut s, k, d),
            Hit::Reset => s.appearance = Default::default(),
            Hit::Section(_) => {},
        }
        s.save();
        true
    }
    fn pointer_down(&mut self, _r: Rect, x: f32, y: f32) -> bool {
        if !self.viewport.contains(x, y) { return false; }
        let Some((track, blur)) = self.tracks.iter().find(|(r, _)| r.contains(x, y)).copied() else { return false };
        let previous = self.settings.borrow().appearance.clone();
        self.drag = Some((track, blur, previous));
        self.set_slider(track, blur, x);
        true
    }
    fn pointer_move(&mut self, x: f32, _y: f32) {
        if let Some((track, blur, _)) = self.drag { self.set_slider(track, blur, x); }
    }
    fn pointer_up(&mut self) {
        if self.drag.take().is_some() { self.settings.borrow().save(); }
    }
    fn cancel_pointer(&mut self) {
        if let Some((_, _, previous)) = self.drag.take() { self.settings.borrow_mut().appearance = previous; }
    }
    fn wheel(&mut self, x: f32, y: f32, delta: i16) -> bool {
        if !self.viewport.contains(x, y) { return false; }
        self.scroll = (self.scroll - delta as f32 / 120.0 * 48.0).clamp(0.0, (self.height() - self.viewport.h).max(0.0));
        true
    }
    fn on_event(&mut self, event: &Event) {
        if let Event::Material(error) = event { self.material_error = error.clone(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_clamp_and_keep_hours_ordered() {
        let mut s = Settings::default();
        step_apply(&mut s, Step::Focus, -1);
        assert_eq!(s.pomodoro.focus_min, 20);
        for _ in 0..100 {
            step_apply(&mut s, Step::Focus, 1);
        }
        assert_eq!(s.pomodoro.focus_min, 120);
        s.water.start_hour = 8;
        s.water.end_hour = 9;
        step_apply(&mut s, Step::WaterStart, 1);
        assert_eq!(s.water.start_hour, 8, "mulai tidak boleh menyusul selesai");
        step_apply(&mut s, Step::WaterEnd, -1);
        assert_eq!(s.water.end_hour, 9, "selesai tidak boleh mendahului mulai");
    }
    #[test]
    fn material_slider_cancels_without_touching_other_settings() {
        let shared = std::rc::Rc::new(std::cell::RefCell::new(Settings::default()));
        let mut page = SettingsPage::new(shared.clone());
        let track = Rect::new(20.0, 0.0, 100.0, 28.0);
        let original = shared.borrow().appearance.clone();
        page.drag = Some((track, false, original.clone()));
        page.pointer_move(200.0, 0.0);
        assert_eq!(shared.borrow().appearance.transparency, 100);
        page.cancel_pointer();
        assert_eq!(shared.borrow().appearance, original);
        page.set_slider(track, true, -100.0);
        assert_eq!(shared.borrow().appearance.blur, 0);
        assert!(shared.borrow().auto_hide);
    }
}
