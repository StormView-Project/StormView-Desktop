import { html, nothing } from 'lit';
import { customElement, query, state } from 'lit/decorators.js';
import { errorText } from '../lib/api';
import { loadWebLLM, sendChat, WEBLLM_MODELS, webgpuAvailable } from '../lib/assistant';
import { $, store, StoreController } from '../lib/store';
import type { ChatMessage } from '../lib/types';
import { SvElement } from '../lib/ui';

const SUGGESTIONS = ['Will it rain today?', 'What should I wear?', 'Any alerts right now?', 'Warmest day this week?'];

@customElement('sv-assistant')
export class SvAssistant extends SvElement {
  private s = new StoreController(this, (s) =>
    [s.assistantOpen, s.assistantProvider, s.assistantModel, s.sidebarPosition, s.isNarrow] as const);
  @state() private messages: ChatMessage[] = [];
  @state() private streaming = false;
  @state() private input = '';
  @state() private webllmState: 'idle' | 'checking' | 'unsupported' | 'loading' | 'ready' | 'error' = 'idle';
  @state() private progress = 0;
  @state() private progressText = '';
  @query('.sv-chat-log') private log!: HTMLElement;

  private async ensureWebLLM() {
    if (this.webllmState === 'ready' || this.webllmState === 'loading') return;
    this.webllmState = 'checking';
    if (!(await webgpuAvailable())) {
      this.webllmState = 'unsupported';
      return;
    }
    this.webllmState = 'loading';
    try {
      await loadWebLLM($().assistantModel || WEBLLM_MODELS[0].id, (p, t) => {
        this.progress = p;
        this.progressText = t;
      });
      this.webllmState = 'ready';
    } catch (e) {
      this.progressText = errorText(e);
      this.webllmState = 'error';
    }
  }

  private async send(text: string) {
    const q = text.trim();
    if (!q || this.streaming) return;
    this.input = '';
    const history: ChatMessage[] = [...this.messages, { role: 'user', content: q }];
    this.messages = [...history, { role: 'assistant', content: '' }];
    this.streaming = true;
    let full = '';
    try {
      await sendChat(history, (d) => {
        full += d;
        this.messages = [...history, { role: 'assistant', content: full }];
      });
      if (!full) this.messages = [...history, { role: 'assistant', content: '(No response from the model.)' }];
    } catch (e) {
      this.messages = [...history, { role: 'assistant', content: `⚠ ${errorText(e)}` }];
    } finally {
      this.streaming = false;
    }
  }

  updated() {
    if (this.log) this.log.scrollTop = this.log.scrollHeight;
  }

  private gate() {
    const [, provider, model] = this.s.value;
    if (provider === 'webllm') {
      if (this.webllmState === 'ready') return null;
      if (this.webllmState === 'unsupported') return html`<m3e-callout variant="error"><m3e-icon slot="icon" name="memory"></m3e-icon>
        WebGPU isn’t available in this window (WebKitGTK usually lacks it). Switch to Ollama or an OpenAI-compatible local server in Settings → Assistant.</m3e-callout>`;
      if (this.webllmState === 'loading' || this.webllmState === 'checking')
        return html`<div class="sv-status sv-col">
          <m3e-loading-indicator variant="contained"></m3e-loading-indicator>
          <div class="sv-title-small">${this.webllmState === 'checking' ? 'Checking WebGPU…' : 'Downloading model…'}</div>
          <m3e-linear-progress-indicator variant="wavy" class="sv-full" value=${this.progress * 100}></m3e-linear-progress-indicator>
          <div class="sv-body-small sv-muted">${this.progressText}</div>
        </div>`;
      return html`<div class="sv-status sv-col">
        ${this.webllmState === 'error' ? html`<m3e-callout variant="error"><m3e-icon slot="icon" name="error"></m3e-icon>${this.progressText}</m3e-callout>` : nothing}
        <p class="sv-body-medium sv-muted">Runs entirely on your device. First use downloads a small AI model.</p>
        <m3e-button variant="filled" @click=${() => this.ensureWebLLM()}>Load assistant</m3e-button>
      </div>`;
    }
    if (!model)
      return html`<div class="sv-status sv-col">
        <p class="sv-body-medium sv-muted">Connect a local model (Ollama or an OpenAI-compatible server) to chat about your forecast.</p>
        <m3e-button variant="filled" @click=${() => store.set({ settingsOpen: true, settingsTab: 'assistant' })}>Set up assistant</m3e-button>
      </div>`;
    return null;
  }

  render() {
    const [open, provider, model, side] = this.s.value;
    const gate = this.gate();
    return html`
      <div class="sv-assistant" data-side=${side === 'left' ? 'right' : 'left'}>
        ${open
          ? html`<section class="sv-chat sv-surface-float">
              <header class="sv-chat-head">
                <m3e-shape name="flower" style="--m3e-shape-size:36px;--m3e-shape-container-color:var(--md-sys-color-tertiary-container);color:var(--md-sys-color-on-tertiary-container)">
                  <m3e-icon name="smart_toy" filled style="--m3e-icon-size:20px"></m3e-icon>
                </m3e-shape>
                <div class="sv-chat-title">
                  <div class="sv-title-medium">Weather Assistant</div>
                  <div class="sv-label-small sv-muted">${provider === 'webllm' ? 'In-app · WebGPU' : model || 'Not configured'}</div>
                </div>
                <m3e-icon-button aria-label="Clear chat" ?disabled=${!this.messages.length} @click=${() => (this.messages = [])}><m3e-icon name="delete_sweep"></m3e-icon></m3e-icon-button>
                <m3e-icon-button aria-label="Close" @click=${() => store.set({ assistantOpen: false })}><m3e-icon name="close"></m3e-icon></m3e-icon-button>
              </header>
              ${gate ??
              html`<div class="sv-chat-log sv-scroll">
                  ${this.messages.length
                    ? this.messages.map((m, i) => html`<div class="sv-msg sv-msg--${m.role}">
                        ${m.content || (this.streaming && i === this.messages.length - 1 ? html`<m3e-loading-indicator class="sv-small-loader"></m3e-loading-indicator>` : '')}
                      </div>`)
                    : html`<div class="sv-chat-empty">
                        <div class="sv-body-medium sv-muted">Ask me anything about your weather.</div>
                        <m3e-chip-set>${SUGGESTIONS.map((q) => html`<m3e-suggestion-chip @click=${() => this.send(q)}>${q}</m3e-suggestion-chip>`)}</m3e-chip-set>
                      </div>`}
                </div>
                <form class="sv-chat-input" @submit=${(e: Event) => { e.preventDefault(); this.send(this.input); }}>
                  <m3e-form-field variant="filled" class="sv-field sv-grow" hide-subscript="always">
                    <label slot="label" for="chat-in">Ask about the weather…</label>
                    <input id="chat-in" autocomplete="off" .value=${this.input} ?disabled=${this.streaming} @input=${(e: Event) => (this.input = (e.target as HTMLInputElement).value)} />
                  </m3e-form-field>
                  <m3e-icon-button variant="filled" size="medium" type="submit" aria-label="Send" ?disabled=${this.streaming || !this.input.trim()}>
                    <m3e-icon name="send" filled></m3e-icon>
                  </m3e-icon-button>
                </form>`}
            </section>`
          : nothing}
        <m3e-fab variant=${open ? 'secondary-container' : 'tertiary-container'} size="medium" aria-label="Weather assistant" @click=${() => store.set({ assistantOpen: !open })}>
          <m3e-icon name=${open ? 'close' : 'smart_toy'} filled></m3e-icon>
        </m3e-fab>
      </div>
    `;
  }
}
