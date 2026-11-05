//! Shutdown is requested only when an explicitly armed music timer completes.
use std::os::windows::process::CommandExt;
use std::process::Command;

const SHUTDOWN_ARGS: [&str; 3] = ["/s", "/t", "0"];

fn perform(preview: bool, execute: impl FnOnce() -> Result<(), String>) -> Result<bool, String> {
    if preview { return Ok(false); }
    execute()?;
    Ok(true)
}

/// Returns false in preview mode. No scheduled OS task or forced app closure.
pub fn shutdown() -> Result<bool, String> {
    perform(crate::dev_instance(), || {
        let mut directory = [0u16; 32768];
        let len = unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut directory)) } as usize;
        if len == 0 || len >= directory.len() { return Err("Folder sistem Windows tidak tersedia".into()); }
        let executable = std::path::PathBuf::from(String::from_utf16_lossy(&directory[..len])).join("shutdown.exe");
        let output = Command::new(executable).args(SHUTDOWN_ARGS).creation_flags(0x08000000)
            .output().map_err(|error| error.to_string())?;
        if output.status.success() { Ok(()) }
        else { Err(format!("Shutdown gagal ({}): {}", output.status, String::from_utf8_lossy(&output.stderr).trim())) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_never_invokes_shutdown_and_errors_are_returned() {
        assert_eq!(perform(true, || panic!("preview must not execute shutdown")), Ok(false));
        assert_eq!(perform(false, || Ok(())), Ok(true));
        assert_eq!(perform(false, || Err("denied".into())), Err("denied".into()));
        // Positive OS timeouts imply /f. Keep all waiting inside Amora instead.
        assert_eq!(SHUTDOWN_ARGS, ["/s", "/t", "0"]);
    }
}
