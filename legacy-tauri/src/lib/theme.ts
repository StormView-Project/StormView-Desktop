import {
  argbFromHex, hexFromArgb, Hct, SchemeTonalSpot, MaterialDynamicColors,
} from '@material/material-color-utilities';
import { adaptiveSeed } from './format';
import { $, store } from './store';

/**
 * Material You seed presets. The web app's hand-tuned palettes (Ocean, Sunset,
 * Forest, Crimson) survive as *seeds* — every color in the UI is generated
 * from them by Google's HCT dynamic-color algorithm, never hand-picked.
 */
export const COLOR_PRESETS: { id: string; label: string; seed: string }[] = [
  { id: 'storm', label: 'StormView', seed: '#4f8ef5' },
  { id: 'baseline', label: 'Baseline', seed: '#6750a4' },
  { id: 'ocean', label: 'Ocean', seed: '#0891b2' },
  { id: 'sunset', label: 'Sunset', seed: '#ea580c' },
  { id: 'forest', label: 'Forest', seed: '#16a34a' },
  { id: 'crimson', label: 'Crimson', seed: '#e11d48' },
  { id: 'lavender', label: 'Lavender', seed: '#8b5cf6' },
  { id: 'sand', label: 'Sand', seed: '#b08d57' },
  { id: 'rose', label: 'Rose', seed: '#db2777' },
  { id: 'mint', label: 'Mint', seed: '#10b981' },
];

export const VARIANTS: { id: string; label: string; desc: string }[] = [
  { id: 'tonal-spot', label: 'Tonal Spot', desc: 'The Material You default — calm, low-chroma accents.' },
  { id: 'expressive', label: 'Expressive', desc: 'Playful hue rotations for a bolder, livelier palette.' },
  { id: 'vibrant', label: 'Vibrant', desc: 'Maximum chroma for primary colors.' },
  { id: 'fidelity', label: 'Fidelity', desc: 'Stays as close to the seed color as possible.' },
  { id: 'content', label: 'Content', desc: 'Matches the seed like Fidelity, with analogous accents.' },
  { id: 'rainbow', label: 'Rainbow', desc: 'Colorful accents with neutral surfaces.' },
  { id: 'fruit-salad', label: 'Fruit Salad', desc: 'A playful mix of complementary hues.' },
  { id: 'neutral', label: 'Neutral', desc: 'Nearly grayscale with a hint of the seed.' },
  { id: 'monochrome', label: 'Monochrome', desc: 'Pure grayscale.' },
];

/** The seed currently in effect, given the color source setting. */
export function activeSeed(): string {
  const s = $();
  if (s.colorSource === 'custom') return s.customSeed;
  if (s.colorSource === 'adaptive') {
    const cu = s.weather?.current;
    return adaptiveSeed(cu?.conditionCode, cu?.isDay);
  }
  return COLOR_PRESETS.find((p) => p.id === s.colorSource)?.seed ?? COLOR_PRESETS[0].seed;
}

/** Primary/secondary/tertiary preview dots for a seed, like Android's wallpaper color picker. */
export function previewSwatch(seed: string, dark: boolean) {
  const scheme = new SchemeTonalSpot(Hct.fromInt(argbFromHex(seed)), dark, 0);
  const c = (dc: { getArgb: (s: SchemeTonalSpot) => number }) => hexFromArgb(dc.getArgb(scheme));
  return {
    primary: c(MaterialDynamicColors.primary),
    secondary: c(MaterialDynamicColors.secondaryContainer),
    tertiary: c(MaterialDynamicColors.tertiaryContainer),
    surface: c(MaterialDynamicColors.surfaceContainerHigh),
  };
}

const ROLES = [
  'primary', 'on-primary', 'primary-container', 'on-primary-container',
  'secondary', 'on-secondary', 'secondary-container', 'on-secondary-container',
  'tertiary', 'on-tertiary', 'tertiary-container', 'on-tertiary-container',
  'error', 'error-container', 'on-error-container',
  'surface', 'on-surface', 'on-surface-variant', 'surface-variant',
  'surface-dim', 'surface-bright', 'surface-container-lowest', 'surface-container-low',
  'surface-container', 'surface-container-high', 'surface-container-highest',
  'outline', 'outline-variant', 'inverse-surface', 'inverse-on-surface', 'inverse-primary',
];

const camel = (s: string) => s.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

const media = window.matchMedia('(prefers-color-scheme: dark)');

function readTokens(el: HTMLElement) {
  const cs = getComputedStyle(el);
  const tokens: Record<string, string> = {};
  for (const r of ROLES) {
    const v = cs.getPropertyValue(`--md-sys-color-${r}`).trim();
    if (v) tokens[camel(r)] = v;
  }
  return tokens;
}

/** Pushes theme settings onto <m3e-theme> and mirrors its resolved tokens into the store. */
export function initTheme() {
  const el = document.getElementById('theme') as HTMLElement & { isDark?: boolean };
  let lastKey = '';

  const sync = () => {
    const tokens = readTokens(el);
    const isDark = $().themeMode === 'dark' || ($().themeMode === 'system' && media.matches);
    document.documentElement.dataset.theme = isDark ? 'dark' : 'light';
    if (tokens.primary) store.set({ tokens, isDark });
  };

  const apply = () => {
    const s = $();
    const seed = activeSeed();
    const key = [seed, s.themeMode, s.schemeVariant, s.contrast, s.motion].join('|');
    if (key === lastKey) return;
    lastKey = key;
    el.setAttribute('color', seed);
    el.setAttribute('scheme', s.themeMode === 'system' ? 'auto' : s.themeMode);
    el.setAttribute('variant', s.schemeVariant);
    el.setAttribute('contrast', s.contrast);
    el.setAttribute('motion', s.motion);
    // m3e-theme applies on its next update; read the tokens once it has.
    requestAnimationFrame(() => requestAnimationFrame(sync));
  };

  el.addEventListener('change', () => requestAnimationFrame(sync));
  media.addEventListener('change', () => {
    lastKey = '';
    apply();
  });
  store.subscribe((s, prev) => {
    if (
      s.colorSource !== prev.colorSource || s.customSeed !== prev.customSeed ||
      s.themeMode !== prev.themeMode || s.schemeVariant !== prev.schemeVariant ||
      s.contrast !== prev.contrast || s.motion !== prev.motion ||
      (s.colorSource === 'adaptive' && s.weather !== prev.weather)
    ) apply();
  });
  apply();
}
