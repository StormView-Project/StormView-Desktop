// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assistant;
mod error;
mod http;
mod map;
mod present;
mod scene;
mod services;
mod settings;
mod shapes;
mod theme;
mod weather;

slint::include_modules!();

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use material_colors::color::Rgb;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, SharedString, Timer, TimerMode};

use present::{icon, model, time_str};
use services::{Place, RadarFrame};
use settings::{Settings, DETAIL_FIELDS, WIDGET_IDS};
use weather::{Alert, Units, WeatherData};

const WHATS_NEW_VERSION: &str = "native-2.0";

/* ------------------------------------------------------------ plumbing */

static RT: OnceLock<tokio::runtime::Handle> = OnceLock::new();
static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
/// Mirror of the settings for the background alert monitor.
static SHARED: OnceLock<Mutex<Settings>> = OnceLock::new();

fn http() -> reqwest::Client {
    HTTP.get().unwrap().clone()
}

/// Run `fut` on the tokio runtime, then `done` with its output on the UI thread.
fn bg<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    RT.get().unwrap().spawn(async move {
        let v = fut.await;
        let _ = slint::invoke_from_event_loop(move || done(v));
    });
}

struct State {
    app: slint::Weak<AppWindow>,
    s: Settings,
    weather: Option<WeatherData>,
    alerts: Vec<Alert>,
    all_frames: Vec<RadarFrame>,
    frames: Vec<RadarFrame>,
    idx: usize,
    playing: bool,
    speed: f32,
    play_timer: Timer,
    primary: Rgb,
    dark: bool,
    ctx: (f64, f64),
    results: Vec<Place>,
    search_timer: Timer,
    aqi_timer: Timer,
    aqi_cache: HashMap<(i64, i64), f64>,
    aqi_seq: u64,
    alert_version: u64,
    chat: Vec<assistant::ChatMessage>,
    toast_timer: Timer,
    weather_seq: u64,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn st<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

fn win() -> Option<AppWindow> {
    st(|s| s.app.upgrade()).flatten()
}

fn toast(msg: impl Into<SharedString>) {
    let Some(a) = win() else { return };
    let ui = a.global::<Ui>();
    ui.set_toast(msg.into());
    ui.set_toast_visible(true);
    st(|s| {
        s.toast_timer.start(TimerMode::SingleShot, Duration::from_millis(3500), || {
            if let Some(a) = win() {
                a.global::<Ui>().set_toast_visible(false);
            }
        })
    });
}

fn save() {
    st(|s| {
        s.s.save();
        if let Some(m) = SHARED.get() {
            *m.lock().unwrap() = s.s.clone();
        }
    });
}

/* ------------------------------------------------------------ theme */

macro_rules! scheme_to_slint {
    ($s:expr) => {{
        let s = &$s;
        let c = theme::rgb;
        MaterialScheme {
            primary: c(s.primary), surfaceTint: c(s.surface_tint), onPrimary: c(s.on_primary),
            primaryContainer: c(s.primary_container), onPrimaryContainer: c(s.on_primary_container),
            secondary: c(s.secondary), onSecondary: c(s.on_secondary), secondaryContainer: c(s.secondary_container),
            onSecondaryContainer: c(s.on_secondary_container), tertiary: c(s.tertiary), onTertiary: c(s.on_tertiary),
            tertiaryContainer: c(s.tertiary_container), onTertiaryContainer: c(s.on_tertiary_container),
            error: c(s.error), onError: c(s.on_error), errorContainer: c(s.error_container),
            onErrorContainer: c(s.on_error_container), background: c(s.background), onBackground: c(s.on_background),
            surface: c(s.surface), onSurface: c(s.on_surface), surfaceVariant: c(s.surface_variant),
            onSurfaceVariant: c(s.on_surface_variant), outline: c(s.outline), outlineVariant: c(s.outline_variant),
            shadow: c(s.shadow), scrim: c(s.scrim), inverseSurface: c(s.inverse_surface),
            inverseOnSurface: c(s.inverse_on_surface), inversePrimary: c(s.inverse_primary),
            primaryFixed: c(s.primary_fixed), onPrimaryFixed: c(s.on_primary_fixed),
            primaryFixedDim: c(s.primary_fixed_dim), onPrimaryFixedVariant: c(s.on_primary_fixed_variant),
            secondaryFixed: c(s.secondary_fixed), onSecondaryFixed: c(s.on_secondary_fixed),
            secondaryFixedDim: c(s.secondary_fixed_dim), onSecondaryFixedVariant: c(s.on_secondary_fixed_variant),
            tertiaryFixed: c(s.tertiary_fixed), onTertiaryFixed: c(s.on_tertiary_fixed),
            tertiaryFixedDim: c(s.tertiary_fixed_dim), onTertiaryFixedVariant: c(s.on_tertiary_fixed_variant),
            surfaceDim: c(s.surface_dim), surfaceBright: c(s.surface_bright),
            surfaceContainerLowest: c(s.surface_container_lowest), surfaceContainerLow: c(s.surface_container_low),
            surfaceContainer: c(s.surface_container), surfaceContainerHigh: c(s.surface_container_high),
            surfaceContainerHighest: c(s.surface_container_highest),
        }
    }};
}

fn active_seed(s: &State) -> u32 {
    match s.s.color_source.as_str() {
        "custom" => theme::parse_hex(&s.s.custom_seed).unwrap_or(0x4f8ef5),
        "adaptive" => s
            .weather
            .as_ref()
            .map(|w| theme::adaptive_seed(w.current.condition_code, w.current.is_day))
            .unwrap_or(0x4f8ef5),
        id => theme::PRESETS.iter().find(|p| p.0 == id).map(|p| p.2).unwrap_or(0x4f8ef5),
    }
}

fn apply_theme() {
    let Some(a) = win() else { return };
    let (seed, variant, contrast, mode) = st(|s| (active_seed(s), s.s.scheme_variant.clone(), theme::contrast_level(&s.s.contrast), s.s.theme_mode.clone())).unwrap();
    let light = theme::build_scheme(seed, &variant, false, contrast);
    let darks = theme::build_scheme(seed, &variant, true, contrast);
    a.global::<MaterialPalette>().set_schemes(MaterialSchemes { light: scheme_to_slint!(light), dark: scheme_to_slint!(darks) });
    let dark = match mode.as_str() {
        "dark" => true,
        "light" => false,
        _ => matches!(dark_light::detect(), Ok(dark_light::Mode::Dark)),
    };
    a.set_dark(dark);
    let scheme = if dark { &darks } else { &light };
    st(|s| {
        s.primary = scheme.primary;
        s.dark = dark;
    });
    a.global::<Ui>().set_dark(dark);
    let sem = |hex| present::tint(theme::semantic(hex, scheme.primary, dark));
    let g = a.global::<Semantic>();
    g.set_sun(sem(0xf2b705));
    g.set_sunset(sem(0xf07a2c));
    g.set_rain(sem(0x4f8ef5));
    map::with(|e| e.set_palette(map::MapPalette::from_scheme(scheme, dark)));
    sync_prefs();
    represent();
    rebuild_alert_shapes();
}

/* ------------------------------------------------------------ prefs → UI */

fn sync_prefs() {
    let Some(a) = win() else { return };
    let p = a.global::<Prefs>();
    st(|st| {
        let s = &st.s;
        p.set_theme_mode(s.theme_mode.clone().into());
        p.set_color_source(s.color_source.clone().into());
        p.set_custom_seed(s.custom_seed.clone().into());
        p.set_variant(s.scheme_variant.clone().into());
        p.set_variant_desc(theme::VARIANTS.iter().find(|v| v.0 == s.scheme_variant).map(|v| v.2).unwrap_or("").into());
        p.set_contrast(s.contrast.clone().into());
        p.set_map_style(s.map_style.clone().into());
        p.set_sidebar(s.sidebar_position.clone().into());
        p.set_compact(s.compact_layout);
        p.set_weather_scene(s.weather_scene);
        p.set_weather_api(s.weather_api.clone().into());
        p.set_units(s.units.clone().into());
        p.set_map_layer(s.map_layer.clone().into());
        p.set_tile_quality(s.radar_tile_quality as i32);
        p.set_opacity(s.radar_opacity);
        p.set_color_scheme(s.radar_color_scheme as i32);
        p.set_polygons(s.show_alert_polygons);
        p.set_advisories(s.show_advisories);
        p.set_radar_toggles(model(vec![
            Toggle { id: "showNowcast".into(), label: "Show 90-min nowcast".into(), on: s.show_nowcast },
            Toggle { id: "radarFrameFade".into(), label: "Fade between frames".into(), on: s.radar_frame_fade },
            Toggle { id: "radarSmooth".into(), label: "Smoothing".into(), on: s.radar_smooth },
            Toggle { id: "radarSnow".into(), label: "Snow colors".into(), on: s.radar_snow },
            Toggle { id: "showSatellite".into(), label: "Satellite imagery".into(), on: s.show_satellite },
            Toggle { id: "showArrows".into(), label: "Storm motion arrows".into(), on: s.show_arrows },
        ]));
        p.set_widgets(model(
            s.widgets
                .iter()
                .map(|w| WidgetRow {
                    id: w.id.clone().into(),
                    label: WIDGET_IDS.iter().find(|x| x.0 == w.id).map(|x| x.1).unwrap_or("").into(),
                    enabled: w.enabled,
                })
                .collect(),
        ));
        p.set_details_enabled(s.widget_enabled("details"));
        p.set_aqi_enabled(s.widget_enabled("aqi"));
        p.set_aqi_pollutants(s.aqi_show_pollutants);
        p.set_detail_fields(model(DETAIL_FIELDS.iter().map(|(k, l)| Toggle { id: (*k).into(), label: (*l).into(), on: s.detail_visible(k) }).collect()));
        p.set_notifications(s.notifications_enabled);
        p.set_notif_nws(s.notif_nws);
        p.set_notif_aqi(s.notif_aqi);
        p.set_ai_provider(s.assistant_provider.clone().into());
        p.set_ai_url(s.assistant_base_url.clone().into());
        p.set_ai_model(s.assistant_model.clone().into());
        let idx = p.get_ai_models().iter().position(|m| m.text == s.assistant_model.as_str()).map(|i| i as i32).unwrap_or(-1);
        p.set_ai_model_index(idx);
        let dark = st.dark;
        let mut sw: Vec<Swatch> = theme::PRESETS
            .iter()
            .map(|(id, label, seed)| {
                let [pr, se, te, su] = theme::swatch(*seed, dark);
                Swatch { id: (*id).into(), label: (*label).into(), primary: pr, secondary: se, tertiary: te, surface: su, icon: theme::swatch_image(*seed, dark, 80), glyph: Default::default(), has_preview: true }
            })
            .collect();
        sw.push(Swatch { id: "adaptive".into(), label: "Weather".into(), glyph: icon("partly_cloudy_day"), has_preview: false, ..Default::default() });
        let custom = theme::parse_hex(&s.custom_seed).unwrap_or(0x4f8ef5);
        let [pr, se, te, su] = theme::swatch(custom, dark);
        let custom_on = s.color_source == "custom";
        let custom_icon = if custom_on { theme::swatch_image(custom, dark, 80) } else { slint::Image::default() };
        sw.push(Swatch { id: "custom".into(), label: "Custom".into(), primary: pr, secondary: se, tertiary: te, surface: su, icon: custom_icon, glyph: icon("colorize"), has_preview: custom_on });
        p.set_swatches(model(sw));
        p.set_variants(model(theme::VARIANTS.iter().map(|v| Choice { id: v.0.into(), label: v.1.into() }).collect()));
        if let Some(l) = &s.location {
            let mut title = l.name.clone();
            if !l.state.is_empty() {
                title.push_str(&format!(", {}", l.state));
            }
            if !l.country.is_empty() && l.country != "US" {
                title.push_str(&format!(" · {}", l.country));
            }
            p.set_location_title(title.into());
            p.set_location_coords(format!("{:.4}, {:.4}", l.lat, l.lon).into());
        }
        a.global::<Ui>().set_press_style(s.press_style.clone().into());
        a.global::<Ui>().set_custom_frame(s.custom_frame);
        p.set_custom_frame(s.custom_frame);
        let r = a.global::<MapBridge>();
        r.set_radar_opacity(s.radar_opacity);
        r.set_radar_fade(s.radar_frame_fade);
        a.global::<Weather>().set_has_location(s.location.is_some());
        if let Some(l) = &s.location {
            a.global::<Weather>().set_location_name(l.name.clone().into());
            a.global::<Weather>().set_location_state(l.state.clone().into());
        }
    });
}

fn radar_schemes() -> Vec<RadarScheme> {
    let hex = |v: &[u32]| model(v.iter().map(|c| theme::color_u32(*c)).collect::<Vec<_>>());
    let list: [(&str, &[u32]); 12] = [
        ("B&W", &[0x000000, 0x888888, 0xffffff]),
        ("RainViewer", &[0x0000ff, 0x00ff00, 0xffff00, 0xff8000, 0xff0000]),
        ("Universal Blue", &[0x1a1aff, 0x0066ff, 0x00ccff, 0x66ffff, 0xffffff]),
        ("TITAN", &[0x0000ff, 0x00ffff, 0x00ff00, 0xffff00, 0xff0000, 0xff00ff]),
        ("Weather Channel", &[0x006600, 0x00cc00, 0xffff00, 0xff6600, 0xff0000, 0xcc00cc]),
        ("Meteored", &[0x004488, 0x0088cc, 0x00cc88, 0x88cc00, 0xcc8800, 0xcc0000]),
        ("NEXRAD III", &[0x004400, 0x008800, 0x00cc00, 0xffff00, 0xff8800, 0xff0000, 0xcc00cc]),
        ("Rainbow", &[0x0000ff, 0x00ffff, 0x00ff00, 0xffff00, 0xff8000, 0xff0000]),
        ("Dark Sky", &[0x0d1117, 0x1f3a5c, 0x2d6a9f, 0x63b3ed, 0xffffff]),
        ("Datameteo", &[0x320064, 0x6400c8, 0x9600ff, 0xc864ff, 0xff96ff]),
        ("Viper HD", &[0x00004c, 0x0000ff, 0x00ffff, 0xffff00, 0xff0000]),
        ("MRMS CREF", &[0x003366, 0x0066cc, 0x00cc66, 0x66ff00, 0xffcc00, 0xff6600, 0xff0000]),
    ];
    list.iter().enumerate().map(|(i, (n, c))| RadarScheme { id: i as i32, name: (*n).into(), stops: hex(c) }).collect()
}

/* ------------------------------------------------------------ weather */

fn represent() {
    let Some(a) = win() else { return };
    st(|s| {
        if let Some(w) = &s.weather {
            present::apply(&a, w, &s.alerts, &s.s, s.primary, s.dark);
        }
    });
}

fn refresh_weather() {
    let Some((loc, api, units, seq)) = st(|s| {
        s.weather_seq += 1;
        (s.s.location.clone(), s.s.weather_api.clone(), s.s.units.clone(), s.weather_seq)
    }) else { return };
    let Some(loc) = loc else { return };
    if let Some(a) = win() {
        a.global::<Weather>().set_loading(true);
        a.global::<Weather>().set_error("".into());
    }
    bg(
        async move {
            let c = http();
            let u = Units::parse(&units);
            let (primary, aq) = tokio::join!(
                async {
                    if api == "nws" {
                        match weather::fetch_nws(&c, loc.lat, loc.lon, u).await {
                            Ok(d) => Ok(d),
                            Err(_) => weather::fetch_open_meteo(&c, loc.lat, loc.lon, u).await.map(|mut d| {
                                d.nws_fallback = true;
                                d.notice = Some("NWS is only available in the US. Showing Open-Meteo data instead.".into());
                                d
                            }),
                        }
                    } else {
                        weather::fetch_open_meteo(&c, loc.lat, loc.lon, u).await
                    }
                },
                services::air_quality(&c, loc.lat, loc.lon)
            );
            primary.map(|mut d| {
                d.air_quality = aq.ok();
                d
            })
        },
        move |res| {
            if st(|s| s.weather_seq != seq).unwrap_or(true) {
                return;
            }
            let Some(a) = win() else { return };
            a.global::<Weather>().set_loading(false);
            match res {
                Ok(d) => {
                    let adaptive = st(|s| {
                        s.weather = Some(d);
                        s.s.color_source == "adaptive"
                    })
                    .unwrap_or(false);
                    if adaptive { apply_theme() } else { represent() }
                }
                Err(e) => a.global::<Weather>().set_error(e.to_string().into()),
            }
        },
    );
}

fn refresh_alerts() {
    let Some(Some(loc)) = st(|s| s.s.location.clone()) else { return };
    bg(
        async move { weather::fetch_point_alerts(&http(), loc.lat, loc.lon).await.unwrap_or_default() },
        |alerts| {
            st(|s| s.alerts = alerts);
            represent();
        },
    );
}

fn set_location(p: Place) {
    st(|s| {
        s.s.location = Some(p.clone());
        s.weather = None;
        s.alerts.clear();
    });
    save();
    sync_prefs();
    if let Some(a) = win() {
        a.global::<Weather>().set_has_data(false);
        a.global::<Ui>().set_welcome_open(false);
    }
    map::with(|e| {
        e.location = Some((map::lon_to_x(p.lon), map::lat_to_y(p.lat)));
        let z = e.zoom.max(7.5);
        e.fly_to(p.lat, p.lon, Some(z));
    });
    refresh_weather();
    refresh_alerts();
}

/* ------------------------------------------------------------ radar */

fn push_radar_ui() {
    let Some(a) = win() else { return };
    let r = a.global::<Radar>();
    st(|s| {
        let n = s.frames.len();
        r.set_count(n as i32);
        r.set_index(s.idx as i32);
        r.set_playing(s.playing);
        r.set_speed(s.speed);
        let past = s.frames.iter().filter(|f| f.kind == "past").count();
        r.set_past_count(past as i32);
        r.set_nowcast_count((n - past) as i32);
        r.set_nowcast_start(if past > 0 && past < n && n > 1 { (past as f32 - 0.5) / (n - 1) as f32 } else { -1.0 });
        r.set_ticks(model(
            s.frames
                .iter()
                .enumerate()
                .map(|(i, f)| FrameTick { pct: if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 }, nowcast: f.kind == "nowcast" })
                .collect(),
        ));
        if let Some(f) = s.frames.get(s.idx) {
            let t = present::local(f.time * 1000);
            r.set_time(t.format("%-I:%M %p").to_string().into());
            r.set_date(t.format("%a, %b %-d").to_string().into());
            r.set_forecast(f.kind == "nowcast");
        }
    });
}

