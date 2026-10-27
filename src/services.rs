//! Everything that isn't the core forecast: geocoding, air quality, the seasonal
//! long-range outlook, LibreWXR radar frames, NWS alert polygons/zones, and IP location.

use std::collections::HashMap;

use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::http::get_json;
use crate::weather::Units;

const NOMINATIM: &str = "https://nominatim.openstreetmap.org";
const AIR_QUALITY: &str = "https://air-quality-api.open-meteo.com/v1/air-quality";
const SEASONAL: &str = "https://seasonal-api.open-meteo.com/v1/seasonal";
pub const LIBREWXR_API: &str = "https://api.librewxr.net/public/weather-maps.json";
const LIBREWXR_HOST: &str = "https://api.librewxr.net";
const NWS_ACTIVE: &str = "https://api.weather.gov/alerts/active?status=actual&message_type=alert,update";
const GEOJSON: &str = "application/geo+json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

fn place_name(addr: &Value, fallback: &str) -> String {
    ["city", "town", "village", "hamlet", "municipality", "county"]
        .iter()
        .find_map(|k| addr[*k].as_str())
        .unwrap_or(fallback)
        .to_string()
}

fn place_from_nominatim(r: &Value, lat: f64, lon: f64) -> Place {
    let addr = &r["address"];
    Place {
        lat,
        lon,
        name: place_name(addr, r["name"].as_str().unwrap_or("Unknown")),
        state: addr["state"].as_str().or(addr["region"].as_str()).unwrap_or("").to_string(),
        country: addr["country_code"].as_str().unwrap_or("").to_uppercase(),
        display: r["display_name"].as_str().map(String::from),
    }
}

pub async fn search_location(client: &Client, query: &str) -> Result<Vec<Place>> {
    let q: String = url_encode(query);
    let url = format!("{NOMINATIM}/search?q={q}&format=json&addressdetails=1&limit=6&accept-language=en-US,en");
    let data: Vec<Value> = get_json(client, &url, None).await?;
    Ok(data
        .iter()
        .filter_map(|r| {
            let lat = r["lat"].as_str()?.parse().ok()?;
            let lon = r["lon"].as_str()?.parse().ok()?;
            Some(place_from_nominatim(r, lat, lon))
        })
        .collect())
}

pub async fn reverse_geocode(client: &Client, lat: f64, lon: f64) -> Result<Place> {
    let url = format!("{NOMINATIM}/reverse?lat={lat}&lon={lon}&format=json&addressdetails=1&accept-language=en-US,en");
    let r: Value = get_json(client, &url, None).await?;
    if r.get("error").is_some() {
        return Err(Error::msg("No place found at these coordinates"));
    }
    let mut p = place_from_nominatim(&r, lat, lon);
    p.display = None;
    Ok(p)
}

