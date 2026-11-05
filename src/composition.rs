//! Click-through composition window behind the existing layered foreground.
//! Visual and HWND clips keep the material and its input region inside the notch.
use std::ffi::c_void;
use windows::core::*;
use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Foundation::Numerics::Vector2;
use windows::Graphics::{IGeometrySource2D, IGeometrySource2D_Impl};
use windows::Graphics::Effects::*;
use windows::System::DispatcherQueueController;
use windows::UI::Composition::*;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::ViewManagement::{AccessibilitySettings, UISettings};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::*;
use windows::Win32::System::WinRT::Composition::*;
use windows::Win32::System::WinRT::Graphics::Direct2D::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::gfx::Rect;

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct BlurEffect { source: IGraphicsEffectSource }
impl IGraphicsEffectSource_Impl for BlurEffect_Impl {}
impl IGraphicsEffect_Impl for BlurEffect_Impl {
    fn Name(&self) -> Result<HSTRING> { Ok("Blur".into()) }
    fn SetName(&self, _: &HSTRING) -> Result<()> { Err(E_NOTIMPL.into()) }
}
impl IGraphicsEffectD2D1Interop_Impl for BlurEffect_Impl {
    fn GetEffectId(&self) -> Result<GUID> { Ok(CLSID_D2D1GaussianBlur) }
    fn GetNamedPropertyMapping(&self, name: &PCWSTR, index: *mut u32, mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING) -> Result<()> {
        if unsafe { name.to_string()? } != "Amount" { return Err(E_INVALIDARG.into()); }
        if index.is_null() || mapping.is_null() { return Err(E_POINTER.into()); }
        unsafe { *index = 0; *mapping = GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT; }
        Ok(())
    }
    fn GetPropertyCount(&self) -> Result<u32> { Ok(3) }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        match index {
            0 => PropertyValue::CreateSingle(24.0)?.cast(),
            1 => PropertyValue::CreateUInt32(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED.0 as u32)?.cast(),
            2 => PropertyValue::CreateUInt32(D2D1_BORDER_MODE_HARD.0 as u32)?.cast(),
            _ => Err(E_INVALIDARG.into()),
        }
    }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        if index == 0 { Ok(self.source.clone()) } else { Err(E_INVALIDARG.into()) }
    }
    fn GetSourceCount(&self) -> Result<u32> { Ok(1) }
}

#[implement(IGeometrySource2D, IGeometrySource2DInterop)]
struct Geometry { geometry: ID2D1Geometry }
impl IGeometrySource2D_Impl for Geometry_Impl {}
impl IGeometrySource2DInterop_Impl for Geometry_Impl {
    fn GetGeometry(&self) -> Result<ID2D1Geometry> { Ok(self.geometry.clone()) }
    fn TryGetGeometryUsingFactory(&self, _: Option<&ID2D1Factory>) -> Result<ID2D1Geometry> {
        Ok(self.geometry.clone())
    }
}

struct GlassWindow(HWND);
impl Drop for GlassWindow {
    fn drop(&mut self) { unsafe { let _ = DestroyWindow(self.0); } }
}

fn input_outline(x: f32, w: f32, h: f32, radius: f32, ear: f32) -> Vec<POINT> {
    let mut outline = vec![POINT { x: (x - ear).round() as i32, y: 0 }];
    let mut arc = |cx: f32, cy: f32, radius: f32, start: f32, end: f32| {
        for step in 0..=16 {
            let angle = (start + (end - start) * step as f32 / 16.0).to_radians();
            outline.push(POINT { x: (cx + radius * angle.cos()).round() as i32,
                y: (cy + radius * angle.sin()).round() as i32 });
        }
    };
    arc(x - ear, ear, ear, -90.0, 0.0);
    arc(x + radius, h - radius, radius, 180.0, 90.0);
    arc(x + w - radius, h - radius, radius, 90.0, 0.0);
    arc(x + w + ear, ear, ear, 180.0, 270.0);
    outline
}

unsafe extern "system" fn background_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub struct Backdrop {
    target: DesktopWindowTarget,
    root: ContainerVisual,
    material: SpriteVisual,
    path: CompositionPathGeometry,
    effect: CompositionEffectBrush,
    compositor: Compositor,
    factory: ID2D1Factory,
    last_shape: Option<(i32, i32, i32, i32)>,
    last_blur: f32,
    window: GlassWindow,
    _queue: DispatcherQueueController,
}

