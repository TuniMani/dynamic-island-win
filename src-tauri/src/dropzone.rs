use crate::progress_tracker::{InternalTasks, TaskProgress};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{BOOL, GlobalFree, HANDLE, HWND, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::UI::Shell::DROPFILES;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CF_UNICODETEXT_U32: u32 = 13;
const CF_HDROP_U32: u32 = 15;

pub fn copy_files_to_clipboard(paths: &[String]) -> Result<usize, String> {
    let valid_paths: Vec<String> = paths
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && Path::new(p).exists())
        .collect();

    if valid_paths.is_empty() {
        return Err("Panoya kopyalanacak geçerli dosya bulunamadı".to_string());
    }

    let mut utf16_chars: Vec<u16> = Vec::new();
    for p in &valid_paths {
        utf16_chars.extend(p.encode_utf16());
        utf16_chars.push(0);
    }
    utf16_chars.push(0); // double null terminator

    let header_size = std::mem::size_of::<DROPFILES>();
    let data_bytes = utf16_chars.len() * std::mem::size_of::<u16>();
    let total_size = header_size + data_bytes;

    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size)
            .map_err(|e| format!("Bellek ayrılamadı: {e}"))?;
        let locked = GlobalLock(hglobal);
        if locked.is_null() {
            let _ = GlobalFree(hglobal);
            return Err("GlobalLock başarısız oldu".to_string());
        }

        let df = DROPFILES {
            pFiles: header_size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: BOOL(0),
            fWide: BOOL(1),
        };
        ptr::write(locked as *mut DROPFILES, df);

        let dst = (locked as *mut u8).add(header_size) as *mut u16;
        ptr::copy_nonoverlapping(utf16_chars.as_ptr(), dst, utf16_chars.len());
        let _ = GlobalUnlock(hglobal);

        if OpenClipboard(HWND::default()).is_err() {
            let _ = GlobalFree(hglobal);
            return Err("Pano açılamadı".to_string());
        }

        let _ = EmptyClipboard();
        let set_res = SetClipboardData(CF_HDROP_U32, HANDLE(hglobal.0));
        let _ = CloseClipboard();

        if set_res.is_err() {
            let _ = GlobalFree(hglobal);
            return Err("Dosyalar panoya aktarılamadı".to_string());
        }
    }

    Ok(valid_paths.len())
}

pub fn copy_paths_text_to_clipboard(paths: &[String]) -> Result<usize, String> {
    let joined = paths
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\r\n");

    if joined.is_empty() {
        return Err("Kopyalanacak yol bulunamadı".to_string());
    }

    let mut utf16: Vec<u16> = joined.encode_utf16().collect();
    utf16.push(0);
    let byte_len = utf16.len() * std::mem::size_of::<u16>();

    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, byte_len)
            .map_err(|e| format!("Bellek ayrılamadı: {e}"))?;
        let locked = GlobalLock(hglobal);
        if locked.is_null() {
            let _ = GlobalFree(hglobal);
            return Err("GlobalLock başarısız oldu".to_string());
        }
        ptr::copy_nonoverlapping(utf16.as_ptr(), locked as *mut u16, utf16.len());
        let _ = GlobalUnlock(hglobal);

        if OpenClipboard(HWND::default()).is_err() {
            let _ = GlobalFree(hglobal);
            return Err("Pano açılamadı".to_string());
        }
        let _ = EmptyClipboard();
        let set_res = SetClipboardData(CF_UNICODETEXT_U32, HANDLE(hglobal.0));
        let _ = CloseClipboard();

        if set_res.is_err() {
            let _ = GlobalFree(hglobal);
            return Err("Metin panoya kopyalanamadı".to_string());
        }
    }

    Ok(paths.len())
}

fn collect_entries_recursive(
    base_prefix: &Path,
    cur: &Path,
    out: &mut Vec<(PathBuf, String, u64)>,
) {
    let Ok(meta) = fs::metadata(cur) else {
        return;
    };

    if meta.is_file() {
        let rel = cur
            .strip_prefix(base_prefix)
            .unwrap_or(cur)
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.is_empty() {
            out.push((cur.to_path_buf(), rel, meta.len()));
        }
        return;
    }

    if meta.is_dir() {
        if let Ok(entries) = fs::read_dir(cur) {
            for entry in entries.flatten() {
                collect_entries_recursive(base_prefix, &entry.path(), out);
            }
        }
    }
}

fn pick_zip_destination(paths: &[String]) -> Result<PathBuf, String> {
    let first = Path::new(
        paths
            .first()
            .ok_or_else(|| "ZIP için en az bir dosya seçilmeli".to_string())?,
    );
    let parent = first
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let stem = if paths.len() == 1 {
        first
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("arsiv")
            .to_string()
    } else {
        "dinamik_ada_arsiv".to_string()
    };

    let candidate = parent.join(format!("{stem}_archive.zip"));
    if !candidate.exists() {
        return Ok(candidate);
    }
    for idx in 1..1000 {
        let next = parent.join(format!("{stem}_archive_{idx}.zip"));
        if !next.exists() {
            return Ok(next);
        }
    }
    Err("Uygun ZIP dosya adı bulunamadı".to_string())
}

