// Build-time loader for the v2 benchmark-of-record site.
//
// The site reads `promptfoo/leaderboard/leaderboard.jsonl` DIRECTLY at build
// time (no Python data step, no re-scoring). Each jsonl line is one
// (candidate, run) row appended by `promptfoo/scripts/leaderboard.py`; the
// board shows the latest row per candidate, and `history` keeps every row so a
// detail page can chart a candidate's overall over time.
//
// Rows carry no brand/display metadata, so we join each row's `candidate` (the
// catalog `spec`) against `promptfoo/catalog/candidates.jsonl` for `family`
// (brand icon), `display_name`, `provider_id`, and `tier`. Provenance
// (judge model, dataset merkle) falls back to `config/judge.yaml` +
// `v2/MANIFEST.json` when the leaderboard is empty — which is the committed
// state until the first funded run lands.
import fs from 'node:fs';
import path from 'node:path';

export interface CategoryScore {
	score: number;
	ci95: [number, number];
	n: number;
}

export interface LeaderRow {
	candidate: string;
	model: string | null;
	tier: string;
	overall: number;
	overall_ci95: [number, number];
	categories: Record<string, CategoryScore>;
	usd_per_game_hour: number;
	value_score: number | null;
	latency_p50_ms: number | null;
	latency_p95_ms: number | null;
	tokens_per_sec: number;
	judge_model: string;
	dataset_merkle: string;
	timestamp: string;
}

// Row enriched with catalog metadata + a stable url slug.
export interface BoardRow extends LeaderRow {
	slug: string;
	display_name: string;
	family: string | null;
	provider_id: string | null;
	// LMArena-style CI-aware rank: 1 + #{j : lo_j > hi_i}. Models with
	// overlapping overall CIs share a rank (a tie).
	rank: number;
	tied: boolean;
	// on the quality-vs-cost Pareto (efficiency) frontier.
	onFrontier: boolean;
}

export interface Summary {
	overall: BoardRow | null; // highest overall
	value: BoardRow | null; // highest value_score (paid only)
	dialogue: BoardRow | null; // highest dialogue category score
	fastest: BoardRow | null; // lowest latency_p50_ms
}

export interface BenchData {
	generatedUtc: string | null; // latest row timestamp, or null when empty
	judgeModel: string;
	merkle: string;
	candidateCount: number;
	hasRuns: boolean;
	rows: BoardRow[];
	summary: Summary;
	// candidate spec → chronological history (oldest→newest) for trend charts
	history: Record<string, LeaderRow[]>;
	// the canonical category order the board renders columns in
	categoryOrder: string[];
}

// The promptfoo suite root that holds leaderboard/, catalog/, config/, v2/.
// During `astro build` the cwd is promptfoo/bench-site, so its parent is the
// suite root. An env override keeps the loader testable from anywhere.
function promptfooDir(): string {
	const override = process.env.RB_PROMPTFOO_DIR;
	if (override) return path.resolve(override);
	return path.resolve(process.cwd(), '..');
}

function readLines(file: string): string[] {
	if (!fs.existsSync(file)) return [];
	return fs
		.readFileSync(file, 'utf-8')
		.split('\n')
		.map((l) => l.trim())
		.filter((l) => l.length > 0 && !l.startsWith('//'));
}

// catalog spec → { family, display_name, provider_id, tier }
function loadCatalog(root: string): Map<string, CatalogMeta> {
	const out = new Map<string, CatalogMeta>();
	for (const line of readLines(
		path.join(root, 'catalog', 'candidates.jsonl'),
	)) {
		let c: Record<string, unknown>;
		try {
			c = JSON.parse(line);
		} catch {
			continue;
		}
		if (!c || typeof c !== 'object') continue;
		const spec = c.spec as string | undefined;
		if (!spec) continue;
		out.set(spec, {
			family: (c.family as string) ?? null,
			display_name: (c.display_name as string) ?? null,
			provider_id: (c.provider_id as string) ?? null,
			tier: (c.tier as string) ?? '',
		});
	}
	return out;
}

interface CatalogMeta {
	family: string | null;
	display_name: string | null;
	provider_id: string | null;
	tier: string;
}

// Minimal `model:` extractor — judge.yaml is a flat mapping; avoid a YAML dep.
function judgeModelFallback(root: string): string {
	const f = path.join(root, 'config', 'judge.yaml');
	if (!fs.existsSync(f)) return 'claude-sonnet-4-6';
	for (const line of fs.readFileSync(f, 'utf-8').split('\n')) {
		const m = /^model:\s*(\S+)/.exec(line.trim());
		if (m) return m[1];
	}
	return 'claude-sonnet-4-6';
}

function merkleFallback(root: string): string {
	const f = path.join(root, 'v2', 'MANIFEST.json');
	if (!fs.existsSync(f)) return '';
	try {
		const m = JSON.parse(fs.readFileSync(f, 'utf-8'));
		return m.merkle_root_sha256 ?? m.merkle ?? '';
	} catch {
		return '';
	}
}

// Stable, filesystem-safe slug from a candidate spec ("model@provider#env:KEY").
function slugFor(candidate: string, model: string | null): string {
	const base = (model || candidate.split('@')[0] || candidate)
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
	return base || 'candidate';
}

