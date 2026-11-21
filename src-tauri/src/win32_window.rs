use crate::settings::{reposition_island, save_settings_to_disk, AppSettings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWCP_DONOTROUND, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
    DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, IsWindowEnabled, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, SHQueryUserNotificationState, SetWindowSubclass, QUNS_BUSY,
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
    WA_INACTIVE, WM_ACTIVATE, WM_MOUSEACTIVATE, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

#[derive(Debug, Clone, Copy)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for HitRect {
    fn default() -> Self {
        Self {
            x: 256.0,
            y: 4.0,
            width: 128.0,
            height: 36.0,
        }
    }
}

pub struct WindowMonitorFlags {
    pub expanded: AtomicBool,
    pub auto_collapse: AtomicBool,
    pub auto_hide_fullscreen: AtomicBool,
    pub user_hidden: AtomicBool,
    pub dragging: AtomicBool,
}

impl WindowMonitorFlags {
    pub fn new(auto_collapse: bool, auto_hide_fullscreen: bool) -> Self {
        Self {
            expanded: AtomicBool::new(false),
            auto_collapse: AtomicBool::new(auto_collapse),
            auto_hide_fullscreen: AtomicBool::new(auto_hide_fullscreen),
            user_hidden: AtomicBool::new(false),
            dragging: AtomicBool::new(false),
        }
    }
}

const SUBCLASS_ID_MAIN: usize = 0x4449_0001;
const SUBCLASS_ID_CHILD: usize = 0x4449_0002;

unsafe fn is_native_dialog_hwnd(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let mut cls_buf = [0u16; 64];
    let cls_len = GetClassNameW(hwnd, &mut cls_buf);
    if cls_len <= 0 {
        return false;
    }
    String::from_utf16_lossy(&cls_buf[..cls_len as usize]) == "#32770"
}

unsafe extern "system" fn main_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ACTIVATE => {
            let state = (wparam.0 & 0xFFFF) as u32;
            if state != WA_INACTIVE {
                let prev_hwnd = HWND(lparam.0 as *mut _);
                let fg_is_dialog = is_native_dialog_hwnd(GetForegroundWindow());
                if !fg_is_dialog
                    && !prev_hwnd.0.is_null()
                    && prev_hwnd != hwnd
                    && IsWindow(prev_hwnd).as_bool()
                    && IsWindowVisible(prev_hwnd).as_bool()
                    && IsWindowEnabled(prev_hwnd).as_bool()
                {
                    let _ = SetForegroundWindow(prev_hwnd);
                }
            }
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn child_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn subclass_child_enum_cb(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    let _ = SetWindowSubclass(hwnd, Some(child_subclass_proc), SUBCLASS_ID_CHILD, 0);
    BOOL(1)
}

pub fn attach_child_noactivate_hooks(hwnd: HWND) {
    unsafe {
        let _ = EnumChildWindows(hwnd, Some(subclass_child_enum_cb), LPARAM(0));
    }
}

fn apply_island_ex_style(hwnd: HWND, click_through: bool) {
    unsafe {
        let cur_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let base = (cur_style
            | (WS_EX_NOACTIVATE.0 as isize)
            | (WS_EX_TOOLWINDOW.0 as isize)
            | (WS_EX_TOPMOST.0 as isize)
            | (WS_EX_LAYERED.0 as isize))
            & !(WS_EX_APPWINDOW.0 as isize);
        let updated = if click_through {
            base | (WS_EX_TRANSPARENT.0 as isize)
        } else {
            base & !(WS_EX_TRANSPARENT.0 as isize)
        };
        if updated != cur_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        }
        if (cur_style & (WS_EX_LAYERED.0 as isize)) == 0 {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        }
    }
}

pub fn apply_island_window_styles(window: &WebviewWindow) -> Result<(), String> {
    let _ = window.set_focusable(false);

    let raw_hwnd = window
        .hwnd()
        .map_err(|e| format!("Failed to obtain HWND: {e}"))?;
    let hwnd = HWND(raw_hwnd.0);

    unsafe {
        apply_island_ex_style(hwnd, true);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );

        let corner_pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        let border_none = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_none as *const _ as *const _,
            std::mem::size_of_val(&border_none) as u32,
        );

        let _ = SetWindowSubclass(hwnd, Some(main_subclass_proc), SUBCLASS_ID_MAIN, 0);
        attach_child_noactivate_hooks(hwnd);
    }

    Ok(())
}