fn latest_past(frames: &[RadarFrame]) -> usize {
    frames.iter().rposition(|f| f.kind == "past").unwrap_or(frames.len().saturating_sub(1))
}

fn apply_frames() {
    st(|s| {
        s.frames = if s.s.show_nowcast { s.all_frames.clone() } else { s.all_frames.iter().filter(|f| f.kind == "past").cloned().collect() };
        s.idx = latest_past(&s.frames);
        let (f, i) = (s.frames.clone(), s.idx);
        map::with(|e| e.set_frames(f, i));
    });
    push_radar_ui();
}

fn refresh_radar() {
    if let Some(a) = win() {
        a.global::<Radar>().set_loading(true);
    }
    bg(async { services::radar_frames(&http()).await }, |res| {
        if let Some(a) = win() {
            a.global::<Radar>().set_loading(false);
        }
        match res {
            Ok(frames) => {
                st(|s| s.all_frames = frames);
                apply_frames();
            }
            Err(e) => eprintln!("[radar] {e}"),
        }
    });
}

fn set_frame(i: usize) {
    st(|s| s.idx = i);
    map::with(|e| e.set_frame(i));
    push_radar_ui();
}

fn restart_playback() {
    st(|s| {
        if s.playing && !s.frames.is_empty() {
            let ms = match s.speed {
                x if x < 0.75 => 600,
                x if x > 1.5 => 200,
                _ => 400,
            };
            s.play_timer.start(TimerMode::Repeated, Duration::from_millis(ms), || {
                let next = st(|s| (s.idx + 1) % s.frames.len().max(1)).unwrap_or(0);
                set_frame(next);
            });
        } else {
            s.play_timer.stop();
        }
    });
    push_radar_ui();
}

