//! Turns weather data into display-ready values for the Slint `Weather` global —
//! the logic of every web-app widget (UV/AQI scales, feels-like explanations,
//! today's outlook tips, moon phase…) lives here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{DateTime, Local, TimeZone, Timelike};
use material_colors::color::Rgb;
use slint::{Image, ModelRc, SharedString, VecModel};

use crate::settings::Settings;
use crate::theme::{semantic, Semantic as Sem};
use crate::weather::{Alert, Day, Hour, WeatherData};
use crate::{AlertItem, Bar, DayItem, HourItem, InfoTile, KeyValue, MiniHour, Tint, Tip};

mod icons {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

thread_local! {
    static ICONS: RefCell<HashMap<(String, u32), Image>> = RefCell::new(HashMap::new());
}

/// Name of a Material Symbols icon (`foo` outlined, `foo-fill` filled) for the UI.
pub fn icon(name: &str) -> SharedString {
    name.into()
}

/// The icon's alpha mask filled with `color`. Tinting here instead of with Slint's
/// `colorize` avoids a renderer path that reads uninitialized GPU memory on some
/// drivers (it showed bits of other icons inside icons). Cached per name + color.
pub fn tinted(name: &str, color: slint::Color) -> Image {
    let key = (name.to_string(), color.as_argb_encoded());
    ICONS.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let Some(img) = icons::icon_bytes(name)
                    .or_else(|| icons::icon_bytes(name.trim_end_matches("-fill")))
                    .and_then(|b| image::load_from_memory(b).ok())
                else {
                    return Image::default();
                };
                let mut rgba = img.to_rgba8();
                let (r, g, b, a) = (color.red(), color.green(), color.blue(), color.alpha() as u32);
                for px in rgba.pixels_mut() {
                    px.0 = [r, g, b, ((px.0[3] as u32 * a) / 255) as u8];
                }
                let (w, h) = rgba.dimensions();
                Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(rgba.as_raw(), w, h))
            })
            .clone()
    })
}

pub fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

pub fn tint(s: Sem) -> Tint {
    Tint { color: s.color, container: s.container, on_container: s.on_container }
}

/* ------------------------------------------------------------- visuals */

pub struct Visual {
    pub icon: &'static str,
    pub shape: &'static str,
    pub hue: u32,
    pub label: &'static str,
}

pub fn visual(code: i64, is_day: bool) -> Visual {
    let label = crate::weather::wmo_description(code);
    let v = |icon, shape, hue| Visual { icon, shape, hue, label };
    match code {
        0 | 1 if is_day => v("clear_day", "sunny", 0xf2b705),
        0 | 1 => v("bedtime", "12-sided-cookie", 0x8d8cff),
        2 if is_day => v("partly_cloudy_day", "puffy", 0xe9a93a),
        2 => v("partly_cloudy_night", "puffy", 0x8d8cff),
        3 => v("cloud", "puffy", 0x7d8ba3),
        45 | 48 => v("foggy", "4-sided-cookie", 0x8a94a6),
        56 | 57 | 66 | 67 => v("weather_mix", "7-sided-cookie", 0x5fb4e8),
        51..=55 => v("rainy_light", "6-sided-cookie", 0x4f8ef5),
        65 | 82 => v("rainy_heavy", "7-sided-cookie", 0x2f6fe0),
        61..=63 | 80 | 81 => v("rainy", "7-sided-cookie", 0x3d7ff0),
        77 => v("grain", "flower", 0x7cc6e8),
        75 | 86 => v("snowing_heavy", "flower", 0x6fc0e8),
        71..=86 => v("weather_snowy", "flower", 0x7cc6e8),
        96 | 99 => v("weather_hail", "burst", 0xa463e8),
        95 => v("thunderstorm", "soft-burst", 0x8f5ee8),
        _ => v("cloud", "puffy", 0x7d8ba3),
    }
}

