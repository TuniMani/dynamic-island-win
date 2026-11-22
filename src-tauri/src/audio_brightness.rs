use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::core::{implement, w, Interface, BSTR, VARIANT};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    IAudioMeterInformation,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl,
    IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoTaskMemFree, CoUninitialize,
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Wmi::{
    IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY,
};
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioAppSession {
    pub pid: u32,
    pub name: String,
    pub process_name: String,
    pub volume: u8,
    pub muted: bool,
    pub peak: f32,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeState {
    pub level: u8,
    pub muted: bool,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self {
            level: 50,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrightnessState {
    pub supported: bool,
    pub level: u8,
}

impl Default for BrightnessState {
    fn default() -> Self {
        Self {
            supported: false,
            level: 100,
        }
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    app: AppHandle,
    state: Arc<Mutex<VolumeState>>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if pnotify.is_null() {
            return Ok(());
        }
        let data = unsafe { &*pnotify };
        let level = (data.fMasterVolume.clamp(0.0, 1.0) * 100.0).round() as u8;
        let muted = data.bMuted.as_bool();
        let next = VolumeState { level, muted };

        if let Ok(mut guard) = self.state.lock() {
            *guard = next.clone();
        }
        let _ = self.app.emit("volume-changed", &next);
        Ok(())
    }
}

fn get_default_endpoint_volume() -> Option<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

pub fn set_system_volume_level(level: u8) -> Result<(), String> {
    let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let res = (|| {
        let vol = get_default_endpoint_volume()
            .ok_or_else(|| "Varsayılan ses aygıtı bulunamadı".to_string())?;
        let scalar = (level.min(100) as f32) / 100.0;
        unsafe {
            vol.SetMasterVolumeLevelScalar(scalar, ptr::null())
                .map_err(|e| format!("Ses seviyesi ayarlanamadı: {e}"))?;
        }
        Ok(())
    })();
    if com_hr.is_ok() {
        unsafe { CoUninitialize() };
    }
    res
}

pub fn toggle_system_mute_state() -> Result<(), String> {
    let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let res = (|| {
        let vol = get_default_endpoint_volume()
            .ok_or_else(|| "Varsayılan ses aygıtı bulunamadı".to_string())?;
        unsafe {
            let cur = vol
                .GetMute()
                .map_err(|e| format!("Sessiz durumu okunamadı: {e}"))?
                .as_bool();
            vol.SetMute(!cur, ptr::null())
                .map_err(|e| format!("Sessiz durumu değiştirilemedi: {e}"))?;
        }
        Ok(())
    })();
    if com_hr.is_ok() {
        unsafe { CoUninitialize() };
    }
    res
}

fn query_process_exe_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok || size == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        full.rsplit('\\').next().map(|s| s.to_string())
    }
}

fn friendly_app_name(exe_name: &str, raw_display_name: &str) -> String {
    let trimmed = raw_display_name.trim();
    if !trimmed.is_empty() && !trimmed.starts_with('@') {
        return trimmed.to_string();
    }
    match exe_name.to_ascii_lowercase().as_str() {
        "spotify.exe" => "Spotify".to_string(),
        "brave.exe" => "Brave".to_string(),
        "chrome.exe" => "Chrome".to_string(),
        "msedge.exe" => "Edge".to_string(),
        "firefox.exe" => "Firefox".to_string(),
        "discord.exe" => "Discord".to_string(),
        "vlc.exe" => "VLC".to_string(),
        "steamwebhelper.exe" | "steam.exe" => "Steam".to_string(),
        "applemusic.exe" => "Apple Music".to_string(),
        _ => {
            let stem = exe_name
                .strip_suffix(".exe")
                .or_else(|| exe_name.strip_suffix(".EXE"))
                .unwrap_or(exe_name);
            let mut chars = stem.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => exe_name.to_string(),
            }
        }
    }
}

unsafe fn read_session_display_name(ctrl: &IAudioSessionControl) -> String {
    let Ok(pwstr) = ctrl.GetDisplayName() else {
        return String::new();
    };
    if pwstr.0.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *pwstr.0.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(pwstr.0, len);
    let text = String::from_utf16_lossy(slice);
    CoTaskMemFree(Some(pwstr.0 as *const _));
    text
}

fn with_audio_session_enumerator<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce(IAudioSessionEnumerator) -> Result<T, String>,
{
    let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let res = (|| unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Audio device enumerator error: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("Default audio endpoint error: {e}"))?;
        let manager: IAudioSessionManager2 = device
            .Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
            .map_err(|e| format!("Audio session manager error: {e}"))?;
        let sessions = manager
            .GetSessionEnumerator()
            .map_err(|e| format!("Session enumerator error: {e}"))?;
        f(sessions)
    })();
    if com_hr.is_ok() {
        unsafe { CoUninitialize() };
    }
    res
}

