use crate::settings::{reposition_island, save_settings_to_disk, AppSettings};
use crate::win32_window::WindowMonitorFlags;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

static CREATING_SETTINGS: AtomicBool = AtomicBool::new(false);

pub const WEBVIEW2_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --enable-gpu-rasterization --disable-background-timer-throttling --disable-renderer-backgrounding";

struct TrayStrings {
    settings: &'static str,
    toggle_island: &'static str,
    toggle_metrics: &'static str,
    reset_position: &'static str,
    quit: &'static str,
    settings_title: &'static str,
}

fn tray_strings(lang: &str) -> TrayStrings {
    match lang {
        "tr" => TrayStrings {
            settings: "Ayarlar",
            toggle_island: "Adayı Göster / Gizle",
            toggle_metrics: "Sistem Metrikleri Modu",
            reset_position: "Üst Ortaya Sıfırla",
            quit: "Kapat",
            settings_title: "Dynamic Island - Ayarlar",
        },
        "de" => TrayStrings {
            settings: "Einstellungen",
            toggle_island: "Insel ein-/ausblenden",
            toggle_metrics: "Systemmetriken-Modus",
            reset_position: "Position zurücksetzen",
            quit: "Beenden",
            settings_title: "Dynamic Island - Einstellungen",
        },
        "es" => TrayStrings {
            settings: "Configuración",
            toggle_island: "Mostrar / Ocultar isla",
            toggle_metrics: "Modo métricas del sistema",
            reset_position: "Restablecer posición",
            quit: "Salir",
            settings_title: "Dynamic Island - Configuración",
        },
        "fr" => TrayStrings {
            settings: "Paramètres",
            toggle_island: "Afficher / Masquer l'îlot",
            toggle_metrics: "Mode métriques système",
            reset_position: "Réinitialiser la position",
            quit: "Quitter",
            settings_title: "Dynamic Island - Paramètres",
        },
        "ru" => TrayStrings {
            settings: "Настройки",
            toggle_island: "Показать / Скрыть остров",
            toggle_metrics: "Режим системных метрик",
            reset_position: "Сбросить позицию",
            quit: "Выйти",
            settings_title: "Dynamic Island - Настройки",
        },
        "ja" => TrayStrings {
            settings: "設定",
            toggle_island: "アイランドを表示 / 非表示",
            toggle_metrics: "システムメトリクスモード",
            reset_position: "位置をリセット",
            quit: "終了",
            settings_title: "Dynamic Island - 設定",
        },
        "pt" => TrayStrings {
            settings: "Configurações",
            toggle_island: "Mostrar / Ocultar ilha",
            toggle_metrics: "Modo métricas do sistema",
            reset_position: "Redefinir posição",
            quit: "Sair",
            settings_title: "Dynamic Island - Configurações",
        },
        _ => TrayStrings {
            settings: "Settings",
            toggle_island: "Show / Hide Island",
            toggle_metrics: "System Metrics Mode",
            reset_position: "Reset Position",
            quit: "Quit",
            settings_title: "Dynamic Island - Settings",
        },
    }
}

pub fn build_tray_menu(app: &AppHandle, lang: &str) -> Result<Menu<tauri::Wry>, String> {
    let s = tray_strings(lang);
    let item_settings = MenuItem::with_id(app, "settings", s.settings, true, None::<&str>)
        .map_err(|e| format!("Tray menu item error: {e}"))?;
    let item_toggle =
        MenuItem::with_id(app, "toggle_island", s.toggle_island, true, None::<&str>)
            .map_err(|e| format!("Tray menu item error: {e}"))?;
    let item_metrics =
        MenuItem::with_id(app, "toggle_metrics", s.toggle_metrics, true, None::<&str>)
            .map_err(|e| format!("Tray menu item error: {e}"))?;
    let item_reset =
        MenuItem::with_id(app, "reset_position", s.reset_position, true, None::<&str>)
            .map_err(|e| format!("Tray menu item error: {e}"))?;
    let sep =
        PredefinedMenuItem::separator(app).map_err(|e| format!("Tray separator error: {e}"))?;
    let item_quit = MenuItem::with_id(app, "quit", s.quit, true, None::<&str>)
        .map_err(|e| format!("Tray menu item error: {e}"))?;

    Menu::with_items(
        app,
        &[
            &item_settings,
            &item_toggle,
            &item_metrics,
            &item_reset,
            &sep,
            &item_quit,
        ],
    )
    .map_err(|e| format!("Failed to create tray menu: {e}"))
}

pub fn update_tray_language(app: &AppHandle, lang: &str) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(menu) = build_tray_menu(app, lang) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    if let Some(win) = app.get_webview_window("settings") {
        let title = tray_strings(lang).settings_title;
        let _ = win.set_title(title);
    }
}

