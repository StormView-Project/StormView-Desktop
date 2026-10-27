import { argbFromHex, Blend, hexFromArgb, Hct } from '@material/material-color-utilities';
import { format } from 'date-fns';
import { $ } from './store';

/* --------------------------------------------------------------- units */

export const tempUnit = () => ($().units === 'imperial' ? '°F' : '°C');
export const speedUnit = () => ($().units === 'imperial' ? 'mph' : 'km/h');
export const distUnit = () => ($().units === 'imperial' ? 'mi' : 'km');
export const precipUnit = () => ($().units === 'imperial' ? 'in' : 'mm');

export const num = (v: number | null | undefined, digits = 0) =>
  v == null || Number.isNaN(v) ? '—' : Number(v).toFixed(digits);

export const deg = (v: number | null | undefined) => (v == null ? '—' : `${Math.round(v)}°`);

export function windLabel(deg: number | null | undefined): string {
  if (deg == null) return '—';
  const dirs = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW'];
  return dirs[Math.round(deg / 22.5) % 16];
}

/* --------------------------------------------------------------- time */

export const fmtTime = (ms: number | null | undefined) => (ms == null ? '—' : format(ms, 'h:mm a'));
export const fmtHour = (ms: number) => format(ms, 'ha');
export const fmtDay = (ms: number) => format(ms, 'EEE');
export const fmtDate = (ms: number, pattern = 'EEE, MMM d') => format(ms, pattern);

/** Index of the most recent hourly slot at or before now (the "current" hour). */
export function currentHourIndex(hours: { time: number }[]): number {
  const now = Date.now();
  let idx = 0;
  for (let i = 0; i < hours.length; i++) {
    if (hours[i].time <= now) idx = i;
    else break;
  }
  return idx;
}