pub fn uv_info(uv: f64) -> (&'static str, u32, &'static str) {
    if uv <= 2.0 {
        ("Low", 0x3fbf5f, "Minimal protection needed for most people.")
    } else if uv <= 5.0 {
        ("Moderate", 0xe8c22a, "Wear sunglasses and use sunscreen on bright days.")
    } else if uv <= 7.0 {
        ("High", 0xf08a2c, "Wear sunscreen and a hat — seek shade around midday.")
    } else if uv <= 10.0 {
        ("Very High", 0xe5484d, "Unprotected skin can burn quickly — take extra precautions.")
    } else {
        ("Extreme", 0xc050e0, "Avoid sun exposure during midday hours if possible.")
    }
}

pub const AQI_SCALE: [(f64, u32, &str, &str); 6] = [
    (50.0, 0x00c853, "Good", "Air quality is satisfactory and poses little or no risk."),
    (100.0, 0xe8c200, "Moderate", "Acceptable quality. Unusually sensitive individuals may experience minor effects."),
    (150.0, 0xff7e00, "Unhealthy for Sensitive Groups", "Sensitive groups may experience health effects. The general public is less likely to be affected."),
    (200.0, 0xff1f1f, "Unhealthy", "Everyone may begin to experience health effects. Sensitive groups may experience more serious effects."),
    (300.0, 0x8f3f97, "Very Unhealthy", "Health alert — everyone may experience serious health effects."),
    (500.0, 0x7e0023, "Hazardous", "Health warning of emergency conditions. The entire population is likely to be affected."),
];

pub fn aqi_category(aqi: f64) -> (f64, u32, &'static str, &'static str) {
    *AQI_SCALE.iter().find(|s| aqi <= s.0).unwrap_or(&AQI_SCALE[5])
}

pub fn severity_hue(s: &str) -> u32 {
    match s {
        "Extreme" => 0xe5484d,
        "Severe" => 0xf07a2c,
        "Moderate" => 0xe8b22a,
        "Minor" => 0x4f8ef5,
        _ => 0x8a94a6,
    }
}

pub fn moon_phase(now: DateTime<Local>) -> (&'static str, i64, f64) {
    let known_new = chrono::Utc.with_ymd_and_hms(2000, 1, 6, 18, 14, 0).unwrap();
    let synodic = 29.53058867;
    let days = (now.with_timezone(&chrono::Utc) - known_new).num_seconds() as f64 / 86400.0;
    let phase = (days % synodic / synodic).rem_euclid(1.0);
    let names = [
        "New Moon", "Waxing Crescent", "First Quarter", "Waxing Gibbous",
        "Full Moon", "Waning Gibbous", "Last Quarter", "Waning Crescent", "New Moon",
    ];
    let name = names[(((phase + 1.0 / 16.0) * 8.0).floor() as usize).min(8)];
    let illum = (((1.0 - (2.0 * std::f64::consts::PI * phase).cos()) / 2.0) * 100.0).round() as i64;
    // Shadow disc offset: slides off to the left while waxing, back in from the right while waning.
    let off = if phase < 0.5 { -phase * 2.0 } else { (1.0 - phase) * 2.0 };
    (name, illum, off)
}

/* ------------------------------------------------------------- formatting */

pub fn local(ms: i64) -> DateTime<Local> {
    Local.timestamp_millis_opt(ms).single().unwrap_or_else(Local::now)
}

pub fn time_str(ms: i64) -> String {
    local(ms).format("%-I:%M %p").to_string()
}

fn hour_str(ms: i64) -> String {
    local(ms).format("%-I%p").to_string()
}

fn n0(v: Option<f64>) -> String {
    v.map(|x| format!("{}", x.round() as i64)).unwrap_or_else(|| "—".into())
}

pub fn current_hour_index(hours: &[Hour]) -> usize {
    let now = Local::now().timestamp_millis();
    hours.iter().rposition(|h| h.time <= now).unwrap_or(0)
}

struct Ctx<'a> {
    primary: Rgb,
    dark: bool,
    s: &'a Settings,
}

