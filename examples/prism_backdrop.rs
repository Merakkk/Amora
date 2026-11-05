//! Local visual fixture: a bright pattern behind the notch, closes after 10 min.
//! Does not read or alter Windows appearance settings.
use windows::{core::*, Win32::{Foundation::*, Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*}};
static CLICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            let count = CLICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = SetWindowTextW(hwnd, &HSTRING::from(format!("Amora · latar uji kaca · klik {count}")));
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let colors = [COLORREF(0xF5D8AA), COLORREF(0xA8CFF5), COLORREF(0xD3EBD1), COLORREF(0xF2EEF0)];
            for y in 0..((client.bottom + 63) / 64) as usize {
                for x in 0..((client.right + 63) / 64) as usize {
                    let brush = CreateSolidBrush(colors[(x + y) % colors.len()]);
                    FillRect(dc, &RECT { left: x as i32 * 64, top: y as i32 * 64,
                        right: (x + 1) as i32 * 64, bottom: (y + 1) as i32 * 64 }, brush);
                    let _ = DeleteObject(brush);
                }
            }
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x181818));
            for y in (28..client.bottom).step_by(80) {
                let mut text: Vec<u16> = "AMORA / LATAR UJI KACA / 0123456789".encode_utf16().collect();
                DrawTextW(dc, &mut text, &mut RECT { left: 24, top: y, right: client.right - 24, bottom: y + 30 }, DT_LEFT);
            }
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_TIMER => { let _ = DestroyWindow(hwnd); LRESULT(0) }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() -> Result<()> { unsafe {
    use windows::Win32::UI::HiDpi::*;
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let args: Vec<String> = std::env::args().collect();
    if let (Some(x), Some(y)) = (args.get(1), args.get(2)) {
        point = POINT { x: x.parse().map_err(|_| Error::from(E_INVALIDARG))?,
            y: y.parse().map_err(|_| Error::from(E_INVALIDARG))? };
    }
    let mut monitor = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut monitor).as_bool() {
        return Err(Error::from_win32());
    }
    let instance = GetModuleHandleW(None)?;
    let class = w!("AmoraPrismBackdropTest");
    RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: instance.into(),
        lpszClassName: class, hCursor: LoadCursorW(None, IDC_ARROW)?, ..Default::default() });
    let rect = monitor.rcMonitor;
    let extra = if args.get(3).map(String::as_str) == Some("--topmost") { WS_EX_TOPMOST } else { WINDOW_EX_STYLE(0) };
    let hwnd = CreateWindowExW(WS_EX_APPWINDOW | extra, class, w!("Amora · latar uji kaca"), WS_POPUP,
        rect.left + (rect.right - rect.left - 1100) / 2, rect.top, 1100, rect.bottom - rect.top, None, None, instance, None)?;
    let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    SetTimer(hwnd, 1, 600_000, None);
    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    Ok(())
} }