pub fn list_audio_sessions() -> Vec<AudioAppSession> {
    let self_pid = std::process::id();
    let result = with_audio_session_enumerator(|enumerator| unsafe {
        let count = enumerator.GetCount().unwrap_or(0).max(0);
        let mut by_app: HashMap<String, AudioAppSession> = HashMap::new();

        for i in 0..count {
            let Ok(ctrl) = enumerator.GetSession(i) else {
                continue;
            };
            let state = ctrl.GetState().unwrap_or(AudioSessionStateExpired);
            if state == AudioSessionStateExpired {
                continue;
            }
            let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let pid = ctrl2.GetProcessId().unwrap_or(0);
            if pid == 0 || pid == self_pid {
                continue;
            }
            let Some(exe_name) = query_process_exe_name(pid) else {
                continue;
            };
            let display = read_session_display_name(&ctrl);
            let name = friendly_app_name(&exe_name, &display);

            let (volume, muted) = match ctrl.cast::<ISimpleAudioVolume>() {
                Ok(vol) => {
                    let scalar = vol.GetMasterVolume().unwrap_or(1.0).clamp(0.0, 1.0);
                    let muted = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    ((scalar * 100.0).round() as u8, muted)
                }
                Err(_) => (100, false),
            };
            let peak = ctrl
                .cast::<IAudioMeterInformation>()
                .ok()
                .and_then(|m| m.GetPeakValue().ok())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let active = state == AudioSessionStateActive;
            let exe_key = exe_name.to_ascii_lowercase();

            if let Some(entry) = by_app.get_mut(&exe_key) {
                entry.peak = entry.peak.max(peak);
                if active && !entry.active {
                    entry.pid = pid;
                    entry.volume = volume;
                    entry.muted = muted;
                    entry.active = true;
                }
            } else {
                by_app.insert(
                    exe_key,
                    AudioAppSession {
                        pid,
                        name,
                        process_name: exe_name,
                        volume,
                        muted,
                        peak,
                        active,
                    },
                );
            }
        }

        let mut list: Vec<AudioAppSession> = by_app.into_values().collect();
        list.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(list)
    });

    result.unwrap_or_default()
}

fn session_matches_pid_or_exe(
    session_pid: u32,
    target_pid: u32,
    target_exe: Option<&str>,
) -> bool {
    if session_pid == target_pid {
        return true;
    }
    if let Some(wanted) = target_exe {
        if let Some(actual) = query_process_exe_name(session_pid) {
            return actual.eq_ignore_ascii_case(wanted);
        }
    }
    false
}

fn apply_to_pid_sessions<F>(pid: u32, mut f: F) -> Result<(), String>
where
    F: FnMut(&ISimpleAudioVolume) -> windows::core::Result<()>,
{
    let target_exe = query_process_exe_name(pid);
    with_audio_session_enumerator(|enumerator| unsafe {
        let count = enumerator.GetCount().unwrap_or(0).max(0);
        let mut updated = false;
        for i in 0..count {
            let Ok(ctrl) = enumerator.GetSession(i) else {
                continue;
            };
            let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(spid) = ctrl2.GetProcessId() else {
                continue;
            };
            if !session_matches_pid_or_exe(spid, pid, target_exe.as_deref()) {
                continue;
            }
            if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                if f(&vol).is_ok() {
                    updated = true;
                }
            }
        }
        if updated {
            Ok(())
        } else {
            Err(format!("No active audio session found for PID {pid}"))
        }
    })
}

pub fn set_app_volume_level(pid: u32, level: u8) -> Result<(), String> {
    let scalar = (level.min(100) as f32) / 100.0;
    apply_to_pid_sessions(pid, |vol| unsafe { vol.SetMasterVolume(scalar, ptr::null()) })
}

pub fn set_app_mute_state(pid: u32, muted: bool) -> Result<bool, String> {
    apply_to_pid_sessions(pid, |vol| unsafe { vol.SetMute(muted, ptr::null()) })?;
    Ok(muted)
}

