import { html, nothing } from 'lit';
import { format } from 'date-fns';
import {
  aqiCategory, currentHourIndex, fmtTime, moonPhase, semantic, SEVERITY_HUE, speedUnit, tempUnit, uvInfo, windLabel,
} from '../../lib/format';
import { $ } from '../../lib/store';
import type { Alert, WeatherData } from '../../lib/types';
import { meter, widget } from './common';

/* ----------------------------------------------------------- alerts */

export function alertsWidget(alerts: Alert[]) {
  if (!alerts.length) return nothing;
  return widget(
    'alerts',
    `Active Alerts (${alerts.length})`,
    'warning',
    html`<m3e-accordion class="sv-alert-list">
      ${alerts.map((a) => {
        const c = semantic(SEVERITY_HUE[a.severity] ?? SEVERITY_HUE.Unknown);
        return html`<m3e-expansion-panel
          class="sv-alert-panel"
          style="--m3e-expansion-panel-container-color:${c.container};--m3e-expansion-panel-text-color:${c.onContainer}"
        >
          <div slot="header" class="sv-alert-header">
            <m3e-icon name="warning" filled style="color:${c.color}"></m3e-icon>
            <span class="sv-alert-title">${a.title}</span>
            <span class="sv-chip" style="background:${c.color};color:${c.container}">${a.severity === 'Unknown' ? 'Alert' : a.severity}</span>
          </div>
          <div class="sv-alert-body">
            ${a.end ? html`<div class="sv-label-medium">Until ${format(a.end, 'EEE h:mm a')}</div>` : nothing}
            ${a.description ? html`<p class="sv-pre">${a.description}</p>` : nothing}
            ${a.instruction ? html`<p class="sv-pre sv-alert-instruction">${a.instruction}</p>` : nothing}
          </div>
        </m3e-expansion-panel>`;
      })}
    </m3e-accordion>`,
  );
}

/* ----------------------------------------------------------- sun */

export function sunriseWidget(d: WeatherData) {
  const sunrise = d.current.sunrise ?? d.daily[0]?.sunrise;
  const sunset = d.current.sunset ?? d.daily[0]?.sunset;
  if (!sunrise && !sunset) return nothing;
  const rise = semantic('#f2b705');
  const set = semantic('#f07a2c');
  return widget('sunrise', 'Sun', 'wb_twilight', html`
    <div class="sv-two-up">
      ${sunrise ? html`<div class="sv-stat-tile" style="background:${rise.container};color:${rise.onContainer}">
        <m3e-icon name="wb_twilight" filled></m3e-icon>
        <div><div class="sv-label-medium">Sunrise</div><div class="sv-title-large">${fmtTime(sunrise)}</div></div>
      </div>` : nothing}
      ${sunset ? html`<div class="sv-stat-tile" style="background:${set.container};color:${set.onContainer}">
        <m3e-icon name="bedtime" filled></m3e-icon>
        <div><div class="sv-label-medium">Sunset</div><div class="sv-title-large">${fmtTime(sunset)}</div></div>
      </div>` : nothing}
    </div>`);
}

/* ----------------------------------------------------------- air quality */

const POLLUTANTS = [
  ['pm2_5', 'PM2.5'], ['pm10', 'PM10'], ['ozone', 'Ozone'], ['nitrogen_dioxide', 'NO₂'],
] as const;

export function aqiWidget(d: WeatherData) {
  const aq = d.airQuality;
  if (aq?.us_aqi == null) return nothing;
  const cat = aqiCategory(aq.us_aqi)!;
  const c = semantic(cat.hue);
  return widget('aqi', 'Air Quality', 'air', html`
    <div class="sv-big-stat">
      <m3e-shape name="soft-burst" style="--m3e-shape-size:64px;--m3e-shape-container-color:${c.container};color:${c.onContainer}">
        <span class="sv-title-large sv-strong">${aq.us_aqi}</span>
      </m3e-shape>
      <div>
        <div class="sv-title-medium sv-strong" style="color:${c.color}">${cat.label}</div>
        <div class="sv-body-small sv-muted">US AQI</div>
      </div>
    </div>
    ${meter((aq.us_aqi / 300) * 100, c.color)}
    <p class="sv-body-medium sv-muted">${cat.desc}</p>
    ${$().aqiShowPollutants
      ? html`<div class="sv-kv-grid">
          ${POLLUTANTS.filter(([k]) => aq[k] != null).map(([k, label]) => html`
            <div class="sv-kv"><span>${label}</span><b>${Math.round(aq[k]!)} <small>µg/m³</small></b></div>`)}
        </div>`
      : nothing}`);
}

/* ----------------------------------------------------------- wind */

