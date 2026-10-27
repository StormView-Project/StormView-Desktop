import { html, nothing, type TemplateResult } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { addDays, endOfDay, format, startOfDay } from 'date-fns';
import { api, errorText } from '../../lib/api';
import {
  aqiCategory, currentHourIndex, distUnit, fmtTime, precipUnit, semantic, speedUnit, tempUnit, toISODateLocal,
  uvInfo, weatherVisual, windLabel,
} from '../../lib/format';
import { $, DETAILS_FIELD_LABELS, StoreController } from '../../lib/store';
import type { Day, Hour, OutlookDay, WeatherData } from '../../lib/types';
import { SvElement } from '../../lib/ui';
import { widget, wxIcon, wxShape } from './common';

const has = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** Re-renders when theme tokens or units change — harmonized colors and labels depend on them. */
class SvThemed extends SvElement {
  protected themed = new StoreController(this, (s) => [s.tokens, s.units, s.location] as const);
}

/* ======================================================= current conditions */

@customElement('sv-current')
export class SvCurrent extends SvThemed {
  @property({ attribute: false }) data!: WeatherData;
  render() {
    const cu = this.data.current;
    const today = this.data.daily[0];
    const v = weatherVisual(cu.conditionCode, cu.isDay);
    return html`
      <section class="sv-hero">
        <div class="sv-hero-main">
          <div class="sv-hero-temp">
            <span class="sv-hero-number">${cu.temp ?? '—'}</span><span class="sv-hero-unit">${tempUnit()}</span>
          </div>
          ${wxShape(cu.conditionCode, cu.isDay, 112)}
        </div>
        <div class="sv-hero-condition">${cu.condition || v.label}</div>
        <div class="sv-hero-chips">
          ${cu.feelsLike != null ? html`<span class="sv-chip sv-chip--surface"><m3e-icon name="thermostat"></m3e-icon>Feels ${cu.feelsLike}°</span>` : nothing}
          ${today
            ? html`<span class="sv-chip sv-chip--surface"><m3e-icon name="arrow_upward"></m3e-icon>${today.tempMax ?? '—'}°</span>
                <span class="sv-chip sv-chip--surface"><m3e-icon name="arrow_downward"></m3e-icon>${today.tempMin ?? '—'}°</span>`
            : nothing}
        </div>
      </section>
    `;
  }
}

/* ============================================================== hourly */

function hourRows(h: Hour) {
  const rows: { icon: string; hue: string; label: string; value: string; valueHue?: string }[] = [];
  if (has(h.feelsLike)) rows.push({ icon: 'thermostat', hue: '#a78bfa', label: 'Feels Like', value: `${h.feelsLike}${tempUnit()}` });
  if (has(h.precipProb)) rows.push({ icon: 'water_drop', hue: '#4f8ef5', label: 'Precip. Chance', value: `${h.precipProb}%` });
  if (has(h.precipitation) && h.precipitation > 0) rows.push({ icon: 'rainy', hue: '#4f8ef5', label: 'Precip. Amount', value: `${h.precipitation} ${precipUnit()}` });
  if (has(h.uvIndex)) {
    const u = uvInfo(h.uvIndex);
    rows.push({ icon: 'sunny', hue: '#f2b705', label: 'UV Index', value: `${Math.round(h.uvIndex)} · ${u.label}`, valueHue: u.hue });
  }
  const dir = h.windDirectionLabel ?? (has(h.windDirection) ? windLabel(h.windDirection) : null);
  if (has(h.windSpeed)) rows.push({ icon: 'air', hue: '#7d8ba3', label: 'Wind', value: `${dir ? dir + ' ' : ''}${h.windSpeed} ${speedUnit()}` });
  if (has(h.humidity)) rows.push({ icon: 'humidity_percentage', hue: '#3fbf9f', label: 'Humidity', value: `${h.humidity}%` });
  if (has(h.pressure)) rows.push({ icon: 'compress', hue: '#3fbf5f', label: 'Pressure', value: `${h.pressure} hPa` });
  if (has(h.visibility)) rows.push({ icon: 'visibility', hue: '#a78bfa', label: 'Visibility', value: `${h.visibility} ${distUnit()}` });
  return rows;
}

