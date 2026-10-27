import { html } from 'lit';
import { customElement, query, state } from 'lit/decorators.js';
import * as maplibregl from 'maplibre-gl';
import {
  type GeoJSONSource, type ImageSource, type LayerSpecification, type MapMouseEvent, type StyleSpecification,
} from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';
import { api } from '../lib/api';
import { setLocation } from '../lib/data';
import { aqiCategory, harmonize, mix, SEVERITY_ORDER } from '../lib/format';
import { $, store, StoreController } from '../lib/store';
import type { Alert, RadarFrame } from '../lib/types';
import { copyText, openExternal, SvElement, toast } from '../lib/ui';

// MapLibre v6 ships its worker as an ES module; let Vite bundle it and tell MapLibre where it lives.
maplibregl.setWorkerUrl(workerUrl);

/* -------------------------------------------------------------- constants */

const BASE_STYLES = {
  material: 'https://tiles.openfreemap.org/styles/positron',
  streets: 'https://tiles.openfreemap.org/styles/liberty',
};
const ESRI = 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}';

// Invisible marker layers that fix overlay z-order no matter when overlays are added:
// base map → satellite → radar/AQI → boundaries → alert polygons → labels.
const ANCHOR_SAT = 'sv-anchor-sat';
const ANCHOR_RADAR = 'sv-anchor-radar';
const ANCHOR_ALERTS = 'sv-anchor-alerts';

const PRELOAD_AHEAD = 4;
const KEEP_BEHIND = 1;
const ALERT_REFRESH = 5 * 60 * 1000;

/** NWS convention colors per event, harmonized toward the theme at render time. */
const EVENT_STYLE: Record<string, [string, number]> = {
  'Tornado Warning': ['#FF0000', 0.38], 'Tornado Watch': ['#FFFF00', 0.28], 'Tornado Emergency': ['#FF00FF', 0.45],
  'Severe Thunderstorm Warning': ['#FFA500', 0.35], 'Severe Thunderstorm Watch': ['#DDBB00', 0.25],
  'Flash Flood Warning': ['#00CC00', 0.35], 'Flash Flood Watch': ['#2E8B57', 0.25], 'Flash Flood Emergency': ['#8B0000', 0.42],
  'Flood Warning': ['#00AA00', 0.3], 'Flood Watch': ['#2E8B57', 0.22], 'Flood Advisory': ['#00FF7F', 0.2],
  'Winter Storm Warning': ['#9400D3', 0.35], 'Winter Storm Watch': ['#4682B4', 0.25], 'Blizzard Warning': ['#FF4500', 0.38],
  'Ice Storm Warning': ['#8B0000', 0.35], 'Winter Weather Advisory': ['#7B68EE', 0.22], 'Wind Advisory': ['#D2691E', 0.22],
  'High Wind Warning': ['#DAA520', 0.3], 'Dense Fog Advisory': ['#708090', 0.22], 'Heat Advisory': ['#FF7F50', 0.22],
  'Excessive Heat Warning': ['#C71585', 0.35], 'Extreme Heat Warning': ['#C71585', 0.35], 'Extreme Cold Warning': ['#0000FF', 0.35],
  'Special Weather Statement': ['#FFE4B5', 0.22], 'Air Quality Alert': ['#8B8B00', 0.2], 'Beach Hazard Statement': ['#40E0D0', 0.2],
  'Rip Current Statement': ['#008B8B', 0.22], 'Small Craft Advisory': ['#D2B48C', 0.2], 'Frost Advisory': ['#6495ED', 0.2],
  'Freeze Warning': ['#483D8B', 0.28], 'Hard Freeze Warning': ['#9400D3', 0.3], 'Freeze Watch': ['#00CED1', 0.2],
  'Lake Wind Advisory': ['#D2691E', 0.18], 'Dust Advisory': ['#BDB76B', 0.22], 'Dust Storm Warning': ['#FFE4C4', 0.3],
  'Smoke Advisory': ['#808080', 0.2], 'Red Flag Warning': ['#FF1493', 0.3], 'Fire Weather Watch': ['#FFDEAD', 0.22],
};
const SEVERITY_FILL: Record<string, [string, number]> = {
  Extreme: ['#FF0000', 0.35], Severe: ['#FF6600', 0.3], Moderate: ['#FFAA00', 0.25], Minor: ['#4169E1', 0.2],
};

/* ---------------------------------------------------------- AQI helpers */

const AQI_RAMP: [number, [number, number, number]][] = [
  [0, [0, 228, 0]], [50, [0, 228, 0]], [100, [255, 255, 0]], [150, [255, 126, 0]],
  [200, [255, 0, 0]], [300, [143, 63, 151]], [500, [126, 0, 35]],
];
const NICE_STEPS = [0.5, 1, 1.5, 2, 3, 4, 5, 8, 10, 15, 20, 30, 45, 90];
const AQI_CACHE = new Map<string, { aqi: number; ts: number }>();
const AQI_TTL = 60 * 60 * 1000;

