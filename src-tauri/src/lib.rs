mod audio_brightness;
mod dropzone;
mod progress_tracker;
mod settings;
mod smtc;
mod sys_metrics;
mod tray;
mod win32_window;

use audio_brightness::{
    set_system_volume_level, start_brightness_listener, start_volume_listener,
    toggle_system_mute_state, BrightnessState, VolumeState,
};
use dropzone::{
    copy_files_to_clipboard, copy_paths_text_to_clipboard, create_zip_with_progress,
    reveal_in_explorer,
};
use progress_tracker::{start_progress_tracker, InternalTasks, TaskProgress};
use serde::Serialize;
use settings::{
    list_monitors, load_settings, reposition_island, save_settings_to_disk, set_windows_autostart,
    AppSettings, MonitorInfo,
};
use smtc::{start_smtc, MediaCommand, MediaState, SmtcController};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use sys_metrics::{start_system_metrics_task, SystemMetrics};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tray::{open_or_focus_settings, setup_tray, update_tray_language};
use win32_window::{
    apply_island_window_styles, reveal_island_window, start_clickthrough_monitor,
    start_free_drag_session, HitRect, WindowMonitorFlags,
};

pub(crate) fn defer_startup(shutdown: &AtomicBool, total_ms: u64) -> bool {
    let mut elapsed = 0u64;
    while elapsed < total_ms {
        if shutdown.load(Ordering::Relaxed) {
            return true;
        }
        let step = (total_ms - elapsed).min(50);
        std::thread::sleep(std::time::Duration::from_millis(step));
        elapsed += step;
    }
    shutdown.load(Ordering::Relaxed)
}

struct AppState {
    settings: Arc<Mutex<AppSettings>>,
    hit_rect: Arc<Mutex<HitRect>>,
    window_flags: Arc<WindowMonitorFlags>,
    window_revealed: Arc<AtomicBool>,
    smtc: SmtcController,
    volume: Arc<Mutex<VolumeState>>,
    brightness: Arc<Mutex<BrightnessState>>,
    metrics: Arc<Mutex<SystemMetrics>>,
    tasks: Arc<Mutex<Vec<TaskProgress>>>,
    internal_tasks: InternalTasks,
    progress_enabled: Arc<AtomicBool>,
}

#[derive(Serialize)]
struct InitialState {
    media: MediaState,
    volume: VolumeState,
    brightness: BrightnessState,
    metrics: SystemMetrics,
    tasks: Vec<TaskProgress>,
    settings: AppSettings,
    monitors: Vec<MonitorInfo>,
}

#[tauri::command]
fn get_initial_state(app: AppHandle, state: State<'_, AppState>) -> InitialState {
    let media = state
        .smtc
        .state
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| MediaState::idle());
    let volume = state.volume.lock().map(|g| g.clone()).unwrap_or_default();
    let brightness = state.brightness.lock().map(|g| g.clone()).unwrap_or_default();
    let metrics = state.metrics.lock().map(|g| g.clone()).unwrap_or_default();
    let tasks = state.tasks.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = state.settings.lock().map(|g| g.clone()).unwrap_or_default();
    let monitors = list_monitors(&app);

    InitialState {
        media,
        volume,
        brightness,
        metrics,
        tasks,
        settings,
        monitors,
    }
}

#[tauri::command]
fn get_monitors(app: AppHandle) -> Vec<MonitorInfo> {
    list_monitors(&app)
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let clean = settings.sanitized();

    let (prev_lang, prev_autostart) = state
        .settings
        .lock()
        .map(|g| (g.language.clone(), g.start_with_windows))
        .unwrap_or_else(|_| ("en".to_string(), false));

    let mut final_settings = clean;
    if final_settings.start_with_windows != prev_autostart {
        if let Err(err) = set_windows_autostart(final_settings.start_with_windows) {
            eprintln!("[Autostart] Failed: {err}");
            final_settings.start_with_windows = prev_autostart;
        }
    }

    save_settings_to_disk(&app, &final_settings)?;

    state
        .smtc
        .set_visualizer_enabled(final_settings.enable_visualizer);
    state
        .progress_enabled
        .store(final_settings.enable_progress_tracker, Ordering::Relaxed);
    state
        .window_flags
        .auto_collapse
        .store(final_settings.auto_collapse_on_focus_loss, Ordering::Relaxed);
    state
        .window_flags
        .auto_hide_fullscreen
        .store(final_settings.auto_hide_fullscreen, Ordering::Relaxed);

    if final_settings.language != prev_lang {
        update_tray_language(&app, &final_settings.language);
    }

    if let Ok(mut guard) = state.settings.lock() {
        *guard = final_settings.clone();
    }

    reposition_island(&app, &final_settings)?;
    let _ = app.emit("settings-changed", &final_settings);
    Ok(final_settings)
}

