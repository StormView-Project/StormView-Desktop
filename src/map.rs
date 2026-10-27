//! Native slippy-map engine: Web-Mercator tile math, async tile loading,
//! Material You recoloring of the base map, LibreWXR radar frames, NWS alert
//! polygons rasterized per tile, and the AQI overlay.
//!
//! Tiles are laid out in an "anchor space": the Slint side draws every layer in a
//! container offset by (anchor - current view origin), so dragging the map only
//! changes two properties. Tile models are rebuilt only when the view leaves the
//! pre-built margin or the zoom changes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::Arc;

use lru::LruCache;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::services::RadarFrame;
use crate::{AppWindow, MapBridge, MapTile};

pub const TILE: f64 = 256.0;
const MAX_ZOOM: f64 = 16.0;
const MIN_ZOOM: f64 = 2.5;
const RADAR_MAX_Z: i32 = 10;
const PRELOAD_AHEAD: i32 = 4;
const KEEP_BEHIND: i32 = 1;
/// Base, label and alert tiles (256 px, ~256 KB each).
const BASE_CACHE: usize = 900;

/* ------------------------------------------------------------ projection */

pub fn lon_to_x(lon: f64) -> f64 {
    (lon + 180.0) / 360.0
}
pub fn lat_to_y(lat: f64) -> f64 {
    let r = lat.clamp(-85.05, 85.05).to_radians();
    (1.0 - (r.tan() + 1.0 / r.cos()).ln() / PI) / 2.0
}
pub fn x_to_lon(x: f64) -> f64 {
    x * 360.0 - 180.0
}
pub fn y_to_lat(y: f64) -> f64 {
    (PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees()
}

/* ------------------------------------------------------------ palette */

/// Target colors for recoloring Esri's Light Gray Canvas into the current M3 scheme.
#[derive(Clone, Copy, PartialEq)]
pub struct MapPalette {
    pub land: [u8; 3],
    pub water: [u8; 3],
    pub road: [u8; 3],
    pub line: [u8; 3],
    pub label: [u8; 3],
    pub halo: [u8; 3],
    pub pin: [u8; 3],
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8)
}

impl MapPalette {
    pub fn from_scheme(s: &material_colors::scheme::Scheme, dark: bool) -> Self {
        let c = |r: material_colors::color::Rgb| [r.red, r.green, r.blue];
        let land = if dark { c(s.surface_container_low) } else { c(s.surface_container) };
        let water = if dark {
            mix(c(s.surface_container_lowest), c(s.primary_container), 0.45)
        } else {
            mix(c(s.surface_container_low), c(s.primary_container), 0.75)
        };
        MapPalette {
            land,
            water,
            road: if dark { c(s.surface_container_highest) } else { c(s.surface_bright) },
            line: c(s.outline),
            label: c(s.on_surface),
            halo: land,
            pin: c(s.primary),
        }
    }
}

/// Esri gray base: water ≈ #d0cfd4, land ≈ #e9e9e9, roads lighter, boundaries darker.
fn recolor_base(px: &mut [u8], p: &MapPalette) {
    for c in px.chunks_exact_mut(4) {
        let l = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0;
        let out = if l >= 0.915 {
            mix(p.land, p.road, ((l - 0.915) / 0.085).min(1.0))
        } else if l >= 0.80 {
            mix(p.water, p.land, (l - 0.80) / 0.115)
        } else {
            mix(p.line, p.water, ((l - 0.55) / 0.25).clamp(0.0, 1.0))
        };
        c[0] = out[0];
        c[1] = out[1];
        c[2] = out[2];
        c[3] = 255;
    }
}

/// Esri reference labels: dark text with light halos on transparency.
fn recolor_labels(px: &mut [u8], p: &MapPalette) {
    for c in px.chunks_exact_mut(4) {
        if c[3] == 0 {
            continue;
        }
        let l = (c[0] as f32 + c[1] as f32 + c[2] as f32) / 765.0;
        let out = mix(p.label, p.halo, l);
        c[0] = out[0];
        c[1] = out[1];
        c[2] = out[2];
        if l > 0.5 {
            c[3] = (c[3] as f32 * 0.85) as u8;
        }
    }
}

/* ------------------------------------------------------------ alerts */

pub struct AlertShape {
    pub rings: Vec<Vec<(f64, f64)>>, // normalized mercator
    pub bbox: (f64, f64, f64, f64),
    pub color: u32,
    pub fill_alpha: f32,
    pub dashed: bool,
    pub props: serde_json::Value,
}

pub struct AlertData {
    pub shapes: Vec<AlertShape>,
    pub version: u64,
}