function aqiRgb(aqi: number): [number, number, number] {
  const v = Math.max(0, Math.min(500, aqi));
  for (let i = 0; i < AQI_RAMP.length - 1; i++) {
    const [lv, lc] = AQI_RAMP[i];
    const [hv, hc] = AQI_RAMP[i + 1];
    if (v <= hv) {
      const t = (v - lv) / (hv - lv || 1);
      return [0, 1, 2].map((j) => Math.round(lc[j] + t * (hc[j] - lc[j]))) as [number, number, number];
    }
  }
  return AQI_RAMP[AQI_RAMP.length - 1][1];
}

const snap = (v: number, step: number) => parseFloat((Math.round(v / step) * step).toFixed(4));
const aqiKey = (lat: number, lon: number) => `${lat.toFixed(2)},${lon.toFixed(2)}`;

/* ---------------------------------------------------- base-map recoloring */

/** Repaints OpenFreeMap's Positron with the live M3 color roles so the map is part of the theme. */
function materialPaint(layer: LayerSpecification, t: Record<string, string>, dark: boolean): Record<string, unknown> | null {
  const id = layer.id;
  const land = dark ? t.surfaceContainerLow : t.surfaceContainer;
  const water = dark ? mix(t.surfaceContainerLowest, t.primaryContainer, 0.45) : mix(t.surfaceContainerLow, t.primaryContainer, 0.75);
  const green = mix(land, t.tertiaryContainer, dark ? 0.14 : 0.3);
  const road = dark ? t.surfaceContainerHighest : t.surfaceBright;
  const casing = mix(land, t.outlineVariant, 0.8);
  switch (layer.type) {
    case 'background':
      return { 'background-color': land };
    case 'fill':
      if (id.includes('water')) return { 'fill-color': water };
      if (id.includes('park') || id.includes('wood')) return { 'fill-color': green };
      if (id.includes('ice') || id.includes('glacier')) return { 'fill-color': mix(land, t.surfaceBright, 0.6) };
      if (id.includes('building')) return { 'fill-color': mix(land, t.surfaceContainerHighest, 0.6), 'fill-outline-color': casing };
      if (id.includes('residential')) return { 'fill-color': mix(land, t.surfaceContainerHigh, 0.5) };
      return { 'fill-color': mix(land, road, 0.5) };
    case 'line':
      if (id.includes('waterway')) return { 'line-color': mix(water, t.primary, 0.15) };
      if (id === 'boundary_2') return { 'line-color': t.onSurfaceVariant };
      if (id.includes('boundary')) return { 'line-color': t.outline };
      if (id.includes('dashline')) return { 'line-color': land };
      if (id.includes('casing') || id.includes('railway')) return { 'line-color': casing };
      if (id.includes('subtle')) return { 'line-color': mix(land, t.outlineVariant, 0.6) };
      if (id.includes('motorway')) return { 'line-color': mix(road, t.secondaryContainer, dark ? 0.55 : 0.7) };
      return { 'line-color': road };
    case 'symbol': {
      const halo = `${land}cc`;
      if (id.includes('water')) return { 'text-color': dark ? t.primary : mix(t.primary, t.onSurface, 0.2), 'text-halo-color': halo };
      if (id.includes('city') || id.includes('town') || id.includes('country') || id.includes('village'))
        return { 'text-color': t.onSurface, 'text-halo-color': halo };
      return { 'text-color': t.onSurfaceVariant, 'text-halo-color': halo };
    }
    default:
      return null;
  }
}

/* -------------------------------------------------------------- element */

@customElement('sv-map')
export class SvMap extends SvElement {
  private s = new StoreController(this, (s) => [s.tokens, s.isDark, s.mapStyle, s.sidebarPosition, s.isNarrow] as const);

  @query('.sv-map-canvas') private container!: HTMLDivElement;
  @query('.sv-ctx-anchor') private ctxAnchor!: HTMLDivElement;
  @query('#map-ctx') private ctxMenu!: HTMLElement & { show: (t: HTMLElement) => void };
  @state() private ctx: { x: number; y: number; lat: number; lon: number } | null = null;

  map!: maplibregl.Map;
  private styleReady = false;
  private styleKind: 'material' | 'streets' | null = null;
  private baseLayerIds = new Set<string>();
  private styleCache = new Map<string, StyleSpecification>();
  private marker: maplibregl.Marker | null = null;
  private nav: maplibregl.NavigationControl | null = null;
  private radarKeys = new Map<string, string>(); // layer id → frame key
  private alertTimer: number | undefined;
  private alertGeo: GeoJSON.FeatureCollection | null = null;
  private advisoryGeo: GeoJSON.FeatureCollection | null = null;
  private aqiTimer: number | undefined;
  private aqiSeq = 0;
  private hovered: { source: string; id: string | number } | null = null;