function tileGrid(rows: ReturnType<typeof hourRows>) {
  return html`<div class="sv-tile-grid">
    ${rows.map((r) => {
      const c = semantic(r.hue);
      return html`<div class="sv-tile">
        <span class="sv-tile-icon" style="background:${c.container};color:${c.onContainer}"><m3e-icon name=${r.icon} filled></m3e-icon></span>
        <div><div class="sv-label-medium sv-muted">${r.label}</div>
        <div class="sv-title-small" style=${r.valueHue ? `color:${semantic(r.valueHue).color}` : ''}>${r.value}</div></div>
      </div>`;
    })}
  </div>`;
}

@customElement('sv-hourly')
export class SvHourly extends SvThemed {
  @property({ attribute: false }) data!: WeatherData;
  @state() private selected: Hour | null = null;

  render() {
    const start = currentHourIndex(this.data.hourly);
    const hours = this.data.hourly.slice(start, start + 24);
    if (!hours.length) return nothing;
    const temps = hours.map((h) => h.temp ?? 0);
    const min = Math.min(...temps), max = Math.max(...temps);
    const sel = this.selected;
    return html`
      ${widget('hourly', 'Hourly', 'schedule', html`
        <div class="sv-hourly-scroll">
          ${hours.map((h, i) => {
            const lift = max === min ? 50 : (((h.temp ?? min) - min) / (max - min)) * 100;
            return html`<button class="sv-hour" @click=${() => (this.selected = h)} title="View hour details">
              <span class="sv-label-medium">${i === 0 ? 'Now' : format(h.time, 'ha')}</span>
              ${wxIcon(h.conditionCode, h.isDay, 26)}
              <span class="sv-hour-precip">${(h.precipProb ?? 0) > 10 ? html`<m3e-icon name="water_drop" filled></m3e-icon>${h.precipProb}%` : ''}</span>
              <span class="sv-hour-temp-wrap"><span class="sv-title-medium sv-hour-temp" style="bottom:${lift * 0.5}%">${h.temp ?? '—'}°</span></span>
            </button>`;
          })}
        </div>`)}
      <m3e-dialog class="sv-dialog" ?open=${!!sel} dismissible @closed=${() => (this.selected = null)}>
        ${sel
          ? html`<div slot="header" class="sv-dialog-head">
                <div class="sv-headline-small">${format(sel.time, 'h:mm a')}</div>
                <div class="sv-body-medium sv-muted">${format(sel.time, 'EEEE, MMM d')}</div>
              </div>
              <div class="sv-hour-summary">
                ${wxShape(sel.conditionCode, sel.isDay, 72)}
                <div><div class="sv-display-small">${sel.temp}${tempUnit()}</div><div class="sv-title-medium">${sel.condition || weatherVisual(sel.conditionCode).label}</div></div>
              </div>
              ${tileGrid(hourRows(sel))}`
          : nothing}
      </m3e-dialog>
    `;
  }
}

/* =============================================================== daily */

