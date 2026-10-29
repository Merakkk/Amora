//! Live system controls. Snapshot refresh is only performed while this page is visible.
use std::time::{Duration, Instant};
use super::{Event, Module};
use crate::{brightness, mixer, monitors::Monitor, system};
use crate::gfx::{Align, Gfx, Rect, DIM, FAINT, WHITE, Color};

const ACCENT: Color = Color(0.35, 0.85, 0.55, 1.0);
const ROW: f32 = 43.0;
const LIST_HEIGHT: f32 = 129.0;
#[derive(Clone)]
enum Target { Master, Brightness, App(String) }
#[derive(Clone)]
enum Hit { Slider(Target, Rect), Mute(Target, bool) }

pub struct Controls {
    monitor: Option<Monitor>,
    brightness: brightness::Service,
    bright_value: Option<u32>,
    bright_requested: Option<u32>,
    bright_write: bool,
    bright_pending: bool,
    bright_checked: bool,
    master: Option<(f32, bool)>,
    snapshot: Option<mixer::Snapshot>,
    hits: Vec<(Rect, Hit)>,
    drag: Option<(Target, Rect)>,
    list: Rect,
    offset: usize,
    audio_at: Instant,
    brightness_at: Instant,
    error: Option<(String, Instant)>,
}

fn fraction(x: f32, r: Rect) -> f32 { ((x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0) }

fn panel_height(apps: usize, error: bool) -> f32 {
    161.0 + apps.clamp(1, 3) as f32 * ROW + if error { 26.0 } else { 8.0 }
}

impl Controls {
    pub fn new() -> Self {
        let old = Instant::now() - Duration::from_secs(10);
        Self { monitor: None, brightness: brightness::Service::new(), bright_value: None,
            bright_requested: None,
            bright_write: false, bright_pending: false, bright_checked: false,
            master: None, snapshot: None, hits: Vec::new(), drag: None,
            list: Rect::new(0.0, 0.0, 0.0, 0.0), offset: 0,
            audio_at: old, brightness_at: old, error: None }
    }
    fn fail(&mut self, text: &str) { self.error = Some((text.into(), Instant::now())); }
    fn read_audio(&mut self) {
        match system::master_state() {
            Ok(state) => self.master = Some(state),
            Err(_) => self.fail("Volume utama tidak tersedia"),
        }
        match mixer::read() {
            Ok(snapshot) => self.snapshot = Some(snapshot),
            Err(_) => self.fail("Daftar audio tidak dapat diperbarui"),
        }
        self.audio_at = Instant::now();
    }
    fn refresh(&mut self) {
        if self.audio_at.elapsed() >= Duration::from_millis(750) { self.read_audio(); }
        if let Some(reply) = self.brightness.reply() {
            self.bright_pending = false;
            self.bright_checked = true;
            if reply.failed {
                if !self.bright_write { self.bright_value = None; }
                else if !matches!(self.drag, Some((Target::Brightness, _))) {
                    self.bright_requested = None;
                    self.fail("Kecerahan gagal diubah");
                }
            } else {
                self.bright_value = reply.value;
                if !matches!(self.drag, Some((Target::Brightness, _))) { self.bright_requested = None; }
                if self.error.as_ref().is_some_and(|(text, _)| text == "Kecerahan gagal diubah") { self.error = None; }
            }
        }
        if !self.bright_pending && self.drag.is_none() && self.brightness_at.elapsed() >= Duration::from_secs(2) {
            if let Some(m) = &self.monitor {
                self.bright_write = false;
                self.bright_pending = true;
                self.brightness.request(m.clone(), None);
                self.brightness_at = Instant::now();
            }
        }
        if self.error.as_ref().is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(4)) { self.error = None; }
    }
    fn change(&mut self, target: &Target, level: Option<f32>, mute: Option<bool>) {
        let result = match target {
            Target::Master => system::change_master(level, mute),
            Target::App(key) => self.snapshot.as_ref().ok_or_else(windows::core::Error::from_win32)
                .and_then(|s| s.change(key, level, mute)),
            Target::Brightness => {
                if let (Some(m), Some(v)) = (&self.monitor, level) {
                    let requested = (v * 100.0).round() as u32;
                    if self.bright_requested == Some(requested) { return; }
                    self.bright_requested = Some(requested);
                    if self.error.as_ref().is_some_and(|(text, _)| text == "Kecerahan gagal diubah") { self.error = None; }
                    self.bright_write = true;
                    self.bright_pending = true;
                    self.brightness.request(m.clone(), Some(requested));
                    self.brightness_at = Instant::now();
                }
                return;
            }
        };
        if result.is_err() { self.fail("Volume gagal diubah; coba lagi"); }
        // Read actual Windows values, including partial failure in multi-session groups.
        self.read_audio();
    }
    fn row(&mut self, g: &mut Gfx, r: Rect, label: &str, level: Option<f32>, muted: Option<bool>, mixed: bool, target: Target) {
        let label_width = (r.w * 0.34).min(175.0);
        g.text(label, Rect::new(r.x, r.y, label_width, r.h), 12.0, 400, WHITE, Align::Left);
        let mute_w = if muted.is_some() { 42.0 } else { 0.0 };
        let value_w = if mixed { 68.0 } else { 42.0 };
        let track = Rect::new(r.x + label_width + 8.0, r.y + 12.0,
            (r.w - label_width - 18.0 - value_w - mute_w).max(12.0), 20.0);
        let bar = Rect::new(track.x, track.y + 8.0, track.w, 4.0);
        g.fill_rrect(bar, 2.0, FAINT);
        if let Some(v) = level {
            g.fill_rrect(Rect::new(bar.x, bar.y, bar.w * v, bar.h), 2.0, ACCENT);
            g.circle(bar.x + bar.w * v, bar.y + 2.0, 5.0, WHITE);
            self.hits.push((track, Hit::Slider(target.clone(), track)));
        }
        let text = if mixed { "Campuran".into() } else { level.map_or("—".into(), |v| format!("{}%", (v * 100.0).round() as u32)) };
        g.text(&text, Rect::new(track.x + track.w + 5.0, r.y, value_w, r.h), 11.0, 400, DIM, Align::Center);
        if let Some(muted) = muted {
            let button = Rect::new(r.x + r.w - 38.0, r.y + 9.0, 38.0, 25.0);
            g.fill_rrect(button, 6.0, FAINT);
            g.text(if muted { "Bisu" } else { "Suara" }, button, 10.0, 400, WHITE, Align::Center);
            self.hits.push((button, Hit::Mute(target, !muted)));
        }
    }
    fn visible_rows(&self) -> usize { (self.list.h / ROW).floor().max(1.0) as usize }
}