  render() {
    const [, , , side] = this.s.value;
    return html`
      <div class="sv-map-canvas" data-side=${side}></div>
      <div class="sv-ctx-anchor" style=${this.ctx ? `left:${this.ctx.x}px;top:${this.ctx.y}px` : ''}></div>
      <m3e-menu id="map-ctx" @toggle=${(e: ToggleEvent) => e.newState === 'closed' && (this.ctx = null)}>
        ${this.ctx
          ? html`
              <m3e-menu-item disabled>
                <m3e-icon slot="icon" name="pin_drop"></m3e-icon>
                ${Math.abs(this.ctx.lat).toFixed(4)}° ${this.ctx.lat >= 0 ? 'N' : 'S'} ·
                ${Math.abs(this.ctx.lon).toFixed(4)}° ${this.ctx.lon >= 0 ? 'E' : 'W'}
              </m3e-menu-item>
              <m3e-divider></m3e-divider>
              <m3e-menu-item @click=${this.ctxSetLocation}>
                <m3e-icon slot="icon" name="add_location"></m3e-icon>Set weather location here
              </m3e-menu-item>
              <m3e-menu-item @click=${this.ctxCenter}>
                <m3e-icon slot="icon" name="center_focus_strong"></m3e-icon>Center map here
              </m3e-menu-item>
              <m3e-divider></m3e-divider>
              <m3e-menu-item @click=${this.ctxCopy}>
                <m3e-icon slot="icon" name="content_copy"></m3e-icon>Copy coordinates
              </m3e-menu-item>
              <m3e-menu-item @click=${this.ctxOpenMaps}>
                <m3e-icon slot="icon" name="open_in_new"></m3e-icon>Open in Google Maps
              </m3e-menu-item>
            `
          : null}
      </m3e-menu>
    `;
  }

  /* ------------------------------------------------------------ lifecycle */

  async firstUpdated() {
    const s = $();
    const center: [number, number] = s.location ? [s.location.lon, s.location.lat] : [-98.35, 39.5];
    this.map = new maplibregl.Map({
      container: this.container,
      style: { version: 8, sources: {}, layers: [] },
      center,
      zoom: s.location ? s.mapZoom : 3.6,
      attributionControl: { compact: true },
      dragRotate: false,
      pitchWithRotate: false,
      fadeDuration: 150,
    });
    this.map.touchZoomRotate.disableRotation();
    this.placeControls();

    this.map.on('style.load', () => this.onStyleLoad());
    this.map.on('moveend', () => {
      const z = Math.round(this.map.getZoom() * 10) / 10;
      if (z !== $().mapZoom && $().location) store.set({ mapZoom: z });
      if ($().mapLayer === 'aqi') this.scheduleAqi();
    });
    this.map.on('sourcedata', () => this.reportRadarProgress());
    this.map.on('idle', () => this.reportRadarProgress());
    this.map.on('contextmenu', (e) => this.openContextMenu(e));
    this.map.on('click', (e) => this.onClick(e));
    this.map.on('mousemove', (e) => this.onHover(e));
    new ResizeObserver(() => this.map.resize()).observe(this.container);
    window.addEventListener('sv-zoom', (e) => this.map.zoomTo(this.map.getZoom() + (e as CustomEvent<number>).detail, { duration: 300 }));

    store.subscribe((st, prev) => {
      if (!this.styleReady) return;
      if (st.location !== prev.location) this.updateLocation(true);
      if (
        st.radarFrames !== prev.radarFrames || st.radarIdx !== prev.radarIdx || st.radarColorScheme !== prev.radarColorScheme ||
        st.radarTileQuality !== prev.radarTileQuality || st.radarSmooth !== prev.radarSmooth || st.radarSnow !== prev.radarSnow ||
        st.showArrows !== prev.showArrows || st.isDark !== prev.isDark || st.mapLayer !== prev.mapLayer ||
        st.radarFrameFade !== prev.radarFrameFade
      ) this.updateRadar();
      if (st.radarOpacity !== prev.radarOpacity) this.applyRadarOpacity();
      if (st.showSatellite !== prev.showSatellite) this.updateSatellite();
      if (st.showAlertPolygons !== prev.showAlertPolygons || st.showAdvisories !== prev.showAdvisories) this.loadAlertPolygons();
      if (st.mapLayer !== prev.mapLayer) this.updateAqi();
      if (st.tokens !== prev.tokens) {
        this.recolorBase();
        this.paintAlertLayers();
      }
    });

    await this.applyBaseStyle();
  }

  protected updated() {
    if (!this.map) return;
    const kind = $().mapStyle;
    if (kind !== this.styleKind && this.styleCache.size) this.applyBaseStyle();
    this.placeControls();
  }