export function toISODateLocal(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${m}-${day}`;
}

/* ------------------------------------------------ Material You harmonizing */

/**
 * Semantic colors (UV, AQI, alert severity, sun/rain accents) keep their meaning
 * but are hue-shifted toward the theme's primary with M3's own harmonization,
 * then re-toned for the current light/dark scheme — exactly how Material You
 * treats custom brand colors.
 */
const harmonyCache = new Map<string, { color: string; container: string; onContainer: string }>();

export function semantic(hex: string) {
  const s = $();
  const primary = s.tokens.primary ?? '#4f8ef5';
  const key = `${hex}|${primary}|${s.isDark}`;
  const hit = harmonyCache.get(key);
  if (hit) return hit;
  const h = Hct.fromInt(Blend.harmonize(argbFromHex(hex), argbFromHex(primary)));
  const chroma = Math.max(h.chroma, 36);
  const tone = (t: number) => hexFromArgb(Hct.from(h.hue, chroma, t).toInt());
  const out = s.isDark
    ? { color: tone(80), container: tone(30), onContainer: tone(90) }
    : { color: tone(40), container: tone(90), onContainer: tone(10) };
  harmonyCache.set(key, out);
  return out;
}

/** Hue-shift only (≤15°) toward primary — for map data colors whose tone carries meaning. */
export function harmonize(hex: string): string {
  const primary = $().tokens.primary;
  return primary ? hexFromArgb(Blend.harmonize(argbFromHex(hex), argbFromHex(primary))) : hex;
}

/** Linear sRGB mix of two hex colors (t = 0 → a, 1 → b). */
export function mix(a: string, b: string, t: number): string {
  const pa = argbFromHex(a), pb = argbFromHex(b);
  const ch = (v: number, s: number) => (v >> s) & 255;
  const c = (s: number) => Math.round(ch(pa, s) + (ch(pb, s) - ch(pa, s)) * t);
  return '#' + [16, 8, 0].map((s) => c(s).toString(16).padStart(2, '0')).join('');
}

/* ------------------------------------------------------- weather visuals */

export interface WeatherVisual {
  icon: string;          // Material Symbols name
  shape: string;         // M3 Expressive shape for hero containers
  hue: string;           // raw semantic color, harmonized at render time
  label: string;
}

const WMO_LABELS: Record<number, string> = {
  0: 'Clear Sky', 1: 'Mainly Clear', 2: 'Partly Cloudy', 3: 'Overcast', 45: 'Fog', 48: 'Icy Fog',
  51: 'Light Drizzle', 53: 'Drizzle', 55: 'Heavy Drizzle', 56: 'Freezing Drizzle', 57: 'Heavy Freezing Drizzle',
  61: 'Light Rain', 63: 'Rain', 65: 'Heavy Rain', 66: 'Freezing Rain', 67: 'Heavy Freezing Rain',
  71: 'Light Snow', 73: 'Snow', 75: 'Heavy Snow', 77: 'Snow Grains', 80: 'Light Showers', 81: 'Showers',
  82: 'Violent Showers', 85: 'Snow Showers', 86: 'Heavy Snow Showers', 95: 'Thunderstorm',
  96: 'Thunderstorm w/ Hail', 99: 'Severe Thunderstorm',
};

export function weatherVisual(code: number, isDay = true): WeatherVisual {
  const label = WMO_LABELS[code] ?? 'Unknown';
  if (code === 0 || code === 1)
    return isDay
      ? { icon: 'clear_day', shape: 'sunny', hue: '#f2b705', label }
      : { icon: 'bedtime', shape: '12-sided-cookie', hue: '#8d8cff', label };
  if (code === 2)
    return { icon: isDay ? 'partly_cloudy_day' : 'partly_cloudy_night', shape: 'puffy', hue: isDay ? '#e9a93a' : '#8d8cff', label };
  if (code === 3) return { icon: 'cloud', shape: 'puffy', hue: '#7d8ba3', label };
  if (code === 45 || code === 48) return { icon: 'foggy', shape: '4-sided-cookie', hue: '#8a94a6', label };
  if ([56, 57, 66, 67].includes(code)) return { icon: 'weather_mix', shape: '7-sided-cookie', hue: '#5fb4e8', label };
  if (code >= 51 && code <= 55) return { icon: 'rainy_light', shape: '6-sided-cookie', hue: '#4f8ef5', label };
  if (code === 65 || code === 82) return { icon: 'rainy_heavy', shape: '7-sided-cookie', hue: '#2f6fe0', label };
  if ((code >= 61 && code <= 63) || code === 80 || code === 81) return { icon: 'rainy', shape: '7-sided-cookie', hue: '#3d7ff0', label };
  if (code === 77) return { icon: 'grain', shape: 'flower', hue: '#7cc6e8', label };
  if (code === 75 || code === 86) return { icon: 'snowing_heavy', shape: 'flower', hue: '#6fc0e8', label };
  if (code >= 71 && code <= 86) return { icon: 'weather_snowy', shape: 'flower', hue: '#7cc6e8', label };
  if (code === 96 || code === 99) return { icon: 'weather_hail', shape: 'burst', hue: '#a463e8', label };
  if (code === 95) return { icon: 'thunderstorm', shape: 'soft-burst', hue: '#8f5ee8', label };
  return { icon: 'cloud', shape: 'puffy', hue: '#7d8ba3', label };
}

/** Seed color that follows the weather, for the "Adaptive" Material You color source. */
export function adaptiveSeed(code: number | undefined, isDay: boolean | undefined): string {
  if (code == null) return '#4f8ef5';
  if (!isDay && code <= 3) return '#5b5bd6';
  return weatherVisual(code, isDay ?? true).hue;
}

/* --------------------------------------------------------- scales */

export function uvInfo(uv: number) {
  if (uv <= 2) return { label: 'Low', hue: '#3fbf5f', tip: 'Minimal protection needed for most people.' };
  if (uv <= 5) return { label: 'Moderate', hue: '#e8c22a', tip: 'Wear sunglasses and use sunscreen on bright days.' };
  if (uv <= 7) return { label: 'High', hue: '#f08a2c', tip: 'Wear sunscreen and a hat — seek shade around midday.' };
  if (uv <= 10) return { label: 'Very High', hue: '#e5484d', tip: 'Unprotected skin can burn quickly — take extra precautions.' };
  return { label: 'Extreme', hue: '#c050e0', tip: 'Avoid sun exposure during midday hours if possible.' };
}

/** US EPA AQI breakpoints. */
export const AQI_SCALE = [
  { max: 50, hue: '#00c853', label: 'Good', desc: 'Air quality is satisfactory and poses little or no risk.' },
  { max: 100, hue: '#e8c200', label: 'Moderate', desc: 'Acceptable quality. Unusually sensitive individuals may experience minor effects.' },
  { max: 150, hue: '#ff7e00', label: 'Unhealthy for Sensitive Groups', desc: 'Sensitive groups may experience health effects. The general public is less likely to be affected.' },
  { max: 200, hue: '#ff1f1f', label: 'Unhealthy', desc: 'Everyone may begin to experience health effects. Sensitive groups may experience more serious effects.' },
  { max: 300, hue: '#8f3f97', label: 'Very Unhealthy', desc: 'Health alert — everyone may experience serious health effects.' },
  { max: 500, hue: '#7e0023', label: 'Hazardous', desc: 'Health warning of emergency conditions. The entire population is likely to be affected.' },
];

export function aqiCategory(aqi: number | null | undefined) {
  if (aqi == null || Number.isNaN(aqi)) return null;
  return AQI_SCALE.find((s) => aqi <= s.max) ?? AQI_SCALE[AQI_SCALE.length - 1];
}

export const SEVERITY_HUE: Record<string, string> = {
  Extreme: '#e5484d',
  Severe: '#f07a2c',
  Moderate: '#e8b22a',
  Minor: '#4f8ef5',
  Unknown: '#8a94a6',
};

export const SEVERITY_ORDER = ['Extreme', 'Severe', 'Moderate', 'Minor', 'Unknown'];

/** Fraction through the ~29.53-day synodic month (0 = new, 0.5 = full). */
export function moonPhase(date = new Date()) {
  const knownNewMoon = Date.UTC(2000, 0, 6, 18, 14, 0);
  const synodic = 29.53058867;
  let phase = (((date.getTime() - knownNewMoon) / 86400000) % synodic) / synodic;
  if (phase < 0) phase += 1;
  const names = [
    'New Moon', 'Waxing Crescent', 'First Quarter', 'Waxing Gibbous',
    'Full Moon', 'Waning Gibbous', 'Last Quarter', 'Waning Crescent', 'New Moon',
  ];
  const name = names[Math.min(8, Math.floor((phase + 1 / 16) * 8))];
  const illumination = Math.round(((1 - Math.cos(2 * Math.PI * phase)) / 2) * 100);
  return { phase, name, illumination };
}