fn rings_of(geom: &serde_json::Value) -> Vec<Vec<(f64, f64)>> {
    let to_ring = |r: &serde_json::Value| -> Vec<(f64, f64)> {
        r.as_array()
            .map(|pts| {
                pts.iter()
                    .filter_map(|p| Some((lon_to_x(p[0].as_f64()?), lat_to_y(p[1].as_f64()?))))
                    .collect()
            })
            .unwrap_or_default()
    };
    match geom["type"].as_str() {
        Some("Polygon") => geom["coordinates"].as_array().map(|rs| rs.iter().map(to_ring).collect()).unwrap_or_default(),
        Some("MultiPolygon") => geom["coordinates"]
            .as_array()
            .map(|ps| ps.iter().flat_map(|p| p.as_array().cloned().unwrap_or_default()).map(|r| to_ring(&r)).collect())
            .unwrap_or_default(),
        _ => vec![],
    }
}

pub fn build_shape(geom: &serde_json::Value, props: serde_json::Value, color: u32, fill_alpha: f32, dashed: bool) -> Option<AlertShape> {
    let rings = rings_of(geom);
    if rings.is_empty() {
        return None;
    }
    let mut bb = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for r in &rings {
        for &(x, y) in r {
            bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
        }
    }
    Some(AlertShape { rings, bbox: bb, color, fill_alpha, dashed, props })
}

