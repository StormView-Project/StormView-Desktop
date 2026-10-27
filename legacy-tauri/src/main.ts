// Typefaces: Google Sans Flex (the M3 Expressive brand face, variable weight/width/roundness)
// and Material Symbols Rounded (variable FILL/weight icon font used by <m3e-icon>).
import '@fontsource-variable/google-sans-flex/full.css';
import 'material-symbols/rounded.css';
import 'maplibre-gl/dist/maplibre-gl.css';

// Material 3 Expressive components.
import '@m3e/web/theme';
import '@m3e/web/button';
import '@m3e/web/button-group';
import '@m3e/web/icon';
import '@m3e/web/icon-button';
import '@m3e/web/toolbar';
import '@m3e/web/card';
import '@m3e/web/shape';
import '@m3e/web/dialog';
import '@m3e/web/tabs';
import '@m3e/web/switch';
import '@m3e/web/segmented-button';
import '@m3e/web/slider';
import '@m3e/web/select';
import '@m3e/web/option';
import '@m3e/web/form-field';
import '@m3e/web/list';
import '@m3e/web/chips';
import '@m3e/web/callout';
import '@m3e/web/expansion-panel';
import '@m3e/web/loading-indicator';
import '@m3e/web/progress-indicator';
import '@m3e/web/menu';
import '@m3e/web/divider';
import '@m3e/web/snackbar';
import '@m3e/web/fab';
import '@m3e/web/nav-bar';
import '@m3e/web/skeleton';
import '@m3e/web/heading';
import '@m3e/web/date-input';
import '@m3e/web/datepicker';
import '@m3e/web/checkbox';

import './styles/app.css';
import { errorText } from './lib/api';
import { startData } from './lib/data';
import { store } from './lib/store';
import { initTheme } from './lib/theme';
import { openExternal, toast } from './lib/ui';
import './components/sv-app';

// Desktop apps shouldn't offer the webview's own context menu (the map has its own).
document.addEventListener('contextmenu', (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest('input, textarea')) e.preventDefault();
});

// Runtime errors surface as a snackbar (the web app's ErrorReporter), at most once per message per minute.
const recentErrors = new Map<string, number>();
function reportError(err: unknown) {
  console.error('[StormView]', err);
  const msg = errorText(err) || 'Unknown error';
  const now = Date.now();
  if (now - (recentErrors.get(msg) ?? 0) < 60_000) return;
  recentErrors.set(msg, now);
  toast(`Something went wrong: ${msg.slice(0, 120)}`, 'Report', () =>
    openExternal('https://github.com/arc360alt/StormView-Rewrite/issues'));
}
window.addEventListener('unhandledrejection', (e) => reportError(e.reason));
window.addEventListener('error', (e) => reportError(e.error ?? e.message));

(async () => {
  await store.hydrate();
  initTheme();
  startData();
})();
