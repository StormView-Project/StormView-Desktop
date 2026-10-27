import { api, errorText } from './api';
import { $, store } from './store';
import type { Place } from './types';

const WEATHER_REFRESH = 10 * 60 * 1000;
const ALERTS_REFRESH = 90 * 1000;
const RADAR_REFRESH = 10 * 60 * 1000; // matches LibreWXR's fetch interval
const FRAME_MS: Record<number, number> = { 0.5: 600, 1: 400, 2: 200 };

let weatherSeq = 0;
let allFrames: Awaited<ReturnType<typeof api.radarFrames>> = [];

export async function refreshWeather() {
  const s = $();
  if (!s.location) return;
  const seq = ++weatherSeq;
  store.set({ weatherLoading: true, weatherError: null });
  try {
    const data = await api.weather(s.location.lat, s.location.lon, s.weatherAPI, s.units);
    if (seq !== weatherSeq) return;
    store.set({ weather: data, weatherLoading: false, weatherError: data.notice });
  } catch (e) {
    if (seq !== weatherSeq) return;
    store.set({ weatherLoading: false, weatherError: errorText(e) || 'Failed to fetch weather data.' });
  }
}

export async function refreshAlerts() {
  const loc = $().location;
  if (!loc) return store.set({ alerts: [] });
  try {
    store.set({ alerts: await api.pointAlerts(loc.lat, loc.lon) });
  } catch {
    // Outside the US or NWS unavailable — hide the banner silently, like the web app.
    store.set({ alerts: [] });
  }
}

function applyFrameFilter() {
  const frames = $().showNowcast ? allFrames : allFrames.filter((f) => f.type === 'past');
  let idx = frames.length - 1;
  for (let i = frames.length - 1; i >= 0; i--) {
    if (frames[i].type === 'past') {
      idx = i;
      break;
    }
  }
  store.set({ radarFrames: frames, radarIdx: Math.max(0, idx) });
}

export async function refreshRadar() {
  store.set({ radarLoading: true });
  try {
    allFrames = await api.radarFrames();
    applyFrameFilter();
  } catch (e) {
    console.error('[radar] failed to load frames', e);
  } finally {
    store.set({ radarLoading: false });
  }
}

export function jumpToNow() {
  const frames = $().radarFrames;
  let idx = frames.length - 1;
  for (let i = frames.length - 1; i >= 0; i--) {
    if (frames[i].type === 'past') {
      idx = i;
      break;
    }
  }
  store.set({ radarIdx: Math.max(0, idx), radarPlaying: false });
}

export function setLocation(place: Place) {
  store.set({ location: place, weather: null, alerts: [] });
}

/** "Use my location": the webview's Geolocation API first, then IP lookup via Rust. */
export async function locateMe(): Promise<Place> {
  const fromBrowser = await new Promise<{ lat: number; lon: number } | null>((resolve) => {
    if (!('geolocation' in navigator)) return resolve(null);
    const timer = setTimeout(() => resolve(null), 6000);
    navigator.geolocation.getCurrentPosition(
      (p) => {
        clearTimeout(timer);
        resolve({ lat: p.coords.latitude, lon: p.coords.longitude });
      },
      () => {
        clearTimeout(timer);
        resolve(null);
      },
      { timeout: 6000, maximumAge: 60000 },
    );
  });
  if (fromBrowser) {
    try {
      return await api.reverseGeocode(fromBrowser.lat, fromBrowser.lon);
    } catch {
      return { ...fromBrowser, name: 'My Location', state: '', country: '' };
    }
  }
  return api.ipLocate();
}

/** Starts every poller and reacts to the settings that invalidate data. */
export function startData() {
  let playTimer: number | undefined;

  const restartPlayback = () => {
    clearInterval(playTimer);
    const s = $();
    if (!s.radarPlaying || s.radarFrames.length === 0) return;
    playTimer = window.setInterval(() => {
      const st = $();
      store.set({ radarIdx: (st.radarIdx + 1) % st.radarFrames.length });
    }, FRAME_MS[s.radarSpeed] ?? 400);
  };

  store.subscribe((s, prev) => {
    if (s.location !== prev.location || s.weatherAPI !== prev.weatherAPI || s.units !== prev.units) {
      refreshWeather();
    }
    if (s.location !== prev.location) refreshAlerts();
    if (s.showNowcast !== prev.showNowcast) applyFrameFilter();
    if (
      s.radarPlaying !== prev.radarPlaying || s.radarSpeed !== prev.radarSpeed ||
      s.radarFrames.length !== prev.radarFrames.length
    ) restartPlayback();
  });

  refreshWeather();
  refreshAlerts();
  refreshRadar();
  setInterval(refreshWeather, WEATHER_REFRESH);
  setInterval(refreshAlerts, ALERTS_REFRESH);
  setInterval(refreshRadar, RADAR_REFRESH);
}