#[tauri::command]
fn reset_position(app: AppHandle, state: State<'_, AppState>) -> Result<AppSettings, String> {
    let updated = {
        let mut guard = state
            .settings
            .lock()
            .map_err(|_| "Settings lock unavailable".to_string())?;
        guard.monitor_name = None;
        guard.position_preset = "top_center".to_string();
        guard.offset_x = 0;
        guard.offset_y = 8;
        guard.custom_x = None;
        guard.custom_y = None;
        guard.clone()
    };
    save_settings_to_disk(&app, &updated)?;
    reposition_island(&app, &updated)?;
    let _ = app.emit("settings-changed", &updated);
    Ok(updated)
}

#[tauri::command]
async fn open_settings_window(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let lang = state
        .settings
        .lock()
        .map(|s| s.language.clone())
        .unwrap_or_else(|_| "en".to_string());
    open_or_focus_settings(&app, &lang);
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn set_island_expanded(state: State<'_, AppState>, expanded: bool) {
    state
        .window_flags
        .expanded
        .store(expanded, Ordering::Relaxed);
}

#[tauri::command]
fn reveal_main_window(app: AppHandle, state: State<'_, AppState>) {
    if !state.window_revealed.swap(true, Ordering::SeqCst) {
        if let Some(main_win) = app.get_webview_window("main") {
            let _ = reveal_island_window(&main_win);
        }
    }
}

#[tauri::command]
fn start_island_drag(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    start_free_drag_session(app, state.settings.clone(), state.window_flags.clone())
}

#[tauri::command]
fn update_island_hit_rect(
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    if let Ok(mut guard) = state.hit_rect.lock() {
        *guard = HitRect {
            x,
            y,
            width: width.max(10.0),
            height: height.max(10.0),
        };
    }
}

#[tauri::command]
fn media_play_pause(state: State<'_, AppState>) {
    state.smtc.send(MediaCommand::PlayPause);
}

#[tauri::command]
fn media_next(state: State<'_, AppState>) {
    state.smtc.send(MediaCommand::Next);
}

#[tauri::command]
fn media_previous(state: State<'_, AppState>) {
    state.smtc.send(MediaCommand::Previous);
}

#[tauri::command]
fn media_seek(state: State<'_, AppState>, position_secs: f64) {
    state.smtc.send(MediaCommand::Seek(position_secs));
}

#[tauri::command]
fn media_select_session(state: State<'_, AppState>, session_id: String) {
    state.smtc.send(MediaCommand::SelectSession(session_id));
}

#[tauri::command]
fn media_cycle_session(state: State<'_, AppState>) {
    state.smtc.send(MediaCommand::CycleSession);
}

#[tauri::command]
fn set_system_volume(level: u8) -> Result<(), String> {
    set_system_volume_level(level)
}

#[tauri::command]
fn toggle_system_mute() -> Result<(), String> {
    toggle_system_mute_state()
}

#[tauri::command]
fn dropzone_copy_files(paths: Vec<String>) -> Result<usize, String> {
    copy_files_to_clipboard(&paths)
}

#[tauri::command]
fn dropzone_copy_paths(paths: Vec<String>) -> Result<usize, String> {
    copy_paths_text_to_clipboard(&paths)
}

#[tauri::command]
async fn dropzone_create_zip(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<String, String> {
    create_zip_with_progress(paths, state.internal_tasks.clone(), app).await
}

#[tauri::command]
fn dropzone_reveal_in_explorer(path: String) -> Result<(), String> {
    reveal_in_explorer(&path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_exit = shutdown.clone();

    let app = tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            let loaded_settings = load_settings(&handle);
            let settings_arc = Arc::new(Mutex::new(loaded_settings.clone()));
            let hit_rect = Arc::new(Mutex::new(HitRect::default()));
            let window_flags = Arc::new(WindowMonitorFlags::new(
                loaded_settings.auto_collapse_on_focus_loss,
                loaded_settings.auto_hide_fullscreen,
            ));
            let window_revealed = Arc::new(AtomicBool::new(false));

            if let Some(main_win) = handle.get_webview_window("main") {
                if let Err(err) = apply_island_window_styles(&main_win) {
                    eprintln!("[Window] Win32 style error: {err}");
                }
                if let Err(err) = reposition_island(&handle, &loaded_settings) {
                    eprintln!("[Window] Reposition error: {err}");
                }
                start_clickthrough_monitor(
                    handle.clone(),
                    main_win.clone(),
                    hit_rect.clone(),
                    window_flags.clone(),
                    shutdown.clone(),
                );

                // Safety fallback: ensure main pill reveals even if frontend IPC is delayed
                let revealed_fb = window_revealed.clone();
                let shutdown_fb = shutdown.clone();
                std::thread::Builder::new()
                    .name("window-reveal-fallback".into())
                    .spawn(move || {
                        let mut elapsed = 0u64;
                        while elapsed < 700 && !shutdown_fb.load(Ordering::Relaxed) {
                            if revealed_fb.load(Ordering::Relaxed) {
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            elapsed += 50;
                        }
                        if !shutdown_fb.load(Ordering::Relaxed)
                            && !revealed_fb.swap(true, Ordering::SeqCst)
                        {
                            let win_copy = main_win.clone();
                            let _ = main_win.run_on_main_thread(move || {
                                let _ = reveal_island_window(&win_copy);
                            });
                        }
                    })
                    .ok();
            }

            if let Err(err) = setup_tray(&handle, settings_arc.clone(), window_flags.clone()) {
                eprintln!("[Tray] Error: {err}");
            }

            let smtc = start_smtc(
                handle.clone(),
                loaded_settings.enable_visualizer,
                shutdown.clone(),
            );
            let volume = start_volume_listener(handle.clone(), shutdown.clone());
            let brightness = start_brightness_listener(handle.clone(), shutdown.clone());
            let metrics = start_system_metrics_task(handle.clone(), shutdown.clone());

            let internal_tasks = InternalTasks::default();
            let tasks = Arc::new(Mutex::new(Vec::new()));
            let progress_enabled =
                Arc::new(AtomicBool::new(loaded_settings.enable_progress_tracker));
            start_progress_tracker(
                handle.clone(),
                progress_enabled.clone(),
                internal_tasks.clone(),
                tasks.clone(),
                shutdown.clone(),
            );

            app.manage(AppState {
                settings: settings_arc,
                hit_rect,
                window_flags,
                window_revealed,
                smtc,
                volume,
                brightness,
                metrics,
                tasks,
                internal_tasks,
                progress_enabled,
            });

            if std::env::args().any(|a| a == "--test-open-settings") {
                let test_handle = handle.clone();
                let test_lang = loaded_settings.language.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    open_or_focus_settings(&test_handle, &test_lang);
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            get_monitors,
            save_settings,
            reset_position,
            open_settings_window,
            quit_app,
            reveal_main_window,
            set_island_expanded,
            start_island_drag,
            update_island_hit_rect,
            media_play_pause,
            media_next,
            media_previous,
            media_seek,
            media_select_session,
            media_cycle_session,
            set_system_volume,
            toggle_system_mute,
            dropzone_copy_files,
            dropzone_copy_paths,
            dropzone_create_zip,
            dropzone_reveal_in_explorer,
        ])
        .build(tauri::generate_context!())
        .expect("Failed to build Dynamic Island application");

    app.run(move |_app_handle, event| {
        if let RunEvent::Exit = event {
            shutdown_exit.store(true, Ordering::Relaxed);
        }
    });
}