pub fn reveal_island_window(window: &WebviewWindow) -> Result<(), String> {
    let raw_hwnd = window
        .hwnd()
        .map_err(|e| format!("Failed to obtain HWND: {e}"))?;
    let hwnd = HWND(raw_hwnd.0);
    apply_island_ex_style(hwnd, true);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    Ok(())
}

fn is_foreground_fullscreen(self_pid: u32, main_hwnd: HWND) -> bool {
    unsafe {
        if let Ok(state) = SHQueryUserNotificationState() {
            if state == QUNS_BUSY
                || state == QUNS_RUNNING_D3D_FULL_SCREEN
                || state == QUNS_PRESENTATION_MODE
            {
                return true;
            }
        }

        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg == main_hwnd {
            return false;
        }
        if !IsWindowVisible(fg).as_bool() || IsIconic(fg).as_bool() {
            return false;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == self_pid {
            return false;
        }

        let mut cls_buf = [0u16; 64];
        let cls_len = GetClassNameW(fg, &mut cls_buf);
        if cls_len > 0 {
            let cls = String::from_utf16_lossy(&cls_buf[..cls_len as usize]);
            if matches!(
                cls.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32770"
            ) {
                return false;
            }
        }

        let style = GetWindowLongW(fg, GWL_STYLE);
        if (style & (WS_CAPTION.0 as i32)) == (WS_CAPTION.0 as i32) {
            return false;
        }

        let mut fg_rect = RECT::default();
        if GetWindowRect(fg, &mut fg_rect).is_err() {
            return false;
        }

        let hmon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return false;
        }

        let m = mi.rcMonitor;
        fg_rect.left <= m.left
            && fg_rect.top <= m.top
            && fg_rect.right >= m.right
            && fg_rect.bottom >= m.bottom
    }
}

