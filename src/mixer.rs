//! Snapshot of sessions on the current default render endpoint. No saved volumes.
use std::collections::BTreeMap;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::*;

#[derive(Clone)]
pub struct AppVolume {
    pub key: String,
    pub name: String,
    pub level: f32,
    pub muted: bool,
    pub mixed: bool,
    controls: Vec<ISimpleAudioVolume>,
}

pub struct Snapshot {
    pub endpoint: String,
    pub apps: Vec<AppVolume>,
}

fn owned_string(p: PWSTR) -> String {
    let s = unsafe { p.to_string().unwrap_or_default() };
    unsafe { CoTaskMemFree(Some(p.0 as _)); }
    s
}

fn endpoint() -> Result<IMMDevice> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        en.GetDefaultAudioEndpoint(eRender, eConsole)
    }
}

enum Process { Path(String), Protected, Exited }

fn process_path(pid: u32) -> Process {
    unsafe {
        let h = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(e) if e.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => return Process::Exited,
            Err(_) => return Process::Protected,
        };
        let mut path = vec![0u16; 32768];
        let mut n = path.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut n).is_ok();
        let mut exit = 0;
        let exited = GetExitCodeProcess(h, &mut exit).is_ok() && exit != 259;
        let _ = CloseHandle(h);
        if exited { Process::Exited }
        else if ok { Process::Path(String::from_utf16_lossy(&path[..n as usize])) }
        else { Process::Protected }
    }
}

fn mixed(a: f32, am: bool, b: f32, bm: bool) -> bool {
    (a - b).abs() > 0.005 || am != bm
}

fn group<'a>(groups: &'a mut BTreeMap<String, AppVolume>, key: String, name: String, level: f32, muted: bool) -> &'a mut AppVolume {
    let group = groups.entry(key.clone()).or_insert_with(|| AppVolume {
        key, name, level, muted, mixed: false, controls: Vec::new(),
    });
    group.mixed |= mixed(group.level, group.muted, level, muted);
    group
}

pub fn read() -> Result<Snapshot> {
    unsafe {
        let dev = endpoint()?;
        let id = owned_string(dev.GetId()?);
        let manager: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        let mut groups: BTreeMap<String, AppVolume> = BTreeMap::new();
        for i in 0..sessions.GetCount()? {
            let Ok(base) = sessions.GetSession(i) else { continue };
            if base.GetState().ok() == Some(AudioSessionStateExpired) { continue; }
            let Ok(control) = base.cast::<IAudioSessionControl2>() else { continue };
            let Ok(volume) = base.cast::<ISimpleAudioVolume>() else { continue };
            let (Ok(level), Ok(mute)) = (volume.GetMasterVolume(), volume.GetMute()) else { continue };
            let system = control.IsSystemSoundsSession() == HRESULT(0);
            let pid = control.GetProcessId().unwrap_or(0);
            let (key, name) = if system {
                ("system".to_string(), "Suara sistem".to_string())
            } else {
                match process_path(pid) {
                    Process::Path(path) => {
                        let name = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("Aplikasi").to_string();
                        (path.to_lowercase(), name)
                    }
                    Process::Exited => continue,
                    // Protected processes must not be combined with unrelated applications.
                    Process::Protected => (format!("pid:{pid}"), format!("Aplikasi {pid}")),
                }
            };
            group(&mut groups, key, name, level, mute.as_bool()).controls.push(volume);
        }
        let mut apps: Vec<_> = groups.into_values().collect();
        apps.sort_by_key(|a| a.name.to_lowercase());
        Ok(Snapshot { endpoint: id, apps })
    }
}

impl Snapshot {
    /// Reject stale rows if Windows switched output during an interaction.
    pub fn change(&self, key: &str, level: Option<f32>, mute: Option<bool>) -> Result<()> {
        unsafe {
            if owned_string(endpoint()?.GetId()?) != self.endpoint {
                return Err(Error::from_hresult(E_ABORT));
            }
            let app = self.apps.iter().find(|a| a.key == key).ok_or_else(|| Error::from_hresult(E_ABORT))?;
            let mut failure = None;
            for volume in &app.controls {
                let result = if let Some(v) = level {
                    volume.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null())
                } else if let Some(m) = mute {
                    volume.SetMute(m, std::ptr::null())
                } else { Ok(()) };
                if let Err(e) = result { failure = Some(e); }
            }
            failure.map_or(Ok(()), Err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_sessions_include_mute_difference() {
        assert!(!mixed(0.5, false, 0.501, false));
        assert!(mixed(0.5, false, 0.8, false));
        assert!(mixed(0.5, false, 0.5, true));
    }
    #[test]
    fn executable_sessions_group_but_other_apps_and_system_stay_separate() {
        let mut groups = BTreeMap::new();
        group(&mut groups, "c:\\app.exe".into(), "app".into(), 0.3, false);
        group(&mut groups, "c:\\app.exe".into(), "app".into(), 0.8, false);
        group(&mut groups, "d:\\app.exe".into(), "app".into(), 0.3, false);
        group(&mut groups, "system".into(), "Suara sistem".into(), 0.3, false);
        assert_eq!(groups.len(), 3);
        assert!(groups["c:\\app.exe"].mixed);
        assert!(!groups["d:\\app.exe"].mixed);
        assert_eq!(groups["system"].name, "Suara sistem");
    }
}
