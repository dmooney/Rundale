export const NOTEBOOK_PERSON_ART_BASE = '/rundale/notebook-ui/';
export const NOTEBOOK_PERSON_ART_MANIFEST = `${NOTEBOOK_PERSON_ART_BASE}asset-manifest.json`;

export interface NotebookPersonArtManifestEntry {
	real_name?: string;
	display_name: string;
	npc_id?: number | null;
	portrait: string;
	marker: string;
	approval_status: string;
	source_config?: string;
	review_notes?: string;
}

export interface NotebookPersonArtManifest {
	source_config?: string;
	portrait_prompt?: string;
	marker_prompt?: string;
	contact_sheet?: string;
	contact_sheet_html?: string;
	fallback: NotebookPersonArtManifestEntry;
	people: NotebookPersonArtManifestEntry[];
}

export interface NotebookPersonArtRuntimeManifest {
	name?: string;
	version?: number;
	source?: string;
	assets?: {
		personArt?: NotebookPersonArtManifest;
	};
}

export interface ResolvedNotebookPersonArt {
	displayName: string;
	portrait: string;
	marker: string;
	fallback: boolean;
}

export interface LoadedNotebookPersonArt {
	byId: Map<number, ResolvedNotebookPersonArt>;
	byName: Map<string, ResolvedNotebookPersonArt>;
	fallback: ResolvedNotebookPersonArt;
	assetUrls: string[];
}

const UNKNOWN_NOTEBOOK_PERSON_ART: ResolvedNotebookPersonArt = {
	displayName: 'Unknown parish neighbour',
	portrait: `${NOTEBOOK_PERSON_ART_BASE}people/portrait-unknown-neighbour.png`,
	marker: `${NOTEBOOK_PERSON_ART_BASE}people/marker-unknown-neighbour.png`,
	fallback: true,
};

export function notebookPersonArtUrl(path: string): string {
	if (/^(?:https?:)?\/\//.test(path) || path.startsWith('/')) return path;
	return `${NOTEBOOK_PERSON_ART_BASE}${path}`;
}

export function normalizeNotebookPersonName(value: string): string {
	return value
		.normalize('NFKD')
		.replace(/[\u0300-\u036f]/g, '')
		.trim()
		.toLowerCase()
		.replace(/\s+/g, ' ');
}

function validAssetEntry(
	entry: unknown,
): entry is NotebookPersonArtManifestEntry {
	if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return false;
	const candidate = entry as Partial<NotebookPersonArtManifestEntry>;
	return (
		candidate.approval_status === 'approved' &&
		typeof candidate.display_name === 'string' &&
		candidate.display_name.trim().length > 0 &&
		typeof candidate.portrait === 'string' &&
		candidate.portrait.trim().length > 0 &&
		typeof candidate.marker === 'string' &&
		candidate.marker.trim().length > 0 &&
		(candidate.real_name === undefined ||
			typeof candidate.real_name === 'string')
	);
}

function validNpcId(value: number | null | undefined): value is number {
	return (
		typeof value === 'number' &&
		Number.isInteger(value) &&
		value > 0 &&
		value <= 0xffffffff
	);
}

function resolvedEntry(
	entry: NotebookPersonArtManifestEntry,
	fallback: boolean,
): ResolvedNotebookPersonArt {
	return {
		displayName: entry.display_name,
		portrait: notebookPersonArtUrl(entry.portrait),
		marker: notebookPersonArtUrl(entry.marker),
		fallback,
	};
}

export function loadNotebookPersonArt(
	manifest: NotebookPersonArtRuntimeManifest | null | undefined,
): LoadedNotebookPersonArt {
	const personArt = manifest?.assets?.personArt;
	const people = Array.isArray(personArt?.people) ? personArt.people : [];
	const fallback =
		personArt?.fallback && validAssetEntry(personArt.fallback)
			? resolvedEntry(personArt.fallback, true)
			: UNKNOWN_NOTEBOOK_PERSON_ART;
	const byId = new Map<number, ResolvedNotebookPersonArt>();
	const byName = new Map<string, ResolvedNotebookPersonArt>();
	const assetUrls = new Set([fallback.portrait, fallback.marker]);
	const idCounts = new Map<number, number>();
	const nameCounts = new Map<string, number>();

	for (const entry of people) {
		if (validAssetEntry(entry) && validNpcId(entry.npc_id)) {
			idCounts.set(entry.npc_id, (idCounts.get(entry.npc_id) ?? 0) + 1);
		}
		if (validAssetEntry(entry) && entry.real_name) {
			const name = normalizeNotebookPersonName(entry.real_name);
			if (name) nameCounts.set(name, (nameCounts.get(name) ?? 0) + 1);
		}
	}

	for (const entry of people) {
		if (!validAssetEntry(entry)) continue;
		if (entry.npc_id !== undefined && !validNpcId(entry.npc_id)) continue;
		if (validNpcId(entry.npc_id) && idCounts.get(entry.npc_id) !== 1) continue;
		const resolved = resolvedEntry(entry, false);
		if (validNpcId(entry.npc_id)) byId.set(entry.npc_id, resolved);
		if (entry.real_name) {
			const name = normalizeNotebookPersonName(entry.real_name);
			if (nameCounts.get(name) === 1) byName.set(name, resolved);
		}
		assetUrls.add(resolved.portrait);
		assetUrls.add(resolved.marker);
	}

	return { byId, byName, fallback, assetUrls: [...assetUrls] };
}

export function resolveNotebookPersonArt(
	personArt: LoadedNotebookPersonArt | null | undefined,
	npcId: number | null | undefined,
	realName: string | null | undefined,
): ResolvedNotebookPersonArt {
	if (!personArt) return UNKNOWN_NOTEBOOK_PERSON_ART;
	if (npcId !== null && npcId !== undefined) {
		if (!validNpcId(npcId)) return personArt.fallback;
		return personArt.byId.get(npcId) ?? personArt.fallback;
	}
	if (!realName) return personArt.fallback;
	return (
		personArt.byName.get(normalizeNotebookPersonName(realName)) ??
		personArt.fallback
	);
}
