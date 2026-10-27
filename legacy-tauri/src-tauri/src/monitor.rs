//! Native replacement for StormView's web-push server: while the app runs (even
//! hidden in the tray) it checks NWS alerts every 90 seconds and air quality
//! every 30 minutes, and raises OS notifications for anything new.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::weather::fetch_point_alerts;
use crate::AppState;

const ALERT_INTERVAL: Duration = Duration::from_secs(90);
const AQI_EVERY_N_TICKS: u32 = 20; // 20 × 90 s = 30 min
const AQI_UNHEALTHY: f64 = 200.0;
const AQI_RENOTIFY_MS: i64 = 12 * 60 * 60 * 1000;

fn seen_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("notified-alerts.json"))
}

fn load_seen(app: &AppHandle) -> HashSet<String> {
    seen_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_seen(app: &AppHandle, seen: &HashSet<String>) {
    if let Some(p) = seen_path(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, serde_json::to_vec(seen).unwrap_or_default());
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("notification failed: {e}");
    }
}

pub async fn run(app: AppHandle) {
    let mut seen = load_seen(&app);
    let mut last_aqi_notice: i64 = 0;
    let mut tick: u32 = 0;
    loop {
        let state = app.state::<AppState>();
        let enabled = state.settings.bool("notificationsEnabled", false);
        if let (true, Some((lat, lon, place))) = (enabled, state.settings.location()) {
            if state.settings.bool("notifNws", true) {
                if let Ok(alerts) = fetch_point_alerts(&state.http, lat, lon).await {
                    let mut changed = false;
                    for a in &alerts {
                        if a.id.is_empty() || !seen.insert(a.id.clone()) {
                            continue;
                        }
                        changed = true;
                        let body = if a.headline.is_empty() { format!("Active for {place}") } else { a.headline.clone() };
                        notify(&app, &format!("⚠ {}", a.title), &body);
                    }
                    // Forget alerts that have expired so the file can't grow forever.
                    let active: HashSet<&String> = alerts.iter().map(|a| &a.id).collect();
                    let before = seen.len();
                    seen.retain(|id| active.contains(id));
                    if changed || seen.len() != before {
                        save_seen(&app, &seen);
                    }
                    let _ = app.emit("alerts-checked", alerts.len());
                }
            }
            if state.settings.bool("notifAqi", true) && tick % AQI_EVERY_N_TICKS == 0 {
                if let Ok(aq) = crate::services::air_quality(&state.http, lat, lon).await {
                    let now = chrono::Utc::now().timestamp_millis();
                    if let Some(aqi) = aq["us_aqi"].as_f64() {
                        if aqi >= AQI_UNHEALTHY && now - last_aqi_notice > AQI_RENOTIFY_MS {
                            last_aqi_notice = now;
                            notify(
                                &app,
                                "Unhealthy air quality",
                                &format!("The US AQI near {place} is {aqi:.0}. Limit time outdoors."),
                            );
                        }
                    }
                }
            }
        }
        tick = tick.wrapping_add(1);
        tokio::time::sleep(ALERT_INTERVAL).await;
    }
}
