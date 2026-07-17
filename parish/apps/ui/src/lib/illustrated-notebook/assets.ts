export const NOTEBOOK_ASSET_BASE = '/rundale/notebook-ui/';

export interface NotebookPersonArtManifestEntry {
	real_name?: string;
	display_name: string;
	npc_id?: number;
	portrait: string;
	marker: string;
	approval_status: string;
	source_config?: string;
	review_notes?: string;
}

export interface NotebookPersonArtManifest {
	source_config: string;
	portrait_prompt: string;
	marker_prompt: string;
	contact_sheet: string;
	contact_sheet_html: string;
	fallback: NotebookPersonArtManifestEntry;
	people: NotebookPersonArtManifestEntry[];
}

export interface NotebookAssetManifest {
	name: string;
	version: number;
	source: string;
	assets: {
		personArt?: NotebookPersonArtManifest;
		portraits?: string[];
		npcMarkers?: string[];
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

export const NOTEBOOK_ASSETS = {
	manifest: `${NOTEBOOK_ASSET_BASE}asset-manifest.json`,
	visualScenes: `${NOTEBOOK_ASSET_BASE}visual-scenes.json`,
	scenePlate: `${NOTEBOOK_ASSET_BASE}scene-crossroads.png`,
	topRibbon: `${NOTEBOOK_ASSET_BASE}top-ribbon.png`,
	spiralNotebookPage: `${NOTEBOOK_ASSET_BASE}spiral-notebook-page.png`,
	notebookBindingRings: `${NOTEBOOK_ASSET_BASE}notebook-binding-rings.png`,
	sideTabs: [
		`${NOTEBOOK_ASSET_BASE}side-tab-notes.png`,
		`${NOTEBOOK_ASSET_BASE}side-tab-people.png`,
		`${NOTEBOOK_ASSET_BASE}side-tab-places.png`,
		`${NOTEBOOK_ASSET_BASE}side-tab-rumours.png`,
		`${NOTEBOOK_ASSET_BASE}side-tab-journal.png`,
	],
	actionStampFrames: [
		`${NOTEBOOK_ASSET_BASE}action-stamp-frame-a.png`,
		`${NOTEBOOK_ASSET_BASE}action-stamp-frame-b.png`,
		`${NOTEBOOK_ASSET_BASE}action-stamp-frame-c.png`,
	],
	actionIcons: {
		talk: `${NOTEBOOK_ASSET_BASE}action-icon-talk.png`,
		ask: `${NOTEBOOK_ASSET_BASE}action-icon-ask.png`,
		help: `${NOTEBOOK_ASSET_BASE}action-icon-help.png`,
		observe: `${NOTEBOOK_ASSET_BASE}action-icon-observe.png`,
		leave: `${NOTEBOOK_ASSET_BASE}action-icon-leave.png`,
	},
	intentParchmentStrip: `${NOTEBOOK_ASSET_BASE}intent-parchment-strip.png`,
	handwrittenInputLine: `${NOTEBOOK_ASSET_BASE}handwritten-input-line.png`,
	inkStampSend: `${NOTEBOOK_ASSET_BASE}ink-stamp-send.png`,
	nearbyPortraitStrip: `${NOTEBOOK_ASSET_BASE}nearby-portrait-strip.png`,
	nearbyPortraitCardFrame: `${NOTEBOOK_ASSET_BASE}nearby-portrait-card-frame.png`,
	activeIntentsCard: `${NOTEBOOK_ASSET_BASE}active-intents-card.png`,
	mapCard: `${NOTEBOOK_ASSET_BASE}map-card.png`,
	timeCard: `${NOTEBOOK_ASSET_BASE}time-card.png`,
	mapIcon: `${NOTEBOOK_ASSET_BASE}map-icon.png`,
	timeIcon: `${NOTEBOOK_ASSET_BASE}time-icon.png`,
	paperExitLabel: `${NOTEBOOK_ASSET_BASE}paper-exit-label.png`,
	npcSelectionRing: `${NOTEBOOK_ASSET_BASE}npc-selection-ring.png`,
	playerMarker: `${NOTEBOOK_ASSET_BASE}player-marker.png`,
} as const;

const UNKNOWN_NOTEBOOK_PERSON_ART: ResolvedNotebookPersonArt = {
	displayName: 'Unknown parish neighbour',
	portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
	marker: `${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
	fallback: true,
};

export const NOTEBOOK_ASSET_URLS = [
	NOTEBOOK_ASSETS.scenePlate,
	NOTEBOOK_ASSETS.topRibbon,
	NOTEBOOK_ASSETS.spiralNotebookPage,
	NOTEBOOK_ASSETS.notebookBindingRings,
	...NOTEBOOK_ASSETS.sideTabs,
	...NOTEBOOK_ASSETS.actionStampFrames,
	...Object.values(NOTEBOOK_ASSETS.actionIcons),
	NOTEBOOK_ASSETS.intentParchmentStrip,
	NOTEBOOK_ASSETS.handwrittenInputLine,
	NOTEBOOK_ASSETS.inkStampSend,
	NOTEBOOK_ASSETS.nearbyPortraitStrip,
	NOTEBOOK_ASSETS.nearbyPortraitCardFrame,
	NOTEBOOK_ASSETS.activeIntentsCard,
	NOTEBOOK_ASSETS.mapCard,
	NOTEBOOK_ASSETS.timeCard,
	NOTEBOOK_ASSETS.mapIcon,
	NOTEBOOK_ASSETS.timeIcon,
	NOTEBOOK_ASSETS.paperExitLabel,
	NOTEBOOK_ASSETS.npcSelectionRing,
	NOTEBOOK_ASSETS.playerMarker,
];

export function notebookAssetUrl(path: string): string {
	if (/^(?:https?:)?\/\//.test(path) || path.startsWith('/')) return path;
	return `${NOTEBOOK_ASSET_BASE}${path}`;
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

function validNpcId(value: number | undefined): value is number {
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
		portrait: notebookAssetUrl(entry.portrait),
		marker: notebookAssetUrl(entry.marker),
		fallback,
	};
}

export function loadNotebookPersonArt(
	manifest: NotebookAssetManifest | null | undefined,
): LoadedNotebookPersonArt {
	const personArt =
		manifest?.assets && typeof manifest.assets === 'object'
			? manifest.assets.personArt
			: undefined;
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