  /* --------------------------------------------------------- base style */

  private async fetchStyle(kind: 'material' | 'streets') {
    const hit = this.styleCache.get(kind);
    if (hit) return structuredClone(hit);
    const res = await fetch(BASE_STYLES[kind]);
    const json = (await res.json()) as StyleSpecification;
    this.styleCache.set(kind, json);
    return structuredClone(json);
  }

  private async applyBaseStyle() {
    const kind = $().mapStyle;
    this.styleKind = kind;
    try {
      const style = await this.fetchStyle(kind);
      this.baseLayerIds = new Set(style.layers.map((l) => l.id));
      if (kind === 'material' && $().tokens.primary) {
        for (const l of style.layers) {
          const p = materialPaint(l, $().tokens, $().isDark);
          if (p) (l as { paint?: Record<string, unknown> }).paint = { ...(l as { paint?: object }).paint, ...p };
        }
      }
      this.styleReady = false;
      this.map.setStyle(style, { diff: false });
    } catch (e) {
      console.error('[map] style failed', e);
      toast('Couldn’t load the base map. Check your connection.');
    }
  }

  private recolorBase() {
    if (!this.styleReady || this.styleKind !== 'material') return;
    const t = $().tokens;
    for (const layer of this.map.getStyle().layers) {
      if (!this.baseLayerIds.has(layer.id)) continue;
      const p = materialPaint(layer, t, $().isDark);
      if (!p) continue;
      for (const [k, v] of Object.entries(p)) this.map.setPaintProperty(layer.id, k as never, v as never);
    }
  }

  private onStyleLoad() {
    const layers = this.map.getStyle().layers;
    const firstSymbol = layers.find((l) => l.type === 'symbol')?.id;
    const firstBoundary = layers.find((l) => l.id.startsWith('boundary'))?.id ?? firstSymbol;
    const anchor = (id: string, before?: string) =>
      this.map.addLayer({ id, type: 'background', layout: { visibility: 'none' }, paint: {} }, before);
    anchor(ANCHOR_SAT, firstBoundary);
    anchor(ANCHOR_RADAR, firstBoundary);
    anchor(ANCHOR_ALERTS, firstSymbol);
    this.styleReady = true;
    this.radarKeys.clear();
    if (this.styleKind === 'material') this.recolorBase();
    this.updateSatellite();
    this.updateRadar();
    this.updateAqi();
    this.loadAlertPolygons(true);
    this.updateLocation(false);
  }

  private placeControls() {
    const side = $().sidebarPosition === 'left' ? 'right' : 'left';
    const pos = `top-${side}` as const;
    if (this.nav && (this.nav as unknown as { _pos?: string })._pos === pos) return;
    if (this.nav) this.map.removeControl(this.nav);
    this.nav = new maplibregl.NavigationControl({ showCompass: false });
    (this.nav as unknown as { _pos?: string })._pos = pos;
    this.map.addControl(this.nav, pos);
  }

  /* ----------------------------------------------------------- location */

  private updateLocation(fly: boolean) {
    const loc = $().location;
    this.marker?.remove();
    this.marker = null;
    if (!loc) return;
    const el = document.createElement('div');
    el.className = 'sv-pin';
    el.innerHTML = '<span class="sv-pin-halo"></span><m3e-shape class="sv-pin-shape" name="very-sunny"></m3e-shape>';
    el.title = `${loc.name}${loc.state ? `, ${loc.state}` : ''}`;
    this.marker = new maplibregl.Marker({ element: el }).setLngLat([loc.lon, loc.lat]).addTo(this.map);
    if (fly) this.map.flyTo({ center: [loc.lon, loc.lat], zoom: Math.max(this.map.getZoom(), 7.5), duration: 1400 });
  }

  /* ---------------------------------------------------------- satellite */

  private updateSatellite() {
    const on = $().showSatellite;
    const has = !!this.map.getLayer('sv-satellite');
    if (on && !has) {
      if (!this.map.getSource('sv-satellite'))
        this.map.addSource('sv-satellite', { type: 'raster', tiles: [ESRI], tileSize: 256, maxzoom: 19, attribution: 'Imagery © Esri' });
      this.map.addLayer({ id: 'sv-satellite', type: 'raster', source: 'sv-satellite' }, ANCHOR_SAT);
    } else if (!on && has) {
      this.map.removeLayer('sv-satellite');
    }
  }

  /* -------------------------------------------------------------- radar */

  private tileUrl(f: RadarFrame) {
    const s = $();
    const base = `${f.host}${f.path}/${s.radarTileQuality}/{z}/{x}/{y}/${s.radarColorScheme}/${s.radarSmooth ? 1 : 0}_${s.radarSnow ? 1 : 0}.png`;
    // Arrows are drawn server-side; light arrows read best on a dark UI and vice versa.
    return s.showArrows ? `${base}?arrows=${s.isDark ? 'light' : 'dark'}` : base;
  }

