import { html, nothing, type TemplateResult } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { api, errorText } from '../lib/api';
import { clearWebLLMCache, WEBLLM_MODELS } from '../lib/assistant';
import { locateMe, setLocation } from '../lib/data';
import { COLOR_PRESETS, previewSwatch, VARIANTS } from '../lib/theme';
import {
  $, DETAILS_FIELD_LABELS, SETTINGS_KEYS, store, StoreController, WIDGET_LABELS, type DetailsField, type Settings, type SettingsTab,
} from '../lib/store';
import type { Place } from '../lib/types';
import { checkedOf, openExternal, SvElement, toast, valueOf } from '../lib/ui';

const TABS: { id: SettingsTab; label: string; icon: string }[] = [
  { id: 'location', label: 'Location', icon: 'location_on' },
  { id: 'api', label: 'Weather', icon: 'partly_cloudy_day' },
  { id: 'display', label: 'Display', icon: 'palette' },
  { id: 'widgets', label: 'Widgets', icon: 'dashboard_customize' },
  { id: 'radar', label: 'Radar', icon: 'radar' },
  { id: 'alerts', label: 'Alerts', icon: 'notifications' },
  { id: 'assistant', label: 'Assistant', icon: 'smart_toy' },
];

/** LibreWXR color scheme ids (the integer in the tile URL) with representative ramps. */
const COLOR_SCHEMES = [
  { id: 0, name: 'B&W', g: '#000, #888, #fff' },
  { id: 1, name: 'RainViewer', g: 'transparent, #0000ff, #00ff00, #ffff00, #ff8000, #ff0000' },
  { id: 2, name: 'Universal Blue', g: 'transparent, #1a1aff, #0066ff, #00ccff, #66ffff, #fff' },
  { id: 3, name: 'TITAN', g: 'transparent, #0000ff, #00ffff, #00ff00, #ffff00, #ff0000, #ff00ff' },
  { id: 4, name: 'Weather Channel', g: 'transparent, #006600, #00cc00, #ffff00, #ff6600, #ff0000, #cc00cc' },
  { id: 5, name: 'Meteored', g: 'transparent, #004488, #0088cc, #00cc88, #88cc00, #cc8800, #cc0000' },
  { id: 6, name: 'NEXRAD III', g: '#000, #004400, #008800, #00cc00, #ffff00, #ff8800, #ff0000, #cc00cc' },
  { id: 7, name: 'Rainbow', g: 'transparent, #0000ff, #00ffff, #00ff00, #ffff00, #ff8000, #ff0000' },
  { id: 8, name: 'Dark Sky', g: 'transparent, #0d1117, #1f3a5c, #2d6a9f, #63b3ed, #fff' },
  { id: 9, name: 'Datameteo', g: 'transparent, #320064, #6400c8, #9600ff, #c864ff, #ff96ff' },
  { id: 10, name: 'Viper HD', g: 'transparent, #00004c, #0000ff, #00ffff, #ffff00, #ff0000' },
  { id: 11, name: 'MRMS CREF', g: '#000, #003366, #0066cc, #00cc66, #66ff00, #ffcc00, #ff6600, #ff0000' },
];

const set = (patch: Partial<Settings>) => store.set(patch);

/* ----------------------------------------------------------- primitives */

function group(title: string, body: TemplateResult | (TemplateResult | typeof nothing)[], sub?: string) {
  return html`<section class="sv-set-group">
    <h3 class="sv-set-label">${title}</h3>
    ${sub ? html`<p class="sv-body-medium sv-muted sv-set-sub">${sub}</p>` : nothing}
    <div class="sv-set-card">${body}</div>
  </section>`;
}

function row(label: string, control: TemplateResult, sub?: string | TemplateResult, stack = false) {
  return html`<div class="sv-set-row ${stack ? 'stack' : ''}">
    <div class="sv-set-text"><div class="sv-body-large">${label}</div>${sub ? html`<div class="sv-body-small sv-muted">${sub}</div>` : nothing}</div>
    ${control}
  </div>`;
}

function toggle(label: string, checked: boolean, onChange: (v: boolean) => void, sub?: string) {
  return row(label, html`<m3e-switch icons="selected" ?checked=${checked} @change=${(e: Event) => onChange(checkedOf(e))}></m3e-switch>`, sub);
}

