import { html, nothing } from 'lit';
import { customElement } from 'lit/decorators.js';
import { format } from 'date-fns';
import { AQI_SCALE, harmonize } from '../lib/format';
import { store, StoreController } from '../lib/store';
import { SvElement } from '../lib/ui';
import './sv-map';
import './sv-scrubber';
import './sv-sidebar';
import './sv-settings';
import './sv-dialogs';
import './sv-assistant';

const NARROW = 760;

@customElement('sv-app')
export class SvApp extends SvElement {
  private s = new StoreController(this, (s) =>
    [s.ready, s.isNarrow, s.compactLayout, s.compactTab, s.mapLayer, s.settingsOpen, s.sidebarPosition, s.showSatellite,
      s.radarFrames[s.radarIdx]?.time, s.tokens] as const);

  connectedCallback() {
    super.connectedCallback();
    const onResize = () => {
      const narrow = window.innerWidth < NARROW;
      if (narrow !== store.get().isNarrow) store.set({ isNarrow: narrow });
    };
    window.addEventListener('resize', onResize);
    onResize();
    window.addEventListener('keydown', (e) => {
      if (e.key === ',' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        store.set({ settingsOpen: !store.get().settingsOpen });
      } else if (e.key === 'Tab' && e.ctrlKey && store.get().settingsOpen) {
        e.preventDefault();
        const tabs = ['location', 'api', 'display', 'widgets', 'radar', 'alerts', 'assistant'] as const;
        const i = tabs.indexOf(store.get().settingsTab);
        store.set({ settingsTab: tabs[(i + (e.shiftKey ? tabs.length - 1 : 1)) % tabs.length] });
      } else if ((e.key === '+' || e.key === '=' || e.key === '-') && !(e.target as HTMLElement).closest('input, textarea')) {
        window.dispatchEvent(new CustomEvent('sv-zoom', { detail: e.key === '-' ? -1 : 1 }));
      } else if (e.key === ' ' && e.target === document.body) {
        e.preventDefault();
        store.set({ radarPlaying: !store.get().radarPlaying });
      }
    });
  }

  private toolbar(compact: boolean) {
    const [, , , , layer, settingsOpen, , satellite] = this.s.value;
    return html`<m3e-toolbar variant="vibrant" shape="rounded" elevated class="sv-toolbar ${compact ? 'compact' : ''}">
      ${compact
        ? nothing
        : html`<m3e-button variant="text" size="extra-small" class="sv-beta-btn" @click=${() => store.set({ betaOpen: true })}>BETA</m3e-button>`}
      <m3e-button-group variant="connected" size="small">
        <m3e-icon-button toggle ?selected=${layer === 'radar'} variant="tonal" aria-label="Radar layer"
          @beforeinput=${(e: Event) => layer === 'radar' && e.preventDefault()} @click=${() => store.set({ mapLayer: 'radar' })}>
          <m3e-icon name="radar"></m3e-icon><m3e-icon slot="selected" name="radar" filled></m3e-icon>
        </m3e-icon-button>
        <m3e-icon-button toggle ?selected=${layer === 'aqi'} variant="tonal" aria-label="Air quality layer"
          @beforeinput=${(e: Event) => layer === 'aqi' && e.preventDefault()} @click=${() => store.set({ mapLayer: 'aqi' })}>
          <m3e-icon name="air"></m3e-icon><m3e-icon slot="selected" name="air" filled></m3e-icon>
        </m3e-icon-button>
      </m3e-button-group>
      <m3e-icon-button toggle ?selected=${satellite} aria-label="Satellite imagery" @click=${() => store.set({ showSatellite: !satellite })}>
        <m3e-icon name="satellite_alt"></m3e-icon><m3e-icon slot="selected" name="satellite_alt" filled></m3e-icon>
      </m3e-icon-button>
      ${compact
        ? nothing
        : html`<m3e-icon-button variant="filled" width="wide" toggle ?selected=${settingsOpen} aria-label="Settings" @click=${() => store.set({ settingsOpen: !settingsOpen })}>
            <m3e-icon name="settings"></m3e-icon><m3e-icon slot="selected" name="settings" filled></m3e-icon>
          </m3e-icon-button>`}
    </m3e-toolbar>`;
  }