pub fn toggle_app_mute_state(pid: u32) -> Result<bool, String> {
    let target_exe = query_process_exe_name(pid);
    let target_mute = with_audio_session_enumerator(|enumerator| unsafe {
        let count = enumerator.GetCount().unwrap_or(0).max(0);
        let mut fallback_mute: Option<bool> = None;
        for i in 0..count {
            let Ok(ctrl) = enumerator.GetSession(i) else {
                continue;
            };
            let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(spid) = ctrl2.GetProcessId() else {
                continue;
            };
            if !session_matches_pid_or_exe(spid, pid, target_exe.as_deref()) {
                continue;
            }
            if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                let cur = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                if ctrl.GetState().ok() == Some(AudioSessionStateActive) {
                    return Ok(!cur);
                }
                if fallback_mute.is_none() {
                    fallback_mute = Some(!cur);
                }
            }
        }
        fallback_mute.ok_or_else(|| format!("No active audio session found for PID {pid}"))
    })?;
    set_app_mute_state(pid, target_mute)
}

pub fn start_volume_listener(
    app: AppHandle,
    shutdown: Arc<AtomicBool>,
) -> Arc<Mutex<VolumeState>> {
    let state = Arc::new(Mutex::new(VolumeState::default()));
    let state_thread = state.clone();

    thread::Builder::new()
        .name("volume-listener".into())
        .spawn(move || {
            if crate::defer_startup(&shutdown, 500) {
                return;
            }
            let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

            let Some(endpoint) = get_default_endpoint_volume() else {
                eprintln!("[Audio] Varsayılan ses aygıtı bulunamadı.");
                if com_hr.is_ok() {
                    unsafe { CoUninitialize() };
                }
                return;
            };

            // Read initial volume
            unsafe {
                if let (Ok(scalar), Ok(muted)) =
                    (endpoint.GetMasterVolumeLevelScalar(), endpoint.GetMute())
                {
                    let init = VolumeState {
                        level: (scalar.clamp(0.0, 1.0) * 100.0).round() as u8,
                        muted: muted.as_bool(),
                    };
                    if let Ok(mut g) = state_thread.lock() {
                        *g = init.clone();
                    }
                    let _ = app.emit("volume-changed-init", &init);
                }
            }

            let cb: IAudioEndpointVolumeCallback = VolumeCallback {
                app: app.clone(),
                state: state_thread.clone(),
            }
            .into();

            let registered = unsafe { endpoint.RegisterControlChangeNotify(&cb).is_ok() };
            if !registered {
                eprintln!("[Audio] Ses değişikliği dinleyicisi kaydedilemedi.");
            } else {
                println!("[Audio] Event-driven ses dinleyicisi aktif.");
            }

            while !shutdown.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(400));
            }

            if registered {
                unsafe {
                    let _ = endpoint.UnregisterControlChangeNotify(&cb);
                }
            }
            drop(cb);
            drop(endpoint);
            if com_hr.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .ok();

    state
}

fn connect_wmi_root() -> Option<IWbemServices> {
    unsafe {
        let locator: IWbemLocator =
            CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
        let ns = BSTR::from("ROOT\\WMI");
        locator
            .ConnectServer(&ns, &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)
            .ok()
    }
}

