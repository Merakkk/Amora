//! Hardware brightness on a dedicated COM worker. Requests are coalesced while dragging.
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::Duration;
use windows::core::*;
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::Com::*;
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Wmi::*;
use crate::monitors::{self, Monitor};

struct Request { serial: u64, epoch: u64, monitor: Monitor, value: Option<u32> }
pub struct Reply { pub serial: u64, pub value: Option<u32>, pub failed: bool }
pub struct Service { tx: Sender<Request>, rx: Receiver<Reply>, serial: u64, epoch: u64, latest: Arc<AtomicU64> }

fn coalesced_value(previous: Option<u32>, next: Option<u32>, same_target: bool) -> Option<u32> {
    if same_target { next.or(previous) } else { next }
}

impl Service {
    pub fn new() -> Self {
        let (tx, requests) = mpsc::channel::<Request>();
        let (replies, rx) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = latest.clone();
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() { return; }
            let mut backend: Option<(u64, String, Backend)> = None;
            while let Ok(mut request) = requests.recv() {
                // Bounded latency: drain currently queued requests, keeping the last write
                // when a refresh for the same monitor follows it.
                std::thread::sleep(Duration::from_millis(60));
                while let Ok(mut next) = requests.try_recv() {
                    next.value = coalesced_value(request.value, next.value,
                        next.epoch == request.epoch && next.monitor.id == request.monitor.id);
                    request = next;
                }
                if !current_reply(request.serial, worker_latest.load(Ordering::Acquire)) { continue; }
                let result = (|| {
                    if backend.as_ref().is_none_or(|(epoch, id, _)| *epoch != request.epoch || *id != request.monitor.id) {
                        backend = Some((request.epoch, request.monitor.id.clone(), Backend::open(&request.monitor)?));
                    }
                    backend.as_ref().unwrap().2.apply(request.value)
                })();
                if result.is_err() { backend = None; } // Reconnect after a device/provider failure.
                let reply = Reply { serial: request.serial, value: result.as_ref().ok().copied(), failed: result.is_err() };
                if replies.send(reply).is_err() { break; }
            }
            drop(backend); // Release COM objects before uninitializing the worker apartment.
            CoUninitialize();
        });
        Self { tx, rx, serial: 0, epoch: 0, latest }
    }
    pub fn request(&mut self, monitor: Monitor, value: Option<u32>) {
        self.serial += 1;
        self.latest.store(self.serial, Ordering::Release);
        let _ = self.tx.send(Request { serial: self.serial, epoch: self.epoch, monitor, value });
    }
    pub fn invalidate(&mut self) {
        self.serial += 1;
        self.epoch += 1;
        self.latest.store(self.serial, Ordering::Release);
    }
    pub fn reply(&self) -> Option<Reply> {
        let mut latest = None;
        loop {
            match self.rx.try_recv() {
                Ok(reply) => if current_reply(reply.serial, self.serial) { latest = Some(reply); },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    latest = Some(Reply { serial: self.serial, value: None, failed: true });
                    break;
                }
            }
        }
        latest
    }
}

fn current_reply(reply: u64, requested: u64) -> bool { reply == requested }

/// COM must already be initialized on the calling diagnostics thread.
pub fn probe(monitor: &Monitor) -> Result<u32> { unsafe { operate(monitor, None) } }

fn normalize_instance(s: &str) -> String {
    s.trim_end_matches("_0").to_ascii_lowercase()
}

unsafe fn property(o: &IWbemClassObject, name: PCWSTR) -> Result<VARIANT> {
    let mut v = VARIANT::default();
    o.Get(name, 0, &mut v, None, None)?;
    Ok(v)
}

fn wmi_number(value: u32) -> VARIANT { VARIANT::from(value as i32) }

unsafe fn query(s: &IWbemServices, q: &str) -> Result<Vec<IWbemClassObject>> {
    let e = s.ExecQuery(&BSTR::from("WQL"), &BSTR::from(q), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)?;
    let mut objects = Vec::new();
    loop {
        let mut item = [None];
        let mut n = 0;
        e.Next(2000, &mut item, &mut n).ok()?;
        if n == 0 { break; }
        if let Some(o) = item[0].take() { objects.push(o); }
    }
    Ok(objects)
}

struct WmiPanel {
    service: IWbemServices,
    brightness_path: BSTR,
    method_path: BSTR,
    signature: IWbemClassObject,
}

