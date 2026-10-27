import type { ReactiveController, ReactiveControllerHost } from 'lit';
import { api } from './api';
import type { Alert, Place, RadarFrame, WeatherData } from './types';

/* ------------------------------------------------------------------ widgets */

export const WIDGET_LABELS = {
  alerts: 'Weather Alerts',
  hourly: 'Hourly Forecast',
  daily: 'Daily Forecast',
  sunrise: 'Sunrise & Sunset',
  details: 'Weather Details',
  aqi: 'Air Quality',
  windCompass: 'Wind Compass',
  feelsLike: 'Feels Like',
  precipOutlook: 'Precipitation Outlook',
  daylight: 'Daylight',
  uvIndex: 'UV Index',
  moonPhase: 'Moon Phase',
  outlook: "Today's Outlook",
  futureWeather: 'Future Weather',
  monthOutlook: 'Month Outlook',
} as const;

export type WidgetId = keyof typeof WIDGET_LABELS;

export const DEFAULT_WIDGETS: { id: WidgetId; enabled: boolean }[] = [
  { id: 'alerts', enabled: true },
  { id: 'hourly', enabled: true },
  { id: 'daily', enabled: true },
  { id: 'sunrise', enabled: true },
  { id: 'details', enabled: true },
  { id: 'aqi', enabled: false },
  { id: 'windCompass', enabled: false },
  { id: 'feelsLike', enabled: false },
  { id: 'precipOutlook', enabled: false },
  { id: 'daylight', enabled: false },
  { id: 'uvIndex', enabled: false },
  { id: 'moonPhase', enabled: false },
  { id: 'outlook', enabled: false },
  { id: 'futureWeather', enabled: false },
  { id: 'monthOutlook', enabled: false },
];

export const DETAILS_FIELD_LABELS = {
  humidity: 'Humidity',
  wind: 'Wind',
  visibility: 'Visibility',
  pressure: 'Pressure',
  dewPoint: 'Dew Point',
  uvIndex: 'UV Index',
  cloudCover: 'Cloud Cover',
  precip: 'Precipitation',
  aqi: 'Air Quality',
} as const;

export type DetailsField = keyof typeof DETAILS_FIELD_LABELS;

/* ----------------------------------------------------------------- settings */

export type ThemeVariant =
  | 'tonal-spot' | 'expressive' | 'vibrant' | 'fidelity' | 'content'
  | 'neutral' | 'monochrome' | 'rainbow' | 'fruit-salad';

export interface Settings {
  // Appearance — Material You
  themeMode: 'system' | 'light' | 'dark';
  colorSource: string;          // a preset id, 'adaptive' (follows the weather) or 'custom'
  customSeed: string;           // seed used when colorSource === 'custom'
  schemeVariant: ThemeVariant;
  contrast: 'standard' | 'medium' | 'high';
  motion: 'expressive' | 'standard';
  mapStyle: 'material' | 'streets';
  // Layout
  sidebarPosition: 'left' | 'right';
  compactLayout: boolean;       // the web app's "New Mobile Layout", used for narrow windows
  // Widgets
  widgets: { id: WidgetId; enabled: boolean }[];
  detailsFields: Record<DetailsField, boolean>;
  aqiShowPollutants: boolean;
  // Weather
  weatherAPI: 'openmeteo' | 'nws';
  units: 'imperial' | 'metric';
  location: Place | null;
  // Radar (LibreWXR)
  mapLayer: 'radar' | 'aqi';
  radarOpacity: number;
  radarTileQuality: 256 | 512;
  radarColorScheme: number;
  radarFrameFade: boolean;
  radarSmooth: boolean;
  radarSnow: boolean;
  showNowcast: boolean;
  showSatellite: boolean;
  showAlertPolygons: boolean;
  showAdvisories: boolean;
  showArrows: boolean;
  mapZoom: number;
  // Alerts / notifications
  notificationsEnabled: boolean;
  notifNws: boolean;
  notifAqi: boolean;
  runInBackground: boolean;
  // Weather assistant
  assistantProvider: 'ollama' | 'openai' | 'webllm';
  assistantBaseUrl: string;
  assistantModel: string;
  // Misc
  dismissedWhatsNewVersion: string | null;
}

export const DEFAULT_SETTINGS: Settings = {
  themeMode: 'system',
  colorSource: 'storm',
  customSeed: '#4f8ef5',
  schemeVariant: 'tonal-spot',
  contrast: 'standard',
  motion: 'expressive',
  mapStyle: 'material',
  sidebarPosition: 'left',
  compactLayout: true,
  widgets: DEFAULT_WIDGETS,
  detailsFields: {
    humidity: true, wind: true, visibility: true, pressure: true, dewPoint: true,
    uvIndex: true, cloudCover: true, precip: true, aqi: true,
  },
  aqiShowPollutants: true,
  weatherAPI: 'openmeteo',
  units: 'imperial',
  location: null,
  mapLayer: 'radar',
  radarOpacity: 0.75,
  radarTileQuality: 512,
  radarColorScheme: 7,
  radarFrameFade: false,
  radarSmooth: true,
  radarSnow: true,
  showNowcast: true,
  showSatellite: false,
  showAlertPolygons: true,
  showAdvisories: false,
  showArrows: false,
  mapZoom: 7,
  notificationsEnabled: false,
  notifNws: true,
  notifAqi: true,
  runInBackground: false,
  assistantProvider: 'ollama',
  assistantBaseUrl: 'http://localhost:11434',
  assistantModel: '',
  dismissedWhatsNewVersion: null,
};