  private frameKey(f: RadarFrame) {
    const s = $();
    return `${f.time}:${s.radarColorScheme}:${s.radarTileQuality}:${s.radarSmooth}:${s.radarSnow}:${s.showArrows ? s.isDark : 'x'}`;
  }

  private wantedFrames(): RadarFrame[] {
    const s = $();
    const n = s.radarFrames.length;
    if (!n || s.mapLayer !== 'radar') return [];
    const out: RadarFrame[] = [];
    for (let j = -KEEP_BEHIND; j <= PRELOAD_AHEAD; j++) out.push(s.radarFrames[(((s.radarIdx + j) % n) + n) % n]);
    return out;
  }

  private updateRadar() {
    if (!this.styleReady) return;
    const s = $();
    const wanted = this.wantedFrames();
    const wantedIds = new Map(wanted.map((f) => [`radar-${this.frameKey(f).replace(/[^a-z0-9]/gi, '_')}`, f]));

    for (const [id] of this.radarKeys) {
      if (!wantedIds.has(id)) {
        if (this.map.getLayer(id)) this.map.removeLayer(id);
        if (this.map.getSource(id)) this.map.removeSource(id);
        this.radarKeys.delete(id);
      }
    }
    const fade = s.radarFrameFade ? 280 : 0;
    for (const [id, f] of wantedIds) {
      if (!this.map.getSource(id)) {
        this.map.addSource(id, {
          type: 'raster',
          tiles: [this.tileUrl(f)],
          tileSize: 512, // 512 display tiles = z-1 requests, 4× fewer tiles per frame (as in the web app)
          maxzoom: 12,
          attribution: '<a href="https://github.com/JoshuaKimsey/LibreWXR">LibreWXR</a>',
        });
      }
      if (!this.map.getLayer(id)) {
        this.map.addLayer(
          {
            id,
            type: 'raster',
            source: id,
            paint: {
              'raster-opacity': 0,
              'raster-opacity-transition': { duration: fade, delay: 0 },
              'raster-fade-duration': 0,
              'raster-resampling': 'linear',
            },
          },
          ANCHOR_RADAR,
        );
        this.radarKeys.set(id, this.frameKey(f));
      } else {
        this.map.setPaintProperty(id, 'raster-opacity-transition', { duration: fade, delay: 0 });
      }
    }
    this.applyRadarOpacity();
    this.reportRadarProgress();
  }

  private currentRadarId() {
    const s = $();
    const f = s.radarFrames[s.radarIdx];
    return f && s.mapLayer === 'radar' ? `radar-${this.frameKey(f).replace(/[^a-z0-9]/gi, '_')}` : null;
  }

  private applyRadarOpacity() {
    const cur = this.currentRadarId();
    for (const [id] of this.radarKeys) {
      if (this.map.getLayer(id)) this.map.setPaintProperty(id, 'raster-opacity', id === cur ? $().radarOpacity : 0);
    }
  }

  private progressStart = 0;
  private reportRadarProgress() {
    if (!this.styleReady || !this.radarKeys.size) {
      if ($().radarProgress) store.set({ radarProgress: null });
      return;
    }
    const ids = [...this.radarKeys.keys()];
    const loaded = ids.filter((id) => this.map.getSource(id) && this.map.isSourceLoaded(id)).length;
    if (loaded >= ids.length) {
      this.progressStart = 0;
      if ($().radarProgress) store.set({ radarProgress: null });
    } else {
      if (!this.progressStart) this.progressStart = Date.now();
      const p = $().radarProgress;
      if (!p || p.loaded !== loaded || p.total !== ids.length) {
        store.set({ radarProgress: { loaded, total: ids.length, startTime: this.progressStart } });
      }
    }
  }

  /* ---------------------------------------------------------------- AQI */

  private updateAqi() {
    if (!this.styleReady) return;
    if ($().mapLayer === 'aqi') {
      this.renderAqi();
    } else {
      clearTimeout(this.aqiTimer);
      this.aqiSeq++;
      if (this.map.getLayer('sv-aqi')) this.map.removeLayer('sv-aqi');
      if (this.map.getSource('sv-aqi')) this.map.removeSource('sv-aqi');
    }
  }

  private scheduleAqi() {
    clearTimeout(this.aqiTimer);
    this.aqiSeq++;
    this.aqiTimer = window.setTimeout(() => this.renderAqi(), 1200);
  }

  private aqiBounds() {
    const b = this.map.getBounds();
    return { s: Math.max(-85, b.getSouth()), n: Math.min(85, b.getNorth()), w: b.getWest(), e: b.getEast() };
  }