pub fn start_clickthrough_monitor(
    app: AppHandle,
    window: WebviewWindow,
    hit_rect: Arc<Mutex<HitRect>>,
    flags: Arc<WindowMonitorFlags>,
    shutdown: Arc<AtomicBool>,
) {
    let Ok(raw_hwnd) = window.hwnd() else {
        return;
    };
    let hwnd_val = raw_hwnd.0 as isize;
    let self_pid = std::process::id();

    thread::Builder::new()
        .name("island-hit-monitor".into())
        .spawn(move || {
            let hwnd = HWND(hwnd_val as *mut _);
            let mut is_transparent = true;
            let mut tick_count: u32 = 0;
            let mut was_expanded = false;
            let mut last_fg: isize = 0;
            let mut prev_btn_down = false;
            let mut hidden_by_fs = false;
            let mut last_fs_check = Instant::now();

            while !shutdown.load(Ordering::Relaxed) {
                if !unsafe { IsWindow(hwnd).as_bool() } {
                    break;
                }

                tick_count = tick_count.wrapping_add(1);
                if matches!(tick_count, 1 | 20 | 60) {
                    let _ = window.run_on_main_thread(move || {
                        attach_child_noactivate_hooks(HWND(hwnd_val as *mut _));
                    });
                }

                let dragging = flags.dragging.load(Ordering::Relaxed);
                let scale = window.scale_factor().unwrap_or(1.0);
                let mut cursor = POINT::default();
                let mut rect = RECT::default();

                let got_pos = unsafe { GetCursorPos(&mut cursor).is_ok() };
                let got_rect = unsafe { GetWindowRect(hwnd, &mut rect).is_ok() };

                let mut sleep_ms: u64 = 75;

                if got_pos && got_rect {
                    let hr = hit_rect.lock().map(|g| *g).unwrap_or_default();
                    let pad = 8.0 * scale;

                    let left = rect.left as f64 + hr.x * scale - pad;
                    let top = rect.top as f64 + hr.y * scale - pad;
                    let right = rect.left as f64 + (hr.x + hr.width) * scale + pad;
                    let bottom = rect.top as f64 + (hr.y + hr.height) * scale + pad;

                    let cx = cursor.x as f64;
                    let cy = cursor.y as f64;
                    if cy >= rect.top as f64 - 80.0 * scale
                        && cy <= rect.bottom as f64 + 120.0 * scale
                    {
                        sleep_ms = 20;
                    }

                    let in_pill = cx >= left && cx <= right && cy >= top && cy <= bottom;

                    let lbutton_down =
                        unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16) & 0x8000 != 0 };
                    let rbutton_down =
                        unsafe { (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16) & 0x8000 != 0 };
                    let any_btn_down = lbutton_down || rbutton_down;

                    let fg_hwnd = unsafe { GetForegroundWindow() };
                    let fg_is_dialog = unsafe { is_native_dialog_hwnd(fg_hwnd) };

                    let drop_left = rect.left as f64
                        + ((rect.right - rect.left) as f64 - 360.0 * scale) * 0.5;
                    let drop_right = drop_left + 360.0 * scale;
                    let drop_top = rect.top as f64;
                    let drop_bottom = rect.top as f64 + 90.0 * scale;
                    let in_drop_capture = !fg_is_dialog
                        && lbutton_down
                        && cx >= drop_left
                        && cx <= drop_right
                        && cy >= drop_top
                        && cy <= drop_bottom;

                    if !dragging {
                        let want_transparent = if fg_is_dialog && any_btn_down && !in_pill {
                            true
                        } else {
                            !(in_pill || in_drop_capture)
                        };
                        if want_transparent != is_transparent {
                            let _ = window.run_on_main_thread(move || {
                                apply_island_ex_style(HWND(hwnd_val as *mut _), want_transparent);
                            });
                            is_transparent = want_transparent;
                        }
                    }

                    // Auto-collapse on outside click or foreground focus change
                    let expanded_now = flags.expanded.load(Ordering::Relaxed);
                    let fg_now = fg_hwnd.0 as isize;
                    if expanded_now && !was_expanded {
                        last_fg = fg_now;
                    }
                    was_expanded = expanded_now;

                    if expanded_now && flags.auto_collapse.load(Ordering::Relaxed) && !dragging {
                        if !prev_btn_down && any_btn_down && !in_pill {
                            let _ = app.emit("island-auto-collapse", "outside_click");
                            flags.expanded.store(false, Ordering::Relaxed);
                            was_expanded = false;
                        } else if fg_now != 0 && fg_now != hwnd_val && fg_now != last_fg {
                            let mut fg_pid = 0u32;
                            unsafe { GetWindowThreadProcessId(HWND(fg_now as *mut _), Some(&mut fg_pid)) };
                            if fg_pid != self_pid {
                                let _ = app.emit("island-auto-collapse", "focus_changed");
                                flags.expanded.store(false, Ordering::Relaxed);
                                was_expanded = false;
                            }
                            last_fg = fg_now;
                        }
                    } else if fg_now != 0 && fg_now != hwnd_val {
                        last_fg = fg_now;
                    }

                    prev_btn_down = any_btn_down;
                }

                // Fullscreen auto-hide check (~400ms interval)
                if last_fs_check.elapsed() >= Duration::from_millis(400) {
                    last_fs_check = Instant::now();
                    let fs_enabled = flags.auto_hide_fullscreen.load(Ordering::Relaxed);
                    let user_hidden = flags.user_hidden.load(Ordering::Relaxed);

                    if fs_enabled && !user_hidden {
                        let is_fs = is_foreground_fullscreen(self_pid, hwnd);
                        if is_fs != hidden_by_fs {
                            hidden_by_fs = is_fs;
                            let _ = window.run_on_main_thread(move || unsafe {
                                let target = HWND(hwnd_val as *mut _);
                                if is_fs {
                                    let _ = ShowWindow(target, SW_HIDE);
                                } else {
                                    let _ = ShowWindow(target, SW_SHOWNOACTIVATE);
                                    let _ = SetWindowPos(
                                        target,
                                        HWND_TOPMOST,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                    );
                                }
                            });
                        }
                    } else if hidden_by_fs && !user_hidden {
                        hidden_by_fs = false;
                        let _ = window.run_on_main_thread(move || unsafe {
                            let target = HWND(hwnd_val as *mut _);
                            let _ = ShowWindow(target, SW_SHOWNOACTIVATE);
                            let _ = SetWindowPos(
                                target,
                                HWND_TOPMOST,
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                        });
                    }
                }

                thread::sleep(Duration::from_millis(sleep_ms));
            }
        })
        .ok();
}