function dayDetail(day: Day, hourly: Hour[]): TemplateResult {
  const dayStart = startOfDay(day.date).getTime();
  const dayEnd = endOfDay(day.date).getTime();
  const all = hourly.filter((h) => h.time >= dayStart && h.time <= dayEnd);
  const nowFloor = new Date().setMinutes(0, 0, 0);
  const hours = startOfDay(Date.now()).getTime() === dayStart ? all.filter((h) => h.time >= nowFloor) : all;
  const avg = (vals: number[]) => (vals.length ? Math.round(vals.reduce((a, b) => a + b, 0) / vals.length) : null);
  const humidity = avg(all.map((h) => h.humidity).filter(has));
  const pressure = avg(all.map((h) => h.pressure).filter(has));
  const uvVals = all.map((h) => h.uvIndex).filter(has);
  const uv = has(day.uvIndexMax) ? day.uvIndexMax : uvVals.length ? Math.max(...uvVals) : null;
  const feelsVals = all.map((h) => h.feelsLike).filter(has);
  const feelsHi = has(day.feelsMax) ? day.feelsMax : feelsVals.length ? Math.max(...feelsVals) : null;
  const feelsLo = has(day.feelsMin) ? day.feelsMin : feelsVals.length ? Math.min(...feelsVals) : null;

  const rows: ReturnType<typeof hourRows> = [];
  if (has(day.windSpeedMax)) rows.push({ icon: 'air', hue: '#7d8ba3', label: 'Max Wind', value: `${day.windSpeedMax} ${speedUnit()}${has(day.windGustsMax) ? ` · gusts ${day.windGustsMax}` : ''}` });
  if (uv != null) rows.push({ icon: 'sunny', hue: '#f2b705', label: 'UV Index', value: `${Math.round(uv)} — ${uvInfo(uv).label}`, valueHue: uvInfo(uv).hue });
  if (day.precipProb != null) rows.push({
    icon: 'rainy', hue: '#4f8ef5', label: 'Precipitation',
    value: (day.precipSum ?? 0) > 0 ? `${day.precipSum} ${precipUnit()} · ${day.precipProb}%` : `${day.precipProb}%`,
  });
  if (day.sunrise) rows.push({ icon: 'wb_twilight', hue: '#f2b705', label: 'Sunrise', value: fmtTime(day.sunrise) });
  if (day.sunset) rows.push({ icon: 'bedtime', hue: '#f07a2c', label: 'Sunset', value: fmtTime(day.sunset) });
  if (humidity != null) rows.push({ icon: 'humidity_percentage', hue: '#4f8ef5', label: 'Avg Humidity', value: `${humidity}%` });
  if (pressure != null) rows.push({ icon: 'compress', hue: '#3fbf5f', label: 'Avg Pressure', value: `${pressure} hPa` });
  if (feelsHi != null) rows.push({ icon: 'thermostat', hue: '#a78bfa', label: 'Feels Like', value: `${Math.round(feelsHi)}${tempUnit()}${feelsLo != null ? ` / ${Math.round(feelsLo)}°` : ''}` });

  return html`
    <div class="sv-day-detail">
      <div class="sv-label-large sv-muted">Hourly</div>
      ${hours.length
        ? html`<div class="sv-mini-hours">
            ${hours.map((h) => html`<div class="sv-mini-hour">
              <span class="sv-label-small">${format(h.time, 'ha').toLowerCase()}</span>
              ${wxIcon(h.conditionCode, h.isDay, 20)}
              ${(h.precipProb ?? 0) > 10 ? html`<span class="sv-label-small sv-rain">${h.precipProb}%</span>` : html`<span></span>`}
              <span class="sv-title-small">${h.temp}°</span>
            </div>`)}
          </div>`
        : html`<p class="sv-body-small sv-muted">No hourly data for this day</p>`}
      ${rows.length ? html`<div class="sv-label-large sv-muted">Details</div>${tileGrid(rows)}` : nothing}
    </div>`;
}

