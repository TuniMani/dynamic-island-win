use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::core::{w, Interface};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationRangeValuePattern, TreeScope_Descendants,
    UIA_ItemStatusPropertyId, UIA_NamePropertyId, UIA_RangeValuePatternId,
};
use windows::Win32::UI::Controls::{PBM_GETPOS, PBM_GETRANGE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GetClassNameW,
    GetForegroundWindow, GetParent, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW, GW_OWNER, SMTO_ABORTIFHUNG,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProgress {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub percent: u8,
    pub source: String,
}

#[derive(Clone, Default)]
pub struct InternalTasks {
    tasks: Arc<Mutex<Vec<TaskProgress>>>,
}

impl InternalTasks {
    pub fn upsert(&self, task: TaskProgress) {
        if let Ok(mut guard) = self.tasks.lock() {
            if let Some(existing) = guard.iter_mut().find(|t| t.id == task.id) {
                *existing = task;
            } else {
                guard.push(task);
            }
        }
    }

    pub fn remove(&self, id: &str) {
        if let Ok(mut guard) = self.tasks.lock() {
            guard.retain(|t| t.id != id);
        }
    }

    pub fn snapshot(&self) -> Vec<TaskProgress> {
        self.tasks.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

pub fn parse_percentage_from_text(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    let len = bytes.len();

    for (i, &b) in bytes.iter().enumerate() {
        if b != b'%' {
            continue;
        }
        // Prefix style: "%45"
        if i + 1 < len && bytes[i + 1].is_ascii_digit() {
            let mut end = i + 1;
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end - (i + 1) <= 3 {
                if let Ok(val) = text[i + 1..end].parse::<u16>() {
                    if val <= 100 {
                        return Some(val as u8);
                    }
                }
            }
        }
        // Suffix style: "45%"
        if i > 0 && bytes[i - 1].is_ascii_digit() {
            let mut start = i;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if i - start <= 3 {
                if let Ok(val) = text[start..i].parse::<u16>() {
                    if val <= 100 {
                        return Some(val as u8);
                    }
                }
            }
        }
    }

    None
}

fn is_ignored_system_widget_text(lower: &str) -> bool {
    const IGNORED: &[&str] = &[
        "ses",
        "hoparlör",
        "speaker",
        "volume",
        "pil",
        "battery",
        "mikrofon",
        "microphone",
        "parlaklık",
        "brightness",
        "cpu",
        "ram",
        "nem",
        "humidity",
    ];
    IGNORED.iter().any(|kw| lower.contains(kw))
}

fn is_browser_process(proc_lower: &str) -> bool {
    matches!(
        proc_lower,
        "chrome.exe"
            | "brave.exe"
            | "msedge.exe"
            | "firefox.exe"
            | "opera.exe"
            | "vivaldi.exe"
            | "arc.exe"
            | "chromium.exe"
            | "waterfox.exe"
            | "librewolf.exe"
            | "floorp.exe"
    )
}

fn is_known_transfer_process(proc_lower: &str) -> bool {
    const TRANSFER_PROCS: &[&str] = &[
        "7zg.exe",
        "7zfm.exe",
        "winrar.exe",
        "qbittorrent.exe",
        "idman.exe",
        "fdm.exe",
        "jdownloader",
        "teracopy.exe",
        "msiexec.exe",
    ];
    TRANSFER_PROCS.iter().any(|p| proc_lower.contains(p))
}

fn is_file_dialog_text(lower: &str) -> bool {
    const DIALOG_LABELS: &[&str] = &[
        "save as",
        "common item dialog",
        "#32770",
        "farklı kaydet",
        "speichern unter",
        "guardar como",
        "enregistrer sous",
        "сохранить как",
        "名前を付けて保存",
        "salvar como",
        "select folder",
        "klasör seç",
        "ordner auswählen",
    ];
    DIALOG_LABELS.iter().any(|kw| lower.contains(kw))
}

fn has_transfer_context(title_lower: &str, proc_lower: &str) -> bool {
    const TITLE_KEYWORDS: &[&str] = &[
        "kopyala",
        "taşınıyor",
        "siliniyor",
        "tamamlandı",
        "kaldı",
        "indir",
        "yüklen",
        "ayıkla",
        "sıkıştır",
        "copying",
        "moving",
        "deleting",
        "complete",
        "remaining",
        "download",
        "upload",
        "extract",
        "compress",
        "installing",
        "kuruluyor",
        "kopier",
        "herunterlad",
        "descarg",
        "télécharg",
        "копир",
        "загруз",
        "コピー",
        "ダウンロード",
        "baixando",
    ];

    TITLE_KEYWORDS.iter().any(|kw| title_lower.contains(kw))
        || is_known_transfer_process(proc_lower)
}

fn get_window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..copied as usize])
    }
}