fn apply_radar_params() {
    st(|s| {
        let arrows = s.s.show_arrows.then_some(if s.dark { "light" } else { "dark" });
        let p = map::RadarParams {
            scheme: s.s.radar_color_scheme,
            quality: s.s.radar_tile_quality,
            smooth: s.s.radar_smooth,
            snow: s.s.radar_snow,
            arrows,
        };
        let radar_on = s.s.map_layer == "radar";
        let sat = s.s.show_satellite;
        map::with(|e| {
            e.radar = p;
            e.radar_on = radar_on;
            e.satellite = sat;
            e.invalidate();
        });
    });
}

/* ------------------------------------------------------------ alert polygons */

const EVENT_STYLE: [(&str, u32, f32); 41] = [
    ("Tornado Warning", 0xFF0000, 0.38), ("Tornado Watch", 0xFFFF00, 0.28), ("Tornado Emergency", 0xFF00FF, 0.45),
    ("Severe Thunderstorm Warning", 0xFFA500, 0.35), ("Severe Thunderstorm Watch", 0xDDBB00, 0.25),
    ("Flash Flood Warning", 0x00CC00, 0.35), ("Flash Flood Watch", 0x2E8B57, 0.25), ("Flash Flood Emergency", 0x8B0000, 0.42),
    ("Flood Warning", 0x00AA00, 0.30), ("Flood Watch", 0x2E8B57, 0.22), ("Flood Advisory", 0x00FF7F, 0.20),
    ("Winter Storm Warning", 0x9400D3, 0.35), ("Winter Storm Watch", 0x4682B4, 0.25), ("Blizzard Warning", 0xFF4500, 0.38),
    ("Ice Storm Warning", 0x8B0000, 0.35), ("Winter Weather Advisory", 0x7B68EE, 0.22), ("Wind Advisory", 0xD2691E, 0.22),
    ("High Wind Warning", 0xDAA520, 0.30), ("Dense Fog Advisory", 0x708090, 0.22), ("Heat Advisory", 0xFF7F50, 0.22),
    ("Excessive Heat Warning", 0xC71585, 0.35), ("Extreme Heat Warning", 0xC71585, 0.35), ("Extreme Cold Warning", 0x0000FF, 0.35),
    ("Special Weather Statement", 0xFFE4B5, 0.22), ("Air Quality Alert", 0x8B8B00, 0.20), ("Beach Hazard Statement", 0x40E0D0, 0.20),
    ("Rip Current Statement", 0x008B8B, 0.22), ("Small Craft Advisory", 0xD2B48C, 0.20), ("Frost Advisory", 0x6495ED, 0.20),
    ("Freeze Warning", 0x483D8B, 0.28), ("Hard Freeze Warning", 0x9400D3, 0.30), ("Freeze Watch", 0x00CED1, 0.20),
    ("Lake Wind Advisory", 0xD2691E, 0.18), ("Dust Advisory", 0xBDB76B, 0.22), ("Dust Storm Warning", 0xFFE4C4, 0.30),
    ("Smoke Advisory", 0x808080, 0.20), ("Red Flag Warning", 0xFF1493, 0.30), ("Fire Weather Watch", 0xFFDEAD, 0.22),
    ("Severe Weather Statement", 0xFFA500, 0.25), ("Coastal Flood Warning", 0x228B22, 0.30), ("Coastal Flood Advisory", 0x7CFC00, 0.20),
];

fn feature_style(p: &Value) -> (u32, f32) {
    let ev = p["event"].as_str().unwrap_or("");
    if let Some((_, c, a)) = EVENT_STYLE.iter().find(|x| x.0 == ev) {
        return (*c, *a);
    }
    match p["severity"].as_str().unwrap_or("") {
        "Extreme" => (0xFF0000, 0.35),
        "Severe" => (0xFF6600, 0.30),
        "Moderate" => (0xFFAA00, 0.25),
        "Minor" => (0x4169E1, 0.20),
        _ => (0x888888, 0.18),
    }
}

thread_local! {
    static ALERT_GEO: RefCell<(Vec<Value>, Vec<(Value, Value)>)> = const { RefCell::new((vec![], vec![])) };
}