  private aqiLegend() {
    return html`<div class="sv-aqi-legend sv-surface-float">
      <div class="sv-title-small">US Air Quality Index</div>
      <div class="sv-aqi-ramp" style="background:linear-gradient(90deg, ${AQI_SCALE.map((c) => harmonize(c.hue)).join(', ')})"></div>
      <div class="sv-row-between sv-label-small sv-muted"><span>Good</span><span>Moderate</span><span>Unhealthy</span><span>Hazardous</span></div>
      <div class="sv-label-small sv-muted">Click the map for exact readings</div>
    </div>`;
  }

  render() {
    const [ready, narrow, compactPref, compactTab, layer, , side, , frameTime] = this.s.value;
    const compact = narrow && compactPref;
    return html`
      <div class="sv-app" data-side=${side} data-compact=${compact} data-tab=${compactTab} data-ready=${ready}>
        <sv-map></sv-map>
        ${compact
          ? html`
              ${compactTab === 'home'
                ? html`<div class="sv-compact-home sv-scroll">
                    <sv-location-header class="sv-compact-head"></sv-location-header>
                    <sv-alert-banner></sv-alert-banner>
                    <m3e-card variant="filled" actionable class="sv-radar-card" @click=${() => store.set({ compactTab: 'radar' })}>
                      <div class="sv-radar-card-inner">
                        <m3e-shape name="soft-burst" style="--m3e-shape-size:56px;--m3e-shape-container-color:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)">
                          <m3e-icon name="radar" filled></m3e-icon>
                        </m3e-shape>
                        <div class="sv-grow"><div class="sv-title-medium">Live radar</div>
                          <div class="sv-body-small sv-muted">${frameTime ? `Latest frame ${format(frameTime * 1000, 'h:mm a')}` : 'Loading…'}</div></div>
                        <m3e-icon name="chevron_right"></m3e-icon>
                      </div>
                    </m3e-card>
                    <sv-weather-content></sv-weather-content>
                  </div>`
                : html`<div class="sv-compact-radar">
                    ${this.toolbar(true)}
                    ${layer === 'radar' ? html`<sv-scrubber></sv-scrubber>` : this.aqiLegend()}
                  </div>`}
              <m3e-nav-bar class="sv-navbar" @change=${(e: Event) => {
                const sel = (e.currentTarget as unknown as { selected: HTMLElement | null }).selected;
                const v = sel?.getAttribute('data-tab');
                if (v === 'settings') store.set({ settingsOpen: true });
                else if (v === 'home' || v === 'radar') store.set({ compactTab: v, settingsOpen: false });
              }}>
                <m3e-nav-item data-tab="home" ?selected=${compactTab === 'home'}><m3e-icon slot="icon" name="home"></m3e-icon><m3e-icon slot="selected-icon" name="home" filled></m3e-icon>Home</m3e-nav-item>
                <m3e-nav-item data-tab="radar" ?selected=${compactTab === 'radar'}><m3e-icon slot="icon" name="radar"></m3e-icon><m3e-icon slot="selected-icon" name="radar" filled></m3e-icon>Radar</m3e-nav-item>
                <m3e-nav-item data-tab="settings"><m3e-icon slot="icon" name="settings"></m3e-icon><m3e-icon slot="selected-icon" name="settings" filled></m3e-icon>Settings</m3e-nav-item>
              </m3e-nav-bar>
            `
          : html`
              <sv-sidebar></sv-sidebar>
              <div class="sv-top-corner">${this.toolbar(false)}</div>
              <div class="sv-bottom-dock">${layer === 'radar' ? html`<sv-scrubber></sv-scrubber>` : this.aqiLegend()}</div>
            `}
        <sv-assistant></sv-assistant>
        <sv-settings></sv-settings>
        <sv-dialogs></sv-dialogs>
        ${ready ? nothing : html`<div class="sv-splash"><m3e-loading-indicator variant="contained"></m3e-loading-indicator></div>`}
      </div>
    `;
  }
}
