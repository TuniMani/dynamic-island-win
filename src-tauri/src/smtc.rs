use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

const MAX_THUMB_BYTES: u64 = 8 * 1024 * 1024;
const WINDOWS_TO_UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSessionSummary {
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaState {
    pub has_media: bool,
    pub is_playing: bool,
    pub status: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub app_id: String,
    pub thumbnail: Option<String>,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub playback_rate: f64,
    pub start_time_secs: f64,
    pub updated_at_ms: u64,
    pub sessions: Vec<MediaSessionSummary>,
}

impl MediaState {
    pub fn idle() -> Self {
        Self {
            has_media: false,
            is_playing: false,
            status: "Idle".to_string(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            app_id: String::new(),
            thumbnail: None,
            position_secs: 0.0,
            duration_secs: 0.0,
            playback_rate: 1.0,
            start_time_secs: 0.0,
            updated_at_ms: now_ms(),
            sessions: Vec::new(),
        }
    }
}

pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    Seek(f64),
    SelectSession(String),
    CycleSession,
}

#[derive(Clone)]
pub struct SmtcController {
    cmd_tx: Sender<MediaCommand>,
    pub state: Arc<Mutex<MediaState>>,
    pub visualizer_enabled: Arc<AtomicBool>,
    is_playing_flag: Arc<AtomicBool>,
    peak_wakeup: Arc<(Mutex<bool>, Condvar)>,
}

impl SmtcController {
    pub fn send(&self, cmd: MediaCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn set_visualizer_enabled(&self, enabled: bool) {
        self.visualizer_enabled.store(enabled, Ordering::Relaxed);
        let (lock, cvar) = &*self.peak_wakeup;
        if let Ok(mut active) = lock.lock() {
            *active = enabled && self.is_playing_flag.load(Ordering::Relaxed);
            cvar.notify_all();
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

type ThumbKey = (String, String, String, String);
type ThumbCache = Option<(ThumbKey, Option<String>)>;

enum InternalSignal {
    CurrentSessionChanged,
    SessionsChanged,
    MediaPropsDirty,
    PlaybackOrTimelineDirty,
    Command(MediaCommand),
}

struct SessionHooks {
    session: GlobalSystemMediaTransportControlsSession,
    media_token: i64,
    playback_token: i64,
    timeline_token: i64,
}

impl SessionHooks {
    fn detach(&self) {
        let _ = self
            .session
            .RemoveMediaPropertiesChanged(windows::Foundation::EventRegistrationToken {
                Value: self.media_token,
            });
        let _ = self
            .session
            .RemovePlaybackInfoChanged(windows::Foundation::EventRegistrationToken {
                Value: self.playback_token,
            });
        let _ = self
            .session
            .RemoveTimelinePropertiesChanged(windows::Foundation::EventRegistrationToken {
                Value: self.timeline_token,
            });
    }
}

fn detect_image_mime(bytes: &[u8], content_type: &str) -> String {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg".to_string();
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return "image/png".to_string();
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "image/webp".to_string();
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif".to_string();
    }
    if bytes.starts_with(b"BM") {
        return "image/bmp".to_string();
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return "image/x-icon".to_string();
    }
    let trimmed = content_type.trim();
    if trimmed.to_ascii_lowercase().starts_with("image/") {
        return trimmed.to_string();
    }
    "image/png".to_string()
}

fn filetime_to_unix_ms(universal_time_ticks: i64) -> Option<u64> {
    if universal_time_ticks > WINDOWS_TO_UNIX_EPOCH_TICKS {
        Some(((universal_time_ticks - WINDOWS_TO_UNIX_EPOCH_TICKS) / 10_000) as u64)
    } else {
        None
    }
}

fn compute_live_timeline(
    start_ticks: i64,
    end_ticks: i64,
    pos_ticks: i64,
    last_updated_ticks: i64,
    playback_rate: f64,
    is_playing: bool,
    now_unix_ms: u64,
) -> (f64, f64, f64) {
    let start_secs = (start_ticks as f64 / 10_000_000.0).max(0.0);
    let end_secs = (end_ticks as f64 / 10_000_000.0).max(0.0);
    let raw_pos_secs = (pos_ticks as f64 / 10_000_000.0).max(0.0);
    let duration_secs = (end_secs - start_secs).max(0.0);
    let base_pos = (raw_pos_secs - start_secs).max(0.0);

    let rate = if playback_rate.is_finite() && playback_rate > 0.0 && playback_rate <= 8.0 {
        playback_rate
    } else {
        1.0
    };

    let elapsed = if is_playing {
        match filetime_to_unix_ms(last_updated_ticks) {
            Some(lu_ms) if now_unix_ms >= lu_ms && (now_unix_ms - lu_ms) <= 86_400_000 => {
                ((now_unix_ms - lu_ms) as f64 / 1000.0) * rate
            }
            _ => 0.0,
        }
    } else {
        0.0
    };

    let live_pos = if duration_secs > 0.0 {
        (base_pos + elapsed).clamp(0.0, duration_secs)
    } else {
        base_pos + elapsed
    };

    (live_pos, duration_secs, start_secs)
}

fn read_thumbnail(thumb_ref: &IRandomAccessStreamReference) -> Option<String> {
    let op = thumb_ref.OpenReadAsync().ok()?;
    let stream = op.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > MAX_THUMB_BYTES {
        return None;
    }

    let content_type = stream
        .ContentType()
        .map(|h| h.to_string())
        .unwrap_or_default();

    let _ = stream.Seek(0);
    let input = stream.GetInputStreamAt(0).ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;

    let target = size as usize;
    let mut buf = Vec::with_capacity(target.min(512 * 1024));
    while buf.len() < target {
        let remaining = (target - buf.len()).min(256 * 1024) as u32;
        let loaded = match reader.LoadAsync(remaining).and_then(|op| op.get()) {
            Ok(n) if n > 0 => n as usize,
            _ => break,
        };
        let start = buf.len();
        buf.resize(start + loaded, 0);
        if reader.ReadBytes(&mut buf[start..]).is_err() {
            buf.truncate(start);
            break;
        }
    }

    if buf.is_empty() {
        return None;
    }

    let mime = detect_image_mime(&buf, &content_type);
    let b64 = STANDARD.encode(&buf);
    Some(format!("data:{mime};base64,{b64}"))
}

fn read_playback_status(
    session: &GlobalSystemMediaTransportControlsSession,
) -> (String, bool, f64) {
    match session.GetPlaybackInfo() {
        Ok(info) => {
            let (status_str, is_playing) = match info.PlaybackStatus() {
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing) => {
                    ("Playing".to_string(), true)
                }
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused) => {
                    ("Paused".to_string(), false)
                }
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped) => {
                    ("Stopped".to_string(), false)
                }
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing) => {
                    ("Paused".to_string(), false)
                }
                _ => ("Idle".to_string(), false),
            };
            let rate = info
                .PlaybackRate()
                .and_then(|r| r.Value())
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 8.0)
                .unwrap_or(1.0);
            (status_str, is_playing, rate)
        }
        Err(_) => ("Idle".to_string(), false, 1.0),
    }
}