fn get_window_class(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 128];
        let len = GetClassNameW(hwnd, &mut buf);
        if len <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

fn get_process_name(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        if !ok || size == 0 {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        full.rsplit('\\').next().unwrap_or(&full).to_string()
    }
}

unsafe fn has_common_item_dialog_child(hwnd: HWND) -> bool {
    let classes = [
        w!("DUIViewWndClassName"),
        w!("SHELLDLL_DefView"),
        w!("ComboBoxEx32"),
        w!("WorkerW"),
        w!("ReBarWindow32"),
        w!("DirectUIHWND"),
        w!("FloatNotifySink"),
        w!("SysTreeView32"),
    ];
    for cls in classes {
        if let Ok(child) = FindWindowExW(hwnd, None, cls, None) {
            if !child.0.is_null() {
                return true;
            }
        }
    }
    false
}

unsafe fn is_native_file_or_modal_dialog(
    hwnd: HWND,
    class: &str,
    proc_lower: &str,
    title_lower: &str,
) -> bool {
    if class != "#32770" {
        return false;
    }
    if is_browser_process(proc_lower)
        || has_common_item_dialog_child(hwnd)
        || is_file_dialog_text(title_lower)
    {
        return true;
    }
    let has_owner = GetWindow(hwnd, GW_OWNER)
        .map(|o| !o.0.is_null())
        .unwrap_or(false);
    has_owner && !is_known_transfer_process(proc_lower)
}

unsafe extern "system" fn find_progress_child_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let class = get_window_class(hwnd);
    if class != "msctls_progress32" {
        return BOOL(1);
    }

    // Never touch address-bar or shell-view embedded progress bars
    let mut cur = hwnd;
    for _ in 0..4 {
        let Ok(parent) = GetParent(cur) else {
            break;
        };
        if parent.0.is_null() {
            break;
        }
        let pclass = get_window_class(parent);
        if matches!(
            pclass.as_str(),
            "Address Band Root"
                | "TravelBand"
                | "ReBarWindow32"
                | "WorkerW"
                | "Breadcrumb Parent"
                | "msctls_statusbar32"
                | "DUIViewWndClassName"
                | "SHELLDLL_DefView"
        ) {
            return BOOL(1);
        }
        cur = parent;
    }

    // Cross-process safe scalar PBM_* queries (LPARAM(0) only; never pass a local pointer)
    let mut low_res: usize = 0;
    let ok_low = SendMessageTimeoutW(
        hwnd,
        PBM_GETRANGE,
        WPARAM(1),
        LPARAM(0),
        SMTO_ABORTIFHUNG,
        50,
        Some(&mut low_res),
    );

    let mut high_res: usize = 0;
    let ok_high = SendMessageTimeoutW(
        hwnd,
        PBM_GETRANGE,
        WPARAM(0),
        LPARAM(0),
        SMTO_ABORTIFHUNG,
        50,
        Some(&mut high_res),
    );

    let mut pos_res: usize = 0;
    let ok_pos = SendMessageTimeoutW(
        hwnd,
        PBM_GETPOS,
        WPARAM(0),
        LPARAM(0),
        SMTO_ABORTIFHUNG,
        50,
        Some(&mut pos_res),
    );

    let low = low_res as i32;
    let high = high_res as i32;
    let pos = pos_res as i32;
    if ok_low.0 != 0 && ok_high.0 != 0 && ok_pos.0 != 0 && high > low && pos > low && pos < high {
        let pct = (((pos - low) as f64 / (high - low) as f64) * 100.0)
            .round()
            .clamp(1.0, 99.0) as u8;
        let out_ptr = lparam.0 as *mut Option<u8>;
        if !out_ptr.is_null() {
            *out_ptr = Some(pct);
        }
        return BOOL(0);
    }

    BOOL(1)
}

