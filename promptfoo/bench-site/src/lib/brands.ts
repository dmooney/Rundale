// Family → company brand resolver for leaderboard rows.
//
// The catalog's `family` field is a model lineage tag (e.g. "kimi-k2.5",
// "qwen3.6", "glm"). This helper maps each family to the brand-icon slug
// that owns the underlying model. simple-icons is the source of truth
// for the icon path + brand colour; brands it doesn't carry (OpenAI,
// Zhipu/Z.ai) fall back to a colored circle with initials.
//
// Add a new family by extending FAMILY_TO_SLUG. If simple-icons gains a
// missing brand later, just point the family at the new slug — no other
// changes needed.
import * as si from 'simple-icons';

export type Brand =
	| {
			kind: 'icon';
			title: string;
			svgPath: string;
			hex: string;
			lowLum: boolean;
	  }
	| { kind: 'fallback'; title: string; initials: string; hex: string };

// True when a hex colour is so dark it would vanish on the dark theme (e.g.
// Anthropic #181818, OpenAI/GitHub/xAI black). Callers flag such glyphs so the
// stylesheet can recolour them to the foreground on dark backgrounds while
// keeping the real brand colour on light. Uses relative luminance (sRGB).
export function isLowLum(hex: string): boolean {
	const h = hex.replace('#', '');
	if (h.length !== 6) return false;
	const c = [0, 2, 4].map((i) => {
		const v = parseInt(h.slice(i, i + 2), 16) / 255;
		return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
	});
	const lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
	return lum < 0.12;
}

// Slugs verified against simple-icons (probed 2026-05-25). When adding a
// new family, run: node -e "console.log(require('simple-icons').si<Name>)"
// to confirm the named export exists before listing it here.
const FAMILY_TO_SLUG: Record<string, string> = {
	'kimi-k2.5': 'moonshotai',
	'kimi-k2.6': 'moonshotai',
	'kimi-k2-thinking': 'moonshotai',
	'qwen2.5': 'qwen',
	qwen3: 'qwen',
	'qwen3.5': 'qwen',
	'qwen3.6': 'qwen',
	deepseek: 'deepseek',
	'deepseek-flash': 'deepseek',
	'deepseek-thinking': 'deepseek',
	'minimax-m2.5': 'minimax',
	'minimax-m2.7': 'minimax',
	'mimo-v2.5': 'xiaomi',
	'mimo-v2.5-pro': 'xiaomi',
	'mimo-v2-omni': 'xiaomi',
	'mimo-v2-pro': 'xiaomi',
	claude: 'anthropic',
	gemma: 'google',
	gemini: 'google',
	'llama-3.3': 'meta',
	llama: 'meta',
	mistral: 'mistralai',
	grok: 'x',
};

// simple-icons exports each brand as `si<PascalSlug>`. e.g. `moonshotai`
// → `siMoonshotai`. PascalCase only the first char per the package's
// convention (lowercased slugs stay lowercased internally).
function slugToExportName(slug: string): string {
	return 'si' + slug.charAt(0).toUpperCase() + slug.slice(1);
}

interface SimpleIcon {
	title: string;
	path: string;
	hex: string;
}

function loadIcon(slug: string): SimpleIcon | null {
	const name = slugToExportName(slug);
	const icon = (si as Record<string, unknown>)[name] as SimpleIcon | undefined;
	return icon ?? null;
}

// Deterministic HSL color from family string. Same family always gets
// the same hue, so a missing brand stays visually stable across runs.
function fallbackHex(seed: string): string {
	let h = 0;
	for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
	const hue = h % 360;
	// Convert HSL(h, 55%, 38%) to hex without depending on a CSS layer.
	return hslToHex(hue, 0.55, 0.38);
}

function hslToHex(h: number, s: number, l: number): string {
	const c = (1 - Math.abs(2 * l - 1)) * s;
	const hp = h / 60;
	const x = c * (1 - Math.abs((hp % 2) - 1));
	let r = 0,
		g = 0,
		b = 0;
	if (hp >= 0 && hp < 1) [r, g, b] = [c, x, 0];
	else if (hp < 2) [r, g, b] = [x, c, 0];
	else if (hp < 3) [r, g, b] = [0, c, x];
	else if (hp < 4) [r, g, b] = [0, x, c];
	else if (hp < 5) [r, g, b] = [x, 0, c];
	else [r, g, b] = [c, 0, x];
	const m = l - c / 2;
	const to8 = (v: number) =>
		Math.round((v + m) * 255)
			.toString(16)
			.padStart(2, '0');
	return to8(r) + to8(g) + to8(b);
}

function fallbackInitials(family: string): string {
	// First letter of each alpha run, up to two letters. "mimo-v2.5-pro" → "MV".
	const runs = family.match(/[A-Za-z]+/g) ?? [];
	const letters = runs
		.slice(0, 2)
		.map((r) => r[0].toUpperCase())
		.join('');
	return letters || family.slice(0, 2).toUpperCase();
}

// Vendor keyword → simple-icons slug. v2 catalog families are finer-grained
// than v1's (e.g. "claude-sonnet-4", "deepseek-v3.1-terminus", "qwen3.6-flash"),
// so an exact FAMILY_TO_SLUG lookup misses most of them. We fall back to
// substring matching on the family string against these vendor keywords —
// longest keyword first so "deepseek" wins before a hypothetical "seek".
const VENDOR_KEYWORDS: Array<[string, string]> = [
	['claude', 'anthropic'],
	['deepseek', 'deepseek'],
	['qwen', 'qwen'],
	['llama', 'meta'],
	['gemini', 'google'],
	['gemma', 'google'],
	['mistral', 'mistralai'],
	['ministral', 'mistralai'],
	['mixtral', 'mistralai'],
	['nemo', 'mistralai'],
	['grok', 'x'],
	['kimi', 'moonshotai'],
	['minimax', 'minimax'],
	['mimo', 'xiaomi'],
];
VENDOR_KEYWORDS.sort((a, b) => b[0].length - a[0].length);

// Resolve a catalog family string to a renderable Brand. Returns null
// when family is empty / 'unknown' so callers can choose to skip the
// logo column entirely. Tries the exact FAMILY_TO_SLUG map first, then a
// vendor-keyword substring match, then a deterministic initials fallback.
export function brandForFamily(
	family: string | undefined | null,
): Brand | null {
	if (!family || family === 'unknown') return null;
	let slug: string | undefined = FAMILY_TO_SLUG[family];
	if (!slug) {
		const lower = family.toLowerCase();
		for (const [kw, s] of VENDOR_KEYWORDS) {
			if (lower.includes(kw)) {
				slug = s;
				break;
			}
		}
	}
	if (slug) {
		const icon = loadIcon(slug);
		if (icon) {
			return {
				kind: 'icon',
				title: icon.title,
				svgPath: icon.path,
				hex: icon.hex,
				lowLum: isLowLum(icon.hex),
			};
		}
	}
	return {
		kind: 'fallback',
		title: family,
		initials: fallbackInitials(family),
		hex: fallbackHex(family),
	};
}