@customElement('sv-daily')
export class SvDaily extends SvThemed {
  @property({ attribute: false }) data!: WeatherData;
  render() {
    const daily = this.data.daily;
    if (!daily.length) return nothing;
    const lo = Math.min(...daily.map((d) => d.tempMin ?? Infinity));
    const hi = Math.max(...daily.map((d) => d.tempMax ?? -Infinity));
    const range = hi - lo || 1;
    return widget('daily', `${daily.length}-Day Forecast`, 'calendar_month', html`
      <m3e-accordion class="sv-days">
        ${daily.map((d, i) => {
          const left = (((d.tempMin ?? lo) - lo) / range) * 100;
          const width = Math.max(4, (((d.tempMax ?? hi) - (d.tempMin ?? lo)) / range) * 100);
          return html`<m3e-expansion-panel class="sv-day-panel">
            <div slot="header" class="sv-day-row">
              <span class="sv-title-small sv-day-name">${i === 0 ? 'Today' : format(d.date, 'EEE')}</span>
              ${wxIcon(d.conditionCode, true, 24)}
              <span class="sv-label-medium sv-rain sv-day-pop">${(d.precipProb ?? 0) > 10 ? `${d.precipProb}%` : ''}</span>
              <span class="sv-title-small sv-muted sv-day-lo">${d.tempMin ?? '—'}°</span>
              <span class="sv-range"><span class="sv-range-fill" style="left:${left}%;width:${width}%"></span></span>
              <span class="sv-title-small sv-day-hi">${d.tempMax ?? '—'}°</span>
            </div>
            ${dayDetail(d, this.data.hourly)}
          </m3e-expansion-panel>`;
        })}
      </m3e-accordion>`);
  }
}

/* ============================================================= details */

@customElement('sv-details')
export class SvDetails extends SvThemed {
  @property({ attribute: false }) data!: WeatherData;
  @state() private precipOpen = false;
  private s = new StoreController(this, (s) => [s.detailsFields] as const);

  render() {
    const [fields] = this.s.value;
    const cu = this.data.current;
    const aq = this.data.airQuality ?? {};
    const today = this.data.daily[0];
    const pu = precipUnit();
    const uv = cu.uvIndex != null ? uvInfo(cu.uvIndex) : null;
    const aqi = aqiCategory(aq.us_aqi);
    const items: { key: keyof typeof DETAILS_FIELD_LABELS; icon: string; hue: string; label: string; value: string; sub?: string; valueHue?: string; onClick?: () => void }[] = [
      { key: 'humidity', icon: 'humidity_percentage', hue: '#4f8ef5', label: 'Humidity', value: cu.humidity != null ? `${cu.humidity}%` : '—' },
      { key: 'wind', icon: 'air', hue: '#7d8ba3', label: 'Wind', value: cu.windSpeed != null ? `${cu.windDirectionLabel ?? windLabel(cu.windDirection)} ${cu.windSpeed} ${speedUnit()}` : '—', sub: cu.windGusts != null ? `Gusts ${cu.windGusts} ${speedUnit()}` : undefined },
      { key: 'visibility', icon: 'visibility', hue: '#a78bfa', label: 'Visibility', value: cu.visibility != null ? `${cu.visibility} ${distUnit()}` : '—' },
      { key: 'pressure', icon: 'compress', hue: '#3fbf5f', label: 'Pressure', value: cu.pressure != null ? `${cu.pressure} hPa` : '—' },
      { key: 'dewPoint', icon: 'dew_point', hue: '#3fbf9f', label: 'Dew Point', value: cu.dewPoint != null ? `${cu.dewPoint}${tempUnit()}` : '—' },
      { key: 'uvIndex', icon: 'sunny', hue: '#f2b705', label: 'UV Index', value: uv ? `${cu.uvIndex} ${uv.label}` : '—', valueHue: uv?.hue },
      { key: 'cloudCover', icon: 'cloud', hue: '#7d8ba3', label: 'Cloud Cover', value: cu.cloudCover != null ? `${cu.cloudCover}%` : '—' },
      {
        key: 'precip', icon: 'rainy', hue: '#4f8ef5', label: 'Precip. Today',
        // The current-hour amount badly understates a day's rain; prefer the daily total.
        value: today?.precipSum != null ? `${today.precipSum} ${pu}` : cu.precipitation != null ? `${cu.precipitation} ${pu}` : cu.precipProb != null ? `${cu.precipProb}% chance` : '—',
        onClick: () => (this.precipOpen = true),
      },
      { key: 'aqi', icon: 'masks', hue: '#7d8ba3', label: 'Air Quality', value: aqi ? `${aq.us_aqi} ${aqi.label.replace('Unhealthy for Sensitive Groups', 'Sensitive')}` : '—', valueHue: aqi?.hue },
    ];
    const visible = items
      .filter((i) => fields[i.key] !== false)
      .filter((i) => i.value !== '—' || i.key === 'humidity' || i.key === 'uvIndex');

    return html`
      ${widget('details', 'Details', 'info', html`
        <div class="sv-details-grid">
          ${visible.map((i) => {
            const c = semantic(i.hue);
            return html`<m3e-card
              variant="filled"
              class="sv-detail"
              ?actionable=${!!i.onClick}
              @click=${i.onClick ?? (() => {})}
            >
              <div class="sv-detail-inner">
                <span class="sv-tile-icon" style="background:${c.container};color:${c.onContainer}"><m3e-icon name=${i.icon} filled></m3e-icon></span>
                <div class="sv-label-medium sv-muted">${i.label}${i.onClick ? html`<m3e-icon name="chevron_right" class="sv-inline-icon"></m3e-icon>` : nothing}</div>
                <div class="sv-title-medium" style=${i.valueHue ? `color:${semantic(i.valueHue).color}` : ''}>${i.value}</div>
                ${i.sub ? html`<div class="sv-label-small sv-muted">${i.sub}</div>` : nothing}
              </div>
            </m3e-card>`;
          })}
        </div>`)}
      <m3e-dialog class="sv-dialog" ?open=${this.precipOpen} dismissible @closed=${() => (this.precipOpen = false)}>
        ${this.precipOpen ? this.precipDialog() : nothing}
      </m3e-dialog>
    `;
  }

