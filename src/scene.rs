//! Animated weather scene behind the hero (like the Pixel Weather app).
//! Rust picks the scene and its colors; `ui/scene.slint` animates it.
//! Every color is an HCT tone of a hue harmonized toward the theme's primary,
//! so the scene follows the Material You scheme in light and dark mode.

use material_colors::{blend::harmonize, color::Rgb, hct::Hct};

use crate::SceneInfo;

const SUN: u32 = 0xf2b705;
const RAIN: u32 = 0x4f8ef5;
const NIGHT: u32 = 0x5b5bd6;

/// (kind, intensity 0..1). Kinds match the branches in `ui/scene.slint`.
fn kind(code: i64, is_day: bool) -> (&'static str, f32) {
    let sky = |d, n| if is_day { d } else { n };
    match code {
        0 => (sky("clear-day", "clear-night"), 0.0),
        1 => (sky("clear-day", "clear-night"), 0.3),
        2 => (sky("partly-day", "partly-night"), 0.5),
        3 => ("cloudy", 0.6),
        45 | 48 => ("fog", 0.7),
        51 | 53 | 55 => ("rain", 0.2),
        56 | 57 | 66 | 67 => ("sleet", 0.5),
        61 | 80 => ("rain", 0.45),
        63 | 81 => ("rain", 0.7),
        65 | 82 => ("rain", 1.0),
        71 | 85 => ("snow", 0.4),
        73 | 77 => ("snow", 0.65),
        75 | 86 => ("snow", 1.0),
        95 => ("storm", 0.8),
        96 | 99 => ("hail", 0.9),
        _ => ("cloudy", 0.5),
    }
}

pub fn info(code: i64, is_day: bool, wind_mph: f64, primary: Rgb, dark: bool) -> SceneInfo {
    // STORMVIEW_SCENE=<WMO code>[,night] previews a scene regardless of the weather.
    let (code, is_day) = match std::env::var("STORMVIEW_SCENE") {
        Ok(v) => (v.split(',').next().and_then(|c| c.trim().parse().ok()).unwrap_or(code), !v.contains("night")),
        Err(_) => (code, is_day),
    };
    let (kind, intensity) = kind(code, is_day);
    let hue = |hex: u32| Hct::new(harmonize(Rgb::from_u32(hex), primary)).get_hue();
    let c = |h: f64, chroma: f64, tone: f64| crate::theme::rgb(Rgb::from(Hct::from(h, chroma, tone)));
    let gloomy = matches!(kind, "storm" | "hail") || (kind == "rain" && intensity > 0.6);
    let night = !is_day;
    let sky_hue = if night { hue(NIGHT) } else if matches!(kind, "clear-day" | "partly-day") { hue(RAIN) } else { hue(crate::present::visual(code, is_day).hue) };
    let sky_chroma = if gloomy || kind == "fog" { 10.0 } else if matches!(kind, "cloudy" | "snow" | "sleet") { 16.0 } else { 32.0 };
    let (top, bottom) = match (dark, night, gloomy) {
        (true, _, true) => (20.0, 14.0),
        (true, true, _) => (22.0, 14.0),
        (true, false, _) => (34.0, 20.0),
        (false, _, true) => (72.0, 86.0),
        (false, true, _) => (70.0, 88.0),
        (false, false, _) if matches!(kind, "snow" | "sleet") => (74.0, 86.0),
        (false, false, _) => (84.0, 95.0),
    };
    let cloud_hue = sky_hue;
    let (front, back) = match (dark, gloomy) {
        (true, true) => (34.0, 26.0),
        (true, false) => (if night { 40.0 } else { 52.0 }, if night { 32.0 } else { 44.0 }),
        (false, true) => (58.0, 50.0),
        (false, false) => (99.0, 92.0),
    };
    SceneInfo {
        kind: kind.into(),
        intensity,
        wind: (wind_mph / 30.0).clamp(0.0, 1.0) as f32,
        sky_top: c(sky_hue, sky_chroma, top),
        sky_bottom: c(sky_hue, sky_chroma * 0.6, bottom),
        sun: c(hue(SUN), 70.0, if dark { 82.0 } else { 80.0 }),
        glow: c(hue(SUN), 50.0, if dark { 70.0 } else { 90.0 }).with_alpha(0.45),
        moon: c(hue(SUN), 12.0, 94.0),
        cloud: c(cloud_hue, 6.0, front),
        cloud_back: c(cloud_hue, 8.0, back),
        drop: c(hue(RAIN), 48.0, if dark { 76.0 } else if gloomy { 30.0 } else { 52.0 }),
        flake: c(cloud_hue, 4.0, if dark { 96.0 } else { 99.0 }),
        star: c(hue(SUN), 8.0, 96.0),
        bolt: c(hue(SUN), 80.0, 88.0),
    }
}
