// Shared formatters + small HTML helpers for the v2 site. Number formatters
// mirror the v1 site's render.ts; tier helpers are new (v2 carries a cost tier
// on every row).

export function fmt(v: unknown, d = 2): string {
	return typeof v === 'number' ? v.toFixed(d) : '—';
}

export function fmtInt(v: unknown): string {
	return typeof v === 'number' ? Math.round(v).toString() : '—';
}

export function fmtMoneyHour(v: unknown): string {
	return typeof v === 'number' ? `$${v.toFixed(4)}` : '—';
}

// value_score is null for free providers ($/game-hour == 0).
export function fmtValue(v: unknown): string {
	return typeof v === 'number' ? v.toFixed(1) : 'free';
}

export function fmtCi(ci: unknown): string {
	if (
		Array.isArray(ci) &&
		ci.length === 2 &&
		typeof ci[0] === 'number' &&
		typeof ci[1] === 'number'
	) {
		return `${ci[0].toFixed(2)}–${ci[1].toFixed(2)}`;
	}
	return '';
}

export const TIERS = ['free', 'budget', 'mid', 'premium'] as const;
export type Tier = (typeof TIERS)[number];

// Badge CSS class for a tier — colours live in theme.css (.tier-free etc.) so
// they track the active light/dark theme. Returns '' for unknown tiers.
export function tierClass(tier: string): string {
	return (TIERS as readonly string[]).includes(tier) ? `tier-${tier}` : '';
}

// True when two overall 95% CIs overlap — i.e. the ranking gap between the two
// rows is NOT statistically real.
export function ciOverlap(a: [number, number], b: [number, number]): boolean {
	return a[0] <= b[1] && b[0] <= a[1];
}