  private precipDialog() {
    const todayStr = new Date().toDateString();
    const hours = this.data.hourly.filter((h) => new Date(h.time).toDateString() === todayStr);
    const today = this.data.daily[0];
    const pu = precipUnit();
    const total = today?.precipSum ?? (hours.some((h) => h.precipitation != null) ? hours.reduce((s, h) => s + (h.precipitation ?? 0), 0) : null);
    const peak = Math.max(0, today?.precipProb ?? 0, ...hours.map((h) => h.precipProb ?? 0));
    return html`
      <div slot="header" class="sv-dialog-head"><m3e-icon name="rainy" filled></m3e-icon><span>Precipitation Today</span></div>
      <div class="sv-two-up">
        <div class="sv-stat-tile sv-stat-tile--primary"><div class="sv-headline-small">${total != null ? `${total.toFixed(2)} ${pu}` : '—'}</div><div class="sv-label-medium">Total today</div></div>
        <div class="sv-stat-tile sv-stat-tile--tertiary"><div class="sv-headline-small">${peak}%</div><div class="sv-label-medium">Peak chance</div></div>
      </div>
      ${hours.length
        ? html`<div class="sv-bars sv-bars--tall">
            ${hours.map((h) => {
              const pct = h.precipProb ?? 0;
              return html`<div class="sv-bar-col ${h.time < Date.now() ? 'past' : ''}">
                <span class="sv-label-small">${pct > 0 ? `${pct}%` : ''}</span>
                <div class="sv-bar-track"><div class="sv-bar-fill" style="height:${pct > 0 ? Math.max(pct, 6) : 0}%"></div></div>
                <span class="sv-label-small sv-muted">${format(h.time, 'ha')}</span>
                ${(h.precipitation ?? 0) > 0 ? html`<span class="sv-label-small sv-rain">${h.precipitation}</span>` : nothing}
              </div>`;
            })}
          </div>`
        : html`<p class="sv-muted">No hourly data available.</p>`}
    `;
  }
}

/* ======================================================= long range */

export const MIN_FUTURE_DAYS = 17;
export const MAX_FUTURE_DAYS = 210;