/// Rebuild the rasterizable polygons (colors depend on the theme via harmonization).
fn rebuild_alert_shapes() {
    let Some((on, primary, version)) = st(|s| {
        s.alert_version += 1;
        (s.s.show_alert_polygons, s.primary, s.alert_version)
    }) else { return };
    if !on {
        map::with(|e| e.set_alerts(None));
        return;
    }
    let shapes = ALERT_GEO.with(|g| {
        let g = g.borrow();
        let mut shapes = vec![];
        // Advisories (zone outlines, dashed) underneath; warnings on top, most severe last.
        for (geom, props) in &g.1 {
            let (c, a) = feature_style(props);
            shapes.extend(map::build_shape(geom, props.clone(), theme::harmonize_u32(c, primary), a, true));
        }
        let rank = |p: &Value| ["Extreme", "Severe", "Moderate", "Minor"].iter().position(|x| Some(*x) == p["severity"].as_str()).unwrap_or(4);
        let mut feats: Vec<&Value> = g.0.iter().collect();
        feats.sort_by_key(|f| std::cmp::Reverse(rank(&f["properties"])));
        for f in feats {
            let (c, a) = feature_style(&f["properties"]);
            shapes.extend(map::build_shape(&f["geometry"], f["properties"].clone(), theme::harmonize_u32(c, primary), a, false));
        }
        shapes
    });
    map::with(|e| e.set_alerts(Some(Arc::new(map::AlertData { shapes, version }))));
}

fn load_alert_polygons() {
    let Some((on, adv)) = st(|s| (s.s.show_alert_polygons, s.s.show_advisories)) else { return };
    if !on {
        rebuild_alert_shapes();
        return;
    }
    bg(
        async move {
            let c = http();
            let all = services::active_alerts_geojson(&c).await.ok()?;
            let feats = all["features"].as_array().cloned().unwrap_or_default();
            let has_poly = |f: &Value| matches!(f["geometry"]["type"].as_str(), Some("Polygon") | Some("MultiPolygon"));
            let polys: Vec<Value> = feats.iter().filter(|f| has_poly(f)).cloned().collect();
            let mut zones = vec![];
            if adv {
                let mut zone_to: HashMap<String, Value> = HashMap::new();
                for f in feats.iter().filter(|f| !has_poly(f)) {
                    for z in f["properties"]["geocode"]["UGC"].as_array().cloned().unwrap_or_default() {
                        if let Some(z) = z.as_str() {
                            zone_to.entry(z.to_string()).or_insert_with(|| f["properties"].clone());
                        }
                    }
                }
                let cache = ZONE_CACHE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
                let geos = services::zone_geometries(&c, cache, zone_to.keys().cloned().collect()).await.unwrap_or_default();
                zones = geos.into_iter().filter_map(|(z, g)| zone_to.get(&z).map(|p| (g, p.clone()))).collect();
            }
            Some((polys, zones))
        },
        |res| {
            if let Some(g) = res {
                ALERT_GEO.with(|a| *a.borrow_mut() = g);
            }
            rebuild_alert_shapes();
        },
    );
}

static ZONE_CACHE: OnceLock<tokio::sync::Mutex<HashMap<String, Option<Value>>>> = OnceLock::new();

/* ------------------------------------------------------------ AQI overlay */

fn schedule_aqi() {
    st(|s| {
        s.aqi_seq += 1;
        if s.s.map_layer != "aqi" {
            s.aqi_timer.stop();
            map::with(|e| e.set_aqi(None));
            return;
        }
        s.aqi_timer.start(TimerMode::SingleShot, Duration::from_millis(900), refresh_aqi);
    });
}

fn draw_aqi() {
    let Some((w, n, e, s)) = map::with(|e| e.bounds()) else { return };
    let pts: Vec<(f64, f64, f64)> = st(|st| {
        st.aqi_cache
            .iter()
            .map(|((la, lo), v)| (*la as f64 / 100.0, *lo as f64 / 100.0, *v))
            .filter(|(la, lo, _)| *la > s - 6.0 && *la < n + 6.0 && *lo > w - 6.0 && *lo < e + 6.0)
            .collect()
    })
    .unwrap_or_default();
    if pts.len() < 2 {
        return;
    }
    let rgba = map::render_aqi(&pts, w, n, e, s);
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, 160, 160);
    let img = slint::Image::from_rgba8(buf);
    let (x0, y0, x1, y1) = (map::lon_to_x(w), map::lat_to_y(n), map::lon_to_x(e), map::lat_to_y(s));
    map::with(|en| en.set_aqi(Some((img, x0, y0, x1, y1))));
}

fn refresh_aqi() {
    let Some((w, n, e, s)) = map::with(|e| e.bounds()) else { return };
    let span = (n - s).max(e - w);
    let steps = [0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 8.0];
    let step: f64 = steps.iter().cloned().find(|x| *x >= span / 25.0).unwrap_or(8.0).min(2.0);
    let snap = |v: f64| (v / step).round() * step;
    let (cx, cy) = ((w + e) / 2.0, (n + s) / 2.0);
    let mut pts = vec![];
    let mut lat = snap(s - step);
    while lat <= n + step {
        let mut lon = snap(w - step);
        while lon <= e + step {
            let key = ((lat * 100.0).round() as i64, (lon * 100.0).round() as i64);
            if lat.abs() < 85.0 && st(|st| !st.aqi_cache.contains_key(&key)).unwrap_or(false) {
                pts.push(services::LatLon { lat, lon });
            }
            lon += step;
        }
        lat += step;
    }
    draw_aqi();
    if pts.is_empty() {
        return;
    }
    pts.sort_by(|a, b| ((a.lat - cy).powi(2) + (a.lon - cx).powi(2)).total_cmp(&((b.lat - cy).powi(2) + (b.lon - cx).powi(2))));
    pts.truncate(200);
    let seq = st(|s| s.aqi_seq).unwrap_or(0);
    bg(async move { services::air_quality_batch(&http(), pts).await.unwrap_or_default() }, move |res| {
        st(|s| {
            for p in res {
                if let Some(v) = p.aqi {
                    s.aqi_cache.insert(((p.lat * 100.0).round() as i64, (p.lon * 100.0).round() as i64), v);
                }
            }
        });
        if st(|s| s.aqi_seq == seq && s.s.map_layer == "aqi").unwrap_or(false) {
            draw_aqi();
        }
    });
}

/* ------------------------------------------------------------ dialogs */

fn show_alert_dialog(title: &str, alerts: Vec<Alert>) {
    let Some(a) = win() else { return };
    let (p, d) = st(|s| (s.primary, s.dark)).unwrap();
    a.global::<Weather>().set_dialog_title(title.into());
    a.global::<Weather>().set_dialog_alerts(model(present::alert_items(&alerts, p, d)));
    a.global::<Ui>().set_alert_dialog_open(true);
}

fn map_clicked(x: f64, y: f64) {
    let hits = map::with(|e| e.alerts_at(x, y)).unwrap_or_default();
    if !hits.is_empty() {
        let mut seen = HashSet::new();
        let alerts: Vec<Alert> = hits
            .into_iter()
            .filter(|p| seen.insert(p["id"].as_str().unwrap_or("").to_string()))
            .map(|p| weather::normalize_alert(&json!({ "properties": p })))
            .collect();
        let title = if alerts.len() > 1 { format!("Active alerts ({})", alerts.len()) } else { alerts[0].title.clone() };
        show_alert_dialog(&title, alerts);
        return;
    }
    if st(|s| s.s.map_layer == "aqi").unwrap_or(false) {
        let Some((lon, lat)) = map::with(|e| e.lonlat_at(x, y)) else { return };
        bg(async move { services::air_quality(&http(), lat, lon).await }, move |res| match res {
            Ok(aq) => {
                let Some(v) = aq["us_aqi"].as_f64() else {
                    toast("No AQI data available for this location.");
                    return;
                };
                let (_, _, label, desc) = present::aqi_category(v);
                let f = |k: &str| aq[k].as_f64().map(|x| format!("{x:.1} µg/m³")).unwrap_or("—".into());
                let body = format!(
                    "{desc}\n\nPM2.5  {}\nPM10  {}\nOzone  {}\nNO₂  {}\nSO₂  {}\nCO  {}\n\nUS AQI · Open-Meteo Air Quality",
                    f("pm2_5"), f("pm10"), f("ozone"), f("nitrogen_dioxide"), f("sulphur_dioxide"), f("carbon_monoxide")
                );
                let severity = if v > 200.0 { "Extreme" } else if v > 150.0 { "Severe" } else if v > 100.0 { "Moderate" } else { "Minor" };
                show_alert_dialog(
                    &format!("Air quality · {lat:.2}, {lon:.2}"),
                    vec![Alert {
                        id: String::new(), title: format!("US AQI {v} — {label}"), headline: String::new(), description: body,
                        instruction: String::new(), severity: severity.into(), certainty: String::new(), urgency: String::new(),
                        start: None, end: None,
                    }],
                );
            }
            Err(e) => toast(format!("Failed to load air quality: {e}")),
        });
    }
}

