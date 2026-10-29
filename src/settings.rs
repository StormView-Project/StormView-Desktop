//! Persisted user settings — every option from the web app's settings panel,
//! plus Material You theming. Stored as JSON in the platform config directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::services::Place;

pub const WIDGET_IDS: [(&str, &str); 15] = [
    ("alerts", "Weather Alerts"),
    ("hourly", "Hourly Forecast"),
    ("daily", "Daily Forecast"),
    ("sunrise", "Sunrise & Sunset"),
    ("details", "Weather Details"),
    ("aqi", "Air Quality"),
    ("windCompass", "Wind Compass"),
    ("feelsLike", "Feels Like"),
    ("precipOutlook", "Precipitation Outlook"),
    ("daylight", "Daylight"),
    ("uvIndex", "UV Index"),
    ("moonPhase", "Moon Phase"),
    ("outlook", "Today's Outlook"),
    ("futureWeather", "Future Weather"),
    ("monthOutlook", "Month Outlook"),
];

pub const DETAIL_FIELDS: [(&str, &str); 9] = [
    ("humidity", "Humidity"),
    ("wind", "Wind"),
    ("visibility", "Visibility"),
    ("pressure", "Pressure"),
    ("dewPoint", "Dew Point"),
    ("uvIndex", "UV Index"),
    ("cloudCover", "Cloud Cover"),
    ("precip", "Precipitation"),
    ("aqi", "Air Quality"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetPref {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Material You
    pub theme_mode: String,      // system | light | dark
    pub color_source: String,    // preset id | adaptive | custom
    pub custom_seed: String,     // #rrggbb
    pub scheme_variant: String,  // tonal-spot | expressive | vibrant | ...
    pub contrast: String,        // standard | medium | high
    pub map_style: String,       // material | streets
    pub press_style: String,     // morph | ripple | both
    pub weather_scene: bool,     // animated scene behind the hero
    // Layout
    pub sidebar_position: String, // left | right
    pub compact_layout: bool,
    pub custom_frame: bool, // Material title bar instead of the OS window frame
    // Widgets
    pub widgets: Vec<WidgetPref>,
    pub details_fields: Vec<String>, // hidden detail fields
    pub aqi_show_pollutants: bool,
    // Weather
    pub weather_api: String, // openmeteo | nws
    pub units: String,       // imperial | metric
    pub location: Option<Place>,
    // Radar (LibreWXR)
    pub map_layer: String, // radar | aqi
    pub radar_opacity: f32,
    pub radar_tile_quality: u32,
    pub radar_color_scheme: u32,
    pub radar_frame_fade: bool,
    pub radar_smooth: bool,
    pub radar_snow: bool,
    pub show_nowcast: bool,
    pub show_satellite: bool,
    pub show_alert_polygons: bool,
    pub show_advisories: bool,
    pub show_arrows: bool,
    pub map_zoom: f64,
    // Notifications
    pub notifications_enabled: bool,
    pub notif_nws: bool,
    pub notif_aqi: bool,
    // Assistant
    pub assistant_provider: String, // ollama | openai
    pub assistant_base_url: String,
    pub assistant_model: String,
    // Misc
    pub dismissed_whats_new: String,
}

impl Default for Settings {
    fn default() -> Self {
        let default_on = ["alerts", "hourly", "daily", "sunrise", "details"];
        Settings {
            theme_mode: "system".into(),
            color_source: "storm".into(),
            custom_seed: "#4f8ef5".into(),
            scheme_variant: "tonal-spot".into(),
            contrast: "standard".into(),
            map_style: "material".into(),
            press_style: "morph".into(),
            weather_scene: true,
            sidebar_position: "left".into(),
            compact_layout: true,
            custom_frame: true,
            widgets: WIDGET_IDS
                .iter()
                .map(|(id, _)| WidgetPref { id: id.to_string(), enabled: default_on.contains(id) })
                .collect(),
            details_fields: vec![],
            aqi_show_pollutants: true,
            weather_api: "openmeteo".into(),
            units: "imperial".into(),
            location: None,
            map_layer: "radar".into(),
            radar_opacity: 0.75,
            radar_tile_quality: 512,
            radar_color_scheme: 7,
            radar_frame_fade: false,
            radar_smooth: true,
            radar_snow: true,
            show_nowcast: true,
            show_satellite: false,
            show_alert_polygons: true,
            show_advisories: false,
            show_arrows: false,
            map_zoom: 7.0,
            notifications_enabled: false,
            notif_nws: true,
            notif_aqi: true,
            assistant_provider: "ollama".into(),
            assistant_base_url: "http://localhost:11434".into(),
            assistant_model: String::new(),
            dismissed_whats_new: String::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "arc360hub", "StormView").map(|d| d.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s: Settings = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        // Keep saved order/enabled state, drop unknown ids, append widgets added since.
        s.widgets.retain(|w| WIDGET_IDS.iter().any(|(id, _)| *id == w.id));
        for (id, _) in WIDGET_IDS {
            if !s.widgets.iter().any(|w| w.id == id) {
                s.widgets.push(WidgetPref { id: id.into(), enabled: false });
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
    }

    pub fn widget_enabled(&self, id: &str) -> bool {
        self.widgets.iter().any(|w| w.id == id && w.enabled)
    }

    pub fn detail_visible(&self, key: &str) -> bool {
        !self.details_fields.iter().any(|k| k == key)
    }

    pub fn metric(&self) -> bool {
        self.units == "metric"
    }
}
