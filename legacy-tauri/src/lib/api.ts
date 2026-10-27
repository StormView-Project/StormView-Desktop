import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AirQuality, Alert, AqiPoint, ChatMessage, OutlookDay, Place, RadarFrame, WeatherData,
} from './types';

/** Thin typed wrappers over the Rust commands in src-tauri/src/lib.rs. */
export const api = {
  weather: (lat: number, lon: number, provider: string, units: string) =>
    invoke<WeatherData>('get_weather', { lat, lon, provider, units }),
  pointAlerts: (lat: number, lon: number) => invoke<Alert[]>('get_point_alerts', { lat, lon }),
  search: (query: string) => invoke<Place[]>('search_location', { query }),
  reverseGeocode: (lat: number, lon: number) => invoke<Place>('reverse_geocode', { lat, lon }),
  ipLocate: () => invoke<Place>('ip_locate'),
  airQuality: (lat: number, lon: number) => invoke<AirQuality>('get_air_quality', { lat, lon }),
  airQualityBatch: (points: { lat: number; lon: number }[]) =>
    invoke<AqiPoint[]>('get_air_quality_batch', { points }),
  futureRange: (lat: number, lon: number, start: string, end: string, units: string) =>
    invoke<OutlookDay[]>('get_future_range', { lat, lon, start, end, units }),
  radarFrames: () => invoke<RadarFrame[]>('get_radar_frames'),
  activeAlerts: () => invoke<GeoJSON.FeatureCollection>('get_active_alerts'),
  zoneGeometries: (ids: string[]) => invoke<Record<string, GeoJSON.Geometry>>('get_zone_geometries', { ids }),
  loadSettings: () => invoke<Record<string, unknown>>('load_settings'),
  saveSettings: (settings: object) => invoke<void>('save_settings', { settings }),
  testNotification: () => invoke<void>('test_notification'),
  assistantModels: (provider: string, baseUrl: string) =>
    invoke<string[]>('assistant_models', { provider, baseUrl }),
  assistantChat: (
    provider: string,
    baseUrl: string,
    model: string,
    messages: ChatMessage[],
    onDelta: (text: string) => void,
  ) => {
    const onEvent = new Channel<{ event: 'delta'; data: { text: string } } | { event: 'done' }>();
    onEvent.onmessage = (m) => {
      if (m.event === 'delta') onDelta(m.data.text);
    };
    return invoke<void>('assistant_chat', { provider, baseUrl, model, messages, onEvent });
  },
};

export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
