import { LitElement } from 'lit';
import { M3eSnackbar } from '@m3e/web/snackbar';
import { openUrl } from '@tauri-apps/plugin-opener';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

/**
 * All app components render into light DOM so one global stylesheet (built on
 * the --md-sys-* tokens <m3e-theme> provides) styles everything consistently.
 */
export class SvElement extends LitElement {
  protected createRenderRoot() {
    return this;
  }
}

export function toast(message: string, action?: string, onAction?: () => void) {
  if (action) {
    M3eSnackbar.open(message, action, true, { duration: 6000, actionCallback: onAction });
  } else {
    M3eSnackbar.open(message, { duration: 4000 });
  }
}

export async function openExternal(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, '_blank', 'noopener');
  }
}

export async function copyText(text: string) {
  try {
    await writeText(text);
  } catch {
    await navigator.clipboard.writeText(text);
  }
}

/** Reads `.value` / `.checked` from M3E form controls in event handlers. */
export const valueOf = (e: Event) => (e.target as unknown as { value: string }).value;
export const checkedOf = (e: Event) => (e.target as unknown as { checked: boolean }).checked;
