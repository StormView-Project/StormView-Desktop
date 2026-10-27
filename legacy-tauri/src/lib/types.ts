/** Shapes returned by the Rust commands (see src-tauri/src). All times are epoch ms. */

export interface Place {
  lat: number;
  lon: number;
  name: string;
  state: string;
  country: string;
  display?: string;
}

export interface Current {
  temp: number | null;
  feelsLike: number | null;
  humidity: number | null;
  windSpeed: number | null;
  windDirection: number | null;
  windDirectionLabel: string | null;
  windGusts: number | null;
  visibility: number | null;
  pressure: number | null;
  dewPoint: number | null;
  cloudCover: number | null;
  uvIndex: number | null;
  precipProb: number | null;
  precipitation: number | null;
  condition: string;
  conditionCode: number;
  isDay: boolean;
  sunrise: number | null;
  sunset: number | null;
}

export interface Hour {
  time: number;
  temp: number | null;
  feelsLike: number | null;
  precipProb: number | null;
  precipitation: number | null;
  conditionCode: number;
  condition: string;
  windSpeed: number | null;
  windDirection: number | null;
  windDirectionLabel: string | null;
  uvIndex: number | null;
  visibility: number | null;
  humidity: number | null;
  pressure: number | null;
  isDay: boolean;
}

export interface Day {
  date: number;
  tempMax: number | null;
  tempMin: number | null;
  feelsMax: number | null;
  feelsMin: number | null;
  conditionCode: number;
  condition: string;
  precipSum: number | null;
  precipProb: number | null;
  windSpeedMax: number | null;
  windGustsMax: number | null;
  uvIndexMax: number | null;
  sunrise: number | null;
  sunset: number | null;
  shortwaveRadiation: number | null;
}

export type Severity = 'Extreme' | 'Severe' | 'Moderate' | 'Minor' | 'Unknown';

export interface Alert {
  id: string;
  title: string;
  headline: string;
  description: string;
  instruction: string;
  severity: Severity | string;
  certainty: string;
  urgency: string;
  start: number | null;
  end: number | null;
}

export interface AirQuality {
  us_aqi?: number | null;
  pm2_5?: number | null;
  pm10?: number | null;
  ozone?: number | null;
  nitrogen_dioxide?: number | null;
  sulphur_dioxide?: number | null;
  carbon_monoxide?: number | null;
}

export interface WeatherData {
  current: Current;
  hourly: Hour[];
  daily: Day[];
  alerts: Alert[];
  source: 'openmeteo' | 'nws';
  airQuality: AirQuality | null;
  nwsFallback: boolean;
  notice: string | null;
  location: { name: string | null; state: string | null } | null;
  fetchedAt: number;
}

export interface OutlookDay {
  date: string;
  tempMax: number | null;
  tempMin: number | null;
  tempMean: number | null;
  precipSum: number | null;
  rainSum: number | null;
  showersSum: number | null;
  snowfallSum: number | null;
  precipType: 'rain' | 'showers' | 'snow' | 'none' | null;
  windSpeedMax: number | null;
  windGustsMax: number | null;
  humidityMean: number | null;
  cloudCoverMean: number | null;
}

export interface RadarFrame {
  time: number;
  path: string;
  host: string;
  type: 'past' | 'nowcast';
}

export interface AqiPoint {
  lat: number;
  lon: number;
  aqi: number | null;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}