function segmented<T extends string | number>(opts: { label: string; value: T; icon?: string }[], value: T, onChange: (v: T) => void) {
  return html`<m3e-segmented-button
    class="sv-seg"
    @change=${(e: Event) => {
      const v = valueOf(e);
      const opt = opts.find((o) => String(o.value) === String(v));
      if (opt) onChange(opt.value);
    }}
  >
    ${opts.map((o) => html`<m3e-button-segment value=${String(o.value)} ?checked=${o.value === value}>
      ${o.icon ? html`<m3e-icon slot="icon" name=${o.icon}></m3e-icon>` : nothing}${o.label}
    </m3e-button-segment>`)}
  </m3e-segmented-button>`;
}

/* ---------------------------------------------------------------- panel */

@customElement('sv-settings')
export class SvSettings extends SvElement {
  protected watch = new StoreController(this, (st) => [st.settingsOpen, st.settingsTab, st.isDark, st.tokens, ...SETTINGS_KEYS.map((k) => st[k])]);

  @state() private query = '';
  @state() private results: Place[] = [];
  @state() private searching = false;
  @state() private locating = false;
  @state() private locError = '';
  @state() private advisoryConfirm = false;
  @state() private models: string[] = [];
  @state() private modelsError = '';
  @state() private modelsLoading = false;
  private searchTimer: number | undefined;

  render() {
    const s = $();
    return html`
      <div class="sv-sheet-scrim ${s.settingsOpen ? 'open' : ''}" @click=${() => store.set({ settingsOpen: false })}></div>
      <aside class="sv-sheet ${s.settingsOpen ? 'open' : ''}" data-side=${s.sidebarPosition === 'left' ? 'right' : 'left'} aria-hidden=${!s.settingsOpen}>
        <header class="sv-sheet-head">
          <m3e-heading variant="headline" size="small">Settings</m3e-heading>
          <span class="sv-spacer"></span>
          <m3e-icon-button aria-label="Close settings" @click=${() => store.set({ settingsOpen: false })}><m3e-icon name="close"></m3e-icon></m3e-icon-button>
        </header>
        <m3e-tabs variant="primary" class="sv-sheet-tabs" @change=${(e: Event) => {
          const t = (e.currentTarget as unknown as { selectedIndex: number }).selectedIndex;
          if (TABS[t]) store.set({ settingsTab: TABS[t].id });
        }}>
          ${TABS.map((t) => html`<m3e-tab ?selected=${s.settingsTab === t.id}><m3e-icon slot="icon" name=${t.icon}></m3e-icon>${t.label}</m3e-tab>`)}
        </m3e-tabs>
        <div class="sv-sheet-body sv-scroll">
          ${s.settingsOpen ? this.tab(s.settingsTab) : nothing}
        </div>
        <footer class="sv-sheet-foot">
          <span class="sv-chip sv-chip--tertiary">BETA</span>
          <span class="sv-body-small sv-muted">StormView is in active development — bugs and missing features are expected.</span>
          <m3e-button variant="text" size="extra-small" @click=${() => openExternal('https://github.com/arc360alt/StormView-Rewrite/issues')}>
            Report an issue<m3e-icon slot="trailing-icon" name="open_in_new"></m3e-icon>
          </m3e-button>
        </footer>
      </aside>
      <m3e-dialog class="sv-dialog" ?open=${this.advisoryConfirm} @closed=${() => (this.advisoryConfirm = false)}>
        <div slot="header" class="sv-dialog-head"><m3e-icon name="speed" filled></m3e-icon>Performance warning</div>
        <p class="sv-body-medium">Enabling advisories fetches zone boundary data from the NWS API for every active advisory in the country — this can be <b>dozens of extra network requests</b> and may noticeably slow things down on first load.</p>
        <p class="sv-body-medium sv-muted">Zone boundaries are cached after the first fetch, so later refreshes are faster.</p>
        <div slot="actions" end>
          <m3e-button variant="text"><m3e-dialog-action return-value="cancel">Cancel</m3e-dialog-action></m3e-button>
          <m3e-button variant="filled" @click=${() => set({ showAdvisories: true })}><m3e-dialog-action return-value="ok">Enable anyway</m3e-dialog-action></m3e-button>
        </div>
      </m3e-dialog>
    `;
  }

