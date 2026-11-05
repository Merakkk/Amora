//! Mendeteksi jendela lain yang berada di posisi notch (mis. browser yang dimaksimalkan,
//! tab di atas, atau aplikasi layar penuh). Berbasis event dari Windows: kita hanya
//! menghitung ulang saat ada jendela berpindah, berubah ukuran, muncul, atau hilang.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Timer di jendela notch untuk menghitung ulang (event datang beruntun, jadi ditunda sedikit).
pub const TIMER_RECHECK: usize = 5;
const DEBOUNCE_MS: u32 = 120;

static NOTCH: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Hanya event jendela utuh (bukan kursor, caret, atau elemen di dalam jendela).
    if id_object != OBJID_WINDOW.0 || hwnd.is_invalid() {
        return;
    }
    let notch = HWND(NOTCH.load(Ordering::Relaxed) as _);
    if hwnd == notch {
        return;
    }
    SetTimer(notch, TIMER_RECHECK, DEBOUNCE_MS, None);
}

/// Pasang hook event jendela. Callback berjalan di thread UI lewat loop pesan biasa.
pub fn install(notch: HWND) {
    NOTCH.store(notch.0 as isize, Ordering::Relaxed);
    let ranges = [
        // foreground, pindah/ubah ukuran, minimize/restore
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND),
        // destroy, show, hide, reorder, ... locationchange
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE),
        // jendela UWP yang disembunyikan tanpa ditutup
        (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
    ];
    for (min, max) in ranges {
        unsafe {
            SetWinEventHook(
                min,
                max,
                None,
                Some(on_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn counts(hwnd: HWND, notch: HWND) -> bool {
    unsafe {
        if hwnd == notch || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let ex = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        // Overlay, tooltip, menu, notch lain: bukan "jendela di belakang".
        if ex.contains(WS_EX_TOOLWINDOW) || ex.contains(WS_EX_TRANSPARENT) || ex.contains(WS_EX_NOACTIVATE) {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
        if cloaked != 0 {
            return false;
        }
        !matches!(
            class_name(hwnd).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        )
    }
}

struct Search {
    notch: HWND,
    area: RECT,
    found: bool,
}

unsafe extern "system" fn visit(hwnd: HWND, lp: LPARAM) -> BOOL {
    let s = &mut *(lp.0 as *mut Search);
    if counts(hwnd, s.notch) {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let a = s.area;
            if r.left < a.right && r.right > a.left && r.top < a.bottom && r.bottom > a.top {
                s.found = true;
                return FALSE;
            }
        }
    }
    TRUE
}

/// Ada jendela lain yang menempati `area` (koordinat layar, piksel)?
pub fn covered(notch: HWND, area: RECT) -> bool {
    let mut s = Search { notch, area, found: false };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut s as *mut Search as isize));
    }
    s.found
}

pub fn cursor() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

/// The visible capsule and its resting slot both open Amora. Keeping the resting
/// slot active lets hover recover a capsule that avoidance has already hidden.
pub fn hover_target(p: POINT, visible: RECT, compact: RECT) -> bool {
    let contains = |r: RECT| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom;
    contains(visible) || contains(compact)
}

/// Edge-only reveal uses the monitor's topmost physical pixel row.
pub fn reveal_rect(compact: RECT, edge_only: bool) -> RECT {
    if edge_only {
        RECT { bottom: compact.top + 1, ..compact }
    } else { compact }
}

pub fn opening_target(p: POINT, visible: RECT, compact: RECT, edge_only: bool) -> bool {
    let resting = reveal_rect(compact, edge_only);
    hover_target(p, if edge_only { resting } else { visible }, resting)
}

#[derive(Debug, PartialEq)]
pub enum NavigationHover { Hold, Entered, Left }

pub fn navigation_hover(previous: crate::gfx::Rect, current: crate::gfx::Rect,
    x: f32, y: f32, animating: bool) -> NavigationHover {
    let inside = current.contains(x, y);
    if !inside && !previous.contains(x, y) { NavigationHover::Left }
    else if animating || !inside { NavigationHover::Hold }
    else { NavigationHover::Entered }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_normal_notch_yields_to_tabs_but_opens_from_top_edge() {
        for top in [0, -1080] {
            let compact = RECT { left: -1100, top, right: -860, bottom: top + 32 };
            let capsule = RECT { top: top + 6, bottom: top + 38, ..compact };
            let below = POINT { x: -980, y: top + 20 };
            let edge = POINT { x: -980, y: top };
            assert!(!opening_target(below, capsule, compact, true));
            assert!(opening_target(edge, RECT::default(), compact, true));
            // Ordinary desktop hover and an already-open panel stay interactive.
            assert!(opening_target(below, capsule, compact, false));
            let panel = RECT { left: -1200, top, right: -700, bottom: top + 314 };
            assert!(opening_target(POINT { x: -980, y: top + 200 }, panel, compact, false));
            assert!(!opening_target(POINT { x: -1101, y: top }, capsule, compact, true));
        }
    }

    #[test]
    fn minimal_reveal_requires_the_topmost_pixel_at_every_dpi() {
        for scale in [1.0, 1.25, 2.0] {
            let line = RECT { left: -1100, top: -1080, right: -860,
                bottom: -1080 + (5.0 * scale) as i32 };
            let target = reveal_rect(line, true);
            assert_eq!((target.left, target.top, target.right), (line.left, line.top, line.right));
            assert_eq!(target.bottom - target.top, 1);
            assert!(hover_target(POINT { x: -980, y: line.top }, target, target));
            for offset in [1, 3, 18, 24] {
                assert!(!hover_target(POINT { x: -980, y: line.top + offset }, target, target));
            }
            assert!(!hover_target(POINT { x: -1101, y: line.top }, target, target));
            assert_eq!(reveal_rect(line, false).bottom, line.bottom);
        }
    }

    #[test]
    fn navigation_survives_shrinking_then_releases_on_entry_or_exit() {
        use crate::gfx::Rect;
        let music = Rect::new(74.0, 0.0, 612.0, 314.0);
        let focus = Rect::new(152.0, 0.0, 456.0, 206.0);
        // The cursor stays on the old Music navigation row throughout resizing.
        assert_eq!(navigation_hover(music, music, 245.0, 280.0, true), NavigationHover::Hold);
        assert_eq!(navigation_hover(music, focus, 245.0, 280.0, true), NavigationHover::Hold);
        assert_eq!(navigation_hover(music, focus, 245.0, 280.0, false), NavigationHover::Hold);
        assert_eq!(navigation_hover(music, focus, 300.0, 100.0, false), NavigationHover::Entered);
        assert_eq!(navigation_hover(music, focus, 720.0, 340.0, false), NavigationHover::Left);
        // A larger destination needs no lingering hold once its animation settles.
        assert_eq!(navigation_hover(focus, music, 300.0, 180.0, false), NavigationHover::Entered);
    }

    #[test]
    fn floating_hover_and_hidden_top_edge_work_on_negative_monitors() {
        for (x, y, scale) in [(0, 0, 1), (-1920, -1080, 2)] {
            let compact = RECT { left: x + 200 * scale, top: y,
                right: x + 440 * scale, bottom: y + 32 * scale };
            let visible = RECT { top: y + 6 * scale, bottom: y + 38 * scale, ..compact };
            assert!(hover_target(POINT { x: x + 300 * scale, y: y + 20 * scale }, visible, compact));
            assert!(hover_target(POINT { x: x + 300 * scale, y }, RECT::default(), compact));
            assert!(hover_target(POINT { x: x + 300 * scale, y: y + 20 * scale }, RECT::default(), compact));
            assert!(hover_target(POINT { x: x + 300 * scale, y: y + 4 * scale }, visible, compact));
            assert!(!hover_target(POINT { x: x + 180 * scale, y: y + 20 * scale }, visible, compact));
            assert!(!hover_target(POINT { x: x + 300 * scale, y: y + 45 * scale }, visible, compact));
        }
    }
}
