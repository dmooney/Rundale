// HTML renderers for SortableTable column.render functions.
//
// SortableTable is generic — it doesn't know about brand icons or
// model-detail links. These helpers return safe HTML strings the table
// pipes into `{@html …}` so .astro pages can keep their column specs
// declarative.
import { brandForFamily } from './brands';

// Minimal escaper for values that we don't fully control (model_id is
// catalog-curated but harmless to escape anyway).
function esc(s: string): string {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function brandGlyph(family: string | null | undefined, size = 16): string {
  const b = brandForFamily(family);
  if (!b) return '';
  if (b.kind === 'icon') {
    return `<svg class="brand-icon" viewBox="0 0 24 24" width="${size}" height="${size}" role="img" aria-label="${esc(b.title)}" style="fill: #${b.hex}"><title>${esc(b.title)}</title><path d="${b.svgPath}"/></svg>`;
  }
  return `<span class="brand-fallback" role="img" aria-label="${esc(b.title)}" title="${esc(b.title)}" style="background:#${b.hex}">${esc(b.initials)}</span>`;
}

export function modelCell(row: { model_id: string; slug?: string; family?: string | null; display_name?: string }, base: string): string {
  const label = esc(row.display_name ?? row.model_id);
  const link = row.slug
    ? `<a href="${esc(base)}/models/${esc(row.slug)}">${label}</a>`
    : label;
  return `${brandGlyph(row.family)}${link}`;
}

export function badge(text: string, title?: string, style?: string): string {
  const t = title ? ` title="${esc(title)}"` : '';
  const s = style ? ` style="${esc(style)}"` : '';
  return `<span class="badge"${t}${s}>${esc(text)}</span>`;
}

export function bugPill(n: number): string {
  if (!n || n <= 0) return '';
  return ` <span class="badge" title="responses the judge flagged as bench-harness bugs (chain-of-thought leak / blank reply) and excluded from the score" style="margin-left:0.3rem;background:#fde68a;color:#92400e">🐛 ${n} bug${n === 1 ? '' : 's'}</span>`;
}

export function fmt(v: unknown, d = 2): string {
  return typeof v === 'number' ? v.toFixed(d) : '—';
}

export function fmtInt(v: unknown): string {
  return typeof v === 'number' ? Math.round(v).toString() : '—';
}

export function fmtPct(v: unknown, d = 1): string {
  return typeof v === 'number' ? `${(v * 100).toFixed(d)}%` : '—';
}

export function fmtMoney(v: unknown, d = 4): string {
  return typeof v === 'number' ? `$${v.toFixed(d)}` : '—';
}

export function fmtMoneyHour(v: unknown): string {
  return typeof v === 'number' ? `$${v.toFixed(2)}` : '—';
}
