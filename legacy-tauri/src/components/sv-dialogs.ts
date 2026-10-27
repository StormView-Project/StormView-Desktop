import { html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { format } from 'date-fns';
import { errorText } from '../lib/api';
import { locateMe, setLocation } from '../lib/data';
import { semantic, SEVERITY_HUE } from '../lib/format';
import { $, store, StoreController } from '../lib/store';
import { openExternal, SvElement } from '../lib/ui';

export const WHATS_NEW_VERSION = 'desktop-1.0.0';

const WHATS_NEW = [
  { icon: 'desktop_windows', title: 'StormView on your desktop', desc: 'A native Linux & Windows app built in Rust with Tauri — same radar, forecasts and alerts as the web app.' },
  { icon: 'palette', title: 'Material You, for real', desc: 'Every color comes from Google’s dynamic color algorithm. Pick a seed, a scheme style and contrast — or let the weather choose.' },
  { icon: 'interests', title: 'Material 3 Expressive', desc: 'Shape-morphing icons, springy motion, wavy progress and expressive components throughout.' },
  { icon: 'radar', title: 'LibreWXR radar on a vector map', desc: 'A crisp MapLibre map tinted to your theme, with the 90-minute nowcast, color schemes and storm arrows.' },
  { icon: 'notifications_active', title: 'Native alert notifications', desc: 'Get NWS warnings and unhealthy-air alerts as system notifications — even from the tray.' },
  { icon: 'smart_toy', title: 'Local weather assistant', desc: 'Ask about the forecast with Ollama, any OpenAI-compatible local server, or an in-app WebGPU model.' },
];

@customElement('sv-dialogs')
export class SvDialogs extends SvElement {
  private s = new StoreController(this, (s) =>
    [s.alertDialog, s.welcomeOpen, s.betaOpen, s.location, s.dismissedWhatsNewVersion, s.ready, s.tokens] as const);
  @state() private locating = false;
  @state() private locError = '';
  @state() private whatsNewSession = false;
  @state() private neverShow = false;

  private async locate() {
    this.locating = true;
    this.locError = '';
    try {
      setLocation(await locateMe());
      store.set({ welcomeOpen: false });
    } catch (e) {
      this.locError = errorText(e);
    } finally {
      this.locating = false;
    }
  }

  render() {
    const [alerts, welcome, beta, location, dismissed, ready] = this.s.value;
    const whatsNew = ready && !!location && !welcome && !this.whatsNewSession && dismissed !== WHATS_NEW_VERSION;
    return html`
      <m3e-dialog class="sv-dialog sv-dialog--wide" ?open=${!!alerts} dismissible @closed=${() => store.set({ alertDialog: null })}>
        ${alerts
          ? html`<div slot="header" class="sv-dialog-head"><m3e-icon name="warning" filled></m3e-icon>${alerts.length > 1 ? `Active alerts (${alerts.length})` : alerts[0]?.title}</div>
              <div class="sv-alert-cards">
                ${alerts.map((a) => {
                  const c = semantic(SEVERITY_HUE[a.severity] ?? SEVERITY_HUE.Unknown);
                  return html`<article class="sv-alert-card" style="background:${c.container};color:${c.onContainer}">
                    <header>
                      <m3e-shape name="burst" style="--m3e-shape-size:40px;--m3e-shape-container-color:${c.color};color:${c.container}"><m3e-icon name="warning" filled></m3e-icon></m3e-shape>
                      <div class="sv-title-medium sv-strong">${a.title}</div>
                      <span class="sv-chip" style="background:${c.color};color:${c.container}">${a.severity === 'Unknown' ? 'Alert' : a.severity}</span>
                    </header>
                    <div class="sv-label-medium">
                      ${a.end ? `Until ${format(a.end, "EEE, MMM d 'at' h:mm a")}` : ''}${a.certainty && a.certainty !== 'Unknown' ? ` · ${a.certainty}` : ''}
                    </div>
                    ${a.headline ? html`<p class="sv-body-medium sv-strong">${a.headline}</p>` : nothing}
                    ${a.description ? html`<p class="sv-pre sv-body-medium">${a.description}</p>` : nothing}
                    ${a.instruction ? html`<p class="sv-pre sv-body-medium sv-alert-instruction">${a.instruction}</p>` : nothing}
                  </article>`;
                })}
              </div>
              <div slot="actions" end><m3e-button variant="filled"><m3e-dialog-action>Close</m3e-dialog-action></m3e-button></div>`
          : nothing}
      </m3e-dialog>

      <m3e-dialog class="sv-dialog sv-welcome" ?open=${welcome} disable-close>
        <div class="sv-welcome-hero">
          <m3e-shape name="very-sunny" class="sv-welcome-shape" style="--m3e-shape-size:120px;--m3e-shape-container-color:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)">
            <m3e-icon name="partly_cloudy_day" filled style="--m3e-icon-size:56px"></m3e-icon>
          </m3e-shape>
          <m3e-heading variant="headline" size="medium" emphasized>Welcome to StormView</m3e-heading>
          <p class="sv-body-large sv-muted">To show weather and radar for your area, StormView needs your location. You can also set it manually.</p>
          ${this.locError ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>${this.locError}</m3e-callout>` : nothing}
        </div>
        <div slot="actions" class="sv-welcome-actions">
          <m3e-button variant="text" @click=${() => store.set({ welcomeOpen: false, settingsOpen: true, settingsTab: 'location' })}>Set manually</m3e-button>
          <m3e-button variant="filled" size="medium" ?disabled=${this.locating} @click=${this.locate}>
            ${this.locating ? html`<m3e-loading-indicator slot="icon" class="sv-small-loader"></m3e-loading-indicator>` : html`<m3e-icon slot="icon" name="my_location"></m3e-icon>`}
            Use my location
          </m3e-button>
        </div>
      </m3e-dialog>

      <m3e-dialog class="sv-dialog" ?open=${beta} dismissible @closed=${() => store.set({ betaOpen: false })}>
        <div slot="header" class="sv-dialog-head"><m3e-icon name="science" filled></m3e-icon>StormView is in beta</div>
        <m3e-callout variant="tertiary">
          <m3e-icon slot="icon" name="rocket_launch"></m3e-icon>
          <span slot="label">Want to try tomorrow’s StormView today?</span>
          The dev build gets new features, UI experiments and radar improvements weeks before the main app.
        </m3e-callout>
        <m3e-button variant="tonal" class="sv-full" @click=${() => openExternal('https://stormview.develop.arc360hub.com')}>
          <m3e-icon slot="icon" name="rocket_launch"></m3e-icon>Open dev build<m3e-icon slot="trailing-icon" name="open_in_new"></m3e-icon>
        </m3e-button>
        <m3e-divider></m3e-divider>
        <p class="sv-body-medium"><b>Under heavy development.</b> You may run into bugs, missing features, or rough edges. Reporting what breaks goes a long way — every bug report helps.</p>
        <m3e-button variant="outlined" class="sv-full" @click=${() => openExternal('https://github.com/arc360alt/StormView-Rewrite/issues')}>
          <m3e-icon slot="icon" name="bug_report"></m3e-icon>Report an issue on GitHub<m3e-icon slot="trailing-icon" name="open_in_new"></m3e-icon>
        </m3e-button>
        <m3e-divider></m3e-divider>
        <p class="sv-body-medium sv-muted"><b>What happened to the old StormView?</b> The original is still planned to be hosted separately; getting this rewrite stable and feature-complete comes first.</p>
      </m3e-dialog>

      <m3e-dialog class="sv-dialog sv-dialog--wide" ?open=${whatsNew} @closed=${() => this.closeWhatsNew()}>
        <div slot="header" class="sv-dialog-head sv-whatsnew-head">
          <span class="sv-chip sv-chip--primary">New</span>
          <span>What’s new in StormView</span>
        </div>
        <div class="sv-whatsnew">
          ${WHATS_NEW.map((f, i) => html`<div class="sv-whatsnew-item">
            <m3e-shape name=${['sunny', '4-leaf-clover', 'flower', '8-leaf-clover', 'soft-burst', 'puffy'][i % 6]} style="--m3e-shape-size:48px;--m3e-shape-container-color:var(--md-sys-color-${['primary', 'secondary', 'tertiary'][i % 3]}-container);color:var(--md-sys-color-on-${['primary', 'secondary', 'tertiary'][i % 3]}-container)">
              <m3e-icon name=${f.icon} filled></m3e-icon>
            </m3e-shape>
            <div><div class="sv-title-medium">${f.title}</div><div class="sv-body-medium sv-muted">${f.desc}</div></div>
          </div>`)}
        </div>
        <div slot="actions" class="sv-row-between sv-full">
          <label class="sv-check"><m3e-checkbox ?checked=${this.neverShow} @change=${(e: Event) => (this.neverShow = (e.target as unknown as { checked: boolean }).checked)}></m3e-checkbox>Don’t show again until the next update</label>
          <m3e-button variant="filled"><m3e-dialog-action>Got it</m3e-dialog-action></m3e-button>
        </div>
      </m3e-dialog>
    `;
  }

  private closeWhatsNew() {
    if (this.neverShow) store.set({ dismissedWhatsNewVersion: WHATS_NEW_VERSION });
    this.whatsNewSession = true;
  }

  connectedCallback() {
    super.connectedCallback();
    // First launch: offer to locate once settings have loaded and no location exists.
    const unsub = store.subscribe((s) => {
      if (!s.ready) return;
      unsub();
      if (!$().location) store.set({ welcomeOpen: true });
    });
  }
}