  private drawAqi(bounds: ReturnType<SvMap['aqiBounds']>, pad: number) {
    const now = Date.now();
    const pts: { lat: number; lon: number; aqi: number }[] = [];
    for (const [k, v] of AQI_CACHE) {
      if (now - v.ts > AQI_TTL) {
        AQI_CACHE.delete(k);
        continue;
      }
      const [lat, lon] = k.split(',').map(Number);
      if (lat >= bounds.s - pad && lat <= bounds.n + pad && lon >= bounds.w - pad && lon <= bounds.e + pad) pts.push({ lat, lon, aqi: v.aqi });
    }
    if (pts.length < 2) return;
    // Inverse-distance-weighted interpolation onto a small canvas, then blurred.
    const sz = 192;
    const off = document.createElement('canvas');
    off.width = off.height = sz;
    const octx = off.getContext('2d')!;
    const img = octx.createImageData(sz, sz);
    for (let row = 0; row < sz; row++) {
      const lat = bounds.n - (row / sz) * (bounds.n - bounds.s);
      for (let col = 0; col < sz; col++) {
        const lon = bounds.w + (col / sz) * (bounds.e - bounds.w);
        let ws = 0, vs = 0, exact = -1;
        for (const p of pts) {
          const d2 = (lat - p.lat) ** 2 + (lon - p.lon) ** 2;
          if (d2 < 1e-8) { exact = p.aqi; break; }
          const w = 1 / d2 ** 1.5;
          ws += w;
          vs += w * p.aqi;
        }
        const [r, g, b] = aqiRgb(exact >= 0 ? exact : ws ? vs / ws : 0);
        const i = (row * sz + col) * 4;
        img.data[i] = r; img.data[i + 1] = g; img.data[i + 2] = b; img.data[i + 3] = 170;
      }
    }
    octx.putImageData(img, 0, 0);
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = sz;
    const ctx = canvas.getContext('2d')!;
    ctx.filter = 'blur(4px)';
    ctx.drawImage(off, 0, 0);
    const url = canvas.toDataURL('image/png');
    const coordinates: [[number, number], [number, number], [number, number], [number, number]] = [
      [bounds.w, bounds.n], [bounds.e, bounds.n], [bounds.e, bounds.s], [bounds.w, bounds.s],
    ];
    const src = this.map.getSource('sv-aqi') as ImageSource | undefined;
    if (src) {
      src.updateImage({ url, coordinates });
    } else {
      this.map.addSource('sv-aqi', { type: 'image', url, coordinates });
      this.map.addLayer({ id: 'sv-aqi', type: 'raster', source: 'sv-aqi', paint: { 'raster-opacity': 0.65, 'raster-fade-duration': 0 } }, ANCHOR_RADAR);
    }
  }

  private async renderAqi() {
    const seq = ++this.aqiSeq;
    const bounds = this.aqiBounds();
    const span = Math.max(bounds.n - bounds.s, bounds.e - bounds.w);
    const step = Math.min(NICE_STEPS.find((x) => x >= span / 25) ?? 90, 2);
    const pad = step * 2;
    this.drawAqi(bounds, pad);

    const c = this.map.getCenter();
    const pts: { lat: number; lon: number }[] = [];
    for (let lat = snap(bounds.s - step, step); lat <= bounds.n + step; lat = parseFloat((lat + step).toFixed(4))) {
      if (lat < -85 || lat > 85) continue;
      for (let lon = snap(bounds.w - step, step); lon <= bounds.e + step; lon = parseFloat((lon + step).toFixed(4))) {
        const hit = AQI_CACHE.get(aqiKey(lat, lon));
        if (!hit || Date.now() - hit.ts > AQI_TTL) pts.push({ lat, lon });
      }
    }
    pts.sort((a, b) => (a.lat - c.lat) ** 2 + (a.lon - c.lng) ** 2 - ((b.lat - c.lat) ** 2 + (b.lon - c.lng) ** 2));
    if (!pts.length) return;
    try {
      const res = await api.airQualityBatch(pts.slice(0, 200));
      const now = Date.now();
      for (const r of res) if (r.aqi != null) AQI_CACHE.set(aqiKey(r.lat, r.lon), { aqi: r.aqi, ts: now });
    } catch (e) {
      console.warn('[aqi]', e);
    }
    if (seq !== this.aqiSeq || $().mapLayer !== 'aqi') return;
    this.drawAqi(this.aqiBounds(), pad);
  }