/// Desktop webviews rarely have a working Geolocation API (WebKitGTK needs GeoClue,
/// WebView2 needs OS permission plumbing), so "use my location" falls back to IP lookup.
pub async fn ip_locate(client: &Client) -> Result<Place> {
    if let Ok(v) = get_json::<Value>(client, "https://ipwho.is/", None).await {
        if v["success"].as_bool() == Some(true) {
            if let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64()) {
                return Ok(Place {
                    lat,
                    lon,
                    name: v["city"].as_str().unwrap_or("My Location").to_string(),
                    state: v["region"].as_str().unwrap_or("").to_string(),
                    country: v["country_code"].as_str().unwrap_or("").to_string(),
                    display: None,
                });
            }
        }
    }
    let v: Value = get_json(client, "https://ipapi.co/json/", None).await?;
    match (v["latitude"].as_f64(), v["longitude"].as_f64()) {
        (Some(lat), Some(lon)) => Ok(Place {
            lat,
            lon,
            name: v["city"].as_str().unwrap_or("My Location").to_string(),
            state: v["region"].as_str().unwrap_or("").to_string(),
            country: v["country_code"].as_str().unwrap_or("").to_string(),
            display: None,
        }),
        _ => Err(Error::msg("Couldn't determine your location from your network.")),
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// --------------------------------------------------------------- Air quality

pub async fn air_quality(client: &Client, lat: f64, lon: f64) -> Result<Value> {
    let url = format!(
        "{AIR_QUALITY}?latitude={lat:.6}&longitude={lon:.6}&timezone=auto\
&current=us_aqi,pm2_5,pm10,ozone,nitrogen_dioxide,sulphur_dioxide,carbon_monoxide"
    );
    let v: Value = get_json(client, &url, None).await?;
    Ok(v["current"].clone())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AqiPoint {
    pub lat: f64,
    pub lon: f64,
    pub aqi: Option<f64>,
}

/// US AQI for many grid points; Open-Meteo accepts comma-separated coordinate lists,
/// chunked to stay under URL length limits.
pub async fn air_quality_batch(client: &Client, points: Vec<LatLon>) -> Result<Vec<AqiPoint>> {
    let chunks: Vec<Vec<LatLon>> = points.chunks(100).map(|c| c.to_vec()).collect();
    let results = stream::iter(chunks)
        .map(|chunk| async move {
            let lats: Vec<String> = chunk.iter().map(|p| format!("{:.4}", p.lat)).collect();
            let lons: Vec<String> = chunk.iter().map(|p| format!("{:.4}", p.lon)).collect();
            let url = format!(
                "{AIR_QUALITY}?latitude={}&longitude={}&current=us_aqi&timezone=auto",
                lats.join(","),
                lons.join(",")
            );
            let v: Value = match get_json(client, &url, None).await {
                Ok(v) => v,
                Err(_) => return vec![],
            };
            let arr = match v {
                Value::Array(a) => a,
                other => vec![other],
            };
            chunk
                .iter()
                .zip(arr.iter())
                .map(|(p, d)| AqiPoint { lat: p.lat, lon: p.lon, aqi: d["current"]["us_aqi"].as_f64() })
                .collect::<Vec<_>>()
        })
        .buffer_unordered(3)
        .collect::<Vec<_>>()
        .await;
    Ok(results.into_iter().flatten().collect())
}

// ------------------------------------------------------ Seasonal (long range)

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookDay {
    pub date: String,
    pub temp_max: Option<f64>,
    pub temp_min: Option<f64>,
    pub temp_mean: Option<f64>,
    pub precip_sum: Option<f64>,
    pub rain_sum: Option<f64>,
    pub showers_sum: Option<f64>,
    pub snowfall_sum: Option<f64>,
    /// "rain" | "showers" | "snow" | "none" — absent when no breakdown exists.
    pub precip_type: Option<&'static str>,
    pub wind_speed_max: Option<f64>,
    pub wind_gusts_max: Option<f64>,
    pub humidity_mean: Option<f64>,
    pub cloud_cover_mean: Option<f64>,
}

fn classify_precip(rain: Option<f64>, showers: Option<f64>, snow: Option<f64>, total: Option<f64>) -> Option<&'static str> {
    if rain.is_none() && showers.is_none() && snow.is_none() && total.is_none() {
        return None;
    }
    let pos = |v: Option<f64>| v.unwrap_or(0.0) > 0.0;
    Some(if pos(snow) {
        "snow"
    } else if pos(showers) {
        "showers"
    } else if pos(rain) || pos(total) {
        "rain"
    } else {
        "none"
    })
}

pub async fn future_range(client: &Client, lat: f64, lon: f64, start: &str, end: &str, units: Units) -> Result<Vec<OutlookDay>> {
    let (t, w, p) = match units {
        Units::Imperial => ("fahrenheit", "mph", "inch"),
        Units::Metric => ("celsius", "kmh", "mm"),
    };
    let vars = "temperature_2m_max,temperature_2m_min,temperature_2m_mean,precipitation_sum,rain_sum,showers_sum,\
snowfall_sum,wind_speed_10m_max,wind_gusts_10m_max,relative_humidity_2m_mean,cloud_cover_mean";
    let url = format!(
        "{SEASONAL}?latitude={lat:.4}&longitude={lon:.4}&daily={vars}&start_date={start}&end_date={end}\
&temperature_unit={t}&wind_speed_unit={w}&precipitation_unit={p}&timezone=auto"
    );
    let v: Value = get_json(client, &url, None).await?;
    let d = &v["daily"];
    let times = d["time"].as_array().cloned().unwrap_or_default();
    let get = |k: &str, i: usize| d[k].get(i).and_then(Value::as_f64);
    let r = |x: Option<f64>| x.map(f64::round);
    let r2 = |x: Option<f64>| x.map(|v| (v * 100.0).round() / 100.0);
    Ok(times
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let rain = get("rain_sum", i);
            let showers = get("showers_sum", i);
            let snow = get("snowfall_sum", i);
            let total = get("precipitation_sum", i);
            OutlookDay {
                date: t.as_str().unwrap_or("").to_string(),
                temp_max: r(get("temperature_2m_max", i)),
                temp_min: r(get("temperature_2m_min", i)),
                temp_mean: r(get("temperature_2m_mean", i)),
                precip_sum: r2(total),
                rain_sum: rain,
                showers_sum: showers,
                snowfall_sum: snow,
                precip_type: classify_precip(rain, showers, snow, total),
                wind_speed_max: r(get("wind_speed_10m_max", i)),
                wind_gusts_max: r(get("wind_gusts_10m_max", i)),
                humidity_mean: r(get("relative_humidity_2m_mean", i)),
                cloud_cover_mean: r(get("cloud_cover_mean", i)),
            }
        })
        // Dates at the edge of the model's range come back present but empty.
        .filter(|d| d.temp_max.is_some() || d.temp_min.is_some() || d.precip_sum.is_some())
        .collect())
}

// --------------------------------------------------------------- LibreWXR