impl Module for Controls {
    fn tab(&self) -> Option<&'static str> { Some("Kontrol") }
    fn prepare(&mut self) { self.refresh(); }
    fn expanded_size(&self) -> (f32, f32) {
        (560.0, panel_height(self.snapshot.as_ref().map_or(0, |s| s.apps.len()), self.error.is_some()))
    }
    fn tick_ms(&self, expanded: bool) -> Option<u32> { expanded.then_some(100) }
    fn draw_expanded(&mut self, g: &mut Gfx, r: Rect) {
        self.hits.clear();
        let x = r.x + 20.0;
        let w = (r.w - 40.0).max(150.0);
        g.text("Kontrol", Rect::new(x, r.y + 5.0, w, 24.0), 15.0, 600, WHITE, Align::Left);
        let master = self.master;
        self.row(g, Rect::new(x, r.y + 30.0, w, ROW), "Volume utama", master.map(|v| v.0), master.map(|v| v.1), false, Target::Master);
        self.row(g, Rect::new(x, r.y + 73.0, w, ROW), "Kecerahan", self.bright_requested.or(self.bright_value).map(|v| v as f32 / 100.0), None, false, Target::Brightness);
        let monitor = self.monitor.as_ref().map(|m| format!("{} ({})", m.name, m.device)).unwrap_or_else(|| "Monitor".into());
        let subtitle = if self.bright_value.is_some() { monitor }
            else if !self.bright_checked { format!("{monitor} · Membaca kecerahan…") }
            else { format!("{monitor} · Kecerahan monitor ini tidak tersedia") };
        g.text(&subtitle, Rect::new(x, r.y + 112.0, w, 20.0), 10.0, 400, DIM, Align::Left);
        g.text("Aplikasi · output utama Windows", Rect::new(x, r.y + 138.0, w, 20.0), 11.0, 600, DIM, Align::Left);
        let footer = if self.error.is_some() { 26.0 } else { 8.0 };
        self.list = Rect::new(x, r.y + 161.0, w, LIST_HEIGHT.min((r.h - 161.0 - footer).max(ROW)));
        let apps = self.snapshot.as_ref().map(|s| s.apps.clone()).unwrap_or_default();
        let visible = self.visible_rows();
        self.offset = self.offset.min(apps.len().saturating_sub(visible));
        if apps.is_empty() {
            g.text("Belum ada aplikasi dengan sesi audio", self.list, 12.0, 400, DIM, Align::Center);
        } else {
            for (i, app) in apps.iter().skip(self.offset).take(visible).enumerate() {
                self.row(g, Rect::new(x, self.list.y + i as f32 * ROW, w - 8.0, ROW), &app.name,
                    Some(app.level), Some(app.muted), app.mixed, Target::App(app.key.clone()));
            }
            if apps.len() > visible {
                let height = self.list.h * visible as f32 / apps.len() as f32;
                let y = self.list.y + (self.list.h - height) * self.offset as f32 / (apps.len() - visible) as f32;
                g.fill_rrect(Rect::new(x + w - 3.0, y, 3.0, height), 1.5, DIM);
            }
        }
        if let Some((error, _)) = &self.error {
            g.text(error, Rect::new(x, r.y + r.h - 22.0, w, 20.0), 10.0, 400, DIM, Align::Left);
        }
    }
    fn pointer_down(&mut self, _r: Rect, x: f32, y: f32) -> bool {
        let hit = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, h)| h.clone());
        match hit {
            Some(Hit::Slider(target, track)) => {
                self.change(&target, Some(fraction(x, track)), None);
                self.drag = Some((target, track));
                true
            }
            Some(Hit::Mute(target, mute)) => { self.change(&target, None, Some(mute)); true }
            None => false,
        }
    }
    fn pointer_move(&mut self, x: f32, _y: f32) {
        if let Some((target, track)) = self.drag.clone() { self.change(&target, Some(fraction(x, track)), None); }
    }
    fn pointer_up(&mut self) {
        if matches!(self.drag.take(), Some((Target::Brightness, _))) {
            // Always commit the final position, even if an intermediate driver call failed.
            if let (Some(monitor), Some(value)) = (&self.monitor, self.bright_requested) {
                self.brightness.request(monitor.clone(), Some(value));
                self.bright_pending = true;
                self.bright_write = true;
                self.brightness_at = Instant::now();
            }
        }
    }
    fn wheel(&mut self, x: f32, y: f32, delta: i16) -> bool {
        if !self.list.contains(x, y) { return false; }
        let max = self.snapshot.as_ref().map_or(0, |s| s.apps.len().saturating_sub(self.visible_rows()));
        self.offset = if delta > 0 { self.offset.saturating_sub(1) } else { (self.offset + 1).min(max) };
        true
    }
    fn on_event(&mut self, event: &Event) {
        match event {
            Event::MonitorChanged(monitor) => {
                self.drag = None;
                self.brightness.invalidate();
                self.monitor = Some(monitor.clone());
                self.bright_value = None;
                self.bright_requested = None;
                self.bright_pending = false;
                self.bright_checked = false;
                self.brightness_at = Instant::now() - Duration::from_secs(10);
            }
            Event::Volume { .. } => self.audio_at = Instant::now() - Duration::from_secs(10),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slider_maps_and_clamps_captured_pointer() {
        let r = Rect::new(20.0, 0.0, 100.0, 20.0);
        assert_eq!(fraction(-50.0, r), 0.0);
        assert_eq!(fraction(70.0, r), 0.5);
        assert_eq!(fraction(500.0, r), 1.0);
    }
    #[test]
    fn page_grows_with_apps_then_caps_at_three_rows() {
        assert_eq!(panel_height(0, false), panel_height(1, false));
        assert_eq!(panel_height(2, false) - panel_height(1, false), ROW);
        assert_eq!(panel_height(10, false), panel_height(3, false));
        assert_eq!(panel_height(1, true) - panel_height(1, false), 18.0);
    }
}