impl Ctx<'_> {
    fn sem(&self, hue: u32) -> Sem {
        semantic(hue, self.primary, self.dark)
    }
    fn tint(&self, hue: u32) -> Tint {
        tint(self.sem(hue))
    }
    fn tu(&self) -> &'static str {
        if self.s.metric() { "°C" } else { "°F" }
    }
    fn su(&self) -> &'static str {
        if self.s.metric() { "km/h" } else { "mph" }
    }
    fn du(&self) -> &'static str {
        if self.s.metric() { "km" } else { "mi" }
    }
    fn pu(&self) -> &'static str {
        if self.s.metric() { "mm" } else { "in" }
    }
    #[allow(clippy::too_many_arguments)]
    fn tile(&self, key: &str, icon_name: &str, hue: u32, label: &str, value: String, sub: String, value_hue: Option<u32>, clickable: bool) -> InfoTile {
        InfoTile {
            key: key.into(),
            icon: icon(&format!("{icon_name}-fill")),
            label: label.into(),
            value: value.into(),
            sub: sub.into(),
            tint: self.tint(hue),
            value_color: value_hue.map(|h| self.sem(h).color).unwrap_or_default(),
            has_value_color: value_hue.is_some(),
            clickable,
        }
    }
}

/* ------------------------------------------------------------- per-widget */

pub fn alert_items(alerts: &[Alert], primary: Rgb, dark: bool) -> Vec<AlertItem> {
    alerts
        .iter()
        .map(|a| {
            let until = a.end.map(|e| format!("Until {}", local(e).format("%a, %b %-d at %-I:%M %p"))).unwrap_or_default();
            let certainty = if a.certainty != "Unknown" && !a.certainty.is_empty() { format!(" · {}", a.certainty) } else { String::new() };
            AlertItem {
                title: a.title.clone().into(),
                severity: if a.severity == "Unknown" { "Alert".into() } else { a.severity.clone().into() },
                until: format!("{until}{certainty}").into(),
                headline: a.headline.clone().into(),
                description: a.description.trim().to_string().into(),
                instruction: a.instruction.trim().to_string().into(),
                tint: tint(semantic(severity_hue(&a.severity), primary, dark)),
            }
        })
        .collect()
}

fn hour_tiles(c: &Ctx, h: &Hour) -> Vec<InfoTile> {
    let mut v = vec![];
    if let Some(f) = h.feels_like {
        v.push(c.tile("feels", "thermostat", 0xa78bfa, "Feels Like", format!("{}{}", f.round(), c.tu()), String::new(), None, false));
    }
    if let Some(p) = h.precip_prob {
        v.push(c.tile("pop", "water_drop", 0x4f8ef5, "Precip. Chance", format!("{p}%"), String::new(), None, false));
    }
    if let Some(a) = h.precipitation.filter(|a| *a > 0.0) {
        v.push(c.tile("amt", "rainy", 0x4f8ef5, "Precip. Amount", format!("{a} {}", c.pu()), String::new(), None, false));
    }
    if let Some(uv) = h.uv_index {
        let (l, hue, _) = uv_info(uv);
        v.push(c.tile("uv", "sunny", 0xf2b705, "UV Index", format!("{} · {l}", uv.round()), String::new(), Some(hue), false));
    }
    if let Some(w) = h.wind_speed {
        let dir = h.wind_direction_label.clone().unwrap_or_default();
        v.push(c.tile("wind", "air", 0x7d8ba3, "Wind", format!("{dir} {w} {}", c.su()).trim().to_string(), String::new(), None, false));
    }
    if let Some(hu) = h.humidity {
        v.push(c.tile("hum", "humidity_percentage", 0x3fbf9f, "Humidity", format!("{hu}%"), String::new(), None, false));
    }
    if let Some(p) = h.pressure {
        v.push(c.tile("pres", "compress", 0x3fbf5f, "Pressure", format!("{p} hPa"), String::new(), None, false));
    }
    if let Some(vis) = h.visibility {
        v.push(c.tile("vis", "visibility", 0xa78bfa, "Visibility", format!("{vis} {}", c.du()), String::new(), None, false));
    }
    v
}