fn read_wmi_brightness(svc: &IWbemServices) -> Option<u8> {
    unsafe {
        let lang = BSTR::from("WQL");
        let query = BSTR::from("SELECT CurrentBrightness FROM WmiMonitorBrightness");
        let enum_obj = svc
            .ExecQuery(
                &lang,
                &query,
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .ok()?;

        let mut objects = [None];
        let mut returned = 0u32;
        if enum_obj.Next(2000, &mut objects, &mut returned).is_err() || returned == 0 {
            return None;
        }

        let obj = objects[0].as_ref()?;
        let mut val = VARIANT::default();
        if obj
            .Get(w!("CurrentBrightness"), 0, &mut val, None, None)
            .is_ok()
        {
            if let Ok(b) = u32::try_from(&val) {
                return Some(b.min(100) as u8);
            }
        }
        None
    }
}

fn read_dxva2_brightness() -> Option<u8> {
    unsafe {
        let desktop = GetDesktopWindow();
        let hmon = MonitorFromWindow(desktop, MONITOR_DEFAULTTOPRIMARY);
        let mut count = 0u32;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count).is_err() || count == 0 {
            return None;
        }
        let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
        if GetPhysicalMonitorsFromHMONITOR(hmon, &mut monitors).is_err() {
            return None;
        }

        let mut min_b = 0u32;
        let mut cur_b = 0u32;
        let mut max_b = 0u32;
        let ok = GetMonitorBrightness(monitors[0].hPhysicalMonitor, &mut min_b, &mut cur_b, &mut max_b) != 0;
        let _ = DestroyPhysicalMonitors(&monitors);

        if ok && max_b > min_b {
            let pct = (((cur_b - min_b) as f64 / (max_b - min_b) as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            Some(pct)
        } else {
            None
        }
    }
}

pub fn start_brightness_listener(
    app: AppHandle,
    shutdown: Arc<AtomicBool>,
) -> Arc<Mutex<BrightnessState>> {
    let state = Arc::new(Mutex::new(BrightnessState::default()));
    let state_thread = state.clone();

    thread::Builder::new()
        .name("brightness-listener".into())
        .spawn(move || {
            if crate::defer_startup(&shutdown, 950) {
                return;
            }
            let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            unsafe {
                let _ = CoInitializeSecurity(
                    None,
                    -1,
                    None,
                    None,
                    RPC_C_AUTHN_LEVEL_CALL,
                    RPC_C_IMP_LEVEL_IMPERSONATE,
                    None,
                    EOAC_NONE,
                    None,
                );
            }

            if let Some(svc) = connect_wmi_root() {
                if let Some(init_level) = read_wmi_brightness(&svc) {
                    let next = BrightnessState {
                        supported: true,
                        level: init_level,
                    };
                    if let Ok(mut g) = state_thread.lock() {
                        *g = next.clone();
                    }
                    println!("[Brightness] WMI parlaklık dinleyicisi aktif ({init_level}%).");

                    // Event-driven WMI notification loop
                    let lang = BSTR::from("WQL");
                    let query = BSTR::from("SELECT * FROM WmiMonitorBrightnessEvent");
                    if let Ok(event_enum) = unsafe {
                        svc.ExecNotificationQuery(
                            &lang,
                            &query,
                            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                            None,
                        )
                    } {
                        while !shutdown.load(Ordering::Relaxed) {
                            let mut objects = [None];
                            let mut returned = 0u32;
                            let hr = unsafe { event_enum.Next(1500, &mut objects, &mut returned) };
                            if hr.is_ok() && returned > 0 {
                                if let Some(obj) = &objects[0] {
                                    let mut val = VARIANT::default();
                                    let got = unsafe {
                                        obj.Get(w!("Brightness"), 0, &mut val, None, None)
                                            .is_ok()
                                    };
                                    if got {
                                        if let Ok(raw_lvl) = u32::try_from(&val) {
                                            let lvl = raw_lvl.min(100) as u8;
                                            let st = BrightnessState {
                                                supported: true,
                                                level: lvl,
                                            };
                                            if let Ok(mut g) = state_thread.lock() {
                                                *g = st.clone();
                                            }
                                            let _ = app.emit("brightness-changed", &st);
                                        }
                                    }
                                }
                            }
                        }

                        if com_hr.is_ok() {
                            unsafe { CoUninitialize() };
                        }
                        return;
                    }
                }
            }

            // Fallback: DDC/CI check for external monitors
            if let Some(mut last_lvl) = read_dxva2_brightness() {
                let st = BrightnessState {
                    supported: true,
                    level: last_lvl,
                };
                if let Ok(mut g) = state_thread.lock() {
                    *g = st;
                }
                println!("[Brightness] DDC/CI parlaklık takibi aktif ({last_lvl}%).");

                while !shutdown.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(3000));
                    if let Some(cur) = read_dxva2_brightness() {
                        if cur != last_lvl {
                            last_lvl = cur;
                            let st = BrightnessState {
                                supported: true,
                                level: cur,
                            };
                            if let Ok(mut g) = state_thread.lock() {
                                *g = st.clone();
                            }
                            let _ = app.emit("brightness-changed", &st);
                        }
                    }
                }
            } else {
                println!(
                    "[Brightness] WMI/DDC brightness unavailable on this display; listener parked."
                );
                while !shutdown.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(600));
                }
            }

            if com_hr.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .ok();

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_app_name_formats_known_and_custom_executables() {
        assert_eq!(friendly_app_name("Spotify.exe", ""), "Spotify");
        assert_eq!(friendly_app_name("brave.exe", ""), "Brave");
        assert_eq!(friendly_app_name("chrome.exe", ""), "Chrome");
        assert_eq!(friendly_app_name("msedge.exe", ""), "Edge");
        assert_eq!(friendly_app_name("discord.exe", ""), "Discord");
        assert_eq!(friendly_app_name("vlc.exe", ""), "VLC");
        assert_eq!(friendly_app_name("steamwebhelper.exe", ""), "Steam");
        assert_eq!(friendly_app_name("applemusic.exe", ""), "Apple Music");
        assert_eq!(friendly_app_name("mygame.exe", ""), "Mygame");
        assert_eq!(
            friendly_app_name("svchost.exe", "@%SystemRoot%\\System32\\AudioSrv.Dll,-202"),
            "Svchost"
        );
        assert_eq!(friendly_app_name("foo.exe", "Custom Player"), "Custom Player");
    }
}