impl WmiPanel {
unsafe fn open(monitor: &Monitor) -> Result<Self> {
    let pnp = monitor.pnp.as_ref().ok_or_else(|| Error::from_hresult(E_NOTIMPL))?;
    let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
    let empty = BSTR::new();
    let s = locator.ConnectServer(&BSTR::from("ROOT\\WMI"), &empty, &empty, &empty, 0, &empty, None)?;
    CoSetProxyBlanket(&s, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE, None, RPC_C_AUTHN_LEVEL_CALL,
        RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)?;
    let find = |items: Vec<IWbemClassObject>| -> Result<IWbemClassObject> {
        let mut matched = items.into_iter().filter(|o| {
            property(o, w!("InstanceName")).ok().and_then(|v| BSTR::try_from(&v).ok())
                .is_some_and(|n| normalize_instance(&n.to_string()) == normalize_instance(pnp))
        });
        let item = matched.next().ok_or_else(|| Error::from_hresult(E_NOTIMPL))?;
        if matched.next().is_some() { return Err(Error::from_hresult(E_NOTIMPL)); }
        Ok(item)
    };
    let brightness = find(query(&s, "SELECT * FROM WmiMonitorBrightness WHERE Active = TRUE")?)?;
    let method = find(query(&s, "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active = TRUE")?)?;
        let mut class = None;
        s.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), WBEM_GENERIC_FLAG_TYPE(0), None, Some(&mut class), None)?;
        let mut signature = None;
        class.ok_or_else(|| Error::from_hresult(E_NOTIMPL))?.GetMethod(w!("WmiSetBrightness"), 0, &mut signature, std::ptr::null_mut())?;
    Ok(Self {
        brightness_path: BSTR::try_from(&property(&brightness, w!("__PATH"))?)?,
        method_path: BSTR::try_from(&property(&method, w!("__PATH"))?)?,
        signature: signature.ok_or_else(|| Error::from_hresult(E_NOTIMPL))?,
        service: s,
    })
}

unsafe fn read(&self) -> Result<u32> {
    let mut object = None;
    self.service.GetObject(&self.brightness_path, WBEM_GENERIC_FLAG_TYPE(0), None, Some(&mut object), None)?;
    let object = object.ok_or_else(|| Error::from_hresult(E_FAIL))?;
    Ok(u32::try_from(&property(&object, w!("CurrentBrightness"))?)?.min(100))
}

unsafe fn write(&self, v: u32) -> Result<u32> {
        let params = self.signature.SpawnInstance(0)?;
        // WMI represents CIM_UINT8 and CIM_UINT32 inputs as VT_I4, not VT_UI1/UI4.
        params.Put(w!("Timeout"), 0, &wmi_number(0), 0)?;
        params.Put(w!("Brightness"), 0, &wmi_number(v.min(100)), 0)?;
        let mut output = None;
        self.service.ExecMethod(&self.method_path, &BSTR::from("WmiSetBrightness"), WBEM_GENERIC_FLAG_TYPE(0), None, &params, Some(&mut output), None)?;
        let output = output.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        if u32::try_from(&property(&output, w!("ReturnValue"))?)? != 0 { return Err(Error::from_hresult(E_FAIL)); }
        // ReturnValue=0 acknowledges the write. Some panels publish their new
        // brightness later; a stale/failed immediate read is not a failed write.
        Ok(v.min(100))
}
}

struct Physical(Vec<PHYSICAL_MONITOR>);
impl Drop for Physical {
    fn drop(&mut self) { unsafe { let _ = DestroyPhysicalMonitors(&self.0); } }
}

fn percent(value: u32, min: u32, max: u32) -> u32 {
    if max <= min { return 0; }
    (((value.clamp(min, max) - min) as f64 * 100.0) / (max - min) as f64).round() as u32
}
fn raw(value: u32, min: u32, max: u32) -> u32 {
    min + ((max.saturating_sub(min) as f64 * value.min(100) as f64 / 100.0).round() as u32)
}

unsafe fn ddc(monitor: &Monitor, value: Option<u32>) -> Result<u32> {
    let h = HMONITOR(monitor.handle as *mut _);
    let mut count = 0;
    GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut count)?;
    // Mirrored/tiled targets are ambiguous: do not change multiple physical panels.
    if count != 1 { return Err(Error::from_hresult(E_NOTIMPL)); }
    let mut physical = Physical(vec![PHYSICAL_MONITOR::default(); count as usize]);
    GetPhysicalMonitorsFromHMONITOR(h, &mut physical.0)?;
    let handle = physical.0[0].hPhysicalMonitor;
    let (mut caps, mut temps) = (0, 0);
    if GetMonitorCapabilities(handle, &mut caps, &mut temps) == 0 || caps & MC_CAPS_BRIGHTNESS == 0 {
        return Err(Error::from_hresult(E_NOTIMPL));
    }
    let (mut min, mut now, mut max) = (0, 0, 0);
    if GetMonitorBrightness(handle, &mut min, &mut now, &mut max) == 0 || max <= min { return Err(Error::from_hresult(E_FAIL)); }
    if let Some(v) = value {
        if SetMonitorBrightness(handle, raw(v, min, max)) == 0 { return Err(Error::from_win32()); }
        return Ok(v.min(100));
    }
    Ok(percent(now, min, max))
}