export function windCompassWidget(d: WeatherData) {
  const cu = d.current;
  if (cu.windSpeed == null || cu.windDirection == null) return nothing;
  const label = cu.windDirectionLabel ?? windLabel(cu.windDirection);
  const ticks = ['N', 'E', 'S', 'W'];
  return widget('windCompass', 'Wind', 'air', html`
    <div class="sv-wind">
      <div class="sv-wind-rose">
        <m3e-shape name="12-sided-cookie" style="--m3e-shape-size:120px;--m3e-shape-container-color:var(--md-sys-color-secondary-container)"></m3e-shape>
        <svg viewBox="0 0 100 100">
          ${ticks.map((t, i) => {
            const rad = (i * 90 - 90) * (Math.PI / 180);
            return html`<text x=${50 + 36 * Math.cos(rad)} y=${50 + 36 * Math.sin(rad)} text-anchor="middle" dominant-baseline="central">${t}</text>`;
          })}
          <g style="transform: rotate(${cu.windDirection + 180}deg); transform-origin: 50px 50px; transition: transform 600ms var(--sv-spring)">
            <line x1="50" y1="50" x2="50" y2="22"></line>
            <polygon points="50,14 43,26 57,26"></polygon>
            <circle cx="50" cy="50" r="4"></circle>
          </g>
        </svg>
      </div>
      <div>
        <div class="sv-display-small">${cu.windSpeed}<span class="sv-title-medium"> ${speedUnit()}</span></div>
        <div class="sv-title-small">From ${label}</div>
        ${cu.windGusts != null ? html`<div class="sv-body-medium sv-muted">Gusts ${cu.windGusts} ${speedUnit()}</div>` : nothing}
      </div>
    </div>`);
}

/* ----------------------------------------------------------- feels like */

function explainFeels(delta: number, d: WeatherData) {
  const cu = d.current;
  if (Math.abs(delta) < 3) return 'Feels about the same as the actual temperature.';
  if (delta < 0) {
    if (cu.windSpeed != null && cu.windSpeed >= 10) return `Wind is making it feel ${Math.abs(delta)}° cooler.`;
    return `Feels ${Math.abs(delta)}° cooler than the actual temperature.`;
  }
  if (cu.humidity != null && cu.humidity >= 60) return `Humidity is making it feel ${delta}° warmer.`;
  return `Feels ${delta}° warmer than the actual temperature.`;
}

export function feelsLikeWidget(d: WeatherData) {
  const cu = d.current;
  if (cu.temp == null || cu.feelsLike == null) return nothing;
  const delta = Math.round(cu.feelsLike - cu.temp);
  return widget('feelsLike', 'Feels Like', 'thermostat', html`
    <div class="sv-two-up">
      <div class="sv-stat-tile sv-stat-tile--neutral">
        <div class="sv-label-medium">Actual</div><div class="sv-headline-medium">${Math.round(cu.temp)}${tempUnit()}</div>
      </div>
      <div class="sv-stat-tile sv-stat-tile--primary">
        <div class="sv-label-medium">Feels like</div><div class="sv-headline-medium">${Math.round(cu.feelsLike)}${tempUnit()}</div>
      </div>
    </div>
    <p class="sv-body-medium sv-muted">${explainFeels(delta, d)}</p>`);
}

/* ----------------------------------------------------------- precip outlook */

export function precipOutlookWidget(d: WeatherData) {
  const start = currentHourIndex(d.hourly);
  const hours = d.hourly.slice(start, start + 8);
  if (!hours.length || hours.every((h) => h.precipProb == null)) return nothing;
  return widget('precipOutlook', 'Precipitation Outlook', 'umbrella', html`
    <div class="sv-bars">
      ${hours.map((h) => {
        const pct = h.precipProb ?? 0;
        return html`<div class="sv-bar-col">
          <span class="sv-label-small">${pct}%</span>
          <div class="sv-bar-track"><div class="sv-bar-fill" style="height:${Math.max(pct, 4)}%"></div></div>
          <span class="sv-label-small sv-muted">${format(h.time, 'ha')}</span>
        </div>`;
      })}
    </div>`);
}

/* ----------------------------------------------------------- daylight */

function duration(ms: number) {
  const min = Math.round(ms / 60000);
  return `${Math.floor(min / 60)}h ${min % 60}m`;
}