/* ------------------------------------------------------------ settings writes */

fn set_str(key: &str, v: &str) {
    let v = v.to_string();
    let mut theme_changed = false;
    let mut weather_changed = false;
    let mut radar_changed = false;
    let mut map_layer = false;
    st(|st| {
        let s = &mut st.s;
        match key {
            "themeMode" => { s.theme_mode = v; theme_changed = true; }
            "colorSource" => { s.color_source = v; theme_changed = true; }
            "customSeed" => {
                if theme::parse_hex(&v).is_some() {
                    s.custom_seed = if v.starts_with('#') { v } else { format!("#{v}") };
                    s.color_source = "custom".into();
                    theme_changed = true;
                }
            }
            "variant" => { s.scheme_variant = v; theme_changed = true; }
            "contrast" => { s.contrast = v; theme_changed = true; }
            "mapStyle" => { s.map_style = v; theme_changed = true; }
            "pressStyle" => s.press_style = v,
            "sidebar" => s.sidebar_position = v,
            "weatherApi" => { s.weather_api = v; weather_changed = true; }
            "units" => { s.units = v; weather_changed = true; }
            "mapLayer" => { s.map_layer = v; radar_changed = true; map_layer = true; }
            "aiProvider" => {
                s.assistant_base_url = if v == "ollama" { "http://localhost:11434".into() } else { "http://localhost:1234".into() };
                s.assistant_provider = v;
                s.assistant_model.clear();
            }
            "aiUrl" => s.assistant_base_url = v,
            "aiModel" => s.assistant_model = v,
            _ => {}
        }
    });
    save();
    sync_prefs();
    if theme_changed {
        apply_theme();
    }
    if weather_changed {
        refresh_weather();
    }
    if radar_changed {
        apply_radar_params();
    }
    if map_layer {
        schedule_aqi();
    }
    if key == "aiProvider" || key == "aiUrl" {
        refresh_models();
    }
}

fn set_bool(key: &str, v: bool) {
    let mut radar = false;
    let mut alerts = false;
    let mut weather = false;
    let mut frames = false;
    st(|st| {
        let s = &mut st.s;
        match key {
            "compact" => s.compact_layout = v,
            "weatherScene" => s.weather_scene = v,
            "customFrame" => s.custom_frame = v,
            "showNowcast" => { s.show_nowcast = v; frames = true; }
            "radarFrameFade" => s.radar_frame_fade = v,
            "radarSmooth" => { s.radar_smooth = v; radar = true; }
            "radarSnow" => { s.radar_snow = v; radar = true; }
            "showSatellite" => { s.show_satellite = v; radar = true; }
            "showArrows" => { s.show_arrows = v; radar = true; }
            "polygons" => { s.show_alert_polygons = v; alerts = true; }
            "advisories" => { s.show_advisories = v; alerts = true; }
            "aqiPollutants" => { s.aqi_show_pollutants = v; weather = true; }
            "notifications" => s.notifications_enabled = v,
            "notifNws" => s.notif_nws = v,
            "notifAqi" => s.notif_aqi = v,
            k if k.starts_with("widget:") => {
                let id = &k[7..];
                if let Some(w) = s.widgets.iter_mut().find(|w| w.id == id) {
                    w.enabled = v;
                }
                weather = true;
            }
            k if k.starts_with("detail:") => {
                let id = k[7..].to_string();
                s.details_fields.retain(|x| *x != id);
                if !v {
                    s.details_fields.push(id);
                }
                weather = true;
            }
            _ => {}
        }
    });
    save();
    sync_prefs();
    if radar {
        apply_radar_params();
    }
    if frames {
        apply_frames();
    }
    if alerts {
        load_alert_polygons();
    }
    if weather {
        represent();
    }
    if key == "notifications" && v {
        notify("StormView notifications are on", "You'll be alerted about NWS warnings and unhealthy air for your location.");
    }
}

fn set_num(key: &str, v: f32) {
    st(|st| match key {
        "opacity" => st.s.radar_opacity = v.clamp(0.1, 1.0),
        "quality" => st.s.radar_tile_quality = if v < 384.0 { 256 } else { 512 },
        "scheme" => st.s.radar_color_scheme = v as u32,
        _ => {}
    });
    save();
    sync_prefs();
    if key != "opacity" {
        apply_radar_params();
    }
}

fn refresh_models() {
    let Some((provider, url)) = st(|s| (s.s.assistant_provider.clone(), s.s.assistant_base_url.clone())) else { return };
    if let Some(a) = win() {
        a.global::<Prefs>().set_ai_status("Looking for models…".into());
    }
    bg(async move { assistant::list_models(&http(), &provider, &url).await }, |res| {
        let Some(a) = win() else { return };
        let p = a.global::<Prefs>();
        match res {
            Ok(models) => {
                p.set_ai_status(if models.is_empty() { "Server reachable, but it has no models installed." } else { "" }.into());
                let first = models.first().cloned();
                let current = st(|s| s.s.assistant_model.clone()).unwrap_or_default();
                p.set_ai_model_index(models.iter().position(|m| *m == current).map(|i| i as i32).unwrap_or(-1));
                p.set_ai_models(model(models.into_iter().map(|m| MenuItem { text: m.into(), enabled: true, ..Default::default() }).collect()));
                let empty = st(|s| s.s.assistant_model.is_empty()).unwrap_or(false);
                if let (true, Some(m)) = (empty, first) {
                    set_str("aiModel", &m);
                }
                update_chat_label();
            }
            Err(e) => {
                p.set_ai_models(model(vec![]));
                p.set_ai_status(format!("{e}").into());
            }
        }
    });
}

fn update_chat_label() {
    let Some(a) = win() else { return };
    let m = st(|s| s.s.assistant_model.clone()).unwrap_or_default();
    a.global::<Ui>().set_chat_ready(!m.is_empty());
    a.global::<Ui>().set_chat_model_label(if m.is_empty() { "Not configured".into() } else { m.into() });
}

/* ------------------------------------------------------------ assistant */

fn weather_context(s: &State) -> String {
    let Some(w) = &s.weather else { return "No weather data is currently loaded for the user.".into() };
    let tu = if s.s.metric() { "°C" } else { "°F" };
    let su = if s.s.metric() { "km/h" } else { "mph" };
    let n = |v: Option<f64>| v.map(|x| format!("{}", x.round())).unwrap_or("—".into());
    let cu = &w.current;
    let mut l = vec![format!("Today's date: {}", Local::now().format("%A, %B %-d"))];
    if let Some(loc) = &s.s.location {
        l.push(format!("Location: {}{}", loc.name, if loc.state.is_empty() { String::new() } else { format!(", {}", loc.state) }));
    }
    let mut cur = format!("Current conditions right now: {}, temperature {}{tu}", cu.condition, n(cu.temp));
    if let Some(f) = cu.feels_like { cur.push_str(&format!(", feels like {}{tu}", f.round())); }
    if let Some(h) = cu.humidity { cur.push_str(&format!(", humidity {h}%")); }
    if let Some(ws) = cu.wind_speed { cur.push_str(&format!(", wind {ws} {su}")); }
    if let Some(uv) = cu.uv_index { cur.push_str(&format!(", UV index {uv}")); }
    l.push(cur);
    let start = present::current_hour_index(&w.hourly);
    l.push(String::new());
    l.push("Hour-by-hour (starting from the current hour):".into());
    for h in w.hourly.iter().skip(start).take(8) {
        l.push(format!("- {}: {}{tu}{}", present::local(h.time).format("%-I %p"), n(h.temp), h.precip_prob.map(|p| format!(", {p}% chance of precipitation")).unwrap_or_default()));
    }
    l.push(String::new());
    l.push("Daily forecast — use these exact numbers, do not recompute or guess new ones:".into());
    for (i, d) in w.daily.iter().take(7).enumerate() {
        let name = match i { 0 => "Today".to_string(), 1 => "Tomorrow".to_string(), _ => present::local(d.date).format("%A").to_string() };
        l.push(format!(
            "- {name} ({}): high {}{tu}, low {}{tu}, {}{}",
            present::local(d.date).format("%b %-d"), n(d.temp_max), n(d.temp_min), d.condition,
            d.precip_prob.map(|p| format!(", {p}% chance of precipitation")).unwrap_or_default()
        ));
    }
    l.push(String::new());
    let alerts = if s.alerts.is_empty() { &w.alerts } else { &s.alerts };
    l.push(if alerts.is_empty() { "No active weather alerts.".into() } else { format!("Active alerts: {}", alerts.iter().map(|a| format!("{} ({})", a.title, a.severity)).collect::<Vec<_>>().join("; ")) });
    if let Some(a) = w.air_quality.as_ref().and_then(|a| a["us_aqi"].as_f64()) {
        l.push(format!("Air quality: US AQI {a}"));
    }
    l.join("\n")
}