pub fn create_zip_sync<F>(paths: &[String], mut on_progress: F) -> Result<String, String>
where
    F: FnMut(u8),
{
    let dest = pick_zip_destination(paths)?;
    let mut entries: Vec<(PathBuf, String, u64)> = Vec::new();

    for raw in paths {
        let p = Path::new(raw);
        if !p.exists() {
            continue;
        }
        let base = p.parent().unwrap_or_else(|| Path::new(""));
        collect_entries_recursive(base, p, &mut entries);
    }

    if entries.is_empty() {
        return Err("Sıkıştırılacak okunabilir dosya bulunamadı".to_string());
    }

    let total_bytes: u64 = entries.iter().map(|(_, _, sz)| *sz).sum::<u64>().max(1);
    let file = File::create(&dest).map_err(|e| format!("ZIP dosyası oluşturulamadı: {e}"))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut processed: u64 = 0;
    let mut last_pct: u8 = 0;
    let mut last_emit = Instant::now();
    let mut buf = vec![0u8; 64 * 1024];
    on_progress(0);

    for (src_path, rel_name, _) in entries {
        let Ok(mut src_file) = File::open(&src_path) else {
            continue;
        };
        if writer.start_file(&rel_name, options).is_err() {
            continue;
        }

        loop {
            let n = src_file
                .read(&mut buf)
                .map_err(|e| format!("Dosya okuma hatası: {e}"))?;
            if n == 0 {
                break;
            }
            writer
                .write_all(&buf[..n])
                .map_err(|e| format!("ZIP yazma hatası: {e}"))?;
            processed = processed.saturating_add(n as u64);
            let pct = ((processed as f64 / total_bytes as f64) * 100.0)
                .round()
                .clamp(0.0, 99.0) as u8;
            if pct > last_pct && last_emit.elapsed() >= Duration::from_millis(60) {
                last_pct = pct;
                last_emit = Instant::now();
                on_progress(pct);
            }
        }
    }

    writer
        .finish()
        .map_err(|e| format!("ZIP tamamlanamadı: {e}"))?;
    on_progress(100);

    Ok(dest.to_string_lossy().to_string())
}

pub async fn create_zip_with_progress(
    paths: Vec<String>,
    internal_tasks: InternalTasks,
    app: AppHandle,
) -> Result<String, String> {
    let task_id = "dropzone-zip".to_string();
    let emit_progress = |pct: u8| {
        let task = TaskProgress {
            id: task_id.clone(),
            title: "ZIP Arşivi Oluşturuluyor".to_string(),
            subtitle: format!("{} öğe sıkıştırılıyor", paths.len()),
            percent: pct,
            source: "Dropzone".to_string(),
        };
        internal_tasks.upsert(task);
        let snap = internal_tasks.snapshot();
        let _ = app.emit("progress-update", &snap);
    };

    emit_progress(0);
    let paths_clone = paths.clone();
    let app_clone = app.clone();
    let internal_clone = internal_tasks.clone();
    let task_id_clone = task_id.clone();

    let res = tokio::task::spawn_blocking(move || {
        create_zip_sync(&paths_clone, |pct| {
            let task = TaskProgress {
                id: task_id_clone.clone(),
                title: "ZIP Arşivi Oluşturuluyor".to_string(),
                subtitle: format!("{} öğe sıkıştırılıyor", paths_clone.len()),
                percent: pct,
                source: "Dropzone".to_string(),
            };
            internal_clone.upsert(task);
            let snap = internal_clone.snapshot();
            let _ = app_clone.emit("progress-update", &snap);
        })
    })
    .await
    .map_err(|e| format!("ZIP görevi beklenemedi: {e}"))?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    internal_tasks.remove(&task_id);
    let snap = internal_tasks.snapshot();
    let _ = app.emit("progress-update", &snap);

    res
}

pub fn reveal_in_explorer(path: &str) -> Result<(), String> {
    let clean = path.trim();
    if !Path::new(clean).exists() {
        return Err("Hedef dosya bulunamadı".to_string());
    }
    Command::new("explorer.exe")
        .arg(format!("/select,{clean}"))
        .spawn()
        .map_err(|e| format!("Gezgin başlatılamadı: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::ZipArchive;

    #[test]
    fn creates_valid_zip_archive_from_temp_files() {
        let base = std::env::temp_dir().join("di_zip_unit_test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("temp dir");

        let file_a = base.join("ornek.txt");
        let file_b = base.join("veri.json");
        fs::write(&file_a, b"Merhaba Dinamik Ada!").expect("write a");
        fs::write(&file_b, br#"{"ok":true}"#).expect("write b");

        let mut seen = Vec::new();
        let zip_path = create_zip_sync(
            &[
                file_a.to_string_lossy().to_string(),
                file_b.to_string_lossy().to_string(),
            ],
            |p| seen.push(p),
        )
        .expect("zip created");

        assert!(Path::new(&zip_path).exists());
        assert_eq!(seen.last().copied(), Some(100));

        let f = File::open(&zip_path).expect("open zip");
        let mut archive = ZipArchive::new(f).expect("valid zip");
        assert_eq!(archive.len(), 2);

        let mut entry = archive.by_name("ornek.txt").expect("ornek.txt in zip");
        let mut content = String::new();
        entry.read_to_string(&mut content).expect("read zip entry");
        assert_eq!(content, "Merhaba Dinamik Ada!");

        drop(entry);
        drop(archive);
        let _ = fs::remove_dir_all(&base);
    }
}