export function daylightWidget(d: WeatherData) {
  const sunrise = d.current.sunrise ?? d.daily[0]?.sunrise;
  const sunset = d.current.sunset ?? d.daily[0]?.sunset;
  if (!sunrise || !sunset) return nothing;
  const now = Date.now();
  const total = sunset - sunrise;
  const pct = Math.min(100, Math.max(0, ((now - sunrise) / total) * 100));
  const isDay = now >= sunrise && now <= sunset;
  const sun = semantic('#f2b705');
  return widget('daylight', 'Daylight', 'clear_day', html`
    <div class="sv-title-medium">${duration(total)} of daylight</div>
    <div class="sv-daylight">
      <svg viewBox="0 0 200 70" preserveAspectRatio="none">
        <path d="M6 64 Q100 -24 194 64" class="sv-daylight-arc"></path>
        <path d="M6 64 Q100 -24 194 64" class="sv-daylight-arc-on" style="stroke:${sun.color};stroke-dasharray:${pct * 2.6} 400"></path>
      </svg>
      ${isDay
        ? html`<m3e-shape name="sunny" class="sv-daylight-sun" style="--m3e-shape-size:22px;--m3e-shape-container-color:${sun.color};left:${pct}%;bottom:${
            Math.sin((pct / 100) * Math.PI) * 62}%"></m3e-shape>`
        : nothing}
    </div>
    <div class="sv-row-between sv-label-medium sv-muted">
      <span>${fmtTime(sunrise)}</span>
      ${!isDay ? html`<span>${now < sunrise ? 'Before sunrise' : 'After sunset'}</span>` : nothing}
      <span>${fmtTime(sunset)}</span>
    </div>`);
}

/* ----------------------------------------------------------- UV */

export function uvWidget(d: WeatherData) {
  const uv = d.current.uvIndex;
  if (uv == null) return nothing;
  const info = uvInfo(uv);
  const c = semantic(info.hue);
  return widget('uvIndex', 'UV Index', 'sunny', html`
    <div class="sv-big-stat">
      <span class="sv-display-small sv-strong" style="color:${c.color}">${Math.round(uv * 10) / 10}</span>
      <span class="sv-chip" style="background:${c.container};color:${c.onContainer}">${info.label}</span>
    </div>
    ${meter((uv / 11) * 100, c.color)}
    <p class="sv-body-medium sv-muted">${info.tip}</p>`);
}

/* ----------------------------------------------------------- moon */

export function moonWidget() {
  const m = moonPhase();
  // A lit disc with a shadow disc sliding across it: waxing slides it off to the
  // left, waning slides it back in from the right.
  const off = m.phase < 0.5 ? -m.phase * 200 : (1 - m.phase) * 200;
  return widget('moonPhase', 'Moon Phase', 'dark_mode', html`
    <div class="sv-big-stat">
      <div class="sv-moon" style="--off:${off}%">
        <div class="sv-moon-shadow"></div>
      </div>
      <div>
        <div class="sv-title-medium sv-strong">${m.name}</div>
        <div class="sv-body-medium sv-muted">${m.illumination}% illuminated</div>
      </div>
    </div>`);
}

/* ----------------------------------------------------------- today's outlook */

export function outlookWidget(d: WeatherData) {
  const cu = d.current;
  if (cu.temp == null) return nothing;
  const metric = $().units === 'metric';
  const aqi = d.airQuality?.us_aqi;
  const tips: { icon: string; hue: string; text: string }[] = [];
  const nextHours = d.hourly.slice(currentHourIndex(d.hourly), currentHourIndex(d.hourly) + 6);
  if ((cu.precipProb ?? 0) >= 40 || (cu.precipitation ?? 0) > 0 || nextHours.some((h) => (h.precipProb ?? 0) >= 50))
    tips.push({ icon: 'umbrella', hue: '#4f8ef5', text: 'Bring an umbrella' });
  if (cu.uvIndex != null && cu.uvIndex >= 6) tips.push({ icon: 'sunny', hue: '#f2b705', text: 'High UV — wear sunscreen' });
  if ((cu.windGusts ?? cu.windSpeed ?? 0) >= (metric ? 48 : 30)) tips.push({ icon: 'air', hue: '#7d8ba3', text: 'Windy — secure loose objects' });
  if (aqi != null && aqi >= 100) tips.push({ icon: 'masks', hue: '#f07a2c', text: 'Poor air quality — limit outdoor exertion' });
  if (cu.temp <= (metric ? 0 : 32)) tips.push({ icon: 'ac_unit', hue: '#5fb4e8', text: 'Freezing — bundle up' });
  else if (cu.temp >= (metric ? 32 : 90)) tips.push({ icon: 'emergency_heat', hue: '#e5484d', text: 'Very hot — stay hydrated' });
  if (!tips.length) tips.push({ icon: 'check_circle', hue: '#3fbf5f', text: 'Great day to be outside' });
  return widget('outlook', "Today's Outlook", 'tips_and_updates', html`
    <div class="sv-tips">
      ${tips.map((t) => {
        const c = semantic(t.hue);
        return html`<div class="sv-tip" style="background:${c.container};color:${c.onContainer}">
          <m3e-icon name=${t.icon} filled></m3e-icon><span class="sv-title-small">${t.text}</span>
        </div>`;
      })}
    </div>`);
}
