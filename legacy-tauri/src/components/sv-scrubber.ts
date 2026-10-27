import { html, nothing } from 'lit';
import { customElement, query, state } from 'lit/decorators.js';
import { format } from 'date-fns';
import { jumpToNow, refreshRadar } from '../lib/data';
import { store, StoreController } from '../lib/store';
import { SvElement } from '../lib/ui';

const SPEEDS = [0.5, 1, 2] as const;

@customElement('sv-scrubber')
export class SvScrubber extends SvElement {
  private s = new StoreController(this, (s) =>
    [s.radarFrames, s.radarIdx, s.radarPlaying, s.radarSpeed, s.radarProgress, s.radarLoading] as const);

  @query('.sv-timeline') private timeline!: HTMLDivElement;
  @state() private hoverIdx: number | null = null;
  @state() private dragging = false;

  private posToIdx(clientX: number) {
    const frames = this.s.value[0];
    const r = this.timeline.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (clientX - r.left) / r.width));
    return Math.round(pct * (frames.length - 1));
  }

  private pct(i: number) {
    const n = this.s.value[0].length;
    return n > 1 ? (i / (n - 1)) * 100 : 0;
  }

  private onPointerDown = (e: PointerEvent) => {
    e.preventDefault();
    this.timeline.setPointerCapture(e.pointerId);
    this.dragging = true;
    store.set({ radarPlaying: false, radarIdx: this.posToIdx(e.clientX) });
  };

  private onPointerMove = (e: PointerEvent) => {
    const idx = this.posToIdx(e.clientX);
    if (this.dragging) {
      if (idx !== this.s.value[1]) store.set({ radarIdx: idx });
    } else {
      this.hoverIdx = idx;
    }
  };

  private onPointerUp = (e: PointerEvent) => {
    this.dragging = false;
    this.timeline.releasePointerCapture(e.pointerId);
  };

  private onKey = (e: KeyboardEvent) => {
    const [frames, idx] = this.s.value;
    if (e.key === 'ArrowRight') store.set({ radarPlaying: false, radarIdx: Math.min(frames.length - 1, idx + 1) });
    else if (e.key === 'ArrowLeft') store.set({ radarPlaying: false, radarIdx: Math.max(0, idx - 1) });
    else return;
    e.preventDefault();
  };

  render() {
    const [frames, idx, playing, speed, progress, loading] = this.s.value;
    if (!frames.length) {
      return html`<div class="sv-scrubber sv-surface-float sv-scrubber--empty">
        <m3e-loading-indicator></m3e-loading-indicator>
        <span class="sv-label-large">${loading ? 'Loading radar…' : 'Radar unavailable'}</span>
        ${loading ? nothing : html`<m3e-button variant="tonal" @click=${refreshRadar}>Retry</m3e-button>`}
      </div>`;
    }
    const pastCount = frames.filter((f) => f.type === 'past').length;
    const nowcastCount = frames.length - pastCount;
    const shown = frames[this.hoverIdx ?? idx] ?? frames[idx];
    const time = shown ? shown.time * 1000 : null;
    const nowcastStart = pastCount > 0 && pastCount < frames.length ? ((pastCount - 0.5) / (frames.length - 1)) * 100 : null;
    const thumb = this.pct(idx);

    return html`
      <div class="sv-scrubber sv-surface-float">
        ${progress
          ? html`<div class="sv-scrubber-progress">
              <m3e-linear-progress-indicator variant="wavy" value=${(progress.loaded / progress.total) * 100}></m3e-linear-progress-indicator>
              <span class="sv-label-small">Caching frames ${progress.loaded}/${progress.total}</span>
            </div>`
          : nothing}
        <div class="sv-scrubber-top">
          <m3e-icon-button
            variant="filled"
            size="medium"
            width="wide"
            toggle
            ?selected=${playing}
            aria-label=${playing ? 'Pause' : 'Play'}
            @click=${() => store.set({ radarPlaying: !playing })}
          >
            <m3e-icon name="play_arrow" filled></m3e-icon>
            <m3e-icon slot="selected" name="pause" filled></m3e-icon>
          </m3e-icon-button>

          <m3e-button-group variant="connected" size="extra-small">
            ${SPEEDS.map(
              (sp) => html`<m3e-button
                variant="tonal"
                toggle
                ?selected=${speed === sp}
                @beforeinput=${(e: Event) => speed === sp && e.preventDefault()}
                @click=${() => store.set({ radarSpeed: sp })}
              >${sp}×</m3e-button>`,
            )}
          </m3e-button-group>

          <div class="sv-scrubber-time">
            <div class="sv-scrubber-clock">
              ${time ? format(time, 'h:mm a') : '—'}
              ${shown?.type === 'nowcast' ? html`<span class="sv-chip sv-chip--tertiary">Forecast</span>` : nothing}
            </div>
            <div class="sv-label-medium sv-muted">${time ? format(time, 'EEE, MMM d') : ''}</div>
          </div>

          <m3e-button variant="tonal" size="extra-small" @click=${jumpToNow}>
            <m3e-icon slot="icon" name="skip_next"></m3e-icon>Now
          </m3e-button>
          <m3e-icon-button variant="standard" size="small" aria-label="Refresh radar" ?disabled=${loading} @click=${refreshRadar}>
            <m3e-icon name="refresh"></m3e-icon>
          </m3e-icon-button>
        </div>

        <div
          class="sv-timeline"
          tabindex="0"
          role="slider"
          aria-label="Radar time"
          aria-valuemin="0"
          aria-valuemax=${frames.length - 1}
          aria-valuenow=${idx}
          @pointerdown=${this.onPointerDown}
          @pointermove=${this.onPointerMove}
          @pointerup=${this.onPointerUp}
          @pointerleave=${() => (this.hoverIdx = null)}
          @keydown=${this.onKey}
        >
          <div class="sv-timeline-track">
            <div class="sv-timeline-active" style="width: calc(${thumb}% - 6px)"></div>
            <div class="sv-timeline-inactive" style="left: calc(${thumb}% + 6px)"></div>
            ${nowcastStart != null
              ? html`<div class="sv-timeline-nowcast" style="left: max(calc(${thumb}% + 6px), ${nowcastStart}%)"></div>`
              : nothing}
            ${frames.map((f, i) =>
              i === idx ? nothing : html`<span class="sv-timeline-stop ${i < idx ? 'on' : ''} ${f.type}" style="left:${this.pct(i)}%"></span>`,
            )}
          </div>
          <div class="sv-timeline-handle ${this.dragging ? 'dragging' : ''}" style="left:${thumb}%"></div>
          ${this.hoverIdx != null && !this.dragging && frames[this.hoverIdx]
            ? html`<div class="sv-tooltip" style="left:${this.pct(this.hoverIdx)}%">
                ${format(frames[this.hoverIdx].time * 1000, 'h:mm a')}${frames[this.hoverIdx].type === 'nowcast' ? ' · Forecast' : ''}
              </div>`
            : nothing}
        </div>

        <div class="sv-scrubber-legend sv-label-small">
          <span><i class="dot dot--primary"></i>${pastCount} past frames</span>
          ${nowcastCount ? html`<span><i class="dot dot--tertiary"></i>${nowcastCount} nowcast frames</span>` : nothing}
          <span class="sv-muted">LibreWXR</span>
        </div>
      </div>
    `;
  }
}