const CANONICAL_ORDER = [
	'intent',
	'dialogue',
	'reaction',
	'simulation',
	'gaeilge',
	'multiturn',
];

export function loadBenchData(): BenchData {
	const root = promptfooDir();
	const judgeModel = judgeModelFallback(root);
	const merkle = merkleFallback(root);
	const catalog = loadCatalog(root);

	const history: Record<string, LeaderRow[]> = {};
	for (const line of readLines(
		path.join(root, 'leaderboard', 'leaderboard.jsonl'),
	)) {
		let r: LeaderRow;
		try {
			r = JSON.parse(line);
		} catch {
			continue;
		}
		if (!r || typeof r !== 'object' || !r.candidate) continue;
		// normalise categories so every downstream access is crash-safe even if
		// a row in the jsonl omits or nulls it.
		if (!r.categories || typeof r.categories !== 'object') r.categories = {};
		(history[r.candidate] ??= []).push(r);
	}

	// latest row per candidate → enriched board rows, ranked by overall desc.
	const latest: LeaderRow[] = Object.values(history).map(
		(rs) => rs[rs.length - 1],
	);
	const slugSeen = new Map<string, number>();
	const rows: BoardRow[] = latest
		.sort((a, b) => b.overall - a.overall)
		.map((r) => {
			const meta = catalog.get(r.candidate);
			let slug = slugFor(r.candidate, r.model);
			// de-dup slugs deterministically (two specs collapsing to one base)
			const seen = slugSeen.get(slug) ?? 0;
			slugSeen.set(slug, seen + 1);
			if (seen > 0) slug = `${slug}-${seen + 1}`;
			return {
				...r,
				slug,
				display_name:
					meta?.display_name ?? r.model ?? r.candidate.split('@')[0],
				family: meta?.family ?? null,
				provider_id: meta?.provider_id ?? null,
				// prefer the row's own tier; fall back to the catalog's
				tier: r.tier || meta?.tier || '',
				rank: 0,
				tied: false,
				onFrontier: false,
			};
		});

	annotateRanks(rows);
	annotateFrontier(rows);

	// category columns: canonical order first, then any extras the data carries.
	const present = new Set<string>();
	for (const r of rows)
		for (const k of Object.keys(r.categories ?? {})) present.add(k);
	const categoryOrder = [
		...CANONICAL_ORDER.filter((c) => present.has(c)),
		...[...present].filter((c) => !CANONICAL_ORDER.includes(c)).sort(),
	];

	const generatedUtc = rows.length
		? rows
				.map((r) => r.timestamp)
				.sort()
				.slice(-1)[0]
		: null;

	return {
		generatedUtc,
		judgeModel: rows[0]?.judge_model || judgeModel,
		merkle: rows[0]?.dataset_merkle || merkle,
		candidateCount: rows.length,
		hasRuns: rows.length > 0,
		rows,
		summary: buildSummary(rows),
		history,
		categoryOrder,
	};
}

// LMArena-style CI-aware ranking: a model's rank is 1 + the number of models
// whose lower 95% CI strictly exceeds this model's upper 95% CI. Models that no
// one dominates by CI share rank 1 (a tie); `tied` marks any shared rank.
function annotateRanks(rows: BoardRow[]): void {
	for (const r of rows) {
		const [, hi] = r.overall_ci95;
		let dominators = 0;
		for (const o of rows) {
			if (o === r) continue;
			if (o.overall_ci95[0] > hi) dominators++;
		}
		r.rank = dominators + 1;
	}
	const counts = new Map<number, number>();
	for (const r of rows) counts.set(r.rank, (counts.get(r.rank) ?? 0) + 1);
	for (const r of rows) r.tied = (counts.get(r.rank) ?? 0) > 1;
}

// Pareto / efficiency frontier over (maximize overall, minimize $/game-hour).
// A row is on the frontier when no other row is at least as cheap AND strictly
// higher quality (or equal cost and higher quality). Free/$0 models all share
// the cheapest cost band, so only the best-quality free model(s) make it.
function annotateFrontier(rows: BoardRow[]): void {
	for (const r of rows) {
		const dominated = rows.some(
			(o) =>
				o !== r &&
				o.usd_per_game_hour <= r.usd_per_game_hour &&
				o.overall > r.overall,
		);
		r.onFrontier = !dominated;
	}
}

function buildSummary(rows: BoardRow[]): Summary {
	if (!rows.length) {
		return { overall: null, value: null, dialogue: null, fastest: null };
	}
	const best = (
		score: (r: BoardRow) => number | null | undefined,
	): BoardRow | null => {
		let win: BoardRow | null = null;
		let bestVal = -Infinity;
		for (const r of rows) {
			const v = score(r);
			if (typeof v === 'number' && v > bestVal) {
				bestVal = v;
				win = r;
			}
		}
		return win;
	};
	return {
		overall: best((r) => r.overall),
		value: best((r) => r.value_score),
		dialogue: best((r) => r.categories?.dialogue?.score),
		fastest: best((r) => (r.latency_p50_ms != null ? -r.latency_p50_ms : null)),
	};
}
