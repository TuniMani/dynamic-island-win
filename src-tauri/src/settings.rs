use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Position};
use windows::core::w;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub language: String,
    pub monitor_name: Option<String>,
    pub position_preset: String,
    pub offset_x: i32,
    pub offset_y: i32,
    pub custom_x: Option<i32>,
    pub custom_y: Option<i32>,
    pub unlock_position: bool,
    pub scale: f64,
    pub border_radius_mode: u32,
    pub appearance: String,
    pub background_opacity: u8,
    pub accent_mode: String,
    pub accent_color: String,
    pub auto_collapse_on_focus_loss: bool,
    pub mouse_leave_delay_ms: i32,
    pub interaction_mode: String,
    pub animation_speed: String,
    pub start_with_windows: bool,
    pub auto_hide_fullscreen: bool,
    pub auto_hide_idle_secs: u32,
    pub show_system_metrics_in_idle: bool,
    pub hud_duration_ms: u64,
    pub enable_visualizer: bool,
    pub enable_media_module: bool,
    pub enable_volume_module: bool,
    pub enable_system_module: bool,
    pub enable_progress_tracker: bool,
    pub enable_timer_module: bool,
    pub enable_dropzone_module: bool,
    pub module_order: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            monitor_name: None,
            position_preset: "top_center".to_string(),
            offset_x: 0,
            offset_y: 8,
            custom_x: None,
            custom_y: None,
            unlock_position: false,
            scale: 1.0,
            border_radius_mode: 999,
            appearance: "pure_black".to_string(),
            background_opacity: 85,
            accent_mode: "preset".to_string(),
            accent_color: "#0A84FF".to_string(),
            auto_collapse_on_focus_loss: true,
            mouse_leave_delay_ms: 1000,
            interaction_mode: "hover".to_string(),
            animation_speed: "balanced".to_string(),
            start_with_windows: false,
            auto_hide_fullscreen: false,
            auto_hide_idle_secs: 0,
            show_system_metrics_in_idle: false,
            hud_duration_ms: 2200,
            enable_visualizer: true,
            enable_media_module: true,
            enable_volume_module: true,
            enable_system_module: true,
            enable_progress_tracker: true,
            enable_timer_module: true,
            enable_dropzone_module: true,
            module_order: vec![
                "media".to_string(),
                "volume".to_string(),
                "system".to_string(),
                "progress".to_string(),
                "timer".to_string(),
            ],
        }
    }
}

pub const SUPPORTED_LANGUAGES: &[&str] = &["en", "tr", "de", "es", "fr", "ru", "ja", "pt"];

impl AppSettings {
    pub fn sanitized(mut self) -> Self {
        if !SUPPORTED_LANGUAGES.contains(&self.language.as_str()) {
            self.language = "en".to_string();
        }
        self.scale = self.scale.clamp(0.75, 1.40);
        self.border_radius_mode = self.border_radius_mode.clamp(16, 999);
        self.background_opacity = self.background_opacity.clamp(20, 100);
        if self.module_order.is_empty() {
            self.module_order = Self::default().module_order;
        } else if !self.module_order.iter().any(|m| m == "volume") {
            let idx = 1.min(self.module_order.len());
            self.module_order.insert(idx, "volume".to_string());
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

pub fn load_settings(app: &AppHandle) -> AppSettings {
    let Some(path) = settings_path(app) else {
        return AppSettings::default();
    };
    let mut loaded = match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<AppSettings>(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    };
    loaded.start_with_windows = is_windows_autostart_enabled();
    loaded.sanitized()
}

pub fn save_settings_to_disk(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app).ok_or_else(|| "Failed to resolve config directory".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("JSON serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write settings file: {e}"))?;
    Ok(())
}

const RUN_SUBKEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: windows::core::PCWSTR = w!("DynamicIsland");

pub fn is_windows_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return false;
        }
        let mut len: u32 = 0;
        let res = RegQueryValueExW(hkey, RUN_VALUE, None, None, None, Some(&mut len));
        let _ = RegCloseKey(hkey);
        res == ERROR_SUCCESS && len > 0
    }
}

pub fn set_windows_autostart(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut hkey = HKEY::default();
        let err = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_SET_VALUE, &mut hkey);
        if err != ERROR_SUCCESS {
            return Err(format!("Registry open error: {:?}", err));
        }

        let res = if enabled {
            let exe_path = std::env::current_exe()
                .map_err(|e| format!("Executable path error: {e}"))?;
            let quoted = format!("\"{}\"", exe_path.to_string_lossy());
            let mut wide: Vec<u16> = quoted.encode_utf16().collect();
            wide.push(0);
            let bytes: &[u8] =
                std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(hkey, RUN_VALUE, 0, REG_SZ, Some(bytes))
        } else {
            let del = RegDeleteValueW(hkey, RUN_VALUE);
            // Deleting a non-existent value is harmless
            if del == ERROR_SUCCESS || del.0 == 2 {
                ERROR_SUCCESS
            } else {
                del
            }
        };

        let _ = RegCloseKey(hkey);
        if res != ERROR_SUCCESS {
            return Err(format!("Registry update failed: {:?}", res));
        }
        Ok(())
    }
}

pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary_name = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned());

    let monitors = app.available_monitors().unwrap_or_default();
    monitors
        .into_iter()
        .enumerate()
        .map(|(idx, m)| {
            let name = m
                .name()
                .cloned()
                .unwrap_or_else(|| format!("Display {}", idx + 1));
            let pos = m.position();
            let size = m.size();
            let is_primary = primary_name
                .as_deref()
                .map(|p| p == name.as_str())
                .unwrap_or(idx == 0);
            MonitorInfo {
                name,
                width: size.width,
                height: size.height,
                x: pos.x,
                y: pos.y,
                scale_factor: m.scale_factor(),
                is_primary,
            }
        })
        .collect()
}

pub fn find_target_monitor(app: &AppHandle, monitor_name: Option<&str>) -> Option<tauri::Monitor> {
    let avail = app.available_monitors().unwrap_or_default();
    monitor_name
        .and_then(|wanted| {
            avail
                .iter()
                .find(|m| m.name().map(|n| n.as_str() == wanted).unwrap_or(false))
                .cloned()
        })
        .or_else(|| app.primary_monitor().ok().flatten())
        .or_else(|| avail.into_iter().next())
}

pub fn reposition_island(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let main_win = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    let Some(monitor) = find_target_monitor(app, settings.monitor_name.as_deref()) else {
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let work = monitor.work_area();
    let win_size = main_win
        .outer_size()
        .unwrap_or_else(|_| PhysicalSize::new((640.0 * scale) as u32, (320.0 * scale) as u32));

    let off_x = (settings.offset_x as f64 * scale).round() as i32;
    let off_y = (settings.offset_y as f64 * scale).round() as i32;
    let win_w = win_size.width as i32;
    let win_h = win_size.height as i32;

    let (target_x, target_y) = match settings.position_preset.as_str() {
        "top_left" => (mon_pos.x + off_x, mon_pos.y + off_y),
        "top_right" => (
            mon_pos.x + mon_size.width as i32 - win_w + off_x,
            mon_pos.y + off_y,
        ),
        "bottom_center" => (
            mon_pos.x + ((mon_size.width as i32 - win_w) / 2) + off_x,
            work.position.y + work.size.height as i32 - win_h - off_y,
        ),
        "custom" => {
            let cx = settings
                .custom_x
                .unwrap_or(((mon_size.width as i32 - win_w) / 2) + off_x);
            let cy = settings.custom_y.unwrap_or(off_y);
            (mon_pos.x + cx, mon_pos.y + cy)
        }
        _ => (
            mon_pos.x + ((mon_size.width as i32 - win_w) / 2) + off_x,
            mon_pos.y + off_y,
        ),
    };

    main_win
        .set_position(Position::Physical(PhysicalPosition::new(target_x, target_y)))
        .map_err(|e| format!("Failed to set window position: {e}"))?;

    if let Ok(hwnd) = main_win.hwnd() {
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd.0),
                HWND_TOPMOST,
                target_x,
                target_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_and_auto_collapse_enabled() {
        let def = AppSettings::default();
        assert_eq!(def.language, "en");
        assert_eq!(def.position_preset, "top_center");
        assert!(def.auto_collapse_on_focus_loss);
        assert_eq!(def.scale, 1.0);
        assert_eq!(def.border_radius_mode, 999);
        assert_eq!(def.accent_color, "#0A84FF");
    }

    #[test]
    fn deserializes_partial_legacy_settings_cleanly() {
        let legacy_json = r#"{"offset_x": 12, "offset_y": 20, "appearance": "glass"}"#;
        let parsed: AppSettings = serde_json::from_str(legacy_json).expect("legacy json");
        assert_eq!(parsed.language, "en");
        assert_eq!(parsed.offset_x, 12);
        assert_eq!(parsed.offset_y, 20);
        assert_eq!(parsed.appearance, "glass");
        assert!(parsed.auto_collapse_on_focus_loss);
        assert!(parsed.enable_media_module);
        assert!(parsed.enable_volume_module);
        assert!(parsed.module_order.contains(&"volume".to_string()));
    }

    #[test]
    fn sanitizes_all_supported_languages_and_rejects_unknown() {
        for code in SUPPORTED_LANGUAGES {
            let mut s = AppSettings::default();
            s.language = (*code).to_string();
            assert_eq!(s.sanitized().language, *code);
        }
        let mut unknown = AppSettings::default();
        unknown.language = "xx".to_string();
        assert_eq!(unknown.sanitized().language, "en");
    }
}