impl Backdrop {
    pub fn new(foreground: HWND) -> Result<Self> { unsafe {
        let advanced = UISettings::new()?.AdvancedEffectsEnabled()?;
        let high_contrast = AccessibilitySettings::new()?.HighContrast()?;
        if !advanced || high_contrast {
            return Err(Error::new(E_NOTIMPL, "efek transparansi Windows dimatikan atau kontras tinggi aktif"));
        }
        let queue = CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT, apartmentType: DQTAT_COM_NONE,
        })?;
        let instance = GetModuleHandleW(None)?;
        let class = w!("AmoraPrismBackdrop");
        let registered = RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(background_proc),
            lpszClassName: class, hInstance: instance.into(), ..Default::default() });
        if registered == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS { return Err(Error::from_win32()); }
        let mut bounds = RECT::default();
        GetWindowRect(foreground, &mut bounds)?;
        let window = GlassWindow(CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST |
            WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class, w!("Amora Prism material"), WS_POPUP | WS_DISABLED, bounds.left, bounds.top,
            (bounds.right - bounds.left).max(1), (bounds.bottom - bounds.top).max(1),
            None, None, instance, None)?);
        let enabled = TRUE;
        DwmSetWindowAttribute(window.0, DWMWA_USE_HOSTBACKDROPBRUSH,
            &enabled as *const _ as *const c_void, std::mem::size_of::<BOOL>() as u32)?;
        let compositor = Compositor::new()?;
        let source = CompositionEffectSourceParameter::Create(&HSTRING::from("backdrop"))?;
        let effect: IGraphicsEffect = BlurEffect { source: source.cast()? }.into();
        let names: windows::Foundation::Collections::IIterable<HSTRING> = vec![HSTRING::from("Blur.Amount")].try_into()?;
        let effect = compositor.CreateEffectFactoryWithProperties(&effect, &names)?.CreateBrush()?;
        effect.SetSourceParameter(&HSTRING::from("backdrop"), &compositor.CreateBackdropBrush()?)?;
        let root = compositor.CreateContainerVisual()?;
        let material = compositor.CreateSpriteVisual()?;
        material.SetBrush(&effect)?;
        root.Children()?.InsertAtBottom(&material)?;
        let path = compositor.CreatePathGeometry()?;
        root.SetClip(&compositor.CreateGeometricClipWithGeometry(&path)?)?;
        let target = compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(window.0, false)?;
        target.SetRoot(&root)?;
        let factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        Ok(Self { target, root, material, path, effect, compositor, factory, last_shape: None,
            last_blur: -1.0, window, _queue: queue })
    } }

    pub fn present(&mut self, foreground: HWND, pos: POINT, size: (i32, i32),
        island: Rect, scale: f32, blur: u32) -> Result<()> { unsafe {
        self.root.SetSize(Vector2 { X: size.0 as f32, Y: size.1 as f32 })?;
        self.material.SetSize(Vector2 { X: size.0 as f32, Y: size.1 as f32 })?;
        let amount = blur.min(48) as f32 * scale;
        if self.last_blur != amount {
            self.effect.Properties()?.InsertScalar(&HSTRING::from("Blur.Amount"), amount)?;
            self.last_blur = amount;
        }
        let key = ((island.x * scale).round() as i32, (island.w * scale).round() as i32,
            (island.h * scale).round() as i32, (scale * 100.0) as i32);
        if self.last_shape != Some(key) {
            self.set_shape(island, scale)?;
            self.last_shape = Some(key);
        }
        if blur == 0 || island.h <= 0.0 {
            let _ = ShowWindow(self.window.0, SW_HIDE);
        } else {
            // Insert directly behind the interactive foreground without activation.
            SetWindowPos(self.window.0, foreground, pos.x, pos.y, size.0, size.1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW)?;
        }
        Ok(())
    } }

    unsafe fn set_shape(&self, r: Rect, scale: f32) -> Result<()> {
        let x = r.x * scale;
        let w = r.w * scale;
        let h = r.h.max(0.0) * scale;
        let radius = (r.h * 0.5).min(22.0) * scale;
        let ear = (r.h * 0.35).min(8.0) * scale;
        let k = 0.5522848;
        let point = |x, y| D2D_POINT_2F { x, y };
        let geometry = self.factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.BeginFigure(point(x - ear, 0.0), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: point(x - ear + k * ear, 0.0),
            point2: point(x, ear - k * ear), point3: point(x, ear) });
        sink.AddLine(point(x, h - radius));
        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: point(x, h - radius + k * radius),
            point2: point(x + radius - k * radius, h), point3: point(x + radius, h) });
        sink.AddLine(point(x + w - radius, h));
        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: point(x + w - radius + k * radius, h),
            point2: point(x + w, h - radius + k * radius), point3: point(x + w, h - radius) });
        sink.AddLine(point(x + w, ear));
        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: point(x + w, ear - k * ear),
            point2: point(x + w + ear - k * ear, 0.0), point3: point(x + w + ear, 0.0) });
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        let source: IGeometrySource2D = Geometry { geometry: geometry.cast()? }.into();
        self.path.SetPath(&CompositionPath::Create(&source)?)?;
        // Visual clipping does not limit the HWND's hit-test rectangle. Clip the
        // input window too, so points outside the notch reach other processes.
        let outline = input_outline(x, w, h, radius, ear);
        let region = if h <= 0.0 { CreateRectRgn(0, 0, 0, 0) } else { CreatePolygonRgn(&outline, WINDING) };
        if region.is_invalid() { return Err(Error::from_win32()); }
        if SetWindowRgn(self.window.0, region, false) == 0 {
            let _ = DeleteObject(region);
            return Err(Error::from_win32());
        }
        Ok(())
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        let _ = self.target.SetRoot(None::<&Visual>);
        let _ = self.target.Close();
        let _ = self.compositor.Close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(points: &[POINT], x: f32, y: f32) -> bool {
        let mut inside = false;
        let mut previous = points.last().unwrap();
        for point in points {
            let (ax, ay, bx, by) = (point.x as f32, point.y as f32, previous.x as f32, previous.y as f32);
            if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
                inside = !inside;
            }
            previous = point;
        }
        inside
    }

    #[test]
    fn material_hit_region_excludes_empty_canvas_at_multiple_dpi() {
        for scale in [1.0, 1.25, 2.0] {
            let polygon = input_outline(74.0 * scale, 612.0 * scale, 424.0 * scale, 22.0 * scale, 8.0 * scale);
            assert!(contains(&polygon, 380.0 * scale, 40.0 * scale));
            assert!(contains(&polygon, 380.0 * scale, 420.0 * scale));
            assert!(!contains(&polygon, 380.0 * scale, 500.0 * scale));
            assert!(!contains(&polygon, 40.0 * scale, 200.0 * scale));
            assert!(!contains(&polygon, 70.0 * scale, 7.0 * scale));
            assert!(!contains(&polygon, 75.0 * scale, 423.0 * scale));
        }
    }
}