pub fn open_or_focus_settings(app: &AppHandle, lang: &str) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.set_title(tray_strings(lang).settings_title);
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        return;
    }

    if CREATING_SETTINGS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let app_handle = app.clone();
    let title = tray_strings(lang).settings_title.to_string();

    if std::thread::Builder::new()
        .name("settings-window-spawn".into())
        .spawn(move || {
            struct ResetGuard;
            impl Drop for ResetGuard {
                fn drop(&mut self) {
                    CREATING_SETTINGS.store(false, Ordering::SeqCst);
                }
            }
            let _guard = ResetGuard;

            if let Some(win) = app_handle.get_webview_window("settings") {
                let _ = win.set_title(&title);
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
                return;
            }

            match WebviewWindowBuilder::new(
                &app_handle,
                "settings",
                WebviewUrl::App("index.html".into()),
            )
            .title(&title)
            .inner_size(540.0, 680.0)
            .resizable(false)
            .center()
            .additional_browser_args(WEBVIEW2_BROWSER_ARGS)
            .background_color(Color(11, 13, 19, 255))
            .build()
            {
                Ok(win) => {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                Err(err) => {
                    eprintln!("[Settings] Window creation failed: {err}");
                }
            }
        })
        .is_err()
    {
        CREATING_SETTINGS.store(false, Ordering::SeqCst);
    }
}

pub fn setup_tray(
    app: &AppHandle,
    settings_state: Arc<Mutex<AppSettings>>,
    window_flags: Arc<WindowMonitorFlags>,
) -> Result<(), String> {
    let icon_bytes = include_bytes!("../icons/tray.png");
    let icon =
        Image::from_bytes(icon_bytes).map_err(|e| format!("Tray icon load error: {e}"))?;

    let initial_lang = settings_state
        .lock()
        .map(|s| s.language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let menu = build_tray_menu(app, &initial_lang)?;

    let settings_click = settings_state.clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Dynamic Island")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app_handle, event| match event.id().as_ref() {
            "settings" => {
                let lang = settings_state
                    .lock()
                    .map(|s| s.language.clone())
                    .unwrap_or_else(|_| "en".to_string());
                open_or_focus_settings(app_handle, &lang);
            }
            "toggle_island" => {
                if let Some(main_win) = app_handle.get_webview_window("main") {
                    let visible = main_win.is_visible().unwrap_or(true);
                    if visible {
                        window_flags.user_hidden.store(true, Ordering::Relaxed);
                        let _ = main_win.hide();
                    } else {
                        window_flags.user_hidden.store(false, Ordering::Relaxed);
                        let _ = main_win.show();
                        if let Ok(s) = settings_state.lock() {
                            let _ = reposition_island(app_handle, &s);
                        }
                    }
                }
            }
            "toggle_metrics" => {
                if let Ok(mut s) = settings_state.lock() {
                    s.show_system_metrics_in_idle = !s.show_system_metrics_in_idle;
                    let snapshot = s.clone();
                    let _ = save_settings_to_disk(app_handle, &snapshot);
                    let _ = app_handle.emit("settings-changed", &snapshot);
                }
            }
            "reset_position" => {
                if let Ok(mut s) = settings_state.lock() {
                    s.monitor_name = None;
                    s.position_preset = "top_center".to_string();
                    s.offset_x = 0;
                    s.offset_y = 8;
                    s.custom_x = None;
                    s.custom_y = None;
                    let snapshot = s.clone();
                    let _ = save_settings_to_disk(app_handle, &snapshot);
                    let _ = reposition_island(app_handle, &snapshot);
                    let _ = app_handle.emit("settings-changed", &snapshot);
                }
            }
            "quit" => {
                app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let lang = settings_click
                    .lock()
                    .map(|s| s.language.clone())
                    .unwrap_or_else(|_| "en".to_string());
                open_or_focus_settings(tray.app_handle(), &lang);
            }
        })
        .build(app)
        .map_err(|e| format!("Failed to build tray icon: {e}"))?;

    println!("[Tray] Sistem tepsisi ikonu başlatıldı.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SUPPORTED_LANGUAGES;
    use std::collections::HashSet;

    #[test]
    fn tray_strings_localized_for_all_eight_languages() {
        let mut titles = HashSet::new();
        let mut quits = HashSet::new();
        for code in SUPPORTED_LANGUAGES {
            let s = tray_strings(code);
            assert!(!s.settings.is_empty());
            assert!(!s.toggle_island.is_empty());
            assert!(!s.toggle_metrics.is_empty());
            assert!(!s.reset_position.is_empty());
            assert!(!s.quit.is_empty());
            assert!(s.settings_title.starts_with("Dynamic Island - "));
            titles.insert(s.settings_title);
            quits.insert(s.quit);
        }
        assert_eq!(titles.len(), SUPPORTED_LANGUAGES.len());
        assert_eq!(quits.len(), SUPPORTED_LANGUAGES.len());
        assert_eq!(tray_strings("unknown").settings_title, "Dynamic Island - Settings");
    }
}