struct EnumCollector {
    self_pid: u32,
    tasks: Vec<TaskProgress>,
}

unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let collector = &mut *(lparam.0 as *mut EnumCollector);

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == collector.self_pid {
        return BOOL(1);
    }

    let class = get_window_class(hwnd);
    let title = get_window_text(hwnd);
    let title_trim = title.trim();

    if class == "OperationStatusWindow" {
        let pct = parse_percentage_from_text(title_trim).unwrap_or(0);
        collector.tasks.push(TaskProgress {
            id: format!("hwnd-{}", hwnd.0 as usize),
            title: if title_trim.is_empty() {
                "File Operation".to_string()
            } else {
                title_trim.to_string()
            },
            subtitle: "Windows Explorer".to_string(),
            percent: pct,
            source: "Explorer".to_string(),
        });
        return BOOL(1);
    }

    let proc_name = get_process_name(hwnd);
    let proc_lower = proc_name.to_lowercase();
    let title_lower = title_trim.to_lowercase();

    // Completely ignore native Windows Open / Save As (#32770) and browser modal dialogs
    if is_native_file_or_modal_dialog(hwnd, &class, &proc_lower, &title_lower) {
        return BOOL(1);
    }

    // Never enumerate children inside Windows Explorer folder windows
    if matches!(class.as_str(), "CabinetWClass" | "ExploreWClass") {
        return BOOL(1);
    }

    // Non-modal transfer dialogs (#32770) or archivers/installers containing msctls_progress32
    if !is_browser_process(&proc_lower)
        && (is_known_transfer_process(&proc_lower)
            || has_transfer_context(&title_lower, &proc_lower)
            || class == "#32770")
    {
        let mut child_pct: Option<u8> = None;
        let _ = EnumChildWindows(
            hwnd,
            Some(find_progress_child_cb),
            LPARAM(&mut child_pct as *mut Option<u8> as isize),
        );
        if let Some(pct) = child_pct {
            collector.tasks.push(TaskProgress {
                id: format!("hwnd-{}", hwnd.0 as usize),
                title: if title_trim.is_empty() {
                    "Background Task".to_string()
                } else {
                    title_trim.to_string()
                },
                subtitle: if proc_name.is_empty() {
                    "System Task".to_string()
                } else {
                    proc_name
                },
                percent: pct,
                source: if class == "#32770" {
                    "Explorer".to_string()
                } else {
                    "App".to_string()
                },
            });
            return BOOL(1);
        }
    }

    if !title_trim.is_empty() && !is_ignored_system_widget_text(&title_lower) {
        if has_transfer_context(&title_lower, &proc_lower) {
            if let Some(pct) = parse_percentage_from_text(title_trim) {
                collector.tasks.push(TaskProgress {
                    id: format!("hwnd-{}", hwnd.0 as usize),
                    title: title_trim.to_string(),
                    subtitle: if proc_name.is_empty() {
                        "Transfer".to_string()
                    } else {
                        proc_name
                    },
                    percent: pct,
                    source: if proc_lower == "explorer.exe" {
                        "Explorer".to_string()
                    } else {
                        "App".to_string()
                    },
                });
            }
        }
    }

    BOOL(1)
}

