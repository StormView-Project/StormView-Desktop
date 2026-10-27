import { html, nothing, type TemplateResult } from 'lit';
import { semantic, weatherVisual } from '../../lib/format';

/** A widget surface: an M3 filled card with an emphasized title row. */
export function widget(
  id: string,
  title: string,
  icon: string,
  body: TemplateResult | TemplateResult[],
  trailing: TemplateResult | typeof nothing = nothing,
) {
  return html`
    <m3e-card variant="filled" class="sv-widget sv-widget--${id}">
      <div class="sv-widget-inner">
        <div class="sv-widget-head">
          <m3e-icon name=${icon} class="sv-widget-icon"></m3e-icon>
          <span class="sv-widget-title">${title}</span>
          ${trailing}
        </div>
        ${body}
      </div>
    </m3e-card>
  `;
}

/** Material Symbols weather glyph, tinted with a theme-harmonized semantic color. */
export function wxIcon(code: number, isDay = true, size = 24) {
  const v = weatherVisual(code, isDay);
  return html`<m3e-icon
    class="sv-wx"
    name=${v.icon}
    filled
    title=${v.label}
    style="--m3e-icon-size:${size}px;color:${semantic(v.hue).color}"
  ></m3e-icon>`;
}

/** An M3 Expressive shape holding the condition glyph; morphs when the condition changes. */
export function wxShape(code: number, isDay = true, size = 112) {
  const v = weatherVisual(code, isDay);
  const c = semantic(v.hue);
  return html`<m3e-shape
    class="sv-wx-shape"
    name=${v.shape}
    style="--m3e-shape-size:${size}px;--m3e-shape-container-color:${c.container};color:${c.onContainer}"
  >
    <m3e-icon name=${v.icon} filled style="--m3e-icon-size:${Math.round(size * 0.5)}px"></m3e-icon>
  </m3e-shape>`;
}

/** Horizontal meter: track in surface-container-highest, fill in the semantic color. */
export function meter(pct: number, color: string) {
  return html`<div class="sv-meter"><div class="sv-meter-fill" style="width:${Math.max(0, Math.min(100, pct))}%;background:${color}"></div></div>`;
}