#[derive(Debug, Clone, Serialize)]
pub struct RadarFrame {
    pub time: i64,
    pub path: String,
    pub host: &'static str,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// LibreWXR mirrors the RainViewer v2 API. Its `host` field often points at a LAN
/// address, so tiles are always fetched from the public API host instead.
pub async fn radar_frames(client: &Client) -> Result<Vec<RadarFrame>> {
    let v: Value = get_json(client, LIBREWXR_API, None).await?;
    let mut frames = Vec::new();
    for (key, kind) in [("past", "past"), ("nowcast", "nowcast")] {
        if let Some(list) = v["radar"][key].as_array() {
            for f in list {
                if let (Some(time), Some(path)) = (f["time"].as_i64(), f["path"].as_str()) {
                    frames.push(RadarFrame { time, path: path.to_string(), host: LIBREWXR_HOST, kind });
                }
            }
        }
    }
    frames.sort_by_key(|f| f.time);
    Ok(frames)
}

// ------------------------------------------------------- NWS alert geometry

/// The national active-alert feed (GeoJSON FeatureCollection), passed through as-is.
pub async fn active_alerts_geojson(client: &Client) -> Result<Value> {
    get_json(client, NWS_ACTIVE, Some(GEOJSON)).await
}

/// Zone boundaries for zone-coded advisories. Zones are static admin boundaries,
/// so results (including misses) are cached for the life of the process.
pub async fn zone_geometries(
    client: &Client,
    cache: &tokio::sync::Mutex<HashMap<String, Option<Value>>>,
    ids: Vec<String>,
) -> Result<HashMap<String, Value>> {
    let mut out = HashMap::new();
    let mut to_fetch = Vec::new();
    {
        let c = cache.lock().await;
        for id in ids {
            match c.get(&id) {
                Some(Some(g)) => {
                    out.insert(id, g.clone());
                }
                Some(None) => {}
                None => to_fetch.push(id),
            }
        }
    }
    let fetched: Vec<(String, Option<Value>)> = stream::iter(to_fetch)
        .map(|id| async move {
            // UGC format: SSZ000 (forecast/fire zone) or SSC000 (county).
            let kind = if id.as_bytes().get(2) == Some(&b'C') { "county" } else { "forecast" };
            let url = format!("https://api.weather.gov/zones/{kind}/{id}");
            let geo = get_json::<Value>(client, &url, Some(GEOJSON))
                .await
                .ok()
                .map(|v| v["geometry"].clone())
                .filter(|g| !g.is_null());
            (id, geo)
        })
        .buffer_unordered(6)
        .collect()
        .await;
    let mut c = cache.lock().await;
    for (id, geo) in fetched {
        if let Some(g) = &geo {
            out.insert(id.clone(), g.clone());
        }
        c.insert(id, geo);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Live-network checks; run with `cargo test -- --ignored`.
    use super::*;
    use crate::weather::{fetch_nws, fetch_open_meteo, fetch_point_alerts};

    #[tokio::test]
    #[ignore]
    async fn providers_and_services() {
        let c = crate::http::client();
        let nws = fetch_nws(&c, 41.88, -87.63, Units::Imperial).await.expect("nws");
        assert!(nws.current.temp.is_some() && !nws.hourly.is_empty() && !nws.daily.is_empty());
        println!("NWS: {:?}°F {} | {} hours, {} days", nws.current.temp, nws.current.condition, nws.hourly.len(), nws.daily.len());
        let om = fetch_open_meteo(&c, 48.85, 2.35, Units::Metric).await.expect("open-meteo");
        println!("Open-Meteo Paris: {:?}°C, {} days", om.current.temp, om.daily.len());
        assert!(fetch_nws(&c, 48.85, 2.35, Units::Metric).await.is_err(), "NWS must fail outside the US");
        let alerts = fetch_point_alerts(&c, 41.88, -87.63).await.expect("alerts");
        println!("point alerts: {}", alerts.len());
        let places = search_location(&c, "Denver").await.expect("search");
        println!("search: {:?}", places.first().map(|p| (&p.name, &p.state)));
        let rev = reverse_geocode(&c, 42.33, -88.27).await.expect("reverse");
        println!("reverse: {} {}", rev.name, rev.state);
        let batch = air_quality_batch(&c, vec![LatLon { lat: 40.0, lon: -100.0 }, LatLon { lat: 34.0, lon: -118.0 }]).await.unwrap();
        println!("aqi batch: {:?}", batch);
        let start = (chrono::Local::now() + chrono::Duration::days(30)).format("%Y-%m-%d").to_string();
        let out = future_range(&c, 42.33, -88.27, &start, &start, Units::Imperial).await.expect("seasonal");
        println!("seasonal {start}: {:?}", out.first());
        let frames = radar_frames(&c).await.expect("radar");
        println!("radar frames: {}", frames.len());
        let all = active_alerts_geojson(&c).await.expect("national");
        let ugc: Vec<String> = all["features"].as_array().unwrap().iter()
            .filter(|f| f["geometry"].is_null())
            .filter_map(|f| f["properties"]["geocode"]["UGC"][0].as_str().map(String::from)).take(3).collect();
        let cache = tokio::sync::Mutex::new(HashMap::new());
        let geos = zone_geometries(&c, &cache, ugc.clone()).await.unwrap();
        println!("national features: {}, zones {:?} → {} geometries", all["features"].as_array().unwrap().len(), ugc, geos.len());
    }
}
