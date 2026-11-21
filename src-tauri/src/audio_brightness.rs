use serde::{Deserialize, Serialize};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::core::{implement, w, BSTR, VARIANT};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoUninitialize, CLSCTX_ALL,
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Wmi::{
    IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY,
};
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

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