fn point_in_shape(s: &AlertShape, x: f64, y: f64) -> bool {
    if x < s.bbox.0 || x > s.bbox.2 || y < s.bbox.1 || y > s.bbox.3 {
        return false;
    }
    let mut inside = false;
    for ring in &s.rings {
        let n = ring.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

fn render_alert_tile(data: &AlertData, z: i32, tx: i64, ty: i64) -> Option<Vec<u8>> {
    let n = (1i64 << z) as f64;
    let (x0, y0) = (tx as f64 / n, ty as f64 / n);
    let (x1, y1) = ((tx + 1) as f64 / n, (ty + 1) as f64 / n);
    let scale = n * TILE;
    let mut pm = tiny_skia::Pixmap::new(TILE as u32, TILE as u32)?;
    let mut any = false;
    for s in &data.shapes {
        if s.bbox.2 < x0 || s.bbox.0 > x1 || s.bbox.3 < y0 || s.bbox.1 > y1 {
            continue;
        }
        let mut pb = tiny_skia::PathBuilder::new();
        for ring in &s.rings {
            for (i, &(x, y)) in ring.iter().enumerate() {
                let (px, py) = (((x - x0) * scale) as f32, ((y - y0) * scale) as f32);
                if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
            }
            pb.close();
        }
        let Some(path) = pb.finish() else { continue };
        any = true;
        let (r, g, b) = ((s.color >> 16) as u8, (s.color >> 8) as u8, s.color as u8);
        let mut paint = tiny_skia::Paint::default();
        paint.anti_alias = true;
        paint.set_color_rgba8(r, g, b, (s.fill_alpha * 255.0) as u8);
        pm.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, tiny_skia::Transform::identity(), None);
        paint.set_color_rgba8(r, g, b, if s.dashed { 180 } else { 220 });
        let mut stroke = tiny_skia::Stroke { width: if s.dashed { 1.5 } else { 2.0 }, ..Default::default() };
        if s.dashed {
            stroke.dash = tiny_skia::StrokeDash::new(vec![5.0, 4.0], 0.0);
        }
        pm.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
    }
    if !any {
        return None;
    }
    // tiny-skia stores premultiplied RGBA; Slint wants straight alpha.
    let mut px = pm.take();
    for c in px.chunks_exact_mut(4) {
        let a = c[3] as u32;
        if a > 0 && a < 255 {
            for v in &mut c[..3] {
                *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(px)
}

/* ------------------------------------------------------------ AQI overlay */

const AQI_RAMP: [(f64, [f64; 3]); 7] = [
    (0.0, [0.0, 228.0, 0.0]),
    (50.0, [0.0, 228.0, 0.0]),
    (100.0, [255.0, 255.0, 0.0]),
    (150.0, [255.0, 126.0, 0.0]),
    (200.0, [255.0, 0.0, 0.0]),
    (300.0, [143.0, 63.0, 151.0]),
    (500.0, [126.0, 0.0, 35.0]),
];

fn aqi_rgb(aqi: f64) -> [u8; 3] {
    let v = aqi.clamp(0.0, 500.0);
    for w in AQI_RAMP.windows(2) {
        let ((lv, lc), (hv, hc)) = (w[0], w[1]);
        if v <= hv {
            let t = (v - lv) / (hv - lv).max(1e-9);
            return [0, 1, 2].map(|i| (lc[i] + t * (hc[i] - lc[i])).round() as u8);
        }
    }
    [126, 0, 35]
}

/// Inverse-distance-weighted AQI grid over the bounds, box-blurred, as RGBA.
pub fn render_aqi(points: &[(f64, f64, f64)], west: f64, north: f64, east: f64, south: f64) -> Vec<u8> {
    const SZ: usize = 160;
    let mut grid = vec![[0u8; 3]; SZ * SZ];
    for row in 0..SZ {
        let lat = north - (row as f64 / SZ as f64) * (north - south);
        for col in 0..SZ {
            let lon = west + (col as f64 / SZ as f64) * (east - west);
            let (mut ws, mut vs, mut exact) = (0.0, 0.0, None);
            for &(plat, plon, aqi) in points {
                let d2 = (lat - plat).powi(2) + (lon - plon).powi(2);
                if d2 < 1e-8 {
                    exact = Some(aqi);
                    break;
                }
                let w = 1.0 / d2.powf(1.5);
                ws += w;
                vs += w * aqi;
            }
            grid[row * SZ + col] = aqi_rgb(exact.unwrap_or(if ws > 0.0 { vs / ws } else { 0.0 }));
        }
    }
    let mut out = vec![0u8; SZ * SZ * 4];
    let r = 3i32;
    for row in 0..SZ as i32 {
        for col in 0..SZ as i32 {
            let mut acc = [0u32; 3];
            let mut n = 0;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (yy, xx) = (row + dy, col + dx);
                    if yy >= 0 && xx >= 0 && yy < SZ as i32 && xx < SZ as i32 {
                        let c = grid[yy as usize * SZ + xx as usize];
                        for i in 0..3 {
                            acc[i] += c[i] as u32;
                        }
                        n += 1;
                    }
                }
            }
            let o = (row as usize * SZ + col as usize) * 4;
            for i in 0..3 {
                out[o + i] = (acc[i] / n) as u8;
            }
            out[o + 3] = 170;
        }
    }
    out
}

/* ------------------------------------------------------------ engine */

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Base,
    Satellite,
    Labels,
    Radar,
}

pub struct RadarParams {
    pub scheme: u32,
    pub quality: u32,
    pub smooth: bool,
    pub snow: bool,
    pub arrows: Option<&'static str>,
}

/// Lives on the UI thread; network and pixel work happen on the tokio runtime.
pub struct Engine {
    app: slint::Weak<AppWindow>,
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    pub cx: f64,
    pub cy: f64,
    pub zoom: f64,
    pub w: f64,
    pub h: f64,
    images: LruCache<String, Image>,
    /// Radar tiles; resized so the visible tiles × the preload window always fit.
    radar_images: LruCache<String, Image>,
    pending: HashSet<String>,
    failed: HashSet<String>,
    pub palette: Option<MapPalette>,
    theme_gen: u64,
    pub satellite: bool,
    pub frames: Vec<RadarFrame>,
    pub frame_idx: usize,
    pub radar_on: bool,
    pub radar: RadarParams,
    last_radar: HashMap<(i32, i64, i64), Image>,
    pub alerts: Option<Arc<AlertData>>,
    built: Option<(f64, f64, f64, f64, f64, f64, f64)>, // anchor x/y, covered left/top/right/bottom (anchor-relative), zoom
    models: [Rc<VecModel<MapTile>>; 4],
    pub location: Option<(f64, f64)>,
    pub aqi_image: Option<(Image, f64, f64, f64, f64)>, // world-norm bounds x0,y0,x1,y1
    zoom_target: Option<(f64, f64, f64)>, // target zoom, focus sx, sy
    anim: Option<slint::Timer>,
    dirty_timer: Option<slint::Timer>,
}

thread_local! {
    pub static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// Run `f` against the engine. Engine methods never call back into UI code
/// synchronously, so a failed borrow means a bug — skip rather than crash.
pub fn with<R>(f: impl FnOnce(&mut Engine) -> R) -> Option<R> {
    ENGINE.with(|e| match e.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => {
            eprintln!("[map] re-entrant engine access skipped");
            None
        }
    })
}

impl Engine {
    pub fn install(app: &AppWindow, rt: tokio::runtime::Handle, http: reqwest::Client, lat: f64, lon: f64, zoom: f64) {
        let models: [Rc<VecModel<MapTile>>; 4] = std::array::from_fn(|_| Rc::new(VecModel::default()));
        let bridge = app.global::<MapBridge>();
        bridge.set_base_tiles(ModelRc::from(models[0].clone()));
        bridge.set_radar_tiles(ModelRc::from(models[1].clone()));
        bridge.set_alert_tiles(ModelRc::from(models[2].clone()));
        bridge.set_label_tiles(ModelRc::from(models[3].clone()));
        let e = Engine {
            app: app.as_weak(),
            rt,
            http,
            cx: lon_to_x(lon),
            cy: lat_to_y(lat),
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            w: 1200.0,
            h: 800.0,
            images: LruCache::new(NonZeroUsize::new(BASE_CACHE).unwrap()),
            radar_images: LruCache::new(NonZeroUsize::new(160).unwrap()),
            pending: HashSet::new(),
            failed: HashSet::new(),
            palette: None,
            theme_gen: 0,
            satellite: false,
            frames: vec![],
            frame_idx: 0,
            radar_on: true,
            radar: RadarParams { scheme: 7, quality: 512, smooth: true, snow: true, arrows: None },
            last_radar: HashMap::new(),
            alerts: None,
            built: None,
            models,
            location: None,
            aqi_image: None,
            zoom_target: None,
            anim: None,
            dirty_timer: None,
        };
        ENGINE.with(|slot| *slot.borrow_mut() = Some(e));
    }

    fn scale(&self) -> f64 {
        TILE * 2f64.powf(self.zoom)
    }

    /// World pixel (at the current zoom) of the view's top-left corner.
    fn origin(&self) -> (f64, f64) {
        let s = self.scale();
        (self.cx * s - self.w / 2.0, self.cy * s - self.h / 2.0)
    }

    pub fn screen_to_norm(&self, sx: f64, sy: f64) -> (f64, f64) {
        let (ox, oy) = self.origin();
        let s = self.scale();
        ((ox + sx) / s, (oy + sy) / s)
    }

    pub fn lonlat_at(&self, sx: f64, sy: f64) -> (f64, f64) {
        let (x, y) = self.screen_to_norm(sx, sy);
        (x_to_lon(x.rem_euclid(1.0)), y_to_lat(y))
    }

    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let (w, n) = self.lonlat_at(0.0, 0.0);
        let (e, s) = self.lonlat_at(self.w, self.h);
        (w, n, if e < w { e + 360.0 } else { e }, s)
    }

    pub fn resize(&mut self, w: f64, h: f64) {
        if (w - self.w).abs() > 0.5 || (h - self.h).abs() > 0.5 {
            self.w = w.max(1.0);
            self.h = h.max(1.0);
            self.built = None;
            self.render();
        }
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        let s = self.scale();
        self.cx = (self.cx - dx / s).rem_euclid(1.0);
        self.cy = (self.cy - dy / s).clamp(0.0, 1.0);
        self.zoom_target = None;
        self.update_view();
    }

    pub fn fly_to(&mut self, lat: f64, lon: f64, zoom: Option<f64>) {
        self.cx = lon_to_x(lon);
        self.cy = lat_to_y(lat);
        if let Some(z) = zoom {
            self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        }
        self.built = None;
        self.render();
    }

    /// Smoothly zoom by `delta` levels keeping the point under (sx, sy) fixed.
    pub fn zoom_by(&mut self, delta: f64, sx: f64, sy: f64) {
        let base = self.zoom_target.map(|t| t.0).unwrap_or(self.zoom);
        let target = (base + delta).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom_target = Some((target, sx, sy));
        if self.anim.is_none() {
            let timer = slint::Timer::default();
            timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(16), || {
                with(|e| e.step_zoom());
            });
            self.anim = Some(timer);
        }
    }

    fn step_zoom(&mut self) {
        let Some((target, sx, sy)) = self.zoom_target else {
            self.anim = None;
            return;
        };
        let (wx, wy) = self.screen_to_norm(sx, sy);
        let diff = target - self.zoom;
        let next = if diff.abs() < 0.01 { target } else { self.zoom + diff * 0.28 };
        self.zoom = next;
        // Re-center so the world point under the cursor stays put.
        let s = self.scale();
        self.cx = wx - (sx - self.w / 2.0) / s;
        self.cy = (wy - (sy - self.h / 2.0) / s).clamp(0.0, 1.0);
        if next == target {
            self.zoom_target = None;
            self.anim = None;
            // Notify after this borrow of the engine ends.
            let (app, z) = (self.app.clone(), self.zoom as f32);
            slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                if let Some(app) = app.upgrade() {
                    app.invoke_zoom_settled(z);
                }
            });
        }
        self.built = None;
        self.render();
    }

    /// Cheap path for panning: move the container; rebuild only if we left the margin.
    fn update_view(&mut self) {
        let (ox, oy) = self.origin();
        if let Some((ax, ay, l, t, r, b, z)) = self.built {
            let (vx, vy) = (ox - ax, oy - ay);
            let inside = (z - self.zoom).abs() < 1e-9 && vx >= l && vy >= t && vx + self.w <= r && vy + self.h <= b;
            if inside {
                self.apply_offset();
                return;
            }
        }
        self.render();
    }

    fn apply_offset(&self) {
        let Some(app) = self.app.upgrade() else { return };
        let (ax, ay, ..) = self.built.unwrap_or_default();
        let (ox, oy) = self.origin();
        let b = app.global::<MapBridge>();
        // Whole pixels, so tile edges never land between pixels.
        b.set_offset_x((ax - ox).round() as f32);
        b.set_offset_y((ay - oy).round() as f32);
        let s = self.scale();
        if let Some((lx, ly)) = self.location {
            b.set_pin_x((lx * s - ax) as f32);
            b.set_pin_y((ly * s - ay) as f32);
            b.set_has_pin(true);
        } else {
            b.set_has_pin(false);
        }
        if let Some((img, x0, y0, x1, y1)) = &self.aqi_image {
            b.set_aqi_image(img.clone());
            b.set_aqi_x((x0 * s - ax) as f32);
            b.set_aqi_y((y0 * s - ay) as f32);
            b.set_aqi_w(((x1 - x0) * s) as f32);
            b.set_aqi_h(((y1 - y0) * s) as f32);
            b.set_has_aqi(true);
        } else {
            b.set_has_aqi(false);
        }
    }

    /* ------------------------------------------------------- tile urls */

    fn base_key(&self, kind: Kind, z: i32, x: i64, y: i64) -> String {
        match kind {
            Kind::Satellite => format!("sat/{z}/{x}/{y}"),
            Kind::Base => format!("base{}/{z}/{x}/{y}", self.theme_gen),
            _ => format!("lbl{}/{}/{z}/{x}/{y}", self.theme_gen, self.satellite),
        }
    }

    fn url(kind: Kind, z: i32, x: i64, y: i64) -> String {
        let svc = match kind {
            Kind::Satellite => return format!("https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"),
            Kind::Base => "Canvas/World_Light_Gray_Base",
            _ => "Canvas/World_Light_Gray_Reference",
        };
        format!("https://server.arcgisonline.com/ArcGIS/rest/services/{svc}/MapServer/tile/{z}/{y}/{x}")
    }

    fn radar_key(&self, f: &RadarFrame, z: i32, x: i64, y: i64) -> String {
        let r = &self.radar;
        format!("radar/{}/{}/{}/{}{}/{}/{z}/{x}/{y}", f.path, r.scheme, r.quality, r.smooth as u8, r.snow as u8, r.arrows.unwrap_or("-"))
    }

    fn radar_url(&self, f: &RadarFrame, z: i32, x: i64, y: i64) -> String {
        let r = &self.radar;
        let base = format!("{}{}/{}/{z}/{x}/{y}/{}/{}_{}.png", f.host, f.path, r.quality, r.scheme, r.smooth as u8, r.snow as u8);
        match r.arrows {
            Some(a) => format!("{base}?arrows={a}"),
            None => base,
        }
    }

    /* ------------------------------------------------------- loading */

    fn cached(&self, key: &str) -> bool {
        if key.starts_with("radar/") { self.radar_images.contains(key) } else { self.images.contains(key) }
    }

    fn get(&mut self, key: &str) -> Option<Image> {
        if key.starts_with("radar/") { self.radar_images.get(key).cloned() } else { self.images.get(key).cloned() }
    }

    fn request(&mut self, key: String, url: String, kind: Option<Kind>) {
        if self.cached(&key) || self.pending.contains(&key) || self.failed.contains(&key) {
            return;
        }
        self.pending.insert(key.clone());
        let http = self.http.clone();
        let palette = self.palette;
        self.rt.spawn(async move {
            let bytes = async {
                let r = http.get(&url).send().await.ok()?;
                if !r.status().is_success() {
                    return None;
                }
                r.bytes().await.ok()
            }
            .await;
            let decoded = bytes.and_then(|b| image::load_from_memory(&b).ok()).map(|img| {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let mut px = rgba.into_raw();
                if let Some(p) = palette.as_ref() {
                    match kind {
                        Some(Kind::Base) => recolor_base(&mut px, p),
                        Some(Kind::Labels) => recolor_labels(&mut px, p),
                        _ => {}
                    }
                }
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&px, w, h)
            });
            let _ = slint::invoke_from_event_loop(move || {
                with(|e| e.loaded(key, decoded));
            });
        });
    }

    fn loaded(&mut self, key: String, buf: Option<SharedPixelBuffer<Rgba8Pixel>>) {
        self.pending.remove(&key);
        match buf {
            Some(b) => {
                let img = Image::from_rgba8(b);
                if key.starts_with("radar/") { self.radar_images.put(key, img) } else { self.images.put(key, img) };
            }
            None => {
                self.failed.insert(key);
            }
        }
        self.schedule_render();
    }

    /// Coalesce bursts of tile arrivals into one re-render.
    fn schedule_render(&mut self) {
        if self.dirty_timer.is_some() {
            return;
        }
        let t = slint::Timer::default();
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(30), || {
            with(|e| {
                e.dirty_timer = None;
                e.render();
            });
        });
        self.dirty_timer = Some(t);
    }

    /* ------------------------------------------------------- rendering */

    /// Visible tile range at tile zoom `z` with a one-tile margin, in tile indices.
    fn range(&self, z: i32, display_tile: f64) -> (i64, i64, i64, i64, f64) {
        let (ox, oy) = self.origin();
        let s = display_tile; // on-screen size of one tile at zoom z
        let x0 = ((ox - TILE) / s).floor() as i64;
        let y0 = (((oy - TILE) / s).floor() as i64).max(0);
        let x1 = ((ox + self.w + TILE) / s).ceil() as i64;
        let y1 = (((oy + self.h + TILE) / s).ceil() as i64).min(1 << z);
        (x0, y0, x1, y1, s)
    }

    pub fn render(&mut self) {
        let Some(app) = self.app.upgrade() else { return };
        let (ox, oy) = self.origin();
        let anchor = (ox.floor(), oy.floor());
        let world = self.scale();

        // Base + labels at z = round(zoom), 256 px tiles.
        let bz = (self.zoom.round() as i32).clamp(0, 16);
        let bsize = world / 2f64.powi(bz);
        let (x0, y0, x1, y1, _) = self.range(bz, bsize);
        let mut base = Vec::new();
        let mut labels = Vec::new();
        let mut alerts = Vec::new();
        let n = 1i64 << bz;
        let base_kind = if self.satellite { Kind::Satellite } else { Kind::Base };
        let mut wanted: Vec<(f64, String, String, Kind)> = Vec::new();
        let (ccx, ccy) = (self.cx * n as f64, self.cy * n as f64);
        for ty in y0..y1 {
            for tx in x0..x1 {
                let wx = tx.rem_euclid(n);
                let px = tx as f64 * bsize - anchor.0;
                let py = ty as f64 * bsize - anchor.1;
                let dist = (tx as f64 + 0.5 - ccx).powi(2) + (ty as f64 + 0.5 - ccy).powi(2);
                for kind in [base_kind, Kind::Labels] {
                    let key = self.base_key(kind, bz, wx, ty);
                    let out = if kind == Kind::Labels { &mut labels } else { &mut base };
                    let found = self.tile_or_fallback(&|e: &Engine, z, x, y| e.base_key(kind, z, x, y), bz, 16, wx, ty, px, py, bsize, 1.0);
                    out.extend(found);
                    if !self.images.contains(&key) && (kind != Kind::Base || self.palette.is_some()) {
                        wanted.push((dist, key, Self::url(kind, bz, wx, ty), kind));
                    }
                }
                if let Some(data) = self.alerts.clone() {
                    let key = format!("alert{}/{}/{bz}/{wx}/{ty}", data.version, self.theme_gen);
                    if let Some(img) = self.images.get(&key) {
                        alerts.push(tile(img.clone(), px, py, bsize, 1.0));
                    } else if !self.failed.contains(&key) && !self.pending.contains(&key) {
                        self.render_alert(key, data, bz, wx, ty);
                    }
                }
            }
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, key, url, kind) in wanted {
            self.request(key, url, Some(kind));
        }

        // Radar at z = round(zoom) - 1 with 512 px display tiles (4× fewer requests).
        let mut radar = Vec::new();
        let mut progress = (0usize, 0usize);
        if self.radar_on && !self.frames.is_empty() {
            let rz = (self.zoom.round() as i32 - 1).clamp(0, RADAR_MAX_Z);
            let rsize = world / 2f64.powi(rz);
            let (rx0, ry0, rx1, ry1, _) = self.range(rz, rsize);
            // Room for every visible tile across the whole preload window, twice over,
            // so preloading never evicts a tile that is on screen (that caused flashing).
            let visible = ((rx1 - rx0) * (ry1 - ry0)).max(1) as usize;
            let need = visible * (PRELOAD_AHEAD + KEEP_BEHIND + 1) as usize * 2;
            if self.radar_images.cap().get() < need {
                self.radar_images.resize(NonZeroUsize::new(need).unwrap());
            }
            let rn = 1i64 << rz;
            let nframes = self.frames.len() as i32;
            let cur = self.frames[self.frame_idx.min(self.frames.len() - 1)].clone();
            for ty in ry0..ry1 {
                for tx in rx0..rx1 {
                    let wx = tx.rem_euclid(rn);
                    let px = tx as f64 * rsize - anchor.0;
                    let py = ty as f64 * rsize - anchor.1;
                    let key = self.radar_key(&cur, rz, wx, ty);
                    let slot = (rz, wx, ty);
                    if let Some(img) = self.radar_images.get(&key).cloned() {
                        self.last_radar.insert(slot, img.clone());
                        radar.push(tile(img, px, py, rsize, 1.0));
                    } else if let Some(prev) = self.last_radar.get(&slot) {
                        // Keep the previous frame on screen until this one arrives — no flicker.
                        radar.push(tile(prev.clone(), px, py, rsize, 1.0));
                    } else {
                        // New zoom level: stretch the frame from the neighbouring zoom meanwhile.
                        let f = cur.clone();
                        radar.extend(self.tile_or_fallback(&|e: &Engine, z, x, y| e.radar_key(&f, z, x, y), rz, RADAR_MAX_Z, wx, ty, px, py, rsize, 1.0));
                    }
                    // Preload the playback window around the current frame.
                    for j in -KEEP_BEHIND..=PRELOAD_AHEAD {
                        let fi = (self.frame_idx as i32 + j).rem_euclid(nframes) as usize;
                        let f = self.frames[fi].clone();
                        let k = self.radar_key(&f, rz, wx, ty);
                        progress.1 += 1;
                        if self.radar_images.contains(&k) || self.failed.contains(&k) {
                            progress.0 += 1;
                        } else if j == 0 || self.pending.len() < 48 {
                            let url = self.radar_url(&f, rz, wx, ty);
                            self.request(k, url, Some(Kind::Radar));
                        }
                    }
                }
            }
            self.last_radar.retain(|k, _| k.0 == rz);
        }

        sync(&self.models[0], base);
        sync(&self.models[1], radar);
        sync(&self.models[2], alerts);
        sync(&self.models[3], labels);
        // `built` holds the anchor and the anchor-relative extent the models cover.
        let (l, t) = (x0 as f64 * bsize - anchor.0, if y0 == 0 { f64::MIN } else { y0 as f64 * bsize - anchor.1 });
        let (r, b) = (x1 as f64 * bsize - anchor.0, if y1 >= n { f64::MAX } else { y1 as f64 * bsize - anchor.1 });
        self.built = Some((anchor.0, anchor.1, l, t, r, b, self.zoom));
        self.apply_offset();
        let b = app.global::<MapBridge>();
        b.set_radar_progress(if progress.1 == 0 || progress.0 >= progress.1 { -1.0 } else { progress.0 as f32 / progress.1 as f32 });
    }

    /// The tile itself, else the matching part of a loaded ancestor (zooming in),
    /// else its loaded children (zooming out) — so nothing blanks while tiles load.
    #[allow(clippy::too_many_arguments)]
    fn tile_or_fallback(&mut self, key_at: &dyn Fn(&Engine, i32, i64, i64) -> String, z: i32, max_z: i32, x: i64, y: i64, px: f64, py: f64, size: f64, opacity: f32) -> Vec<MapTile> {
        let key = key_at(self, z, x, y);
        if let Some(img) = self.get(&key) {
            return vec![tile(img, px, py, size, opacity)];
        }
        for up in 1..=4 {
            if z - up < 0 {
                break;
            }
            let (pz, pxi, pyi) = (z - up, x >> up, y >> up);
            let pkey = key_at(self, pz, pxi, pyi);
            if let Some(img) = self.get(&pkey) {
                let frac = 1i32 << up;
                let sub = img.size().width as i32 / frac;
                let mut t = tile(img, px, py, size, opacity);
                t.cx = (x - (pxi << up)) as i32 * sub;
                t.cy = (y - (pyi << up)) as i32 * sub;
                t.cw = sub;
                t.ch = sub;
                return vec![t];
            }
        }
        let mut out = Vec::new();
        self.children(key_at, z, max_z, x, y, px, py, size, opacity, 3, &mut out);
        out
    }

    /// Loaded descendants of a tile, up to `depth` levels down (zooming out fast skips levels).
    #[allow(clippy::too_many_arguments)]
    fn children(&mut self, key_at: &dyn Fn(&Engine, i32, i64, i64) -> String, z: i32, max_z: i32, x: i64, y: i64, px: f64, py: f64, size: f64, opacity: f32, depth: u32, out: &mut Vec<MapTile>) {
        if depth == 0 || z >= max_z {
            return;
        }
        let half = size / 2.0;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let (cx, cy) = (x * 2 + dx, y * 2 + dy);
            let (cpx, cpy) = (px + dx as f64 * half, py + dy as f64 * half);
            let ckey = key_at(self, z + 1, cx, cy);
            match self.get(&ckey) {
                Some(img) => out.push(tile(img, cpx, cpy, half, opacity)),
                None => self.children(key_at, z + 1, max_z, cx, cy, cpx, cpy, half, opacity, depth - 1, out),
            }
        }
    }

    fn render_alert(&mut self, key: String, data: Arc<AlertData>, z: i32, x: i64, y: i64) {
        self.pending.insert(key.clone());
        self.rt.spawn_blocking(move || {
            let buf = render_alert_tile(&data, z, x, y).map(|px| SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&px, TILE as u32, TILE as u32));
            let _ = slint::invoke_from_event_loop(move || {
                with(|e| e.loaded(key, buf));
            });
        });
    }

    /* ------------------------------------------------------- state changes */

    pub fn set_palette(&mut self, p: MapPalette) {
        if self.palette != Some(p) {
            self.palette = Some(p);
            self.theme_gen += 1;
            self.failed.retain(|k| !k.starts_with("base") && !k.starts_with("lbl"));
            self.built = None;
            self.render();
        }
    }

    pub fn set_frames(&mut self, frames: Vec<RadarFrame>, idx: usize) {
        self.frames = frames;
        self.frame_idx = idx;
        self.render();
    }

    pub fn set_frame(&mut self, idx: usize) {
        if idx != self.frame_idx {
            self.frame_idx = idx;
            self.render();
        }
    }

    pub fn set_alerts(&mut self, data: Option<Arc<AlertData>>) {
        self.alerts = data;
        self.failed.retain(|k| !k.starts_with("alert"));
        self.render();
    }

    pub fn alerts_at(&self, sx: f64, sy: f64) -> Vec<serde_json::Value> {
        let (x, y) = self.screen_to_norm(sx, sy);
        let x = x.rem_euclid(1.0);
        self.alerts
            .as_ref()
            .map(|d| d.shapes.iter().rev().filter(|s| point_in_shape(s, x, y)).map(|s| s.props.clone()).collect())
            .unwrap_or_default()
    }

    pub fn set_aqi(&mut self, img: Option<(Image, f64, f64, f64, f64)>) {
        self.aqi_image = img;
        self.apply_offset();
    }

    pub fn invalidate(&mut self) {
        self.failed.clear();
        self.built = None;
        self.render();
    }
}