fn push_chat() {
    let Some(a) = win() else { return };
    let msgs = st(|s| s.chat.iter().map(|m| ChatMsg { user: m.role == "user", text: m.content.clone().into() }).collect()).unwrap_or_default();
    a.global::<Ui>().set_chat(model(msgs));
}

fn send_chat(text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let Some((provider, url, mdl, messages)) = st(|s| {
        s.chat.push(assistant::ChatMessage { role: "user".into(), content: text.clone() });
        let system = format!(
            "You are StormView's weather assistant, embedded in a weather radar app.\nRules:\n1. Read the user's question carefully and answer ONLY that question. If they ask about precipitation, answer about precipitation — not temperature, wind, or anything else, even if you discussed something else earlier.\n2. Use ONLY the numbers in the data below — never invent, guess, or recompute a number that isn't already there.\n3. Be concise and conversational.\n\n{}",
            weather_context(s)
        );
        let mut msgs = vec![assistant::ChatMessage { role: "system".into(), content: system }];
        let tail = s.chat.len().saturating_sub(8);
        msgs.extend(s.chat[tail..].iter().cloned());
        s.chat.push(assistant::ChatMessage { role: "assistant".into(), content: String::new() });
        (s.s.assistant_provider.clone(), s.s.assistant_base_url.clone(), s.s.assistant_model.clone(), msgs)
    }) else { return };
    push_chat();
    if let Some(a) = win() {
        a.global::<Ui>().set_chat_busy(true);
    }
    let on_delta = |t: String| {
        let _ = slint::invoke_from_event_loop(move || {
            st(|s| {
                if let Some(last) = s.chat.last_mut() {
                    last.content.push_str(&t);
                }
            });
            push_chat();
        });
    };
    bg(async move { assistant::chat(&http(), &provider, &url, &mdl, messages, on_delta).await }, |res| {
        st(|s| {
            if let Some(last) = s.chat.last_mut() {
                if let Err(e) = &res {
                    last.content = format!("⚠ {e}");
                } else if last.content.is_empty() {
                    last.content = "(No response from the model.)".into();
                }
            }
        });
        push_chat();
        if let Some(a) = win() {
            a.global::<Ui>().set_chat_busy(false);
        }
    });
}

/* ------------------------------------------------------------ notifications */

fn notify(title: &str, body: &str) {
    let _ = notify_rust::Notification::new().appname("StormView").summary(title).body(body).show();
}

/// Native replacement for the web app's push server: NWS alerts every 90 s, AQI every 30 min.
async fn monitor() {
    let mut seen: HashSet<String> = HashSet::new();
    let mut first = true;
    let mut last_aqi: i64 = 0;
    let mut tick: u32 = 0;
    loop {
        let s = SHARED.get().map(|m| m.lock().unwrap().clone());
        if let Some(s) = s.filter(|s| s.notifications_enabled) {
            if let Some(loc) = &s.location {
                let c = http();
                if s.notif_nws {
                    if let Ok(alerts) = weather::fetch_point_alerts(&c, loc.lat, loc.lon).await {
                        for a in &alerts {
                            if !a.id.is_empty() && seen.insert(a.id.clone()) && !first {
                                notify(&format!("⚠ {}", a.title), if a.headline.is_empty() { &loc.name } else { &a.headline });
                            } else if first && !a.id.is_empty() {
                                // Alerts already active at launch: tell the user once, not per alert.
                                seen.insert(a.id.clone());
                            }
                        }
                        if first && !alerts.is_empty() {
                            notify(&format!("{} active alert{} near {}", alerts.len(), if alerts.len() == 1 { "" } else { "s" }, loc.name), &alerts[0].title);
                        }
                        let active: HashSet<&String> = alerts.iter().map(|a| &a.id).collect();
                        seen.retain(|id| active.contains(id));
                        first = false;
                    }
                }
                if s.notif_aqi && tick % 20 == 0 {
                    if let Ok(aq) = services::air_quality(&c, loc.lat, loc.lon).await {
                        let now = chrono::Utc::now().timestamp_millis();
                        if let Some(v) = aq["us_aqi"].as_f64().filter(|v| *v >= 200.0) {
                            if now - last_aqi > 12 * 3600 * 1000 {
                                last_aqi = now;
                                notify("Unhealthy air quality", &format!("The US AQI near {} is {v}. Limit time outdoors.", loc.name));
                            }
                        }
                    }
                }
            }
        }
        tick = tick.wrapping_add(1);
        tokio::time::sleep(Duration::from_secs(90)).await;
    }
}

/* ------------------------------------------------------------ long range */

fn future_lookup(date: String) {
    let Some(a) = win() else { return };
    let w = a.global::<Weather>();
    let Ok(d) = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d") else {
        w.set_future_status("error".into());
        return;
    };
    let today = Local::now().date_naive();
    let days = (d - today).num_days();
    if !(17..=210).contains(&days) {
        toast("Pick a date between 17 days and ~7 months from today.");
        return;
    }
    let Some((loc, units)) = st(|s| (s.s.location.clone(), s.s.units.clone())) else { return };
    let Some(loc) = loc else { return };
    w.set_future_status("loading".into());
    let iso = d.format("%Y-%m-%d").to_string();
    bg(
        async move { services::future_range(&http(), loc.lat, loc.lon, &iso, &iso, Units::parse(&units)).await },
        |res| {
            let Some(a) = win() else { return };
            let w = a.global::<Weather>();
            match res {
                Ok(days) if !days.is_empty() => {
                    let d = &days[0];
                    let (p, dk, metric) = st(|s| (s.primary, s.dark, s.s.metric())).unwrap();
                    let tu = if metric { "°C" } else { "°F" };
                    w.set_future_hi(d.temp_max.map(|v| format!("H {v}{tu}")).unwrap_or_default().into());
                    w.set_future_lo(d.temp_min.map(|v| format!("L {v}{tu}")).unwrap_or_default().into());
                    let (label, ic, hue) = precip_badge(d.precip_type);
                    w.set_future_type(label.into());
                    w.set_future_type_icon(icon(&format!("{ic}-fill")));
                    w.set_future_type_tint(present::tint(theme::semantic(hue, p, dk)));
                    let (su, pu) = if metric { ("km/h", "mm") } else { ("mph", "in") };
                    let mut stats = vec![];
                    if let Some(v) = d.precip_sum { stats.push(KeyValue { key: "Precip.".into(), value: format!("{v} {pu}").into() }); }
                    if let Some(v) = d.wind_speed_max { stats.push(KeyValue { key: "Wind".into(), value: format!("{v} {su}").into() }); }
                    if let Some(v) = d.humidity_mean { stats.push(KeyValue { key: "Humidity".into(), value: format!("{v}%").into() }); }
                    if let Some(v) = d.cloud_cover_mean { stats.push(KeyValue { key: "Cloud cover".into(), value: format!("{v}%").into() }); }
                    w.set_future_stats(model(stats));
                    w.set_future_status("ready".into());
                }
                Ok(_) => w.set_future_status("empty".into()),
                Err(_) => w.set_future_status("error".into()),
            }
        },
    );
}

fn precip_badge(t: Option<&str>) -> (&'static str, &'static str, u32) {
    match t {
        Some("snow") => ("Snow", "weather_snowy", 0x7cc6e8),
        Some("showers") => ("Showers", "rainy_light", 0x4f8ef5),
        Some("rain") => ("Rain", "rainy", 0x3d7ff0),
        Some("none") => ("Dry", "cloud", 0x7d8ba3),
        _ => ("", "cloud", 0x7d8ba3),
    }
}