fn scan_taskbar_uia(automation: &IUIAutomation) -> Vec<TaskProgress> {
    let mut found = Vec::new();
    let tray = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) };
    let Ok(tray_hwnd) = tray else {
        return found;
    };
    if tray_hwnd.0.is_null() {
        return found;
    }

    let Ok(root) = (unsafe { automation.ElementFromHandle(tray_hwnd) }) else {
        return found;
    };
    let Ok(true_cond) = (unsafe { automation.CreateTrueCondition() }) else {
        return found;
    };
    let Ok(elements) = (unsafe { root.FindAll(TreeScope_Descendants, &true_cond) }) else {
        return found;
    };

    let count = unsafe { elements.Length().unwrap_or(0) }.min(200);
    let mut seen_titles = HashSet::new();

    for idx in 0..count {
        let Ok(el) = (unsafe { elements.GetElement(idx) }) else {
            continue;
        };

        let name = unsafe {
            el.GetCurrentPropertyValue(UIA_NamePropertyId)
                .ok()
                .map(|v| v.to_string())
                .unwrap_or_default()
        };
        let status = unsafe {
            el.GetCurrentPropertyValue(UIA_ItemStatusPropertyId)
                .ok()
                .map(|v| v.to_string())
                .unwrap_or_default()
        };

        let combined_lower = format!("{name} {status}").to_lowercase();
        if is_ignored_system_widget_text(&combined_lower) || is_file_dialog_text(&combined_lower) {
            continue;
        }

        // Direct ITaskbarList3 RangeValue pattern exposed on taskbar button/indicator
        if let Ok(unk) = unsafe { el.GetCurrentPattern(UIA_RangeValuePatternId) } {
            if let Ok(rv) = unk.cast::<IUIAutomationRangeValuePattern>() {
                if let (Ok(val), Ok(min), Ok(max)) = unsafe {
                    (
                        rv.CurrentValue(),
                        rv.CurrentMinimum(),
                        rv.CurrentMaximum(),
                    )
                } {
                    if max > min && val > min && val <= max {
                        let pct = (((val - min) / (max - min)) * 100.0)
                            .round()
                            .clamp(1.0, 100.0) as u8;
                        if pct < 100 {
                            let label = if !name.trim().is_empty() {
                                name.trim().to_string()
                            } else {
                                "Taskbar Progress".to_string()
                            };
                            if seen_titles.insert(label.clone()) {
                                found.push(TaskProgress {
                                    id: format!("uia-rv-{idx}"),
                                    title: label,
                                    subtitle: "ITaskbarList3".to_string(),
                                    percent: pct,
                                    source: "App".to_string(),
                                });
                            }
                            continue;
                        }
                    }
                }
            }
        }

        // ItemStatus fallback (e.g. "50% complete" / "%50 tamamlandı")
        if !status.trim().is_empty() && has_transfer_context(&combined_lower, "") {
            if let Some(pct) = parse_percentage_from_text(&status) {
                let label = if !name.trim().is_empty() {
                    name.trim().to_string()
                } else {
                    "Taskbar Task".to_string()
                };
                if seen_titles.insert(label.clone()) {
                    found.push(TaskProgress {
                        id: format!("uia-st-{idx}"),
                        title: label,
                        subtitle: status.trim().to_string(),
                        percent: pct,
                        source: "App".to_string(),
                    });
                }
            }
        }
    }

    found
}

