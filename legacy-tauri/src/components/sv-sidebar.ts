import { html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { formatDistanceToNowStrict } from 'date-fns';
import { refreshWeather } from '../lib/data';
import { semantic, SEVERITY_HUE } from '../lib/format';
import { store, StoreController, type WidgetId } from '../lib/store';
import type { WeatherData } from '../lib/types';
import { SvElement } from '../lib/ui';
import {
  alertsWidget, aqiWidget, daylightWidget, feelsLikeWidget, moonWidget, outlookWidget, precipOutlookWidget,
  sunriseWidget, uvWidget, windCompassWidget,
} from './widgets/simple';
import './widgets/rich';

/** The scrollable weather body shared by the desktop sidebar and the compact home page. */
@customElement('sv-weather-content')
export class SvWeatherContent extends SvElement {
  private s = new StoreController(this, (s) =>
    [s.weather, s.weatherLoading, s.weatherError, s.widgets, s.alerts, s.location, s.tokens, s.aqiShowPollutants, s.units] as const);

  private renderWidget(id: WidgetId, d: WeatherData) {
    const alerts = this.s.value[4].length ? this.s.value[4] : d.alerts;
    switch (id) {
      case 'alerts': return alertsWidget(alerts);
      case 'hourly': return html`<sv-hourly .data=${d}></sv-hourly>`;
      case 'daily': return html`<sv-daily .data=${d}></sv-daily>`;
      case 'sunrise': return sunriseWidget(d);
      case 'details': return html`<sv-details .data=${d}></sv-details>`;
      case 'aqi': return aqiWidget(d);
      case 'windCompass': return windCompassWidget(d);
      case 'feelsLike': return feelsLikeWidget(d);
      case 'precipOutlook': return precipOutlookWidget(d);
      case 'daylight': return daylightWidget(d);
      case 'uvIndex': return uvWidget(d);
      case 'moonPhase': return moonWidget();
      case 'outlook': return outlookWidget(d);
      case 'futureWeather': return html`<sv-future></sv-future>`;
      case 'monthOutlook': return html`<sv-month-outlook></sv-month-outlook>`;
    }
  }

  render() {
    const [weather, loading, error, widgets, , location] = this.s.value;
    if (!location) {
      return html`<div class="sv-empty">
        <m3e-shape name="pentagon" style="--m3e-shape-size:96px;--m3e-shape-container-color:var(--md-sys-color-tertiary-container);color:var(--md-sys-color-on-tertiary-container)">
          <m3e-icon name="location_on" filled style="--m3e-icon-size:44px"></m3e-icon>
        </m3e-shape>
        <div class="sv-title-large">Set a location to see weather</div>
        <m3e-button variant="filled" @click=${() => store.set({ settingsOpen: true, settingsTab: 'location' })}>
          <m3e-icon slot="icon" name="search"></m3e-icon>Choose location
        </m3e-button>
      </div>`;
    }
    if (!weather && loading) {
      return html`<div class="sv-skeletons">
        <m3e-skeleton shape="rounded" style="height:168px"></m3e-skeleton>
        <m3e-skeleton shape="rounded" style="height:132px"></m3e-skeleton>
        <m3e-skeleton shape="rounded" style="height:280px"></m3e-skeleton>
      </div>`;
    }
    if (!weather) {
      return html`<div class="sv-empty">
        <m3e-icon name="cloud_off" style="--m3e-icon-size:48px"></m3e-icon>
        <div class="sv-title-medium">${error ?? 'No weather data'}</div>
        <m3e-button variant="tonal" @click=${refreshWeather}><m3e-icon slot="icon" name="refresh"></m3e-icon>Try again</m3e-button>
      </div>`;
    }
    return html`
      ${error ? html`<m3e-callout variant="tertiary" class="sv-notice"><m3e-icon slot="icon" name="info"></m3e-icon>${error}</m3e-callout>` : nothing}
      <sv-current .data=${weather}></sv-current>
      ${widgets.filter((w) => w.enabled).map((w) => this.renderWidget(w.id, weather))}
      <div class="sv-attrib sv-label-small sv-muted">
        ${weather.source === 'nws' ? 'National Weather Service' : 'Open-Meteo'} · updated
        ${formatDistanceToNowStrict(weather.fetchedAt, { addSuffix: true })}
      </div>
    `;
  }
}

/** Rotating banner for active NWS alerts at this location. */
@customElement('sv-alert-banner')
export class SvAlertBanner extends SvElement {
  private s = new StoreController(this, (s) => [s.alerts, s.tokens] as const);
  @state() private idx = 0;

  render() {
    const [alerts] = this.s.value;
    if (!alerts.length) return nothing;
    const i = Math.min(this.idx, alerts.length - 1);
    const a = alerts[i];
    const c = semantic(SEVERITY_HUE[a.severity] ?? SEVERITY_HUE.Unknown);
    const multi = alerts.length > 1;
    return html`<div class="sv-banner" style="background:${c.container};color:${c.onContainer}">
      <m3e-icon name="warning" filled style="color:${c.color}"></m3e-icon>
      <span class="sv-banner-title">${a.title}</span>
      ${multi
        ? html`<m3e-icon-button size="extra-small" aria-label="Previous alert" @click=${() => (this.idx = (i - 1 + alerts.length) % alerts.length)}><m3e-icon name="chevron_left"></m3e-icon></m3e-icon-button>
            <span class="sv-label-small">${i + 1}/${alerts.length}</span>
            <m3e-icon-button size="extra-small" aria-label="Next alert" @click=${() => (this.idx = (i + 1) % alerts.length)}><m3e-icon name="chevron_right"></m3e-icon></m3e-icon-button>`
        : nothing}
      <m3e-button size="extra-small" variant="filled" style="--m3e-filled-button-container-color:${c.color};--m3e-filled-button-label-text-color:${c.container}" @click=${() => store.set({ alertDialog: alerts })}>
        ${multi ? 'See all' : 'Details'}
      </m3e-button>
    </div>`;
  }
}

/** Location + provider + refresh row, shared by the sidebar and the compact home page. */
@customElement('sv-location-header')
export class SvLocationHeader extends SvElement {
  private s = new StoreController(this, (s) => [s.location, s.weatherLoading, s.weatherAPI, s.weather?.nwsFallback] as const);

  render() {
    const [location, loading, api, fallback] = this.s.value;
    return html`
      <header class="sv-sidebar-head">
        <m3e-button variant="text" class="sv-loc-btn" @click=${() => store.set({ settingsOpen: true, settingsTab: 'location' })}>
          <m3e-icon slot="icon" name="location_on" filled></m3e-icon>
          ${location ? html`<span class="sv-loc-name">${location.name}</span>${location.state ? html`<span class="sv-loc-state">${location.state}</span>` : nothing}` : 'Set your location'}
        </m3e-button>
        <span class="sv-spacer"></span>
        ${location ? html`<span class="sv-chip sv-chip--secondary" title="Weather provider"><i class="dot dot--primary"></i>${api === 'nws' ? 'NWS' : 'Open-Meteo'}${fallback ? ' (fallback)' : ''}</span>` : nothing}
        ${location
          ? html`<m3e-icon-button aria-label="Refresh weather" @click=${refreshWeather} ?disabled=${loading}>
              ${loading ? html`<m3e-loading-indicator class="sv-small-loader"></m3e-loading-indicator>` : html`<m3e-icon name="refresh"></m3e-icon>`}
            </m3e-icon-button>`
          : nothing}
      </header>
    `;
  }
}

@customElement('sv-sidebar')
export class SvSidebar extends SvElement {
  private s = new StoreController(this, (s) => [s.sidebarPosition] as const);

  render() {
    return html`
      <aside class="sv-sidebar sv-surface-float" data-side=${this.s.value[0]}>
        <sv-location-header></sv-location-header>
        <sv-alert-banner></sv-alert-banner>
        <div class="sv-sidebar-body sv-scroll">
          <sv-weather-content></sv-weather-content>
        </div>
      </aside>
    `;
  }
}