struct Backend { monitor: Monitor, wmi: Option<WmiPanel> }

impl Backend {
unsafe fn open(wanted: &Monitor) -> Result<Self> {
    // Re-resolve handles after topology changes; never reuse a disconnected handle.
    let list = monitors::all();
    let monitor = list.iter().find(|m| m.id == wanted.id).ok_or_else(|| Error::from_hresult(E_ABORT))?;
    if monitor.ambiguous { return Err(Error::from_hresult(E_NOTIMPL)); }
    let wmi = WmiPanel::open(monitor).ok();
    if wmi.is_none() { ddc(monitor, None)?; }
    Ok(Self { monitor: monitor.clone(), wmi })
}

unsafe fn read(&self) -> Result<u32> {
    match &self.wmi { Some(panel) => panel.read(), None => ddc(&self.monitor, None) }
}

unsafe fn write(&self, value: u32) -> Result<u32> {
    match &self.wmi { Some(panel) => panel.write(value), None => ddc(&self.monitor, Some(value)) }
}

unsafe fn apply(&self, value: Option<u32>) -> Result<u32> {
    // Cached physical handles/paths must never be used for a removed monitor.
    if !monitors::all().iter().any(|m| m.id == self.monitor.id && m.handle == self.monitor.handle && !m.ambiguous) {
        return Err(Error::from_hresult(E_ABORT));
    }
    let Some(value) = value else { return self.read() };
    match self.write(value) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Busy drivers may apply a write despite returning an error. Check
            // the actual level, then retry once before reporting a real failure.
            std::thread::sleep(Duration::from_millis(100));
            if applied(value, self.read().ok()) { return Ok(value); }
            match self.write(value) {
                Ok(v) => Ok(v),
                Err(error) => {
                    std::thread::sleep(Duration::from_millis(100));
                    if applied(value, self.read().ok()) { Ok(value) } else { Err(error) }
                }
            }
        }
    }
}
}

fn applied(wanted: u32, actual: Option<u32>) -> bool {
    actual.is_some_and(|v| v.abs_diff(wanted) <= 1)
}

unsafe fn operate(wanted: &Monitor, value: Option<u32>) -> Result<u32> {
    Backend::open(wanted)?.apply(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_error_is_accepted_only_when_hardware_confirms_target() {
        assert!(applied(25, Some(25)));
        assert!(applied(25, Some(24)));
        assert!(!applied(25, Some(20)));
        assert!(!applied(25, None));
    }
    #[test]
    fn wmi_unsigned_cim_input_uses_required_signed_variant() {
        let value = wmi_number(25);
        assert_eq!(unsafe { value.as_raw().Anonymous.Anonymous.vt }, 3); // VT_I4
        assert_eq!(u32::try_from(&value).unwrap(), 25);
    }
    #[test]
    fn hardware_ranges_and_clamping() {
        assert_eq!(percent(60, 20, 100), 50);
        assert_eq!(raw(50, 20, 100), 60);
        assert_eq!(raw(200, 20, 100), 100);
        assert_eq!(percent(0, 20, 100), 0);
    }
    #[test]
    fn stale_results_are_rejected_even_after_returning_to_same_monitor() {
        assert!(!current_reply(3, 5));
        assert!(current_reply(5, 5));
    }
    #[test]
    fn pending_writes_coalesce_only_within_same_monitor_epoch() {
        assert_eq!(coalesced_value(Some(30), Some(70), true), Some(70));
        assert_eq!(coalesced_value(Some(30), None, true), Some(30));
        assert_eq!(coalesced_value(Some(30), None, false), None);
    }
    #[test]
    fn wmi_matching_is_exact_except_suffix_and_case() {
        assert_eq!(normalize_instance("DISPLAY\\ABC\\123_0"), normalize_instance("display\\abc\\123"));
        assert_ne!(normalize_instance("DISPLAY\\ABC\\123_0"), normalize_instance("DISPLAY\\ABC\\124_0"));
    }
}
