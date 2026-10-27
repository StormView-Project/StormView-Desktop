//! Forecast providers (Open-Meteo and the US National Weather Service), normalized
//! into one shape so the UI never has to care which provider produced the data.
//! Ported from StormView's `services/openmeteo.js` and `services/nws.js`.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveTime};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::http::get_json;

const OPEN_METEO: &str = "https://api.open-meteo.com/v1/forecast";
const NWS: &str = "https://api.weather.gov";
const GEOJSON: &str = "application/geo+json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Imperial,
    Metric,
}

impl Units {
    pub fn parse(s: &str) -> Self {
        if s == "metric" { Units::Metric } else { Units::Imperial }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub temp: Option<f64>,
    pub feels_like: Option<f64>,
    pub humidity: Option<f64>,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<f64>,
    pub wind_direction_label: Option<String>,
    pub wind_gusts: Option<f64>,
    pub visibility: Option<f64>,
    pub pressure: Option<f64>,
    pub dew_point: Option<f64>,
    pub cloud_cover: Option<f64>,
    pub uv_index: Option<f64>,
    pub precip_prob: Option<f64>,
    pub precipitation: Option<f64>,
    pub condition: String,
    pub condition_code: i64,
    pub is_day: bool,
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hour {
    pub time: i64,
    pub temp: Option<f64>,
    pub feels_like: Option<f64>,
    pub precip_prob: Option<f64>,
    pub precipitation: Option<f64>,
    pub condition_code: i64,
    pub condition: String,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<f64>,
    pub wind_direction_label: Option<String>,
    pub uv_index: Option<f64>,
    pub visibility: Option<f64>,
    pub humidity: Option<f64>,
    pub pressure: Option<f64>,
    pub is_day: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: i64,
    pub temp_max: Option<f64>,
    pub temp_min: Option<f64>,
    pub feels_max: Option<f64>,
    pub feels_min: Option<f64>,
    pub condition_code: i64,
    pub condition: String,
    pub precip_sum: Option<f64>,
    pub precip_prob: Option<f64>,
    pub wind_speed_max: Option<f64>,
    pub wind_gusts_max: Option<f64>,
    pub uv_index_max: Option<f64>,
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    pub shortwave_radiation: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub headline: String,
    pub description: String,
    pub instruction: String,
    pub severity: String,
    pub certainty: String,
    pub urgency: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceName {
    pub name: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherData {
    pub current: Current,
    pub hourly: Vec<Hour>,
    pub daily: Vec<Day>,
    pub alerts: Vec<Alert>,
    pub source: &'static str,
    pub air_quality: Option<Value>,
    pub nws_fallback: bool,
    pub notice: Option<String>,
    pub location: Option<PlaceName>,
    pub fetched_at: i64,
}

pub fn wmo_description(code: i64) -> &'static str {
    match code {
        0 => "Clear Sky",
        1 => "Mainly Clear",
        2 => "Partly Cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Icy Fog",
        51 => "Light Drizzle",
        53 => "Drizzle",
        55 => "Heavy Drizzle",
        56 => "Freezing Drizzle",
        57 => "Heavy Freezing Drizzle",
        61 => "Light Rain",
        63 => "Rain",
        65 => "Heavy Rain",
        66 => "Freezing Rain",
        67 => "Heavy Freezing Rain",
        71 => "Light Snow",
        73 => "Snow",
        75 => "Heavy Snow",
        77 => "Snow Grains",
        80 => "Light Showers",
        81 => "Showers",
        82 => "Violent Showers",
        85 => "Snow Showers",
        86 => "Heavy Snow Showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm w/ Hail",
        99 => "Severe Thunderstorm",
        _ => "Unknown",
    }
}

pub fn wind_label(deg: f64) -> String {
    const DIRS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
    ];
    DIRS[((deg / 22.5).round() as usize) % 16].to_string()
}

fn round(v: Option<f64>) -> Option<f64> {
    v.map(f64::round)
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn at(v: &Value, key: &str, i: usize) -> Option<f64> {
    v.get(key).and_then(|a| a.get(i)).and_then(Value::as_f64)
}

fn at_i(v: &Value, key: &str, i: usize) -> Option<i64> {
    v.get(key).and_then(|a| a.get(i)).and_then(Value::as_i64)
}

/// Meters → miles or km, one decimal place.
fn visibility(m: Option<f64>, units: Units) -> Option<f64> {
    m.map(|m| match units {
        Units::Imperial => (m / 1609.34 * 10.0).round() / 10.0,
        Units::Metric => (m / 1000.0 * 10.0).round() / 10.0,
    })
}

// ------------------------------------------------------------------ Open-Meteo

pub async fn fetch_open_meteo(client: &Client, lat: f64, lon: f64, units: Units) -> Result<WeatherData> {
    let (t, w, p) = match units {
        Units::Imperial => ("fahrenheit", "mph", "inch"),
        Units::Metric => ("celsius", "kmh", "mm"),
    };
    let current = "temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,cloud_cover,\
pressure_msl,surface_pressure,wind_speed_10m,wind_direction_10m,wind_gusts_10m,visibility,uv_index,dew_point_2m,is_day";
    let hourly = "temperature_2m,apparent_temperature,precipitation_probability,weather_code,wind_speed_10m,\
wind_direction_10m,is_day,uv_index,precipitation,visibility,relative_humidity_2m,surface_pressure";
    let daily = "weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,apparent_temperature_min,\
precipitation_sum,precipitation_probability_max,wind_speed_10m_max,wind_gusts_10m_max,sunrise,sunset,uv_index_max,\
shortwave_radiation_sum";
    let url = format!(
        "{OPEN_METEO}?latitude={lat:.6}&longitude={lon:.6}&temperature_unit={t}&wind_speed_unit={w}\
&precipitation_unit={p}&timezone=auto&timeformat=unixtime&forecast_days=7&current={current}&hourly={hourly}&daily={daily}"
    );
    let d: Value = get_json(client, &url, None).await?;

    let cu = &d["current"];
    let dl = &d["daily"];
    let hr = &d["hourly"];
    let code = cu["weather_code"].as_i64().unwrap_or(-1);
    let wind_dir = num(&cu["wind_direction_10m"]);

    let current = Current {
        temp: round(num(&cu["temperature_2m"])),
        feels_like: round(num(&cu["apparent_temperature"])),
        humidity: num(&cu["relative_humidity_2m"]),
        wind_speed: round(num(&cu["wind_speed_10m"])),
        wind_direction: wind_dir,
        wind_direction_label: wind_dir.map(wind_label),
        wind_gusts: round(num(&cu["wind_gusts_10m"])),
        visibility: visibility(num(&cu["visibility"]), units),
        pressure: round(num(&cu["pressure_msl"])),
        dew_point: round(num(&cu["dew_point_2m"])),
        cloud_cover: num(&cu["cloud_cover"]),
        uv_index: num(&cu["uv_index"]),
        precip_prob: None,
        precipitation: num(&cu["precipitation"]),
        condition: wmo_description(code).to_string(),
        condition_code: code,
        is_day: cu["is_day"].as_i64() == Some(1),
        sunrise: at_i(dl, "sunrise", 0).map(|s| s * 1000),
        sunset: at_i(dl, "sunset", 0).map(|s| s * 1000),
    };

    let hours = hr["time"].as_array().map(Vec::len).unwrap_or(0);
    let hourly = (0..hours)
        .map(|i| {
            let code = at_i(hr, "weather_code", i).unwrap_or(-1);
            let dir = at(hr, "wind_direction_10m", i);
            Hour {
                time: at_i(hr, "time", i).unwrap_or(0) * 1000,
                temp: round(at(hr, "temperature_2m", i)),
                feels_like: round(at(hr, "apparent_temperature", i)),
                precip_prob: Some(at(hr, "precipitation_probability", i).unwrap_or(0.0)),
                precipitation: Some(at(hr, "precipitation", i).unwrap_or(0.0)),
                condition_code: code,
                condition: wmo_description(code).to_string(),
                wind_speed: round(at(hr, "wind_speed_10m", i)),
                wind_direction: dir,
                wind_direction_label: dir.map(wind_label),
                uv_index: at(hr, "uv_index", i),
                visibility: visibility(Some(at(hr, "visibility", i).unwrap_or(0.0)), units),
                humidity: at(hr, "relative_humidity_2m", i),
                pressure: round(at(hr, "surface_pressure", i)),
                is_day: at_i(hr, "is_day", i) == Some(1),
            }
        })
        .collect();

    let days = dl["time"].as_array().map(Vec::len).unwrap_or(0);
    let daily = (0..days)
        .map(|i| {
            let code = at_i(dl, "weather_code", i).unwrap_or(-1);
            Day {
                date: at_i(dl, "time", i).unwrap_or(0) * 1000,
                temp_max: round(at(dl, "temperature_2m_max", i)),
                temp_min: round(at(dl, "temperature_2m_min", i)),
                feels_max: round(at(dl, "apparent_temperature_max", i)),
                feels_min: round(at(dl, "apparent_temperature_min", i)),
                condition_code: code,
                condition: wmo_description(code).to_string(),
                precip_sum: Some(at(dl, "precipitation_sum", i).unwrap_or(0.0)),
                precip_prob: Some(at(dl, "precipitation_probability_max", i).unwrap_or(0.0)),
                wind_speed_max: round(at(dl, "wind_speed_10m_max", i)),
                wind_gusts_max: round(at(dl, "wind_gusts_10m_max", i)),
                uv_index_max: at(dl, "uv_index_max", i),
                sunrise: at_i(dl, "sunrise", i).map(|s| s * 1000),
                sunset: at_i(dl, "sunset", i).map(|s| s * 1000),
                shortwave_radiation: at(dl, "shortwave_radiation_sum", i),
            }
        })
        .collect();

    Ok(WeatherData {
        current,
        hourly,
        daily,
        alerts: vec![],
        source: "openmeteo",
        air_quality: None,
        nws_fallback: false,
        notice: None,
        location: None,
        fetched_at: chrono::Utc::now().timestamp_millis(),
    })
}

// ------------------------------------------------------------------------ NWS

fn parse_ts(v: &Value) -> Option<DateTime<FixedOffset>> {
    v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok())
}

fn ms(v: &Value) -> Option<i64> {
    parse_ts(v).map(|d| d.timestamp_millis())
}

fn convert_temp_c(c: Option<f64>, units: Units) -> Option<f64> {
    c.map(|c| match units {
        Units::Imperial => (c * 9.0 / 5.0 + 32.0).round(),
        Units::Metric => c.round(),
    })
}

fn f_to_units(f: Option<f64>, units: Units) -> Option<f64> {
    f.map(|f| match units {
        Units::Imperial => f.round(),
        Units::Metric => ((f - 32.0) * 5.0 / 9.0).round(),
    })
}

/// NWS observations come as km/h (`wmoUnit:km_h-1`) or m/s depending on station.
fn obs_speed(v: &Value, units: Units) -> Option<f64> {
    let val = v["value"].as_f64()?;
    let unit = v["unitCode"].as_str().unwrap_or("");
    let kmh = if unit.contains("m_s-1") { val * 3.6 } else { val };
    Some(match units {
        Units::Imperial => (kmh / 1.609).round(),
        Units::Metric => kmh.round(),
    })
}

fn to_hpa(pa: Option<f64>) -> Option<f64> {
    pa.map(|p| if p > 2000.0 { (p / 100.0).round() } else { p.round() })
}

/// NWS free-text forecast → WMO-compatible code so icons work the same for both providers.
pub fn short_forecast_to_code(text: &str) -> i64 {
    let t = text.to_lowercase();
    let has = |s: &str| t.contains(s);
    if has("thunder") { 95 }
    else if has("snow") && has("rain") { 61 }
    else if has("blizzard") || has("heavy snow") { 75 }
    else if has("snow") { 71 }
    else if has("ice") || has("sleet") { 77 }
    else if has("freezing rain") || has("freezing drizzle") { 53 }
    else if has("heavy rain") || has("heavy shower") { 82 }
    else if has("rain") || has("showers") { 61 }
    else if has("drizzle") { 51 }
    else if has("fog") || has("mist") || has("haze") || has("smoke") { 45 }
    else if has("mostly cloudy") || has("overcast") || has("cloudy") { 3 }
    else if has("partly cloudy") || has("partly sunny") { 2 }
    else if has("mostly clear") || has("mostly sunny") { 1 }
    else if has("sunny") || has("clear") { 0 }
    else { 2 }
}

pub fn normalize_alert(f: &Value) -> Alert {
    let p = &f["properties"];
    let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
    let start = ms(&p["onset"]).or_else(|| ms(&p["sent"]));
    Alert {
        id: p["id"].as_str().or_else(|| f["id"].as_str()).unwrap_or("").to_string(),
        title: s("event"),
        headline: s("headline"),
        description: s("description"),
        instruction: s("instruction"),
        severity: p["severity"].as_str().unwrap_or("Unknown").to_string(),
        certainty: p["certainty"].as_str().unwrap_or("Unknown").to_string(),
        urgency: p["urgency"].as_str().unwrap_or("Unknown").to_string(),
        start,
        end: ms(&p["expires"]).or_else(|| ms(&p["ends"])),
    }
}

pub fn severity_rank(s: &str) -> usize {
    ["Extreme", "Severe", "Moderate", "Minor", "Unknown"]
        .iter()
        .position(|x| *x == s)
        .unwrap_or(4)
}

/// Active alerts for a single point, most severe first.
pub async fn fetch_point_alerts(client: &Client, lat: f64, lon: f64) -> Result<Vec<Alert>> {
    let url = format!("{NWS}/alerts/active?point={lat:.4},{lon:.4}&status=actual");
    let data: Value = get_json(client, &url, Some(GEOJSON)).await?;
    let mut alerts: Vec<Alert> = data["features"]
        .as_array()
        .map(|a| a.iter().map(normalize_alert).collect())
        .unwrap_or_default();
    alerts.sort_by_key(|a| severity_rank(&a.severity));
    Ok(alerts)
}

pub async fn fetch_nws(client: &Client, lat: f64, lon: f64, units: Units) -> Result<WeatherData> {
    let point: Value = get_json(client, &format!("{NWS}/points/{lat:.4},{lon:.4}"), Some(GEOJSON)).await?;
    let p = &point["properties"];
    let forecast_url = p["forecast"].as_str().ok_or_else(|| Error::msg("NWS has no forecast for this point"))?;
    let hourly_url = p["forecastHourly"].as_str().ok_or_else(|| Error::msg("NWS has no hourly forecast"))?;
    let stations_url = p["observationStations"].as_str().unwrap_or("").to_string();
    let city = p["relativeLocation"]["properties"]["city"].as_str().map(String::from);
    let state = p["relativeLocation"]["properties"]["state"].as_str().map(String::from);

    let obs = async {
        let st: Value = get_json(client, &format!("{stations_url}?limit=1"), Some(GEOJSON)).await?;
        let id = st["features"][0]["properties"]["stationIdentifier"]
            .as_str()
            .ok_or_else(|| Error::msg("no station"))?
            .to_string();
        let o: Value = get_json(client, &format!("{NWS}/stations/{id}/observations/latest"), Some(GEOJSON)).await?;
        Ok::<Value, Error>(o["properties"].clone())
    };
    let (forecast, hourly_fc, alerts, obs) = tokio::join!(
        get_json::<Value>(client, forecast_url, Some(GEOJSON)),
        get_json::<Value>(client, hourly_url, Some(GEOJSON)),
        fetch_point_alerts(client, lat, lon),
        obs,
    );
    let forecast = forecast?;
    let hourly_fc = hourly_fc?;
    let o = obs.unwrap_or(Value::Null);
    let periods = forecast["properties"]["periods"].as_array().cloned().unwrap_or_default();
    let p0 = periods.first().cloned().unwrap_or(Value::Null);

    let text = o["textDescription"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| p0["shortForecast"].as_str())
        .unwrap_or("Unknown")
        .to_string();
    let wind_dir = o["windDirection"]["value"].as_f64();
    let mut current = Current {
        temp: convert_temp_c(o["temperature"]["value"].as_f64(), units),
        feels_like: None,
        humidity: round(o["relativeHumidity"]["value"].as_f64()),
        wind_speed: obs_speed(&o["windSpeed"], units),
        wind_direction: wind_dir,
        wind_direction_label: wind_dir.map(wind_label),
        wind_gusts: obs_speed(&o["windGust"], units),
        visibility: visibility(o["visibility"]["value"].as_f64(), units),
        pressure: to_hpa(o["barometricPressure"]["value"].as_f64().or(o["seaLevelPressure"]["value"].as_f64())),
        dew_point: convert_temp_c(o["dewpoint"]["value"].as_f64(), units),
        cloud_cover: None,
        uv_index: None,
        precip_prob: None,
        precipitation: None,
        condition_code: short_forecast_to_code(&text),
        condition: text,
        is_day: p0["isDaytime"].as_bool().unwrap_or(true),
        sunrise: None,
        sunset: None,
    };
    // Heat index / wind chill are what "feels like" means for NWS observations.
    let feels = o["heatIndex"]["value"].as_f64().or(o["windChill"]["value"].as_f64());
    current.feels_like = convert_temp_c(feels, units);
    if current.temp.is_none() {
        current.temp = f_to_units(p0["temperature"].as_f64(), units);
        if let Some(sf) = p0["shortForecast"].as_str() {
            current.condition = sf.to_string();
            current.condition_code = short_forecast_to_code(sf);
        }
    }
    if current.feels_like.is_none() {
        current.feels_like = current.temp;
    }

    let hourly: Vec<Hour> = hourly_fc["properties"]["periods"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .map(|p| {
                    let raw = p["windSpeed"].as_str().unwrap_or("0");
                    let digits: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                    let n: f64 = digits.parse().unwrap_or(0.0);
                    let mph = if raw.to_lowercase().contains("km") { n / 1.609 } else { n };
                    let sf = p["shortForecast"].as_str().unwrap_or("");
                    Hour {
                        time: ms(&p["startTime"]).unwrap_or(0),
                        temp: f_to_units(p["temperature"].as_f64(), units),
                        feels_like: None,
                        precip_prob: Some(p["probabilityOfPrecipitation"]["value"].as_f64().unwrap_or(0.0)),
                        precipitation: None,
                        condition_code: short_forecast_to_code(sf),
                        condition: sf.to_string(),
                        wind_speed: Some(match units {
                            Units::Imperial => mph.round(),
                            Units::Metric => (mph * 1.609).round(),
                        }),
                        wind_direction: None,
                        wind_direction_label: p["windDirection"].as_str().map(String::from),
                        uv_index: None,
                        visibility: None,
                        humidity: round(p["relativeHumidity"]["value"].as_f64()),
                        pressure: None,
                        is_day: p["isDaytime"].as_bool().unwrap_or(true),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Group the 12-hourly day/night periods into calendar days (in the location's own offset).
    let mut by_day: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    for p in &periods {
        if let Some(dt) = parse_ts(&p["startTime"]) {
            let midnight = dt
                .date_naive()
                .and_time(NaiveTime::MIN)
                .and_local_timezone(*dt.offset())
                .single()
                .map(|d| d.timestamp_millis())
                .unwrap_or(dt.timestamp_millis());
            by_day.entry(midnight).or_default().push(p);
        }
    }
    let daily: Vec<Day> = by_day
        .into_iter()
        .take(7)
        .map(|(date, ps)| {
            let day_p = ps.iter().find(|x| x["isDaytime"].as_bool() == Some(true)).unwrap_or(&ps[0]);
            let night_p = ps.iter().find(|x| x["isDaytime"].as_bool() == Some(false));
            let hi = f_to_units(day_p["temperature"].as_f64(), units);
            let lo = night_p.and_then(|n| f_to_units(n["temperature"].as_f64(), units));
            let sf = day_p["shortForecast"].as_str().unwrap_or("");
            Day {
                date,
                temp_max: hi,
                temp_min: lo.or(hi.map(|h| h - 10.0)),
                condition_code: short_forecast_to_code(sf),
                condition: sf.to_string(),
                precip_prob: Some(day_p["probabilityOfPrecipitation"]["value"].as_f64().unwrap_or(0.0)),
                ..Default::default()
            }
        })
        .collect();

    Ok(WeatherData {
        current,
        hourly,
        daily,
        alerts: alerts.unwrap_or_default(),
        source: "nws",
        air_quality: None,
        nws_fallback: false,
        notice: None,
        location: Some(PlaceName { name: city, state }),
        fetched_at: chrono::Utc::now().timestamp_millis(),
    })
}