fn extract_media_state(
    session: &GlobalSystemMediaTransportControlsSession,
    thumb_cache: &mut ThumbCache,
    force_refresh_thumb: bool,
) -> MediaState {
    let app_id = session
        .SourceAppUserModelId()
        .map(|s| s.to_string())
        .unwrap_or_default();

    let (status_str, is_playing, playback_rate) = read_playback_status(session);

    let (title, artist, album, thumb_ref) = match session.TryGetMediaPropertiesAsync() {
        Ok(op) => match op.get() {
            Ok(props) => (
                props.Title().map(|s| s.to_string()).unwrap_or_default(),
                props.Artist().map(|s| s.to_string()).unwrap_or_default(),
                props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default(),
                props.Thumbnail().ok(),
            ),
            Err(_) => (String::new(), String::new(), String::new(), None),
        },
        Err(_) => (String::new(), String::new(), String::new(), None),
    };

    let now_unix = now_ms();
    let (position_secs, duration_secs, start_time_secs) = match session.GetTimelineProperties() {
        Ok(tl) => {
            let start_ticks = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
            let end_ticks = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
            let pos_ticks = tl.Position().map(|t| t.Duration).unwrap_or(0);
            let last_updated_ticks = tl.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);
            compute_live_timeline(
                start_ticks,
                end_ticks,
                pos_ticks,
                last_updated_ticks,
                playback_rate,
                is_playing,
                now_unix,
            )
        }
        Err(_) => (0.0, 0.0, 0.0),
    };

    let has_media = !title.is_empty() || !artist.is_empty() || is_playing;
    if !has_media {
        *thumb_cache = None;
        return MediaState::idle();
    }

    let cache_key: ThumbKey = (
        app_id.clone(),
        title.clone(),
        artist.clone(),
        album.clone(),
    );

    let cached_same = thumb_cache
        .as_ref()
        .and_then(|(k, img)| if *k == cache_key { img.clone() } else { None });

    let thumbnail = if force_refresh_thumb {
        let fresh = thumb_ref.as_ref().and_then(read_thumbnail);
        let chosen = fresh.or(cached_same);
        *thumb_cache = Some((cache_key, chosen.clone()));
        chosen
    } else if let Some(existing) = cached_same {
        Some(existing)
    } else {
        let fresh = thumb_ref.as_ref().and_then(read_thumbnail);
        *thumb_cache = Some((cache_key, fresh.clone()));
        fresh
    };

    MediaState {
        has_media: true,
        is_playing,
        status: status_str,
        title,
        artist,
        album,
        app_id,
        thumbnail,
        position_secs,
        duration_secs,
        playback_rate,
        start_time_secs,
        updated_at_ms: now_unix,
        sessions: Vec::new(),
    }
}