  private async showAqiPopup(e: MapMouseEvent) {
    const { lat, lng } = e.lngLat;
    const popup = new maplibregl.Popup({ className: 'sv-popup', maxWidth: '300px' })
      .setLngLat(e.lngLat)
      .setHTML('<div class="sv-popup-body"><m3e-loading-indicator></m3e-loading-indicator></div>')
      .addTo(this.map);
    try {
      const d = await api.airQuality(lat, lng);
      const cat = aqiCategory(d?.us_aqi);
      if (!cat || d.us_aqi == null) {
        popup.setHTML('<div class="sv-popup-body">No AQI data available for this location.</div>');
        return;
      }
      const f = (v: number | null | undefined) => (v != null ? `${v.toFixed(1)} µg/m³` : '—');
      const color = harmonize(cat.hue);
      popup.setHTML(`
        <div class="sv-popup-body sv-aqi-popup">
          <div class="sv-aqi-popup-head">
            <span class="sv-aqi-popup-num" style="color:${color}">${d.us_aqi}</span>
            <span class="sv-chip" style="background:${color}22;color:${color}">${cat.label}</span>
          </div>
          <p>${cat.desc}</p>
          <div class="sv-aqi-popup-grid">
            <span>PM2.5</span><b>${f(d.pm2_5)}</b><span>PM10</span><b>${f(d.pm10)}</b>
            <span>Ozone</span><b>${f(d.ozone)}</b><span>NO₂</span><b>${f(d.nitrogen_dioxide)}</b>
            <span>SO₂</span><b>${f(d.sulphur_dioxide)}</b><span>CO</span><b>${f(d.carbon_monoxide)}</b>
          </div>
          <small>US AQI · Open-Meteo Air Quality</small>
        </div>`);
    } catch {
      popup.setHTML('<div class="sv-popup-body">Failed to load air quality data. Try again.</div>');
    }
  }

  /* ------------------------------------------------------ alert polygons */

  private fillExpr(): unknown {
    const pairs: unknown[] = [];
    for (const [ev, [c]] of Object.entries(EVENT_STYLE)) pairs.push(ev, harmonize(c));
    const sev: unknown[] = [];
    for (const [k, [c]] of Object.entries(SEVERITY_FILL)) sev.push(k, harmonize(c));
    return ['match', ['get', 'event'], ...pairs, ['match', ['get', 'severity'], ...sev, '#888888']];
  }

  private opacityExpr(base: boolean): unknown {
    const pairs: unknown[] = [];
    for (const [ev, [, o]] of Object.entries(EVENT_STYLE)) pairs.push(ev, o);
    const fallback = ['match', ['get', 'severity'], ...Object.entries(SEVERITY_FILL).flatMap(([k, [, o]]) => [k, o]), 0.18];
    const value = ['match', ['get', 'event'], ...pairs, fallback];
    return base ? ['case', ['boolean', ['feature-state', 'hover'], false], 0.6, value] : value;
  }

  private paintAlertLayers() {
    if (!this.styleReady) return;
    for (const id of ['sv-warn-fill', 'sv-adv-fill']) if (this.map.getLayer(id)) this.map.setPaintProperty(id, 'fill-color', this.fillExpr() as never);
    for (const id of ['sv-warn-line', 'sv-adv-line']) if (this.map.getLayer(id)) this.map.setPaintProperty(id, 'line-color', this.fillExpr() as never);
  }

  private ensureAlertLayers() {
    for (const [src, data] of [['sv-warn', this.alertGeo], ['sv-adv', this.advisoryGeo]] as const) {
      const fc = data ?? { type: 'FeatureCollection', features: [] };
      const source = this.map.getSource(src) as GeoJSONSource | undefined;
      if (source) source.setData(fc);
      else this.map.addSource(src, { type: 'geojson', data: fc, generateId: true });
      if (!this.map.getLayer(`${src}-fill`)) {
        this.map.addLayer({
          id: `${src}-fill`, type: 'fill', source: src,
          paint: { 'fill-color': this.fillExpr() as never, 'fill-opacity': this.opacityExpr(true) as never },
        }, ANCHOR_ALERTS);
        this.map.addLayer({
          id: `${src}-line`, type: 'line', source: src,
          paint: {
            'line-color': this.fillExpr() as never,
            'line-width': ['case', ['boolean', ['feature-state', 'hover'], false], 3, src === 'sv-adv' ? 1.5 : 2],
            'line-opacity': src === 'sv-adv' ? 0.7 : 0.9,
            ...(src === 'sv-adv' ? { 'line-dasharray': [3, 2.5] } : {}),
          },
        }, ANCHOR_ALERTS);
      }
    }
  }

  private removeAlertLayers() {
    for (const src of ['sv-warn', 'sv-adv']) {
      for (const l of [`${src}-fill`, `${src}-line`]) if (this.map.getLayer(l)) this.map.removeLayer(l);
      if (this.map.getSource(src)) this.map.removeSource(src);
    }
  }