function precipBadge(t: OutlookDay['precipType']) {
  switch (t) {
    case 'snow': return { label: 'Snow', icon: 'weather_snowy', hue: '#7cc6e8' };
    case 'showers': return { label: 'Showers', icon: 'rainy_light', hue: '#4f8ef5' };
    case 'rain': return { label: 'Rain', icon: 'rainy', hue: '#3d7ff0' };
    case 'none': return { label: 'Dry', icon: 'cloud', hue: '#7d8ba3' };
    default: return null;
  }
}

@customElement('sv-future')
export class SvFuture extends SvThemed {
  @state() private status: 'idle' | 'loading' | 'ready' | 'error' | 'empty' = 'idle';
  @state() private result: OutlookDay | null = null;
  @state() private error = '';

  private async pick(e: Event) {
    const value = (e.target as unknown as { value: Date | null }).value;
    const loc = $().location;
    if (!value || !loc) {
      this.status = 'idle';
      return;
    }
    this.status = 'loading';
    try {
      const iso = toISODateLocal(value);
      const days = await api.futureRange(loc.lat, loc.lon, iso, iso, $().units);
      this.result = days[0] ?? null;
      this.status = this.result ? 'ready' : 'empty';
    } catch (err) {
      this.error = errorText(err);
      this.status = 'error';
    }
  }