pub fn start_free_drag_session(
    app: AppHandle,
    settings_state: Arc<Mutex<AppSettings>>,
    flags: Arc<WindowMonitorFlags>,
) -> Result<(), String> {
    let unlocked = settings_state
        .lock()
        .map(|s| s.unlock_position)
        .unwrap_or(false);
    if !unlocked {
        return Ok(());
    }

    if flags
        .dragging
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }

    let Some(main_win) = app.get_webview_window("main") else {
        flags.dragging.store(false, Ordering::SeqCst);
        return Err("Main window not found".to_string());
    };
    let Ok(raw_hwnd) = main_win.hwnd() else {
        flags.dragging.store(false, Ordering::SeqCst);
        return Err("HWND unavailable".to_string());
    };
    let hwnd_val = raw_hwnd.0 as isize;

    let mut start_cursor = POINT::default();
    let mut start_rect = RECT::default();
    unsafe {
        if GetCursorPos(&mut start_cursor).is_err()
            || GetWindowRect(HWND(hwnd_val as *mut _), &mut start_rect).is_err()
        {
            flags.dragging.store(false, Ordering::SeqCst);
            return Err("Failed to capture initial drag coordinates".to_string());
        }
    }

    thread::Builder::new()
        .name("island-free-drag".into())
        .spawn(move || {
            let hwnd = HWND(hwnd_val as *mut _);
            let started = Instant::now();

            while started.elapsed() < Duration::from_secs(60) {
                let down = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16) & 0x8000 != 0 };
                if !down {
                    break;
                }

                let mut cur = POINT::default();
                if unsafe { GetCursorPos(&mut cur).is_ok() } {
                    let nx = start_rect.left + (cur.x - start_cursor.x);
                    let ny = start_rect.top + (cur.y - start_cursor.y);
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            nx,
                            ny,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
                thread::sleep(Duration::from_millis(16));
            }

            let mut final_rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut final_rect).is_ok() } {
                let cx = (final_rect.left + final_rect.right) / 2;
                let cy = (final_rect.top + final_rect.bottom) / 2;

                let monitors = app.available_monitors().unwrap_or_default();
                let matched_mon = monitors
                    .into_iter()
                    .find(|m| {
                        let p = m.position();
                        let s = m.size();
                        cx >= p.x
                            && cx <= p.x + s.width as i32
                            && cy >= p.y
                            && cy <= p.y + s.height as i32
                    })
                    .or_else(|| app.primary_monitor().ok().flatten());

                let (mon_name, rel_x, rel_y) = if let Some(m) = matched_mon {
                    let p = m.position();
                    (
                        m.name().cloned(),
                        final_rect.left - p.x,
                        final_rect.top - p.y,
                    )
                } else {
                    (None, final_rect.left, final_rect.top)
                };

                let updated_snapshot = if let Ok(mut guard) = settings_state.lock() {
                    guard.position_preset = "custom".to_string();
                    guard.monitor_name = mon_name;
                    guard.custom_x = Some(rel_x);
                    guard.custom_y = Some(rel_y);
                    guard.offset_x = 0;
                    guard.offset_y = 0;
                    Some(guard.clone())
                } else {
                    None
                };

                if let Some(snap) = updated_snapshot {
                    let _ = save_settings_to_disk(&app, &snap);
                    let _ = reposition_island(&app, &snap);
                    let _ = app.emit("settings-changed", &snap);
                }
            }

            flags.dragging.store(false, Ordering::SeqCst);
        })
        .ok();

    Ok(())
}