fn tile(img: Image, x: f64, y: f64, size: f64, opacity: f32) -> MapTile {
    let w = img.size().width as i32;
    let h = img.size().height as i32;
    // Snap both edges to whole pixels so neighbours share an edge exactly: no
    // overlap (doubled radar alpha showed as lines) and no gap.
    let (l, t) = (x.round(), y.round());
    let (r, b) = ((x + size).round(), (y + size).round());
    MapTile { img, x: l as f32, y: t as f32, w: (r - l) as f32, h: (b - t) as f32, cx: 0, cy: 0, cw: w, ch: h, opacity }
}

/// Update a model in place so Slint reuses existing tile elements.
fn sync(model: &VecModel<MapTile>, items: Vec<MapTile>) {
    let old = model.row_count();
    for (i, item) in items.iter().enumerate() {
        if i < old {
            let cur = model.row_data(i).unwrap();
            if cur.x != item.x || cur.y != item.y || cur.w != item.w || cur.h != item.h || cur.img != item.img || cur.cx != item.cx
                || cur.cy != item.cy || cur.cw != item.cw || cur.opacity != item.opacity
            {
                model.set_row_data(i, item.clone());
            }
        } else {
            model.push(item.clone());
        }
    }
    for i in (items.len()..old).rev() {
        model.remove(i);
    }
}