  render() {
    if (!$().location) return nothing;
    const min = addDays(new Date(), MIN_FUTURE_DAYS);
    const max = addDays(new Date(), MAX_FUTURE_DAYS);
    const r = this.result;
    const badge = r ? precipBadge(r.precipType) : null;
    return widget('futureWeather', 'Future Weather', 'event_upcoming', html`
      <p class="sv-body-medium sv-muted">Pick a date up to ~7 months out to see its long-range outlook.</p>
      <m3e-form-field variant="outlined" class="sv-field">
        <label slot="label" for="future-date">Date</label>
        <m3e-date-input id="future-date" .minDate=${min} .maxDate=${max} @change=${this.pick}></m3e-date-input>
        <m3e-icon-button slot="suffix" aria-label="Open calendar">
          <m3e-icon name="calendar_month"></m3e-icon>
          <m3e-datepicker-toggle for="future-picker"></m3e-datepicker-toggle>
        </m3e-icon-button>
      </m3e-form-field>
      <m3e-datepicker id="future-picker" for="future-date" .minDate=${min} .maxDate=${max} .startAt=${min}></m3e-datepicker>
      ${this.status === 'loading' ? html`<div class="sv-status"><m3e-loading-indicator></m3e-loading-indicator>Loading outlook…</div>` : nothing}
      ${this.status === 'error' ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>Couldn't load a forecast for that date. ${this.error}</m3e-callout>` : nothing}
      ${this.status === 'empty' ? html`<p class="sv-muted">No outlook data available for that date yet.</p>` : nothing}
      ${this.status === 'ready' && r
        ? html`<div class="sv-future-result">
            <div class="sv-row-between">
              <div class="sv-headline-small">
                ${r.tempMax != null ? html`<span>H ${r.tempMax}${tempUnit()}</span>` : nothing}
                ${r.tempMin != null ? html`<span class="sv-muted"> L ${r.tempMin}${tempUnit()}</span>` : nothing}
              </div>
              ${badge ? html`<span class="sv-chip" style="background:${semantic(badge.hue).container};color:${semantic(badge.hue).onContainer}"><m3e-icon name=${badge.icon} filled></m3e-icon>${badge.label}</span>` : nothing}
            </div>
            <div class="sv-kv-grid">
              ${r.precipSum != null ? html`<div class="sv-kv"><span>Precip.</span><b>${r.precipSum} ${precipUnit()}</b></div>` : nothing}
              ${r.windSpeedMax != null ? html`<div class="sv-kv"><span>Wind</span><b>${r.windSpeedMax} ${speedUnit()}</b></div>` : nothing}
              ${r.humidityMean != null ? html`<div class="sv-kv"><span>Humidity</span><b>${r.humidityMean}%</b></div>` : nothing}
              ${r.cloudCoverMean != null ? html`<div class="sv-kv"><span>Cloud Cover</span><b>${r.cloudCoverMean}%</b></div>` : nothing}
            </div>
          </div>`
        : nothing}`);
  }
}

@customElement('sv-month-outlook')
export class SvMonthOutlook extends SvThemed {
  @state() private status: 'idle' | 'loading' | 'ready' | 'error' | 'empty' = 'idle';
  @state() private days: OutlookDay[] = [];
  @state() private month = '';

  private options() {
    const out: { value: string; label: string }[] = [];
    const now = new Date();
    const end = addDays(now, MAX_FUTURE_DAYS);
    const cur = new Date(now.getFullYear(), now.getMonth(), 1);
    while (cur <= end) {
      out.push({ value: `${cur.getFullYear()}-${String(cur.getMonth() + 1).padStart(2, '0')}`, label: format(cur, 'MMMM yyyy') });
      cur.setMonth(cur.getMonth() + 1);
    }
    return out;
  }

  private async pick(e: Event) {
    const value = (e.target as unknown as { value: string }).value;
    const loc = $().location;
    this.month = value;
    if (!value || !loc) return;
    this.status = 'loading';
    const [y, m] = value.split('-').map(Number);
    try {
      const days = await api.futureRange(loc.lat, loc.lon, toISODateLocal(new Date(y, m - 1, 1)), toISODateLocal(new Date(y, m, 0)), $().units);
      this.days = days;
      this.status = days.length ? 'ready' : 'empty';
    } catch {
      this.status = 'error';
    }
  }

  render() {
    if (!$().location) return nothing;
    const firstSnow = this.days.findIndex((d) => d.precipType === 'snow');
    return widget('monthOutlook', 'Month Outlook', 'date_range', html`
      <p class="sv-body-medium sv-muted">Long-range daily outlook for a whole month — a statistical estimate, not a precise forecast.</p>
      <m3e-form-field variant="outlined" class="sv-field">
        <label slot="label" for="month-select">Month</label>
        <m3e-select id="month-select" @change=${this.pick}>
          ${this.options().map((o) => html`<m3e-option value=${o.value} ?selected=${o.value === this.month}>${o.label}</m3e-option>`)}
        </m3e-select>
      </m3e-form-field>
      ${this.status === 'loading' ? html`<div class="sv-status"><m3e-loading-indicator></m3e-loading-indicator>Loading month outlook…</div>` : nothing}
      ${this.status === 'error' ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>Couldn't load that month.</m3e-callout>` : nothing}
      ${this.status === 'empty' ? html`<p class="sv-muted">No outlook data available for that month yet.</p>` : nothing}
      ${this.status === 'ready'
        ? html`<div class="sv-month-list">
            ${this.days.map((d, i) => {
              const date = new Date(`${d.date}T00:00:00`);
              const b = precipBadge(d.precipType);
              return html`<div class="sv-month-row ${i === firstSnow ? 'flag' : ''}">
                <span class="sv-month-date"><span class="sv-label-small sv-muted">${format(date, 'EEE')}</span><span class="sv-title-small">${date.getDate()}</span></span>
                <span class="sv-title-small">${d.tempMax ?? '—'}°</span>
                <span class="sv-body-medium sv-muted">${d.tempMin ?? '—'}°</span>
                <span class="sv-month-precip">
                  ${b ? html`<m3e-icon name=${b.icon} filled style="color:${semantic(b.hue).color}"></m3e-icon>` : nothing}
                  ${(d.precipSum ?? 0) > 0 ? html`<span class="sv-label-small">${d.precipSum}${precipUnit()}</span>` : nothing}
                </span>
                ${i === firstSnow ? html`<span class="sv-chip sv-chip--tertiary">First snow</span>` : nothing}
              </div>`;
            })}
          </div>`
        : nothing}`);
  }
}
