//! Identitas monitor terpisah dari handle/nomor layar yang dapat berubah.
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;

#[derive(Clone, Debug)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub device: String,
    pub rect: RECT,
    pub handle: isize,
    pub scale: f32,
    pub primary: bool,
    /// PnP identity used to match WMI InstanceName exactly, never all displays.
    pub pnp: Option<String>,
    pub ambiguous: bool,
}

fn wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}

pub fn all() -> Vec<Monitor> {
    unsafe extern "system" fn visit(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO).as_bool() { return BOOL(1); }
        let device = wide(&info.szDevice);
        let mut dd = DISPLAY_DEVICEW::default();
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let found = EnumDisplayDevicesW(PCWSTR(info.szDevice.as_ptr()), 0, &mut dd, 1).as_bool();
        let mut extra = DISPLAY_DEVICEW::default();
        extra.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let ambiguous = EnumDisplayDevicesW(PCWSTR(info.szDevice.as_ptr()), 1, &mut extra, 1).as_bool()
            && extra.StateFlags & DISPLAY_DEVICE_ACTIVE != 0;
        let interface = if found { wide(&dd.DeviceID) } else { String::new() };
        let parts: Vec<_> = interface.trim_start_matches("\\\\?\\").split('#').collect();
        let pnp = (parts.len() >= 3).then(|| parts[..3].join("\\"));
        let id = if interface.is_empty() { device.clone() } else { interface };
        let name = if found { wide(&dd.DeviceString) } else { device.clone() };
        let (mut x, mut y) = (96, 96);
        let _ = GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        list.push(Monitor { id, name, device, rect: info.monitorInfo.rcMonitor,
            handle: h.0 as isize, scale: x as f32 / 96.0,
            primary: info.monitorInfo.dwFlags & 1 != 0, pnp, ambiguous });
        BOOL(1)
    }
    let mut list = Vec::new();
    unsafe { let _ = EnumDisplayMonitors(None, None, Some(visit), LPARAM(&mut list as *mut _ as isize)); }
    list
}

pub fn choose<'a>(list: &'a [Monitor], wanted: Option<&str>) -> Option<&'a Monitor> {
    wanted.and_then(|id| list.iter().find(|m| m.id == id))
        .or_else(|| list.iter().find(|m| m.primary)).or_else(|| list.first())
}

pub fn at_point(list: &[Monitor], p: POINT) -> Option<&Monitor> {
    let h = unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST) };
    list.iter().find(|m| m.handle == h.0 as isize)
}

pub fn top_center(rect: RECT, width: i32) -> POINT {
    POINT { x: rect.left + (rect.right - rect.left - width) / 2, y: rect.top }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(id: &str, primary: bool) -> Monitor {
        Monitor { id: id.into(), name: id.into(), device: id.into(), handle: 0,
            rect: RECT::default(), scale: 1.0, primary, pnp: None, ambiguous: false }
    }
    #[test]
    fn selection_preserves_preference_across_disconnect() {
        let list = vec![monitor("main", true), monitor("other", false)];
        assert_eq!(choose(&list, Some("other")).unwrap().id, "other");
        assert_eq!(choose(&list[..1], Some("other")).unwrap().id, "main");
        assert_eq!(choose(&list, Some("other")).unwrap().id, "other");
        assert!(choose(&[], None).is_none());
    }
    #[test]
    fn negative_coordinates_stay_on_target() {
        let p = top_center(RECT { left: -1920, top: -1080, right: 0, bottom: 0 }, 760);
        assert_eq!((p.x, p.y), (-1340, -1080));
    }
}