struct EnumeratedSession {
    id: String,
    session: GlobalSystemMediaTransportControlsSession,
    summary: MediaSessionSummary,
}

fn enumerate_sessions(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Vec<EnumeratedSession> {
    let mut out = Vec::new();
    let list = match manager.GetSessions() {
        Ok(l) => l,
        Err(_) => return out,
    };
    let count = list.Size().unwrap_or(0);
    let mut counts: HashMap<String, usize> = HashMap::new();

    for i in 0..count {
        let Ok(session) = list.GetAt(i) else {
            continue;
        };
        let app_id = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let (_, is_playing, _) = read_playback_status(&session);

        let (title, artist) = match session.TryGetMediaPropertiesAsync() {
            Ok(op) => match op.get() {
                Ok(props) => (
                    props.Title().map(|s| s.to_string()).unwrap_or_default(),
                    props.Artist().map(|s| s.to_string()).unwrap_or_default(),
                ),
                Err(_) => (String::new(), String::new()),
            },
            Err(_) => (String::new(), String::new()),
        };

        if !is_playing && title.is_empty() && artist.is_empty() {
            continue;
        }

        let seq = counts.entry(app_id.clone()).or_insert(0);
        let id = format!("{app_id}#{}", *seq);
        *seq += 1;

        out.push(EnumeratedSession {
            id: id.clone(),
            session,
            summary: MediaSessionSummary {
                id,
                app_id,
                title,
                artist,
                is_playing,
                is_active: false,
            },
        });
    }

    out
}

fn attach_session_hooks(
    session: &GlobalSystemMediaTransportControlsSession,
    sig_tx: &Sender<InternalSignal>,
) -> Option<SessionHooks> {
    let tx1 = sig_tx.clone();
    let media_token = session
        .MediaPropertiesChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            MediaPropertiesChangedEventArgs,
        >::new(move |_, _| {
            let _ = tx1.send(InternalSignal::MediaPropsDirty);
            Ok(())
        }))
        .ok()?
        .Value;

    let tx2 = sig_tx.clone();
    let playback_token = session
        .PlaybackInfoChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            PlaybackInfoChangedEventArgs,
        >::new(move |_, _| {
            let _ = tx2.send(InternalSignal::PlaybackOrTimelineDirty);
            Ok(())
        }))
        .ok()?
        .Value;

    let tx3 = sig_tx.clone();
    let timeline_token = session
        .TimelinePropertiesChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            TimelinePropertiesChangedEventArgs,
        >::new(move |_, _| {
            let _ = tx3.send(InternalSignal::PlaybackOrTimelineDirty);
            Ok(())
        }))
        .ok()?
        .Value;

    Some(SessionHooks {
        session: session.clone(),
        media_token,
        playback_token,
        timeline_token,
    })
}

