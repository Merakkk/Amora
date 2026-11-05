//! Shared Prism Black material and navigation metrics, all in DIP.
use crate::gfx::{Color, Rect};

pub const ACCENT: Color = Color(0.72, 0.73, 0.94, 1.0);
pub const INK: Color = Color(0.08, 0.085, 0.105, 1.0);
pub const SURFACE: Color = Color(1.0, 1.0, 1.0, 0.055);
pub const SELECTED: Color = Color(0.62, 0.61, 0.78, 0.25);
pub const NAV_CELL: f32 = 54.0;
pub const NAV_ROW: f32 = 48.0;
pub const NAV_GAP: f32 = 12.0;

pub fn nav_rows(width: f32, count: usize) -> usize {
    let columns = ((width - 24.0) / NAV_CELL).floor().max(1.0) as usize;
    count.max(1).div_ceil(columns)
}

pub fn nav_rects(r: Rect, count: usize, layout_width: f32) -> Vec<Rect> {
    // Use the settled layout width, never the spring's transient undershoot.
    let columns = ((layout_width - 24.0) / NAV_CELL).floor().max(1.0) as usize;
    (0..count).map(|i| {
        let row = i / columns;
        let n = (count - row * columns).min(columns);
        let x = r.x + (r.w - n as f32 * NAV_CELL) / 2.0;
        Rect::new(x + (i % columns) as f32 * NAV_CELL + (NAV_CELL - 42.0) / 2.0,
            r.y + row as f32 * NAV_ROW + 4.0, 42.0, 38.0)
    }).collect()
}

pub fn page_icon(name: &str) -> char {
    match name {
        "Musik" => '\u{E8D6}', "Fokus" => '\u{E916}', "Minum" => '\u{E9CE}',
        "To-do" => '\u{E73A}', "Jadwal" => '\u{E787}', "Shelf" => '\u{E8B7}',
        "Clipboard" => '\u{E77F}', "Kontrol" => '\u{E9E9}', "Setelan" => '\u{E713}',
        _ => '\u{E10C}',
    }
}

pub fn draw_icon(g: &mut crate::gfx::Gfx, name: &str, r: Rect, color: Color) {
    let points: &[(f32, f32)] = match name {
        "Minum" => &[(9.0, 0.0), (3.0, 8.0), (1.5, 12.0), (2.0, 15.0), (4.0, 17.0),
            (7.0, 18.0), (11.0, 18.0), (14.0, 17.0), (16.0, 15.0), (16.5, 12.0), (15.0, 8.0), (9.0, 0.0)],
        "Shelf" => &[(0.0, 5.0), (18.0, 5.0), (18.0, 18.0), (0.0, 18.0), (0.0, 5.0),
            (0.0, 11.0), (5.0, 11.0), (7.0, 14.0), (11.0, 14.0), (13.0, 11.0), (18.0, 11.0)],
        _ => { g.icon(page_icon(name), r, 15.0, color); return; }
    };
    let x = r.x + (r.w - 15.0) / 2.0;
    let y = r.y + (r.h - 15.0) / 2.0;
    let scale = 15.0 / 18.0;
    let shifted: Vec<_> = points.iter().map(|p| (x + p.0 * scale, y + p.1 * scale)).collect();
    g.polyline(&shifted, 1.15, color);
    if name == "Shelf" { g.polyline(&[(x + 4.0 * scale, y + 5.0 * scale), (x + 4.0 * scale, y + scale),
        (x + 14.0 * scale, y + scale), (x + 14.0 * scale, y + 5.0 * scale)], 1.15, color); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_wraps_without_overlapping_hit_targets() {
        for width in [240.0, 320.0, 456.0, 640.0] {
            let rects = nav_rects(Rect::new(0.0, 0.0, width, 96.0), 9, width);
            for (i, a) in rects.iter().enumerate() {
                assert!(a.x >= 0.0 && a.x + a.w <= width);
                assert!(a.w >= 36.0 && a.h >= 36.0);
                for b in rects.iter().skip(i + 1) {
                    assert!(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y);
                }
            }
            assert_eq!(rects.last().unwrap().y as usize / NAV_ROW as usize + 1, nav_rows(width, 9));
        }
    }

    #[test]
    fn navigation_does_not_wrap_during_spring_undershoot() {
        for (count, settled) in [(8, 456.0), (9, 510.0)] {
            let mut width = crate::anim::Spring::new(612.0);
            width.set(settled);
            for _ in 0..120 {
                width.step(1.0 / 60.0);
                let rects = nav_rects(Rect::new((760.0 - width.value) / 2.0, 0.0, width.value, NAV_ROW), count, settled);
                assert!(rects.iter().all(|r| r.y == 4.0), "icons must stay in one row during resize");
                for pair in rects.windows(2) { assert!((pair[1].x - pair[0].x - NAV_CELL).abs() < 0.001); }
            }
        }
    }
}