fn month_list() -> Vec<NaiveDate> {
    let today = Local::now().date_naive();
    let end = today + chrono::Duration::days(210);
    let mut out = vec![];
    let mut cur = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
    while cur <= end {
        out.push(cur);
        cur = if cur.month() == 12 { NaiveDate::from_ymd_opt(cur.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(cur.year(), cur.month() + 1, 1) }.unwrap();
    }
    out
}

fn month_lookup(i: usize) {
    if let Some(a) = win() {
        a.global::<Weather>().set_month_index(i as i32);
    }
    let months = month_list();
    let Some(start) = months.get(i).cloned() else { return };
    let end = if start.month() == 12 { NaiveDate::from_ymd_opt(start.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1) }.unwrap()
        - chrono::Duration::days(1);
    let Some((loc, units)) = st(|s| (s.s.location.clone(), s.s.units.clone())) else { return };
    let Some(loc) = loc else { return };
    if let Some(a) = win() {
        a.global::<Weather>().set_month_status("loading".into());
    }
    let (s1, e1) = (start.format("%Y-%m-%d").to_string(), end.format("%Y-%m-%d").to_string());
    bg(async move { services::future_range(&http(), loc.lat, loc.lon, &s1, &e1, Units::parse(&units)).await }, move |res| {
        let Some(a) = win() else { return };
        let w = a.global::<Weather>();
        match res {
            Ok(days) if !days.is_empty() => {
                let (p, dk, metric) = st(|s| (s.primary, s.dark, s.s.metric())).unwrap();
                let pu = if metric { "mm" } else { "in" };
                let first_snow = days.iter().position(|d| d.precip_type == Some("snow"));
                let rows = days
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let date = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").unwrap_or(start);
                        let (_, ic, hue) = precip_badge(d.precip_type);
                        MonthRow {
                            weekday: date.format("%a").to_string().into(),
                            day: date.day().to_string().into(),
                            hi: d.temp_max.map(|v| format!("{v}°")).unwrap_or("—".into()).into(),
                            lo: d.temp_min.map(|v| format!("{v}°")).unwrap_or("—".into()).into(),
                            icon: icon(&format!("{ic}-fill")),
                            has_icon: d.precip_type.is_some(),
                            tint: present::tint(theme::semantic(hue, p, dk)),
                            amount: d.precip_sum.filter(|v| *v > 0.0).map(|v| format!("{v}{pu}")).unwrap_or_default().into(),
                            flag: Some(i) == first_snow,
                        }
                    })
                    .collect();
                w.set_month_rows(model(rows));
                w.set_month_status("ready".into());
            }
            Ok(_) => w.set_month_status("empty".into()),
            Err(_) => w.set_month_status("error".into()),
        }
    });
}

/* ------------------------------------------------------------ main */