fn day_item(c: &Ctx, i: usize, d: &Day, hourly: &[Hour], lo: f64, range: f64) -> DayItem {
    let start = local(d.date).date_naive();
    let all: Vec<&Hour> = hourly.iter().filter(|h| local(h.time).date_naive() == start).collect();
    let now_floor = Local::now().with_minute(0).and_then(|t| t.with_second(0)).map(|t| t.timestamp_millis()).unwrap_or(0);
    let is_today = start == Local::now().date_naive();
    let hours: Vec<MiniHour> = all
        .iter()
        .filter(|h| !is_today || h.time >= now_floor)
        .map(|h| {
            let v = visual(h.condition_code, h.is_day);
            MiniHour {
                label: hour_str(h.time).to_lowercase().into(),
                icon: icon(&format!("{}-fill", v.icon)),
                tint: c.tint(v.hue),
                precip: if h.precip_prob.unwrap_or(0.0) > 10.0 { format!("{}%", h.precip_prob.unwrap()).into() } else { "".into() },
                temp: format!("{}°", n0(h.temp)).into(),
            }
        })
        .collect();
    let avg = |vals: Vec<f64>| if vals.is_empty() { None } else { Some((vals.iter().sum::<f64>() / vals.len() as f64).round()) };
    let humidity = avg(all.iter().filter_map(|h| h.humidity).collect());
    let pressure = avg(all.iter().filter_map(|h| h.pressure).collect());
    let uv = d.uv_index_max.or_else(|| all.iter().filter_map(|h| h.uv_index).reduce(f64::max));
    let feels_hi = d.feels_max.or_else(|| all.iter().filter_map(|h| h.feels_like).reduce(f64::max));
    let feels_lo = d.feels_min.or_else(|| all.iter().filter_map(|h| h.feels_like).reduce(f64::min));
    let mut details = vec![];
    if let Some(w) = d.wind_speed_max {
        let g = d.wind_gusts_max.map(|g| format!("Gusts {g} {}", c.su())).unwrap_or_default();
        details.push(c.tile("wind", "air", 0x7d8ba3, "Max Wind", format!("{w} {}", c.su()), g, None, false));
    }
    if let Some(u) = uv {
        let (l, hue, _) = uv_info(u);
        details.push(c.tile("uv", "sunny", 0xf2b705, "UV Index", format!("{} — {l}", u.round()), String::new(), Some(hue), false));
    }
    if let Some(p) = d.precip_prob {
        let sum = d.precip_sum.unwrap_or(0.0);
        let (val, sub) = if sum > 0.0 { (format!("{sum} {}", c.pu()), format!("{p}% chance")) } else { (format!("{p}%"), String::new()) };
        details.push(c.tile("precip", "rainy", 0x4f8ef5, "Precipitation", val, sub, None, false));
    }
    if let Some(sr) = d.sunrise {
        details.push(c.tile("rise", "wb_twilight", 0xf2b705, "Sunrise", time_str(sr), String::new(), None, false));
    }
    if let Some(ss) = d.sunset {
        details.push(c.tile("set", "bedtime", 0xf07a2c, "Sunset", time_str(ss), String::new(), None, false));
    }
    if let Some(h) = humidity {
        details.push(c.tile("hum", "humidity_percentage", 0x4f8ef5, "Avg Humidity", format!("{h}%"), String::new(), None, false));
    }
    if let Some(p) = pressure {
        details.push(c.tile("pres", "compress", 0x3fbf5f, "Avg Pressure", format!("{p} hPa"), String::new(), None, false));
    }
    if let Some(f) = feels_hi {
        let sub = feels_lo.map(|l| format!("Low {}{}", l.round(), c.tu())).unwrap_or_default();
        details.push(c.tile("feels", "thermostat", 0xa78bfa, "Feels Like", format!("{}{}", f.round(), c.tu()), sub, None, false));
    }
    let v = visual(d.condition_code, true);
    let (mn, mx) = (d.temp_min.unwrap_or(lo), d.temp_max.unwrap_or(lo + range));
    DayItem {
        name: if i == 0 { "Today".into() } else { local(d.date).format("%a").to_string().into() },
        icon: icon(&format!("{}-fill", v.icon)),
        tint: c.tint(v.hue),
        pop: if d.precip_prob.unwrap_or(0.0) > 10.0 { format!("{}%", d.precip_prob.unwrap()).into() } else { "".into() },
        lo: format!("{}°", n0(d.temp_min)).into(),
        hi: format!("{}°", n0(d.temp_max)).into(),
        bar_left: ((mn - lo) / range) as f32,
        bar_width: (((mx - mn) / range) as f32).max(0.04),
        hours: model(hours),
        details: model(details),
    }
}