  private async loadAlertPolygons(fromStyleLoad = false) {
    clearInterval(this.alertTimer);
    const s = $();
    if (!s.showAlertPolygons) {
      this.alertGeo = this.advisoryGeo = null;
      this.removeAlertLayers();
      return;
    }
    if (fromStyleLoad && this.alertGeo) this.ensureAlertLayers();
    const load = async () => {
      try {
        const all = await api.activeAlerts();
        const hasPoly = (f: GeoJSON.Feature) => f.geometry?.type === 'Polygon' || f.geometry?.type === 'MultiPolygon';
        const rank = (f: GeoJSON.Feature) => SEVERITY_ORDER.indexOf(String(f.properties?.severity ?? 'Unknown'));
        const polys = all.features.filter(hasPoly).sort((a, b) => rank(b) - rank(a));
        this.alertGeo = { type: 'FeatureCollection', features: polys };
        if ($().showAdvisories) {
          const zoned = all.features.filter((f) => !hasPoly(f)).sort((a, b) => rank(a) - rank(b));
          const zoneToFeature = new Map<string, GeoJSON.Feature>();
          for (const f of zoned) for (const z of (f.properties?.geocode?.UGC ?? []) as string[]) if (!zoneToFeature.has(z)) zoneToFeature.set(z, f);
          const geos = await api.zoneGeometries([...zoneToFeature.keys()]);
          this.advisoryGeo = {
            type: 'FeatureCollection',
            features: Object.entries(geos).map(([z, g]) => ({ type: 'Feature', geometry: g, properties: zoneToFeature.get(z)!.properties })),
          };
        } else {
          this.advisoryGeo = null;
        }
        if (this.styleReady && $().showAlertPolygons) this.ensureAlertLayers();
      } catch (e) {
        console.warn('[alerts] polygons failed', e);
      }
    };
    await load();
    this.alertTimer = window.setInterval(load, ALERT_REFRESH);
  }

  private onHover(e: MapMouseEvent) {
    const layers = ['sv-warn-fill', 'sv-adv-fill'].filter((l) => this.map.getLayer(l));
    const f = layers.length ? this.map.queryRenderedFeatures(e.point, { layers })[0] : undefined;
    if (this.hovered && (!f || f.id !== this.hovered.id || f.source !== this.hovered.source)) {
      this.map.setFeatureState(this.hovered, { hover: false });
      this.hovered = null;
    }
    if (f && f.id != null && !this.hovered) {
      this.hovered = { source: f.source, id: f.id };
      this.map.setFeatureState(this.hovered, { hover: true });
    }
    this.map.getCanvas().style.cursor = f ? 'pointer' : '';
  }

  private onClick(e: MapMouseEvent) {
    const layers = ['sv-warn-fill', 'sv-adv-fill'].filter((l) => this.map.getLayer(l));
    const feats = layers.length ? this.map.queryRenderedFeatures(e.point, { layers }) : [];
    if (feats.length) {
      const seen = new Set<string>();
      const alerts: Alert[] = [];
      for (const f of feats) {
        const p = f.properties as Record<string, string>;
        const id = p.id ?? p['@id'] ?? `${p.event}-${p.expires}`;
        if (seen.has(id)) continue;
        seen.add(id);
        alerts.push({
          id, title: p.event ?? 'Weather Alert', headline: p.headline ?? '', description: p.description ?? '',
          instruction: p.instruction ?? '', severity: p.severity ?? 'Unknown', certainty: p.certainty ?? 'Unknown',
          urgency: p.urgency ?? 'Unknown', start: Date.parse(p.onset ?? p.sent) || null, end: Date.parse(p.expires) || null,
        });
      }
      store.set({ alertDialog: alerts });
      return;
    }
    if ($().mapLayer === 'aqi') this.showAqiPopup(e);
  }

  /* ------------------------------------------------------- context menu */

  private async openContextMenu(e: MapMouseEvent) {
    e.originalEvent.preventDefault();
    const rect = this.getBoundingClientRect();
    this.ctx = { x: e.originalEvent.clientX - rect.left, y: e.originalEvent.clientY - rect.top, lat: e.lngLat.lat, lon: e.lngLat.lng };
    await this.updateComplete;
    this.ctxMenu.show(this.ctxAnchor);
  }

  private async ctxSetLocation() {
    const c = this.ctx;
    if (!c) return;
    try {
      setLocation(await api.reverseGeocode(c.lat, c.lon));
    } catch {
      // Nominatim failed (rate limit etc.) — still commit, named by coordinates.
      setLocation({ lat: c.lat, lon: c.lon, name: `${c.lat.toFixed(4)}, ${c.lon.toFixed(4)}`, state: '', country: '' });
    }
  }

  private ctxCenter() {
    if (this.ctx) this.map.easeTo({ center: [this.ctx.lon, this.ctx.lat], duration: 600 });
  }

  private async ctxCopy() {
    if (!this.ctx) return;
    await copyText(`${this.ctx.lat.toFixed(5)}, ${this.ctx.lon.toFixed(5)}`);
    toast('Coordinates copied');
  }

  private ctxOpenMaps() {
    if (this.ctx) openExternal(`https://maps.google.com/?q=${this.ctx.lat},${this.ctx.lon}`);
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'sv-map': SvMap;
  }
}
