use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::System;
use tauri::{AppHandle, Emitter};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub has_battery: bool,
    pub battery_percent: Option<u8>,
    pub is_charging: bool,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            cpu_percent: 0.0,
            ram_percent: 0.0,
            ram_used_gb: 0.0,
            ram_total_gb: 0.0,
            has_battery: false,
            battery_percent: None,
            is_charging: true,
        }
    }
}

fn read_power_status() -> (bool, Option<u8>, bool) {
    let mut sps = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut sps).is_ok() };
    if !ok {
        return (false, None, true);
    }

    // BatteryFlag 128 = No system battery; 255 = Unknown
    let no_battery = (sps.BatteryFlag & 128) != 0 || sps.BatteryLifePercent > 100;
    let is_charging = sps.ACLineStatus == 1 || (sps.BatteryFlag & 8) != 0;

    if no_battery {
        (false, None, is_charging)
    } else {
        (true, Some(sps.BatteryLifePercent.min(100)), is_charging)
    }
}

pub fn collect_once(sys: &mut System) -> SystemMetrics {
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_percent = sys.global_cpu_usage().clamp(0.0, 100.0);
    let total_bytes = sys.total_memory() as f64;
    let used_bytes = sys.used_memory() as f64;

    let ram_total_gb = (total_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;
    let ram_used_gb = (used_bytes / (1024.0 * 1024.0 * 1024.0)) as f32;
    let ram_percent = if total_bytes > 0.0 {
        ((used_bytes / total_bytes) * 100.0).clamp(0.0, 100.0) as f32
    } else {
        0.0
    };

    let (has_battery, battery_percent, is_charging) = read_power_status();

    SystemMetrics {
        cpu_percent: (cpu_percent * 10.0).round() / 10.0,
        ram_percent: (ram_percent * 10.0).round() / 10.0,
        ram_used_gb: (ram_used_gb * 10.0).round() / 10.0,
        ram_total_gb: (ram_total_gb * 10.0).round() / 10.0,
        has_battery,
        battery_percent,
        is_charging,
    }
}

pub fn start_system_metrics_task(
    app: AppHandle,
    shutdown: Arc<AtomicBool>,
) -> Arc<Mutex<SystemMetrics>> {
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let initial = collect_once(&mut sys);
    let state = Arc::new(Mutex::new(initial.clone()));
    let state_task = state.clone();

    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(2000));
        let mut last_emitted = initial;
        while !shutdown.load(Ordering::Relaxed) {
            interval.tick().await;
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let sample = collect_once(&mut sys);
            if let Ok(mut g) = state_task.lock() {
                *g = sample.clone();
            }
            let changed = (sample.cpu_percent - last_emitted.cpu_percent).abs() >= 0.5
                || (sample.ram_percent - last_emitted.ram_percent).abs() >= 0.5
                || sample.has_battery != last_emitted.has_battery
                || sample.battery_percent != last_emitted.battery_percent
                || sample.is_charging != last_emitted.is_charging;
            if changed {
                last_emitted = sample.clone();
                let _ = app.emit("system-metrics", &sample);
            }
        }
    });

    state
}