fn explain_feels(delta: i64, d: &WeatherData) -> String {
    let cu = &d.current;
    if delta.abs() < 3 {
        "Feels about the same as the actual temperature.".into()
    } else if delta < 0 {
        if cu.wind_speed.unwrap_or(0.0) >= 10.0 {
            format!("Wind is making it feel {}° cooler.", delta.abs())
        } else {
            format!("Feels {}° cooler than the actual temperature.", delta.abs())
        }
    } else if cu.humidity.unwrap_or(0.0) >= 60.0 {
        format!("Humidity is making it feel {delta}° warmer.")
    } else {
        format!("Feels {delta}° warmer than the actual temperature.")
    }
}

/* ------------------------------------------------------------- apply */

pub fn apply(app: &crate::AppWindow, d: &WeatherData, alerts: &[Alert], s: &Settings, primary: Rgb, dark: bool) {
    use slint::ComponentHandle;
    let w = app.global::<crate::Weather>();
    let c = Ctx { primary, dark, s };
    let cu = &d.current;
    let today = d.daily.first();

    w.set_has_data(true);
    w.set_source(if d.source == "nws" { if d.nws_fallback { "NWS (fallback)" } else { "NWS" } } else if d.nws_fallback { "Open-Meteo (fallback)" } else { "Open-Meteo" }.into());
    w.set_notice(d.notice.clone().unwrap_or_default().into());
    w.set_updated(format!("{} · updated {}", if d.source == "nws" { "National Weather Service" } else { "Open-Meteo" }, time_str(d.fetched_at)).into());

    // Hero
    let v = visual(cu.condition_code, cu.is_day);
    w.set_temp(n0(cu.temp).into());
    w.set_unit(c.tu().into());
    w.set_condition(if cu.condition.is_empty() { v.label.into() } else { cu.condition.clone().into() });
    w.set_hero_icon(icon(&format!("{}-fill", v.icon)));
    w.set_hero_shape(v.shape.into());
    w.set_hero_tint(c.tint(v.hue));
    let wind_mph = cu.wind_speed.unwrap_or(0.0) / if s.metric() { 1.609 } else { 1.0 };
    w.set_scene(crate::scene::info(cu.condition_code, cu.is_day, wind_mph, primary, dark));
    w.set_feels(cu.feels_like.map(|f| format!("Feels {}°", f.round())).unwrap_or_default().into());
    w.set_hi(today.map(|t| format!("{}°", n0(t.temp_max))).unwrap_or_default().into());
    w.set_lo(today.map(|t| format!("{}°", n0(t.temp_min))).unwrap_or_default().into());

    // Widget order
    let ids: Vec<SharedString> = s.widgets.iter().filter(|w| w.enabled).map(|w| w.id.clone().into()).collect();
    w.set_widgets(model(ids));

    // Alerts (prefer the live NWS point alerts)
    let list = if alerts.is_empty() { &d.alerts } else { alerts };
    w.set_alerts(model(alert_items(list, primary, dark)));

    // Hourly
    let start = current_hour_index(&d.hourly);
    let hours: Vec<&Hour> = d.hourly.iter().skip(start).take(24).collect();
    let temps: Vec<f64> = hours.iter().filter_map(|h| h.temp).collect();
    let (tmin, tmax) = (temps.iter().cloned().fold(f64::MAX, f64::min), temps.iter().cloned().fold(f64::MIN, f64::max));
    w.set_hourly(model(
        hours
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let v = visual(h.condition_code, h.is_day);
                HourItem {
                    label: if i == 0 { "Now".into() } else { hour_str(h.time).into() },
                    icon: icon(&format!("{}-fill", v.icon)),
                    tint: c.tint(v.hue),
                    precip: if h.precip_prob.unwrap_or(0.0) > 10.0 { format!("{}%", h.precip_prob.unwrap()).into() } else { "".into() },
                    temp: format!("{}°", n0(h.temp)).into(),
                    lift: if tmax > tmin { ((h.temp.unwrap_or(tmin) - tmin) / (tmax - tmin)) as f32 } else { 0.5 },
                    index: (start + i) as i32,
                }
            })
            .collect(),
    ));

    // Daily
    let lo = d.daily.iter().filter_map(|x| x.temp_min).fold(f64::MAX, f64::min);
    let hi = d.daily.iter().filter_map(|x| x.temp_max).fold(f64::MIN, f64::max);
    let range = (hi - lo).max(1.0);
    w.set_day_count_title(format!("{}-Day Forecast", d.daily.len()).into());
    w.set_daily(model(d.daily.iter().enumerate().map(|(i, day)| day_item(&c, i, day, &d.hourly, lo, range)).collect()));

    // Sun
    let sunrise = cu.sunrise.or(today.and_then(|t| t.sunrise));
    let sunset = cu.sunset.or(today.and_then(|t| t.sunset));
    w.set_sunrise(sunrise.map(time_str).unwrap_or_default().into());
    w.set_sunset(sunset.map(time_str).unwrap_or_default().into());

    // Details
    let aq = d.air_quality.clone().unwrap_or_default();
    let us_aqi = aq["us_aqi"].as_f64();
    let pu = c.pu();
    let precip_value = today
        .and_then(|t| t.precip_sum)
        .map(|p| format!("{p} {pu}"))
        .or(cu.precipitation.map(|p| format!("{p} {pu}")))
        .or(cu.precip_prob.map(|p| format!("{p}% chance")))
        .unwrap_or_else(|| "—".into());
    let uv = cu.uv_index.map(uv_info);
    let mut details = vec![
        c.tile("humidity", "humidity_percentage", 0x4f8ef5, "Humidity", cu.humidity.map(|h| format!("{h}%")).unwrap_or("—".into()), String::new(), None, false),
        c.tile(
            "wind", "air", 0x7d8ba3, "Wind",
            cu.wind_speed.map(|w| format!("{} {w} {}", cu.wind_direction_label.clone().unwrap_or_default(), c.su()).trim().to_string()).unwrap_or("—".into()),
            cu.wind_gusts.map(|g| format!("Gusts {g} {}", c.su())).unwrap_or_default(), None, false,
        ),
        c.tile("visibility", "visibility", 0xa78bfa, "Visibility", cu.visibility.map(|v| format!("{v} {}", c.du())).unwrap_or("—".into()), String::new(), None, false),
        c.tile("pressure", "compress", 0x3fbf5f, "Pressure", cu.pressure.map(|p| format!("{p} hPa")).unwrap_or("—".into()), String::new(), None, false),
        c.tile("dewPoint", "dew_point", 0x3fbf9f, "Dew Point", cu.dew_point.map(|p| format!("{p}{}", c.tu())).unwrap_or("—".into()), String::new(), None, false),
        c.tile("uvIndex", "sunny", 0xf2b705, "UV Index", match (cu.uv_index, uv) { (Some(u), Some((l, ..))) => format!("{u} {l}"), _ => "—".into() }, String::new(), uv.map(|u| u.1), false),
        c.tile("cloudCover", "cloud", 0x7d8ba3, "Cloud Cover", cu.cloud_cover.map(|p| format!("{p}%")).unwrap_or("—".into()), String::new(), None, false),
        c.tile("precip", "rainy", 0x4f8ef5, "Precip. Today", precip_value, String::new(), None, true),
    ];
    if let Some(a) = us_aqi {
        let (_, hue, label, _) = aqi_category(a);
        details.push(c.tile("aqi", "masks", 0x7d8ba3, "Air Quality", format!("{a} {}", label.replace("Unhealthy for Sensitive Groups", "Sensitive")), String::new(), Some(hue), false));
    }
    details.retain(|t| s.detail_visible(&t.key) && (t.value != "—" || t.key == "humidity" || t.key == "uvIndex"));
    w.set_details(model(details));

    // Air quality widget
    w.set_has_aqi(us_aqi.is_some());
    if let Some(a) = us_aqi {
        let (_, hue, label, desc) = aqi_category(a);
        w.set_aqi_value(format!("{a}").into());
        w.set_aqi_label(label.into());
        w.set_aqi_desc(desc.into());
        w.set_aqi_tint(c.tint(hue));
        w.set_aqi_pct((a / 300.0).min(1.0) as f32);
        let pol: Vec<KeyValue> = [("pm2_5", "PM2.5"), ("pm10", "PM10"), ("ozone", "Ozone"), ("nitrogen_dioxide", "NO₂")]
            .iter()
            .filter_map(|(k, l)| aq[*k].as_f64().map(|v| KeyValue { key: (*l).into(), value: format!("{} µg/m³", v.round()).into() }))
            .collect();
        w.set_aqi_pollutants(model(if s.aqi_show_pollutants { pol } else { vec![] }));
    }

    // Wind compass
    w.set_has_wind(cu.wind_speed.is_some() && cu.wind_direction.is_some());
    w.set_wind_speed(n0(cu.wind_speed).into());
    w.set_wind_unit(c.su().into());
    w.set_wind_from(format!("From {}", cu.wind_direction_label.clone().unwrap_or_default()).into());
    w.set_wind_gusts(cu.wind_gusts.map(|g| format!("Gusts {g} {}", c.su())).unwrap_or_default().into());
    w.set_wind_deg(cu.wind_direction.unwrap_or(0.0) as f32);

    // Feels like
    w.set_has_feels(cu.temp.is_some() && cu.feels_like.is_some());
    if let (Some(t), Some(f)) = (cu.temp, cu.feels_like) {
        w.set_feels_actual(format!("{}{}", t.round(), c.tu()).into());
        w.set_feels_value(format!("{}{}", f.round(), c.tu()).into());
        w.set_feels_desc(explain_feels((f - t).round() as i64, d).into());
    }

    // Precipitation outlook (next 8 hours)
    w.set_precip_bars(model(
        d.hourly
            .iter()
            .skip(start)
            .take(8)
            .map(|h| {
                let p = h.precip_prob.unwrap_or(0.0);
                Bar { label: hour_str(h.time).into(), value: format!("{p}%").into(), pct: (p / 100.0) as f32, past: false, amount: "".into() }
            })
            .collect(),
    ));

    // Daylight
    let now = Local::now().timestamp_millis();
    w.set_has_daylight(sunrise.is_some() && sunset.is_some());
    if let (Some(sr), Some(ss)) = (sunrise, sunset) {
        let total = ss - sr;
        let mins = total / 60000;
        w.set_daylight_total(format!("{}h {}m of daylight", mins / 60, mins % 60).into());
        w.set_daylight_pct((((now - sr) as f64 / total as f64).clamp(0.0, 1.0)) as f32);
        let is_day = now >= sr && now <= ss;
        w.set_is_daytime(is_day);
        w.set_daylight_note(if is_day { "" } else if now < sr { "Before sunrise" } else { "After sunset" }.into());
    }

    // UV widget
    w.set_has_uv(cu.uv_index.is_some());
    if let Some(u) = cu.uv_index {
        let (l, hue, tip) = uv_info(u);
        w.set_uv_value(format!("{}", (u * 10.0).round() / 10.0).into());
        w.set_uv_label(l.into());
        w.set_uv_tip(tip.into());
        w.set_uv_tint(c.tint(hue));
        w.set_uv_pct((u / 11.0).min(1.0) as f32);
    }

    // Moon
    let (name, illum, off) = moon_phase(Local::now());
    w.set_moon_name(name.into());
    w.set_moon_illum(format!("{illum}% illuminated").into());
    w.set_moon_offset(off as f32);

    // Today's outlook
    let metric = s.metric();
    let mut tips: Vec<(&str, u32, &str)> = vec![];
    let next6 = d.hourly.iter().skip(start).take(6);
    if cu.precip_prob.unwrap_or(0.0) >= 40.0 || cu.precipitation.unwrap_or(0.0) > 0.0 || next6.clone().any(|h| h.precip_prob.unwrap_or(0.0) >= 50.0) {
        tips.push(("umbrella", 0x4f8ef5, "Bring an umbrella"));
    }
    if cu.uv_index.unwrap_or(0.0) >= 6.0 {
        tips.push(("sunny", 0xf2b705, "High UV — wear sunscreen"));
    }
    if cu.wind_gusts.or(cu.wind_speed).unwrap_or(0.0) >= if metric { 48.0 } else { 30.0 } {
        tips.push(("air", 0x7d8ba3, "Windy — secure loose objects"));
    }
    if us_aqi.unwrap_or(0.0) >= 100.0 {
        tips.push(("masks", 0xf07a2c, "Poor air quality — limit outdoor exertion"));
    }
    if let Some(t) = cu.temp {
        if t <= if metric { 0.0 } else { 32.0 } {
            tips.push(("ac_unit", 0x5fb4e8, "Freezing — bundle up"));
        } else if t >= if metric { 32.0 } else { 90.0 } {
            tips.push(("emergency_heat", 0xe5484d, "Very hot — stay hydrated"));
        }
    }
    if tips.is_empty() {
        tips.push(("check_circle", 0x3fbf5f, "Great day to be outside"));
    }
    w.set_tips(model(tips.into_iter().map(|(i, h, t)| Tip { icon: icon(&format!("{i}-fill")), text: t.into(), tint: c.tint(h) }).collect()));

    // Precipitation dialog data
    let today_date = Local::now().date_naive();
    let today_hours: Vec<&Hour> = d.hourly.iter().filter(|h| local(h.time).date_naive() == today_date).collect();
    let total = today.and_then(|t| t.precip_sum).or_else(|| {
        let v: Vec<f64> = today_hours.iter().filter_map(|h| h.precipitation).collect();
        if v.is_empty() { None } else { Some(v.iter().sum()) }
    });
    let peak = today_hours.iter().filter_map(|h| h.precip_prob).fold(today.and_then(|t| t.precip_prob).unwrap_or(0.0), f64::max);
    w.set_precip_total(total.map(|t| format!("{t:.2} {pu}")).unwrap_or("—".into()).into());
    w.set_precip_peak(format!("{peak}%").into());
    w.set_precip_hours(model(
        today_hours
            .iter()
            .map(|h| {
                let p = h.precip_prob.unwrap_or(0.0);
                Bar {
                    label: hour_str(h.time).into(),
                    value: if p > 0.0 { format!("{p}%").into() } else { "".into() },
                    pct: if p > 0.0 { (p / 100.0).max(0.06) as f32 } else { 0.0 },
                    past: h.time < now,
                    amount: h.precipitation.filter(|a| *a > 0.0).map(|a| format!("{a}")).unwrap_or_default().into(),
                }
            })
            .collect(),
    ));
}

/// Fill the hour-detail dialog for one hourly entry.
pub fn apply_hour(app: &crate::AppWindow, h: &Hour, s: &Settings, primary: Rgb, dark: bool) {
    use slint::ComponentHandle;
    let w = app.global::<crate::Weather>();
    let c = Ctx { primary, dark, s };
    let v = visual(h.condition_code, h.is_day);
    w.set_hour_time(time_str(h.time).into());
    w.set_hour_date(local(h.time).format("%A, %b %-d").to_string().into());
    w.set_hour_temp(format!("{}{}", n0(h.temp), c.tu()).into());
    w.set_hour_condition(if h.condition.is_empty() { v.label.into() } else { h.condition.clone().into() });
    w.set_hour_icon(icon(&format!("{}-fill", v.icon)));
    w.set_hour_shape(v.shape.into());
    w.set_hour_tint(c.tint(v.hue));
    w.set_hour_tiles(model(hour_tiles(&c, h)));
}
