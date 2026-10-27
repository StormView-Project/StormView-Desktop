mod assistant;
mod error;
mod http;
mod monitor;
mod services;
mod settings;
mod weather;

use std::collections::HashMap;

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};

use assistant::{ChatEvent, ChatMessage};
use error::Result;
use services::{AqiPoint, LatLon, OutlookDay, Place, RadarFrame};
use settings::Settings;
use weather::{Alert, Units, WeatherData};

pub struct AppState {
    pub http: reqwest::Client,
    pub settings: Settings,
    zones: tokio::sync::Mutex<HashMap<String, Option<Value>>>,
}

// ------------------------------------------------------------------ commands

/// Full forecast + air quality for a point. If NWS fails (e.g. outside the US)
/// it falls back to Open-Meteo, just like the web app.
#[tauri::command]
async fn get_weather(state: State<'_, AppState>, lat: f64, lon: f64, provider: String, units: String) -> Result<WeatherData> {
    let units = Units::parse(&units);
    let http = &state.http;
    let aq = services::air_quality(http, lat, lon);
    let primary = async {
        if provider == "nws" {
            weather::fetch_nws(http, lat, lon, units).await
        } else {
            weather::fetch_open_meteo(http, lat, lon, units).await
        }
    };
    let (result, aq) = tokio::join!(primary, aq);
    let mut data = match result {
        Ok(d) => d,
        Err(e) if provider == "nws" => {
            log::warn!("NWS failed, falling back to Open-Meteo: {e}");
            let mut d = weather::fetch_open_meteo(http, lat, lon, units).await?;
            d.nws_fallback = true;
            d.notice = Some("NWS is only available in the US. Showing Open-Meteo data instead.".into());
            d
        }
        Err(e) => return Err(e),
    };
    data.air_quality = aq.ok();
    Ok(data)
}

#[tauri::command]
async fn get_point_alerts(state: State<'_, AppState>, lat: f64, lon: f64) -> Result<Vec<Alert>> {
    weather::fetch_point_alerts(&state.http, lat, lon).await
}

#[tauri::command]
async fn search_location(state: State<'_, AppState>, query: String) -> Result<Vec<Place>> {
    services::search_location(&state.http, &query).await
}

#[tauri::command]
async fn reverse_geocode(state: State<'_, AppState>, lat: f64, lon: f64) -> Result<Place> {
    services::reverse_geocode(&state.http, lat, lon).await
}

#[tauri::command]
async fn ip_locate(state: State<'_, AppState>) -> Result<Place> {
    let mut place = services::ip_locate(&state.http).await?;
    // Prefer Nominatim's naming so it matches places picked from search.
    if let Ok(p) = services::reverse_geocode(&state.http, place.lat, place.lon).await {
        place.name = p.name;
        place.state = p.state;
        place.country = p.country;
    }
    Ok(place)
}

#[tauri::command]
async fn get_air_quality(state: State<'_, AppState>, lat: f64, lon: f64) -> Result<Value> {
    services::air_quality(&state.http, lat, lon).await
}

#[tauri::command]
async fn get_air_quality_batch(state: State<'_, AppState>, points: Vec<LatLon>) -> Result<Vec<AqiPoint>> {
    services::air_quality_batch(&state.http, points).await
}

#[tauri::command]
async fn get_future_range(
    state: State<'_, AppState>,
    lat: f64,
    lon: f64,
    start: String,
    end: String,
    units: String,
) -> Result<Vec<OutlookDay>> {
    services::future_range(&state.http, lat, lon, &start, &end, Units::parse(&units)).await
}

#[tauri::command]
async fn get_radar_frames(state: State<'_, AppState>) -> Result<Vec<RadarFrame>> {
    services::radar_frames(&state.http).await
}

#[tauri::command]
async fn get_active_alerts(state: State<'_, AppState>) -> Result<Value> {
    services::active_alerts_geojson(&state.http).await
}

#[tauri::command]
async fn get_zone_geometries(state: State<'_, AppState>, ids: Vec<String>) -> Result<HashMap<String, Value>> {
    services::zone_geometries(&state.http, &state.zones, ids).await
}

#[tauri::command]
fn load_settings(state: State<'_, AppState>) -> Value {
    state.settings.get()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Value) -> Result<()> {
    state.settings.save(settings)
}

#[tauri::command]
fn test_notification(app: AppHandle) {
    monitor::notify(&app, "StormView notifications are on", "You'll be alerted about NWS warnings and unhealthy air for your location.");
}

#[tauri::command]
async fn assistant_models(state: State<'_, AppState>, provider: String, base_url: String) -> Result<Vec<String>> {
    assistant::list_models(&state.http, &provider, &base_url).await
}

#[tauri::command]
async fn assistant_chat(
    state: State<'_, AppState>,
    provider: String,
    base_url: String,
    model: String,
    messages: Vec<ChatMessage>,
    on_event: Channel<ChatEvent>,
) -> Result<()> {
    assistant::chat(&state.http, &provider, &base_url, &model, messages, on_event).await
}

// --------------------------------------------------------------------- app

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AppState {
                http: http::client(),
                settings: Settings::load(dir),
                zones: tokio::sync::Mutex::new(HashMap::new()),
            });

            let open = MenuItem::with_id(app, "open", "Open StormView", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("StormView")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            tauri::async_runtime::spawn(monitor::run(app.handle().clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            // "Keep running in the background" makes closing the window hide it to
            // the tray, so alert notifications keep arriving — the desktop analogue
            // of the web app's push notifications.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.settings.bool("runInBackground", false) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_weather,
            get_point_alerts,
            search_location,
            reverse_geocode,
            ip_locate,
            get_air_quality,
            get_air_quality_batch,
            get_future_range,
            get_radar_frames,
            get_active_alerts,
            get_zone_geometries,
            load_settings,
            save_settings,
            test_notification,
            assistant_models,
            assistant_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running StormView");
}