fn main() -> Result<(), slint::PlatformError> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("tokio");
    RT.set(rt.handle().clone()).ok();
    HTTP.set(http::client()).ok();
    let settings = Settings::load();
    SHARED.set(Mutex::new(settings.clone())).ok();

    let app = AppWindow::new()?;
    app.global::<Shapes>().on_path(|name| shapes::path(&name).into());
    // Custom window frame: the Material title bar hands moves and resizes to the OS.
    {
        use slint::winit_030::{winit::window::ResizeDirection, WinitWindowAccessor};
        let ui = app.global::<Ui>();
        let w = app.as_weak();
        ui.on_window_drag(move || {
            if let Some(a) = w.upgrade() {
                a.window().with_winit_window(|ww| ww.drag_window().ok());
            }
        });
        let w = app.as_weak();
        ui.on_window_resize(move |edge| {
            let dir = match edge.as_str() {
                "n" => ResizeDirection::North,
                "s" => ResizeDirection::South,
                "e" => ResizeDirection::East,
                "w" => ResizeDirection::West,
                "ne" => ResizeDirection::NorthEast,
                "nw" => ResizeDirection::NorthWest,
                "se" => ResizeDirection::SouthEast,
                _ => ResizeDirection::SouthWest,
            };
            if let Some(a) = w.upgrade() {
                a.window().with_winit_window(|ww| ww.drag_resize_window(dir).ok());
            }
        });
        let w = app.as_weak();
        ui.on_window_minimize(move || {
            if let Some(a) = w.upgrade() {
                a.window().set_minimized(true);
            }
        });
        let w = app.as_weak();
        ui.on_window_maximize(move || {
            if let Some(a) = w.upgrade() {
                let m = !a.window().is_maximized();
                a.window().set_maximized(m);
                a.global::<Ui>().set_maximized(m);
            }
        });
        ui.on_window_close(|| {
            let _ = slint::quit_event_loop();
        });
        let w = app.as_weak();
        ui.on_window_changed(move || {
            if let Some(a) = w.upgrade() {
                a.global::<Ui>().set_maximized(a.window().is_maximized());
            }
        });
    }
    app.global::<IconStore>().on_get(|name, color| present::tinted(&name, color));

    let (lat, lon) = settings.location.as_ref().map(|l| (l.lat, l.lon)).unwrap_or((39.5, -98.35));
    let zoom = if settings.location.is_some() { settings.map_zoom } else { 3.6 };
    map::Engine::install(&app, rt.handle().clone(), http(), lat, lon, zoom);
    map::with(|e| e.location = settings.location.as_ref().map(|l| (map::lon_to_x(l.lon), map::lat_to_y(l.lat))));

    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            app: app.as_weak(),
            s: settings.clone(),
            weather: None,
            alerts: vec![],
            all_frames: vec![],
            frames: vec![],
            idx: 0,
            playing: false,
            speed: 1.0,
            play_timer: Timer::default(),
            primary: Rgb::from_u32(0x4f8ef5),
            dark: false,
            ctx: (0.0, 0.0),
            results: vec![],
            search_timer: Timer::default(),
            aqi_timer: Timer::default(),
            aqi_cache: HashMap::new(),
            aqi_seq: 0,
            alert_version: 0,
            chat: vec![],
            toast_timer: Timer::default(),
            weather_seq: 0,
        })
    });

    // ---------------------------------------------------------------- map
    let mb = app.global::<MapBridge>();
    mb.on_resized(|w, h| {
        map::with(|e| e.resize(w as f64, h as f64));
        schedule_aqi();
    });
    mb.on_pan(|dx, dy| {
        map::with(|e| e.pan(dx as f64, dy as f64));
        schedule_aqi();
    });
    mb.on_zoom(|delta, x, y| {
        // Wheel deltas arrive in pixels; one notch ≈ half a zoom level. Buttons/keys pass ±1.
        let levels = if delta.abs() <= 2.0 { delta as f64 } else { (delta as f64 / 240.0).clamp(-1.0, 1.0) };
        map::with(|e| {
            let (fx, fy) = if x < 0.0 { (e.w / 2.0, e.h / 2.0) } else { (x as f64, y as f64) };
            e.zoom_by(levels, fx, fy)
        });
    });
    app.on_zoom_settled(|z| {
        st(|s| s.s.map_zoom = (z as f64 * 10.0).round() / 10.0);
        save();
        schedule_aqi();
    });
    mb.on_clicked(|x, y| map_clicked(x as f64, y as f64));
    mb.on_context(|x, y| {
        let Some((lon, lat)) = map::with(|e| e.lonlat_at(x as f64, y as f64)) else { return };
        st(|s| s.ctx = (lat, lon));
        if let Some(a) = win() {
            a.global::<MapBridge>().set_ctx_coords(
                format!("{:.4}° {} · {:.4}° {}", lat.abs(), if lat >= 0.0 { "N" } else { "S" }, lon.abs(), if lon >= 0.0 { "E" } else { "W" }).into(),
            );
        }
    });
    mb.on_ctx_set_location(|| {
        let (lat, lon) = st(|s| s.ctx).unwrap();
        bg(async move { services::reverse_geocode(&http(), lat, lon).await }, move |res| {
            set_location(res.unwrap_or(Place { lat, lon, name: format!("{lat:.4}, {lon:.4}"), state: String::new(), country: String::new(), display: None }));
        });
    });
    mb.on_ctx_center(|| {
        let (lat, lon) = st(|s| s.ctx).unwrap();
        map::with(|e| e.fly_to(lat, lon, None));
    });
    mb.on_ctx_copy(|| {
        let (lat, lon) = st(|s| s.ctx).unwrap();
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(format!("{lat:.5}, {lon:.5}"));
            toast("Coordinates copied");
        }
    });
    mb.on_ctx_open_maps(|| {
        let (lat, lon) = st(|s| s.ctx).unwrap();
        let _ = open::that(format!("https://maps.google.com/?q={lat},{lon}"));
    });

    // ---------------------------------------------------------------- radar
    let r = app.global::<Radar>();
    r.on_seek(|i| {
        st(|s| s.playing = false);
        restart_playback();
        set_frame(i.max(0) as usize);
    });
    r.on_hover_label(|i| {
        st(|s| {
            s.frames
                .get(i.max(0) as usize)
                .map(|f| format!("{}{}", time_str(f.time * 1000), if f.kind == "nowcast" { " · Forecast" } else { "" }))
                .unwrap_or_default()
                .into()
        })
        .unwrap_or_default()
    });
    r.on_toggle_play(|| {
        st(|s| s.playing = !s.playing);
        restart_playback();
    });
    r.on_set_speed(|v| {
        st(|s| s.speed = v);
        restart_playback();
    });
    r.on_now(|| {
        let i = st(|s| {
            s.playing = false;
            latest_past(&s.frames)
        })
        .unwrap_or(0);
        restart_playback();
        set_frame(i);
    });
    r.on_refresh(refresh_radar);

    // ---------------------------------------------------------------- weather
    let w = app.global::<Weather>();
    w.on_refresh(|| {
        refresh_weather();
        refresh_alerts();
    });
    w.on_open_hour(|i| {
        let Some(a) = win() else { return };
        st(|s| {
            if let Some(h) = s.weather.as_ref().and_then(|w| w.hourly.get(i as usize)) {
                present::apply_hour(&a, h, &s.s, s.primary, s.dark);
            }
        });
        a.global::<Ui>().set_hour_dialog_open(true);
    });
    w.on_open_precip(|| {
        if let Some(a) = win() {
            a.global::<Ui>().set_precip_dialog_open(true);
        }
    });
    w.on_future_date(|d| future_lookup(d.to_string()));
    w.on_month_picked(|i| month_lookup(i as usize));
    let today = Local::now().date_naive();
    w.set_future_min_date((today + chrono::Duration::days(17)).format("%Y-%m-%d").to_string().into());
    w.set_future_max_date((today + chrono::Duration::days(210)).format("%Y-%m-%d").to_string().into());
    w.set_month_items(model(
        month_list()
            .iter()
            .map(|m| MenuItem { text: m.format("%B %Y").to_string().into(), enabled: true, ..Default::default() })
            .collect(),
    ));
    w.set_future_status("idle".into());
    w.set_month_status("idle".into());

    // ---------------------------------------------------------------- prefs
    let p = app.global::<Prefs>();
    p.on_set_str(|k, v| set_str(&k, &v));
    p.on_set_bool(|k, v| set_bool(&k, v));
    p.on_set_num(|k, v| set_num(&k, v));
    p.on_move_widget(|i, delta| {
        st(|s| {
            let i = i as usize;
            let j = i as i32 + delta;
            if j >= 0 && (j as usize) < s.s.widgets.len() {
                s.s.widgets.swap(i, j as usize);
            }
        });
        save();
        sync_prefs();
        represent();
    });
    p.on_search(|q| {
        let q = q.to_string();
        st(|s| {
            s.search_timer.start(TimerMode::SingleShot, Duration::from_millis(400), move || {
                let q = q.clone();
                if q.trim().is_empty() {
                    if let Some(a) = win() {
                        a.global::<Prefs>().set_search_results(model(vec![]));
                    }
                    return;
                }
                if let Some(a) = win() {
                    a.global::<Prefs>().set_searching(true);
                }
                bg(async move { services::search_location(&http(), &q).await }, |res| {
                    let Some(a) = win() else { return };
                    let p = a.global::<Prefs>();
                    p.set_searching(false);
                    match res {
                        Ok(places) => {
                            let places: Vec<Place> = places.into_iter().take(5).collect();
                            p.set_search_results(model(
                                places
                                    .iter()
                                    .enumerate()
                                    .map(|(i, r)| PlaceItem {
                                        title: format!("{}{}", r.name, if r.state.is_empty() { String::new() } else { format!(", {}", r.state) }).into(),
                                        subtitle: r.display.clone().unwrap_or_default().into(),
                                        index: i as i32,
                                    })
                                    .collect(),
                            ));
                            p.set_location_error("".into());
                            st(|s| s.results = places);
                        }
                        Err(_) => p.set_location_error("Search failed. Try again.".into()),
                    }
                });
            })
        });
    });
    p.on_pick_place(|i| {
        if let Some(Some(place)) = st(|s| s.results.get(i as usize).cloned()) {
            if let Some(a) = win() {
                a.global::<Prefs>().set_search_results(model(vec![]));
            }
            toast(format!("Location set to {}", place.name));
            set_location(place);
        }
    });
    p.on_locate_me(|| {
        if let Some(a) = win() {
            a.global::<Prefs>().set_locating(true);
            a.global::<Prefs>().set_location_error("".into());
        }
        bg(
            async {
                let c = http();
                let mut place = services::ip_locate(&c).await?;
                if let Ok(p) = services::reverse_geocode(&c, place.lat, place.lon).await {
                    place.name = p.name;
                    place.state = p.state;
                    place.country = p.country;
                }
                Ok::<Place, error::Error>(place)
            },
            |res| {
                if let Some(a) = win() {
                    a.global::<Prefs>().set_locating(false);
                }
                match res {
                    Ok(p) => {
                        toast(format!("Location set to {}", p.name));
                        set_location(p);
                    }
                    Err(e) => {
                        if let Some(a) = win() {
                            a.global::<Prefs>().set_location_error(e.to_string().into());
                        }
                    }
                }
            },
        );
    });
    p.on_refresh_models(refresh_models);
    p.on_test_notification(|| notify("StormView test notification", "Alerts for your location will look like this."));
    p.on_open_url(|u| {
        let _ = open::that(u.as_str());
    });
    p.set_radar_schemes(model(radar_schemes()));

    // ---------------------------------------------------------------- ui
    let ui = app.global::<Ui>();
    ui.on_send_chat(|t| send_chat(t.to_string()));
    ui.on_clear_chat(|| {
        st(|s| s.chat.clear());
        push_chat();
    });
    ui.on_dismiss_whats_new(|never| {
        if never {
            st(|s| s.s.dismissed_whats_new = WHATS_NEW_VERSION.into());
            save();
        }
    });

    // ---------------------------------------------------------------- start
    apply_theme();
    apply_radar_params();
    sync_prefs();
    update_chat_label();
    if settings.location.is_none() {
        app.global::<Ui>().set_welcome_open(true);
    } else if settings.dismissed_whats_new != WHATS_NEW_VERSION {
        app.global::<Ui>().set_whats_new_open(true);
    }
    refresh_weather();
    refresh_alerts();
    refresh_radar();
    load_alert_polygons();
    refresh_models();
    let timers: Vec<Timer> = vec![Timer::default(), Timer::default(), Timer::default(), Timer::default(), Timer::default()];
    // Drives the hero's weather scene at 30 fps. animation-tick() would repaint the
    // whole window (map included) at the display's refresh rate, several times the cost.
    let scene_start = std::time::Instant::now();
    timers[4].start(TimerMode::Repeated, Duration::from_millis(33), move || {
        let Some(a) = win() else { return };
        let w = a.global::<Weather>();
        if w.get_has_data() && a.global::<Prefs>().get_weather_scene() && !a.window().is_minimized() {
            w.set_scene_time(scene_start.elapsed().as_secs_f32());
        }
    });
    // Follow the OS light/dark setting while in "System" mode.
    timers[3].start(TimerMode::Repeated, Duration::from_secs(10), || {
        let system = st(|s| s.s.theme_mode == "system").unwrap_or(false);
        let os_dark = matches!(dark_light::detect(), Ok(dark_light::Mode::Dark));
        if system && st(|s| s.dark != os_dark).unwrap_or(false) {
            apply_theme();
        }
    });
    timers[0].start(TimerMode::Repeated, Duration::from_secs(600), refresh_weather);
    timers[1].start(TimerMode::Repeated, Duration::from_secs(90), refresh_alerts);
    timers[2].start(TimerMode::Repeated, Duration::from_secs(600), || {
        refresh_radar();
        load_alert_polygons();
    });
    rt.spawn(monitor());

    let result = app.run();
    st(|s| s.s.save());
    drop(timers);
    result
}
