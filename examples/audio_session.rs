//! Creates a temporary SILENT WASAPI session for manual mixer testing.
//! Copy the executable under two names to test separate applications. No sound,
//! saved preferences, or changes to other sessions; exits after 120 seconds.
use std::time::{Duration, Instant};
use windows::core::Result;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

fn main() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = endpoint.Activate(CLSCTX_ALL, None)?;
        let format = client.GetMixFormat()?;
        let initialized = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 1_000_000, 0, format, None);
        CoTaskMemFree(Some(format as _));
        initialized?;
        let render: IAudioRenderClient = client.GetService()?;
        let frames = client.GetBufferSize()?;
        let _ = render.GetBuffer(frames)?;
        render.ReleaseBuffer(frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
        client.Start()?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(20));
            let available = frames.saturating_sub(client.GetCurrentPadding()?);
            if available > 0 {
                let _ = render.GetBuffer(available)?;
                render.ReleaseBuffer(available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
            }
        }
        client.Stop()?;
        // COM interfaces are dropped before the apartment is torn down by process exit.
    }
    Ok(())
}