pub fn start_progress_tracker(
    app: AppHandle,
    enabled_flag: Arc<AtomicBool>,
    internal_tasks: InternalTasks,
    latest_tasks: Arc<Mutex<Vec<TaskProgress>>>,
    shutdown: Arc<AtomicBool>,
) {
    let self_pid = std::process::id();

    std::thread::Builder::new()
        .name("progress-tracker".into())
        .spawn(move || {
            if crate::defer_startup(&shutdown, 1000) {
                return;
            }
            let com_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let automation: Option<IUIAutomation> =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };

            let mut last_emitted: Vec<TaskProgress> = Vec::new();
            let mut tick: u32 = 0;

            while !shutdown.load(Ordering::Relaxed) {
                tick = tick.wrapping_add(1);
                let mut merged = internal_tasks.snapshot();

                if enabled_flag.load(Ordering::Relaxed) {
                    let mut collector = EnumCollector {
                        self_pid,
                        tasks: Vec::new(),
                    };
                    unsafe {
                        let _ = EnumWindows(
                            Some(enum_windows_cb),
                            LPARAM(&mut collector as *mut EnumCollector as isize),
                        );
                    }
                    let has_explorer_task = collector.tasks.iter().any(|t| t.source == "Explorer");
                    merged.extend(collector.tasks);

                    let fg = unsafe { GetForegroundWindow() };
                    let fg_is_dialog = !fg.0.is_null() && get_window_class(fg) == "#32770";

                    if !fg_is_dialog && !has_explorer_task && tick % 2 == 1 {
                        if let Some(auto) = &automation {
                            let uia_tasks = scan_taskbar_uia(auto);
                            for t in uia_tasks {
                                if !merged.iter().any(|m| m.title == t.title) {
                                    merged.push(t);
                                }
                            }
                        }
                    }
                }

                if merged != last_emitted {
                    last_emitted = merged.clone();
                    if let Ok(mut guard) = latest_tasks.lock() {
                        *guard = merged.clone();
                    }
                    let _ = app.emit("progress-update", &merged);
                }

                std::thread::sleep(Duration::from_millis(2000));
            }

            drop(automation);
            if com_hr.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_turkish_and_english_percentages() {
        assert_eq!(parse_percentage_from_text("%45 tamamlandı"), Some(45));
        assert_eq!(
            parse_percentage_from_text("Öğeler kopyalanıyor - %100 tamamlandı"),
            Some(100)
        );
        assert_eq!(parse_percentage_from_text("Copying 5 files (72% complete)"), Some(72));
        assert_eq!(parse_percentage_from_text("0% completed"), Some(0));
        assert_eq!(parse_percentage_from_text("No progress here"), None);
        assert_eq!(parse_percentage_from_text("150% invalid"), None);
        assert_eq!(parse_percentage_from_text("1050% invalid"), None);
    }

    #[test]
    fn filters_system_widgets_and_detects_transfer_context() {
        assert!(is_ignored_system_widget_text("hoparlör: %64"));
        assert!(is_ignored_system_widget_text("battery: 88%"));
        assert!(!is_ignored_system_widget_text("dosya kopyalanıyor - %42"));
        assert!(has_transfer_context("dosya indiriliyor %55", "chrome.exe"));
        assert!(has_transfer_context("arsiv.zip %30", "7zfm.exe"));
        assert!(!has_transfer_context("100% organic coffee - wikipedia", "chrome.exe"));
    }

    #[test]
    fn identifies_browser_processes_and_file_dialog_labels() {
        assert!(is_browser_process("brave.exe"));
        assert!(is_browser_process("chrome.exe"));
        assert!(is_browser_process("msedge.exe"));
        assert!(!is_browser_process("explorer.exe"));
        assert!(is_known_transfer_process("7zfm.exe"));
        assert!(is_known_transfer_process("msiexec.exe"));
        assert!(!is_known_transfer_process("brave.exe"));
        assert!(is_file_dialog_text("farklı kaydet"));
        assert!(is_file_dialog_text("save as"));
        assert!(is_file_dialog_text("common item dialog"));
        assert!(is_file_dialog_text("speichern unter"));
        assert!(is_file_dialog_text("名前を付けて保存"));
        assert!(!is_file_dialog_text("copying 10 items - 45%"));
    }
}