pub fn start_smtc(
    app: AppHandle,
    visualizer_enabled: bool,
    shutdown: Arc<AtomicBool>,
) -> SmtcController {
    let state = Arc::new(Mutex::new(MediaState::idle()));
    let (cmd_tx, cmd_rx): (Sender<MediaCommand>, Receiver<MediaCommand>) = mpsc::channel();
    let (sig_tx, sig_rx): (Sender<InternalSignal>, Receiver<InternalSignal>) = mpsc::channel();

    let vis_flag = Arc::new(AtomicBool::new(visualizer_enabled));
    let playing_flag = Arc::new(AtomicBool::new(false));
    let peak_wakeup = Arc::new((Mutex::new(false), Condvar::new()));

    // Bridge user commands into unified signal channel
    let sig_tx_bridge = sig_tx.clone();
    let shutdown_bridge = shutdown.clone();
    thread::Builder::new()
        .name("smtc-cmd-bridge".into())
        .spawn(move || {
            while !shutdown_bridge.load(Ordering::Relaxed) {
                match cmd_rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(cmd) => {
                        let _ = sig_tx_bridge.send(InternalSignal::Command(cmd));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .ok();

    // Main event-driven SMTC thread
    let app_smtc = app.clone();
    let state_smtc = state.clone();
    let vis_smtc = vis_flag.clone();
    let playing_smtc = playing_flag.clone();
    let wakeup_smtc = peak_wakeup.clone();
    let shutdown_smtc = shutdown.clone();

    thread::Builder::new()
        .name("smtc-listener".into())
        .spawn(move || {
            let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                .and_then(|op| op.get())
            {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("[SMTC] SessionManager başlatılamadı (Idle): {err}");
                    if com_hr.is_ok() {
                        unsafe { CoUninitialize() };
                    }
                    return;
                }
            };

            let tx_cur = sig_tx.clone();
            let cur_token = manager.CurrentSessionChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSessionManager,
                CurrentSessionChangedEventArgs,
            >::new(move |_, _| {
                let _ = tx_cur.send(InternalSignal::CurrentSessionChanged);
                Ok(())
            }));

            let tx_ses = sig_tx.clone();
            let ses_token = manager.SessionsChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSessionManager,
                SessionsChangedEventArgs,
            >::new(move |_, _| {
                let _ = tx_ses.send(InternalSignal::SessionsChanged);
                Ok(())
            }));

            struct ListenerCtx<'a> {
                manager: &'a GlobalSystemMediaTransportControlsSessionManager,
                sig_tx: &'a Sender<InternalSignal>,
                app_smtc: &'a AppHandle,
                state_smtc: &'a Arc<Mutex<MediaState>>,
                vis_smtc: &'a Arc<AtomicBool>,
                playing_smtc: &'a Arc<AtomicBool>,
                wakeup_smtc: &'a Arc<(Mutex<bool>, Condvar)>,
                active_hooks: Option<SessionHooks>,
                active_session_id: Option<String>,
                selected_session_id: Option<String>,
                active_start_secs: f64,
                thumb_cache: ThumbCache,
                last_logged_summary: String,
                thumb_retry_key: Option<ThumbKey>,
                thumb_retry_at: Option<Instant>,
                timeline_refresh_at: Option<Instant>,
            }

            impl ListenerCtx<'_> {
                fn refresh_and_emit(
                    &mut self,
                    on_current_session_changed: bool,
                    force_refresh_thumb: bool,
                ) {
                    let mut enumerated = enumerate_sessions(self.manager);
                    let current_session = self.manager.GetCurrentSession().ok();

                    if on_current_session_changed {
                        if let Some(ref pinned) = self.selected_session_id {
                            let pinned_playing = enumerated
                                .iter()
                                .find(|e| e.id == *pinned)
                                .map(|e| e.summary.is_playing);
                            let current_playing = current_session
                                .as_ref()
                                .map(|s| read_playback_status(s).1)
                                .unwrap_or(false);
                            if pinned_playing == Some(false) && current_playing {
                                self.selected_session_id = None;
                            }
                        }
                    }

                    // Resolve target session by priority
                    let chosen_idx = if let Some(ref pinned) = self.selected_session_id {
                        if let Some(idx) = enumerated.iter().position(|e| e.id == *pinned) {
                            Some(idx)
                        } else {
                            self.selected_session_id = None;
                            None
                        }
                    } else {
                        None
                    }
                    .or_else(|| {
                        let cur_app = current_session
                            .as_ref()
                            .and_then(|s| s.SourceAppUserModelId().ok())
                            .map(|h| h.to_string())?;
                        enumerated
                            .iter()
                            .position(|e| e.summary.app_id == cur_app && e.summary.is_playing)
                            .or_else(|| enumerated.iter().position(|e| e.summary.app_id == cur_app))
                    })
                    .or_else(|| enumerated.iter().position(|e| e.summary.is_playing))
                    .or_else(|| if enumerated.is_empty() { None } else { Some(0) });

                    let (target_id, target_session): (
                        Option<String>,
                        Option<GlobalSystemMediaTransportControlsSession>,
                    ) = if let Some(idx) = chosen_idx {
                        enumerated[idx].summary.is_active = true;
                        (
                            Some(enumerated[idx].id.clone()),
                            Some(enumerated[idx].session.clone()),
                        )
                    } else if let Some(cur) = current_session {
                        let app_id = cur
                            .SourceAppUserModelId()
                            .map(|h| h.to_string())
                            .unwrap_or_default();
                        (Some(format!("{app_id}#cur")), Some(cur))
                    } else {
                        (None, None)
                    };

                    let session_switched =
                        target_id != self.active_session_id || self.active_hooks.is_none();
                    if session_switched {
                        if let Some(old) = self.active_hooks.take() {
                            old.detach();
                        }
                        self.active_session_id = target_id;
                        if let Some(sess) = target_session.as_ref() {
                            self.active_hooks = attach_session_hooks(sess, self.sig_tx);
                        }
                    }

                    let mut new_state = match self.active_hooks.as_ref() {
                        Some(h) => extract_media_state(
                            &h.session,
                            &mut self.thumb_cache,
                            force_refresh_thumb || session_switched,
                        ),
                        None => {
                            self.thumb_cache = None;
                            MediaState::idle()
                        }
                    };

                    self.active_start_secs = new_state.start_time_secs;
                    new_state.sessions = enumerated.into_iter().map(|e| e.summary).collect();

                    if new_state.has_media && new_state.thumbnail.is_none() {
                        let cur_key: ThumbKey = (
                            new_state.app_id.clone(),
                            new_state.title.clone(),
                            new_state.artist.clone(),
                            new_state.album.clone(),
                        );
                        if self.thumb_retry_key.as_ref() != Some(&cur_key) {
                            self.thumb_retry_key = Some(cur_key);
                            self.thumb_retry_at =
                                Some(Instant::now() + Duration::from_millis(650));
                        }
                    } else {
                        self.thumb_retry_key = None;
                        self.thumb_retry_at = None;
                    }

                    self.playing_smtc
                        .store(new_state.is_playing, Ordering::Relaxed);
                    let (lock, cvar) = &**self.wakeup_smtc;
                    if let Ok(mut active) = lock.lock() {
                        *active =
                            new_state.is_playing && self.vis_smtc.load(Ordering::Relaxed);
                        cvar.notify_all();
                    }

                    let summary = format!(
                        "{}|{}|{}|{}|{}",
                        new_state.status,
                        new_state.title,
                        new_state.artist,
                        new_state.app_id,
                        new_state.sessions.len()
                    );
                    if summary != self.last_logged_summary {
                        if new_state.has_media {
                            println!(
                                "[SMTC] {} - '{}' ({}) [{}] (sessions: {})",
                                new_state.status,
                                new_state.title,
                                new_state.artist,
                                new_state.app_id,
                                new_state.sessions.len()
                            );
                        } else {
                            println!("[SMTC] No active session (Idle)");
                        }
                        self.last_logged_summary = summary;
                    }

                    if let Ok(mut guard) = self.state_smtc.lock() {
                        *guard = new_state.clone();
                    }
                    let _ = self.app_smtc.emit("media-changed", &new_state);
                }
            }

            let mut ctx = ListenerCtx {
                manager: &manager,
                sig_tx: &sig_tx,
                app_smtc: &app_smtc,
                state_smtc: &state_smtc,
                vis_smtc: &vis_smtc,
                playing_smtc: &playing_smtc,
                wakeup_smtc: &wakeup_smtc,
                active_hooks: None,
                active_session_id: None,
                selected_session_id: None,
                active_start_secs: 0.0,
                thumb_cache: None,
                last_logged_summary: String::new(),
                thumb_retry_key: None,
                thumb_retry_at: None,
                timeline_refresh_at: None,
            };

            // Initial query (safe when idle/empty)
            ctx.refresh_and_emit(true, true);

            while !shutdown_smtc.load(Ordering::Relaxed) {
                let mut wait = Duration::from_millis(200);
                let now = Instant::now();
                if let Some(at) = ctx.thumb_retry_at {
                    wait = wait.min(at.saturating_duration_since(now));
                }
                if let Some(at) = ctx.timeline_refresh_at {
                    wait = wait.min(at.saturating_duration_since(now));
                }

                match sig_rx.recv_timeout(wait) {
                    Ok(InternalSignal::CurrentSessionChanged) => {
                        ctx.refresh_and_emit(true, true);
                    }
                    Ok(InternalSignal::SessionsChanged) => {
                        ctx.refresh_and_emit(false, false);
                    }
                    Ok(InternalSignal::MediaPropsDirty) => {
                        ctx.thumb_retry_key = None;
                        ctx.thumb_retry_at = None;
                        ctx.refresh_and_emit(false, true);
                    }
                    Ok(InternalSignal::PlaybackOrTimelineDirty) => {
                        ctx.refresh_and_emit(false, false);
                    }
                    Ok(InternalSignal::Command(cmd)) => match cmd {
                        MediaCommand::PlayPause => {
                            if let Some(h) = &ctx.active_hooks {
                                let _ = h.session.TryTogglePlayPauseAsync();
                            }
                        }
                        MediaCommand::Next => {
                            if let Some(h) = &ctx.active_hooks {
                                let _ = h.session.TrySkipNextAsync();
                            }
                        }
                        MediaCommand::Previous => {
                            if let Some(h) = &ctx.active_hooks {
                                let _ = h.session.TrySkipPreviousAsync();
                            }
                        }
                        MediaCommand::Seek(secs) => {
                            if let Some(h) = &ctx.active_hooks {
                                let target_secs = ctx.active_start_secs + secs.max(0.0);
                                let ticks = (target_secs * 10_000_000.0) as i64;
                                let _ = h.session.TryChangePlaybackPositionAsync(ticks);
                                ctx.timeline_refresh_at =
                                    Some(Instant::now() + Duration::from_millis(180));
                            }
                        }
                        MediaCommand::SelectSession(id) => {
                            ctx.selected_session_id = Some(id);
                            ctx.refresh_and_emit(false, true);
                        }
                        MediaCommand::CycleSession => {
                            let enumerated = enumerate_sessions(&manager);
                            if enumerated.len() > 1 {
                                let cur_idx = ctx
                                    .active_session_id
                                    .as_ref()
                                    .and_then(|aid| enumerated.iter().position(|e| e.id == *aid))
                                    .unwrap_or(0);
                                let next_idx = (cur_idx + 1) % enumerated.len();
                                ctx.selected_session_id = Some(enumerated[next_idx].id.clone());
                                ctx.refresh_and_emit(false, true);
                            }
                        }
                    },
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                let now_after = Instant::now();
                if let Some(at) = ctx.thumb_retry_at {
                    if now_after >= at {
                        ctx.thumb_retry_at = None;
                        ctx.refresh_and_emit(false, true);
                    }
                }
                if let Some(at) = ctx.timeline_refresh_at {
                    if now_after >= at {
                        ctx.timeline_refresh_at = None;
                        ctx.refresh_and_emit(false, false);
                    }
                }
            }

            if let Some(old) = ctx.active_hooks.take() {
                old.detach();
            }
            if let Ok(tok) = cur_token {
                let _ = manager.RemoveCurrentSessionChanged(tok);
            }
            if let Ok(tok) = ses_token {
                let _ = manager.RemoveSessionsChanged(tok);
            }
            if com_hr.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .ok();

    // Audio peak meter thread: parks with 0% CPU when not playing
    let app_peak = app;
    let wakeup_peak = peak_wakeup.clone();
    let shutdown_peak = shutdown;

    thread::Builder::new()
        .name("audio-peak-meter".into())
        .spawn(move || {
            let com_hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let mut meter: Option<IAudioMeterInformation> = None;
            let mut last_peak: f32 = -1.0;

            'outer: while !shutdown_peak.load(Ordering::Relaxed) {
                let (lock, cvar) = &*wakeup_peak;
                let active = {
                    let mut guard = match lock.lock() {
                        Ok(g) => g,
                        Err(_) => break 'outer,
                    };
                    while !*guard && !shutdown_peak.load(Ordering::Relaxed) {
                        match cvar.wait_timeout(guard, Duration::from_millis(500)) {
                            Ok((g, _)) => guard = g,
                            Err(_) => break 'outer,
                        }
                    }
                    *guard
                };

                if shutdown_peak.load(Ordering::Relaxed) {
                    break;
                }
                if !active {
                    last_peak = -1.0;
                    continue;
                }

                if meter.is_none() {
                    meter = unsafe {
                        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                            .ok()
                            .and_then(|e| e.GetDefaultAudioEndpoint(eRender, eConsole).ok())
                            .and_then(|d| d.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None).ok())
                    };
                }

                if let Some(m) = &meter {
                    match unsafe { m.GetPeakValue() } {
                        Ok(raw_peak) => {
                            let peak = raw_peak.clamp(0.0, 1.0);
                            let zero_edge = peak == 0.0 && last_peak > 0.0;
                            if last_peak < 0.0 || zero_edge || (peak - last_peak).abs() >= 0.03 {
                                last_peak = peak;
                                let _ = app_peak.emit("audio-peak", peak);
                            }
                        }
                        Err(_) => {
                            // Device may have changed; re-acquire next iteration
                            meter = None;
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }

                thread::sleep(Duration::from_millis(120));
            }

            drop(meter);
            if com_hr.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .ok();

    SmtcController {
        cmd_tx,
        state,
        visualizer_enabled: vis_flag,
        is_playing_flag: playing_flag,
        peak_wakeup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_state_idle_is_safe_and_serializable() {
        let idle = MediaState::idle();
        assert!(!idle.has_media);
        assert!(!idle.is_playing);
        assert_eq!(idle.status, "Idle");
        assert_eq!(idle.playback_rate, 1.0);
        assert!(idle.sessions.is_empty());
        assert!(idle.title.is_empty());
        assert!(idle.thumbnail.is_none());
        let json = serde_json::to_string(&idle).expect("serialize idle MediaState");
        assert!(json.contains("\"status\":\"Idle\""));
    }

    #[test]
    fn detect_image_mime_identifies_magic_bytes_and_fallbacks() {
        assert_eq!(detect_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0], ""), "image/jpeg");
        assert_eq!(
            detect_image_mime(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00], ""),
            "image/png"
        );
        assert_eq!(
            detect_image_mime(b"RIFF\x24\x00\x00\x00WEBP", "application/octet-stream"),
            "image/webp"
        );
        assert_eq!(detect_image_mime(b"GIF89a...", ""), "image/gif");
        assert_eq!(detect_image_mime(b"BM\x00\x00", ""), "image/bmp");
        assert_eq!(detect_image_mime(b"unknown", "image/jpeg"), "image/jpeg");
        assert_eq!(
            detect_image_mime(b"unknown", "application/octet-stream"),
            "image/png"
        );
    }

    #[test]
    fn compute_live_timeline_extrapolates_and_clamps() {
        let lu_unix_ms: u64 = 1_700_000_000_000;
        let lu_filetime = WINDOWS_TO_UNIX_EPOCH_TICKS + (lu_unix_ms as i64) * 10_000;
        let sec: i64 = 10_000_000;

        // 1x playback, 10s elapsed since LastUpdatedTime
        let (pos, dur, start) = compute_live_timeline(
            0,
            180 * sec,
            30 * sec,
            lu_filetime,
            1.0,
            true,
            lu_unix_ms + 10_000,
        );
        assert!((pos - 40.0).abs() < 1e-6);
        assert!((dur - 180.0).abs() < 1e-6);
        assert!((start - 0.0).abs() < 1e-6);

        // 1.5x playback, 10s elapsed + 5s non-zero StartTime
        let (pos2, dur2, start2) = compute_live_timeline(
            5 * sec,
            65 * sec,
            25 * sec,
            lu_filetime,
            1.5,
            true,
            lu_unix_ms + 10_000,
        );
        assert!((pos2 - 35.0).abs() < 1e-6);
        assert!((dur2 - 60.0).abs() < 1e-6);
        assert!((start2 - 5.0).abs() < 1e-6);

        // Paused: no extrapolation
        let (pos_paused, _, _) = compute_live_timeline(
            0,
            100 * sec,
            22 * sec,
            lu_filetime,
            1.0,
            false,
            lu_unix_ms + 15_000,
        );
        assert!((pos_paused - 22.0).abs() < 1e-6);

        // Clamped at duration
        let (pos_clamped, _, _) = compute_live_timeline(
            0,
            50 * sec,
            45 * sec,
            lu_filetime,
            1.0,
            true,
            lu_unix_ms + 20_000,
        );
        assert!((pos_clamped - 50.0).abs() < 1e-6);

        // Invalid/zero LastUpdatedTime falls back cleanly to raw position
        let (pos_zero_lu, _, _) =
            compute_live_timeline(0, 100 * sec, 12 * sec, 0, 1.0, true, lu_unix_ms);
        assert!((pos_zero_lu - 12.0).abs() < 1e-6);
    }
}