/* ------------------------------------------------------------ runtime state */

export type SettingsTab = 'location' | 'api' | 'display' | 'widgets' | 'radar' | 'alerts' | 'assistant';

export interface RadarProgress {
  loaded: number;
  total: number;
  startTime: number;
}

export interface Runtime {
  ready: boolean;
  weather: WeatherData | null;
  weatherLoading: boolean;
  weatherError: string | null;
  alerts: Alert[];
  radarFrames: RadarFrame[];
  radarIdx: number;
  radarPlaying: boolean;
  radarSpeed: 0.5 | 1 | 2;
  radarLoading: boolean;
  radarProgress: RadarProgress | null;
  settingsOpen: boolean;
  settingsTab: SettingsTab;
  compactTab: 'home' | 'radar';
  isNarrow: boolean;
  isDark: boolean;
  /** Resolved M3 color roles from <m3e-theme>, e.g. tokens.primary → '#3a5f94'. */
  tokens: Record<string, string>;
  /** Open dialogs; each holds the payload the dialog renders. */
  alertDialog: Alert[] | null;
  welcomeOpen: boolean;
  betaOpen: boolean;
  assistantOpen: boolean;
}

export type State = Settings & Runtime;

const RUNTIME: Runtime = {
  ready: false,
  weather: null,
  weatherLoading: false,
  weatherError: null,
  alerts: [],
  radarFrames: [],
  radarIdx: 0,
  radarPlaying: false,
  radarSpeed: 1,
  radarLoading: false,
  radarProgress: null,
  settingsOpen: false,
  settingsTab: 'location',
  compactTab: 'home',
  isNarrow: false,
  isDark: false,
  tokens: {},
  alertDialog: null,
  welcomeOpen: false,
  betaOpen: false,
  assistantOpen: false,
};

export const SETTINGS_KEYS = Object.keys(DEFAULT_SETTINGS) as (keyof Settings)[];

type Listener = (s: State, prev: State) => void;

class Store {
  private state: State = { ...DEFAULT_SETTINGS, ...RUNTIME };
  private listeners = new Set<Listener>();
  private saveTimer: number | undefined;

  get(): State {
    return this.state;
  }

  set(patch: Partial<State>) {
    const prev = this.state;
    this.state = { ...prev, ...patch };
    if (SETTINGS_KEYS.some((k) => k in patch) && prev.ready) this.queueSave();
    for (const l of this.listeners) l(this.state, prev);
  }

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Loads persisted settings from Rust, reconciling widget lists with new defaults. */
  async hydrate() {
    let saved: Partial<Settings> = {};
    try {
      saved = (await api.loadSettings()) as Partial<Settings>;
    } catch (e) {
      console.warn('[store] could not load settings', e);
    }
    const merged: Partial<Settings> = {};
    for (const k of SETTINGS_KEYS) {
      if (k in saved && saved[k] !== undefined) (merged as Record<string, unknown>)[k] = saved[k];
    }
    if (Array.isArray(saved.widgets)) {
      const valid = saved.widgets.filter((w) => DEFAULT_WIDGETS.some((d) => d.id === w.id));
      const ids = new Set(valid.map((w) => w.id));
      merged.widgets = [...valid, ...DEFAULT_WIDGETS.filter((d) => !ids.has(d.id))];
    }
    merged.detailsFields = { ...DEFAULT_SETTINGS.detailsFields, ...(saved.detailsFields ?? {}) };
    this.set({ ...merged, ready: true });
  }

  settings(): Settings {
    const out = {} as Record<string, unknown>;
    for (const k of SETTINGS_KEYS) out[k] = this.state[k];
    return out as unknown as Settings;
  }

  private queueSave() {
    clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      api.saveSettings(this.settings()).catch((e) => console.error('[store] save failed', e));
    }, 300);
  }
}

export const store = new Store();

/**
 * Re-renders a Lit host only when the values picked by `select` change
 * (shallow compare), so 5×/second radar ticks don't re-render the sidebar.
 */
export class StoreController<T extends readonly unknown[]> implements ReactiveController {
  value: T;
  private unsub?: () => void;

  constructor(private host: ReactiveControllerHost, private select: (s: State) => T) {
    host.addController(this);
    this.value = select(store.get());
  }

  hostConnected() {
    this.value = this.select(store.get());
    this.unsub = store.subscribe((s) => {
      const next = this.select(s);
      if (next.length !== this.value.length || next.some((v, i) => !Object.is(v, this.value[i]))) {
        this.value = next;
        this.host.requestUpdate();
      }
    });
  }

  hostDisconnected() {
    this.unsub?.();
  }
}

/** Convenience: current state snapshot. */
export const $ = () => store.get();