  private tab(id: SettingsTab) {
    switch (id) {
      case 'location': return this.locationTab();
      case 'api': return this.weatherTab();
      case 'display': return this.displayTab();
      case 'widgets': return this.widgetsTab();
      case 'radar': return this.radarTab();
      case 'alerts': return this.alertsTab();
      case 'assistant': return this.assistantTab();
    }
  }

  /* ------------------------------------------------------------ location */

  private onSearch(e: Event) {
    this.query = (e.target as HTMLInputElement).value;
    clearTimeout(this.searchTimer);
    if (!this.query.trim()) {
      this.results = [];
      return;
    }
    this.searchTimer = window.setTimeout(async () => {
      this.searching = true;
      try {
        this.results = (await api.search(this.query)).slice(0, 5);
        this.locError = '';
      } catch {
        this.locError = 'Search failed. Try again.';
      } finally {
        this.searching = false;
      }
    }, 400);
  }

  private async useMyLocation() {
    this.locating = true;
    this.locError = '';
    try {
      setLocation(await locateMe());
      toast('Location updated');
    } catch (e) {
      this.locError = errorText(e);
    } finally {
      this.locating = false;
    }
  }

  private locationTab() {
    const loc = $().location;
    return html`
      ${loc
        ? html`<div class="sv-loc-card">
            <m3e-shape name="sunny" style="--m3e-shape-size:56px;--m3e-shape-container-color:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)">
              <m3e-icon name="location_on" filled></m3e-icon>
            </m3e-shape>
            <div>
              <div class="sv-title-large">${loc.name}${loc.state ? `, ${loc.state}` : ''}${loc.country && loc.country !== 'US' ? ` · ${loc.country}` : ''}</div>
              <div class="sv-body-small sv-muted">${loc.lat.toFixed(4)}, ${loc.lon.toFixed(4)}</div>
            </div>
          </div>`
        : nothing}
      ${group('Your location', [
        html`<m3e-form-field variant="outlined" class="sv-field">
          <m3e-icon slot="prefix" name="search"></m3e-icon>
          <label slot="label" for="loc-search">Search city, zip, or address</label>
          <input id="loc-search" autocomplete="off" spellcheck="false" .value=${this.query} @input=${this.onSearch} />
          ${this.searching ? html`<m3e-loading-indicator slot="suffix" class="sv-small-loader"></m3e-loading-indicator>` : nothing}
        </m3e-form-field>`,
        this.results.length
          ? html`<m3e-action-list class="sv-results">
              ${this.results.map((r) => html`<m3e-list-action @click=${() => { setLocation(r); this.query = ''; this.results = []; toast(`Location set to ${r.name}`); }}>
                <m3e-icon slot="leading" name="location_on"></m3e-icon>
                ${r.name}${r.state ? `, ${r.state}` : ''}
                <span slot="supporting-text">${r.display}</span>
              </m3e-list-action>`)}
            </m3e-action-list>`
          : nothing,
        this.locError ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>${this.locError}</m3e-callout>` : nothing,
        html`<m3e-button variant="tonal" class="sv-full" ?disabled=${this.locating} @click=${this.useMyLocation}>
          ${this.locating ? html`<m3e-loading-indicator slot="icon" class="sv-small-loader"></m3e-loading-indicator>` : html`<m3e-icon slot="icon" name="my_location"></m3e-icon>`}
          Use my current location
        </m3e-button>`,
        html`<p class="sv-body-small sv-muted">Tip: right-click anywhere on the map to set the weather location there.</p>`,
      ])}
    `;
  }

  /* -------------------------------------------------------------- weather */

  private weatherTab() {
    const s = $();
    return html`
      ${group('Weather provider', row('Data source', segmented([{ label: 'NWS', value: 'nws' }, { label: 'Open-Meteo', value: 'openmeteo' }], s.weatherAPI, (v) => set({ weatherAPI: v })), 'NWS is US-only (falls back to Open-Meteo elsewhere). Open-Meteo is global.', true))}
      ${group('Units', row('Temperature & speed', segmented([{ label: 'Imperial', value: 'imperial' }, { label: 'Metric', value: 'metric' }], s.units, (v) => set({ units: v })), s.units === 'imperial' ? '°F · mph · in · mi' : '°C · km/h · mm · km', true))}
    `;
  }

  /* -------------------------------------------------------------- display */

  private swatch(id: string, label: string, seed: string | null, icon?: string) {
    const s = $();
    const active = s.colorSource === id;
    const p = seed ? previewSwatch(seed, s.isDark) : null;
    return html`<button class="sv-swatch ${active ? 'active' : ''}" @click=${() => set({ colorSource: id })} aria-pressed=${active}>
      <m3e-shape name=${active ? '9-sided-cookie' : 'circle'} class="sv-swatch-shape" style="--m3e-shape-size:56px;--m3e-shape-container-color:${p ? p.surface : 'var(--md-sys-color-surface-container-highest)'}">
        ${p
          ? html`<span class="sv-swatch-split"><i style="background:${p.primary}"></i><i style="background:${p.secondary}"></i><i style="background:${p.tertiary}"></i></span>`
          : html`<m3e-icon name=${icon ?? 'palette'} filled></m3e-icon>`}
        ${active ? html`<m3e-icon class="sv-swatch-check" name="check" filled></m3e-icon>` : nothing}
      </m3e-shape>
      <span class="sv-label-medium">${label}</span>
    </button>`;
  }

  private displayTab() {
    const s = $();
    const variant = VARIANTS.find((v) => v.id === s.schemeVariant);
    return html`
      ${group('Mode', row('Theme', segmented([
        { label: 'System', value: 'system', icon: 'contrast' },
        { label: 'Light', value: 'light', icon: 'light_mode' },
        { label: 'Dark', value: 'dark', icon: 'dark_mode' },
      ], s.themeMode, (v) => set({ themeMode: v })), undefined, true))}

      ${group('Material You color', [
        html`<div class="sv-swatch-grid">
          ${COLOR_PRESETS.map((p) => this.swatch(p.id, p.label, p.seed))}
          ${this.swatch('adaptive', 'Weather', null, 'partly_cloudy_day')}
          ${this.swatch('custom', 'Custom', s.colorSource === 'custom' ? s.customSeed : null, 'colorize')}
        </div>`,
        s.colorSource === 'adaptive'
          ? html`<p class="sv-body-small sv-muted">The whole app re-tints itself from the current conditions — amber sun, blue rain, violet storms, icy snow.</p>`
          : nothing,
        s.colorSource === 'custom'
          ? row('Seed color', html`<label class="sv-color-input">
              <input type="color" .value=${s.customSeed} @input=${(e: Event) => set({ customSeed: (e.target as HTMLInputElement).value })} />
              <span class="sv-label-large">${s.customSeed.toUpperCase()}</span>
            </label>`, 'Every color role is generated from this one seed by Google’s HCT algorithm.')
          : nothing,
      ], 'Colors are never hand-picked: a seed goes through Google’s Material color utilities to build the full tonal scheme.')}

      ${group('Scheme style', [
        html`<m3e-form-field variant="outlined" class="sv-field">
          <label slot="label" for="variant-select">Dynamic color variant</label>
          <m3e-select id="variant-select" @change=${(e: Event) => set({ schemeVariant: valueOf(e) as Settings['schemeVariant'] })}>
            ${VARIANTS.map((v) => html`<m3e-option value=${v.id} ?selected=${v.id === s.schemeVariant}>${v.label}</m3e-option>`)}
          </m3e-select>
        </m3e-form-field>`,
        variant ? html`<p class="sv-body-small sv-muted">${variant.desc}</p>` : nothing,
        row('Contrast', segmented([{ label: 'Standard', value: 'standard' }, { label: 'Medium', value: 'medium' }, { label: 'High', value: 'high' }], s.contrast, (v) => set({ contrast: v })), undefined, true),
        row('Motion', segmented([{ label: 'Expressive', value: 'expressive' }, { label: 'Standard', value: 'standard' }], s.motion, (v) => set({ motion: v })), 'Expressive uses M3’s springy, overshooting motion.', true),
      ])}

      ${group('Map', row('Base map', segmented([{ label: 'Material', value: 'material', icon: 'palette' }, { label: 'Streets', value: 'streets', icon: 'map' }], s.mapStyle, (v) => set({ mapStyle: v })), 'Material repaints the map with your color scheme.', true))}

      ${group('Layout', [
        row('Sidebar position', segmented([{ label: 'Left', value: 'left' }, { label: 'Right', value: 'right' }], s.sidebarPosition, (v) => set({ sidebarPosition: v })), undefined, true),
        toggle('Compact layout', s.compactLayout, (v) => set({ compactLayout: v }), 'In narrow windows, use a scrollable weather page with a navigation bar instead of the map + sidebar.'),
      ])}
    `;
  }

  /* -------------------------------------------------------------- widgets */

  private moveWidget(i: number, delta: number) {
    const w = $().widgets.slice();
    const j = i + delta;
    if (j < 0 || j >= w.length) return;
    [w[i], w[j]] = [w[j], w[i]];
    set({ widgets: w });
  }

  private widgetsTab() {
    const s = $();
    const enabled = (id: string) => s.widgets.find((w) => w.id === id)?.enabled;
    return html`
      ${group('Home screen sections', html`<m3e-list class="sv-widget-list">
        ${s.widgets.map((w, i) => html`<m3e-list-item>
          <m3e-switch slot="leading" icons="selected" ?checked=${w.enabled} @change=${(e: Event) => set({ widgets: s.widgets.map((x) => (x.id === w.id ? { ...x, enabled: checkedOf(e) } : x)) })}></m3e-switch>
          ${WIDGET_LABELS[w.id]}
          <span slot="trailing" class="sv-reorder">
            <m3e-icon-button size="extra-small" aria-label="Move up" ?disabled=${i === 0} @click=${() => this.moveWidget(i, -1)}><m3e-icon name="arrow_upward"></m3e-icon></m3e-icon-button>
            <m3e-icon-button size="extra-small" aria-label="Move down" ?disabled=${i === s.widgets.length - 1} @click=${() => this.moveWidget(i, 1)}><m3e-icon name="arrow_downward"></m3e-icon></m3e-icon-button>
          </span>
        </m3e-list-item>`)}
      </m3e-list>`, 'Show, hide, and reorder the sections below Current Conditions. Current Conditions is always shown.')}

      ${enabled('details')
        ? group('Weather Details — fields', html`<m3e-chip-set class="sv-chips">
            ${(Object.keys(DETAILS_FIELD_LABELS) as DetailsField[]).map((k) => html`<m3e-filter-chip
              ?selected=${s.detailsFields[k] !== false}
              @click=${() => set({ detailsFields: { ...s.detailsFields, [k]: s.detailsFields[k] === false } })}
            >${DETAILS_FIELD_LABELS[k]}</m3e-filter-chip>`)}
          </m3e-chip-set>`, 'Hide any info cards you don’t need.')
        : nothing}

      ${enabled('aqi') ? group('Air Quality widget', toggle('Show pollutant breakdown', s.aqiShowPollutants, (v) => set({ aqiShowPollutants: v }))) : nothing}
    `;
  }

  /* ---------------------------------------------------------------- radar */

  private radarTab() {
    const s = $();
    const radar = s.mapLayer === 'radar';
    return html`
      ${group('Map layer', [
        row('Active overlay', segmented([{ label: 'Radar', value: 'radar', icon: 'radar' }, { label: 'Air quality', value: 'aqi', icon: 'air' }], s.mapLayer, (v) => set({ mapLayer: v })), undefined, true),
        !radar
          ? html`<m3e-callout variant="secondary"><m3e-icon slot="icon" name="info"></m3e-icon>A color gradient shows air quality across the map — green is clean, yellow/orange/red is increasing pollution. Click anywhere for exact AQI and pollutants (Open-Meteo, global).</m3e-callout>`
          : nothing,
      ])}
      ${radar
        ? html`
          ${group('Radar source', html`<m3e-callout variant="primary"><m3e-icon slot="icon" name="radar"></m3e-icon>
            <b>LibreWXR</b> — observed radar mosaic with a 90-minute nowcast (best over the US).</m3e-callout>`)}
          ${group('Radar options', [
            row('Tile quality', segmented([{ label: 'Fast', value: 256 }, { label: 'Sharp', value: 512 }], s.radarTileQuality, (v) => set({ radarTileQuality: v as 256 | 512 })),
              s.radarTileQuality === 256 ? 'Fast — 256 px tiles, quicker, blurry when zoomed in' : 'Sharp — 512 px tiles, full detail, slower to load', true),
            html`<div class="sv-set-row stack">
              <div class="sv-set-text"><div class="sv-body-large">Radar opacity</div><div class="sv-body-small sv-muted">${Math.round(s.radarOpacity * 100)}%</div></div>
              <m3e-slider min="10" max="100" step="5" labelled size="small" class="sv-full">
                <m3e-slider-thumb value=${Math.round(s.radarOpacity * 100)} @input=${(e: Event) => set({ radarOpacity: Number((e.target as unknown as { value: number }).value) / 100 })}></m3e-slider-thumb>
              </m3e-slider>
            </div>`,
            toggle('Show 90-min nowcast', s.showNowcast, (v) => set({ showNowcast: v })),
            toggle('Fade between frames', s.radarFrameFade, (v) => set({ radarFrameFade: v })),
            toggle('Smoothing', s.radarSmooth, (v) => set({ radarSmooth: v }), 'Server-side smoothing of radar echoes.'),
            toggle('Snow colors', s.radarSnow, (v) => set({ radarSnow: v }), 'Color frozen precipitation separately using the model snow mask.'),
            toggle('Satellite imagery', s.showSatellite, (v) => set({ showSatellite: v })),
            toggle('Storm motion arrows', s.showArrows, (v) => set({ showArrows: v }), 'Optical-flow storm direction arrows; color follows light/dark mode.'),
          ])}`
        : nothing}
      ${group('Warnings', [
        toggle('NWS warning polygons', s.showAlertPolygons, (v) => set({ showAlertPolygons: v }), 'Active NWS warning polygons (US only). Click any polygon for full details.'),
        s.showAlertPolygons
          ? toggle('Include advisories & statements', s.showAdvisories, (v) => (v ? (this.advisoryConfirm = true) : set({ showAdvisories: false })),
            'Heat, fog, air quality, special statements and more. Zone-based (dashed outlines); slower on first use.')
          : nothing,
      ])}
      ${radar
        ? group('Color scheme', html`<div class="sv-scheme-grid">
            ${COLOR_SCHEMES.map((c) => html`<button class="sv-scheme ${s.radarColorScheme === c.id ? 'active' : ''}" @click=${() => set({ radarColorScheme: c.id })}>
              <span class="sv-scheme-ramp" style="background:linear-gradient(90deg, ${c.g})"></span>
              <span class="sv-label-medium">${c.name}</span>
            </button>`)}
          </div>`)
        : nothing}
    `;
  }

  /* --------------------------------------------------------------- alerts */

  private async setNotifications(on: boolean) {
    if (on) {
      let granted = await isPermissionGranted().catch(() => true);
      if (!granted) granted = (await requestPermission().catch(() => 'denied')) === 'granted';
      if (!granted) {
        toast('Notifications are blocked by your system settings.');
        return;
      }
      set({ notificationsEnabled: true });
      api.testNotification();
    } else {
      set({ notificationsEnabled: false });
    }
  }

  private alertsTab() {
    const s = $();
    const on = s.notificationsEnabled;
    return html`
      <div class="sv-notif-status ${on ? 'on' : ''}">
        <m3e-shape name=${on ? 'sunny' : 'circle'} style="--m3e-shape-size:56px">
          <m3e-icon name=${on ? 'notifications_active' : 'notifications_off'} filled></m3e-icon>
        </m3e-shape>
        <div>
          <div class="sv-title-medium">${on ? 'Notifications enabled' : 'Notifications off'}</div>
          <div class="sv-body-small">${on ? `Alerts for ${s.location?.name ?? 'your location'} arrive as system notifications.` : 'Get native desktop notifications for weather and air quality alerts.'}</div>
        </div>
      </div>
      ${group('Desktop notifications', [
        toggle('Alert notifications', on, (v) => this.setNotifications(v)),
        !s.location ? html`<p class="sv-body-small sv-muted">Set your location first so StormView knows which alerts to send.</p>` : nothing,
        on ? toggle('NWS weather alerts', s.notifNws, (v) => set({ notifNws: v }), 'Tornado, severe thunderstorm, flood warnings & more') : nothing,
        on ? toggle('Air quality alerts', s.notifAqi, (v) => set({ notifAqi: v }), 'When AQI reaches Unhealthy (200+)') : nothing,
        toggle('Keep running in the background', s.runInBackground, (v) => set({ runInBackground: v }), 'Closing the window hides StormView to the system tray so alerts keep arriving.'),
        on ? html`<m3e-button variant="tonal" @click=${() => api.testNotification()}><m3e-icon slot="icon" name="notifications"></m3e-icon>Send a test notification</m3e-button>` : nothing,
      ])}
      ${group('What you’ll receive', html`<ul class="sv-bullets sv-body-medium">
        <li>Tornado Warnings & Watches</li><li>Severe Thunderstorm Warnings</li><li>Flash Flood Warnings</li>
        <li>Winter Storm Warnings & Advisories</li><li>All other active NWS alerts for your area</li>
      </ul><p class="sv-body-small sv-muted">US coverage only — powered by the National Weather Service. Alerts are checked every 90 seconds.</p>`)}
    `;
  }

  /* ------------------------------------------------------------ assistant */

  private async loadModels() {
    const s = $();
    if (s.assistantProvider === 'webllm') return;
    this.modelsLoading = true;
    this.modelsError = '';
    try {
      this.models = await api.assistantModels(s.assistantProvider, s.assistantBaseUrl);
      if (!s.assistantModel && this.models[0]) set({ assistantModel: this.models[0] });
    } catch (e) {
      this.models = [];
      this.modelsError = errorText(e);
    } finally {
      this.modelsLoading = false;
    }
  }

  private assistantTab() {
    const s = $();
    const webllm = s.assistantProvider === 'webllm';
    const models = webllm ? WEBLLM_MODELS.map((m) => ({ id: m.id, label: m.label })) : this.models.map((m) => ({ id: m, label: m }));
    if (!webllm && !this.models.length && !this.modelsLoading && !this.modelsError) queueMicrotask(() => this.loadModels());
    return html`
      ${group('Model provider', [
        row('Run the assistant with', segmented([
          { label: 'Ollama', value: 'ollama' }, { label: 'OpenAI API', value: 'openai' }, { label: 'In-app', value: 'webllm' },
        ], s.assistantProvider, (v) => {
          this.models = [];
          this.modelsError = '';
          set({
            assistantProvider: v,
            assistantModel: '',
            assistantBaseUrl: v === 'ollama' ? 'http://localhost:11434' : v === 'openai' ? 'http://localhost:1234' : s.assistantBaseUrl,
          });
        }), webllm
          ? 'Downloads a small model and runs it inside the app with WebGPU (works best on Windows).'
          : 'Everything stays on your machine — StormView talks to a local model server.', true),
        !webllm
          ? html`<m3e-form-field variant="outlined" class="sv-field">
              <label slot="label" for="ai-url">Server URL</label>
              <input id="ai-url" .value=${s.assistantBaseUrl} @change=${(e: Event) => { set({ assistantBaseUrl: (e.target as HTMLInputElement).value }); this.models = []; this.modelsError = ''; }} />
              <m3e-icon-button slot="suffix" aria-label="Refresh models" @click=${() => this.loadModels()}><m3e-icon name="refresh"></m3e-icon></m3e-icon-button>
            </m3e-form-field>`
          : nothing,
        models.length
          ? html`<m3e-form-field variant="outlined" class="sv-field">
              <label slot="label" for="ai-model">Model</label>
              <m3e-select id="ai-model" @change=${(e: Event) => set({ assistantModel: valueOf(e) })}>
                ${models.map((m) => html`<m3e-option value=${m.id} ?selected=${m.id === (s.assistantModel || (webllm ? WEBLLM_MODELS[0].id : ''))}>${m.label}</m3e-option>`)}
              </m3e-select>
            </m3e-form-field>`
          : nothing,
        this.modelsLoading ? html`<div class="sv-status"><m3e-loading-indicator class="sv-small-loader"></m3e-loading-indicator>Looking for models…</div>` : nothing,
        this.modelsError
          ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>${this.modelsError}
              ${s.assistantProvider === 'ollama' ? html`<br />Install Ollama, then run <code>ollama pull llama3.2</code>.` : nothing}</m3e-callout>`
          : nothing,
      ])}
      ${webllm
        ? group('Downloaded model', row('Clear downloaded AI model', html`<m3e-button variant="outlined" @click=${async () => { await clearWebLLMCache(); toast('Model cache cleared'); }}>Clear</m3e-button>`, 'Frees the space the in-app model uses.'))
        : nothing}
    `;
  }
}
