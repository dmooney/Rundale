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
	portraits: [
		`${NOTEBOOK_ASSET_BASE}portrait-placeholder-1.png`,
		`${NOTEBOOK_ASSET_BASE}portrait-placeholder-2.png`,
		`${NOTEBOOK_ASSET_BASE}portrait-placeholder-3.png`,
		`${NOTEBOOK_ASSET_BASE}portrait-placeholder-4.png`,
	],
	activeIntentsCard: `${NOTEBOOK_ASSET_BASE}active-intents-card.png`,
	mapCard: `${NOTEBOOK_ASSET_BASE}map-card.png`,
	timeCard: `${NOTEBOOK_ASSET_BASE}time-card.png`,
	mapIcon: `${NOTEBOOK_ASSET_BASE}map-icon.png`,
	timeIcon: `${NOTEBOOK_ASSET_BASE}time-icon.png`,
	paperExitLabel: `${NOTEBOOK_ASSET_BASE}paper-exit-label.png`,
	npcSelectionRing: `${NOTEBOOK_ASSET_BASE}npc-selection-ring.png`,
	playerMarker: `${NOTEBOOK_ASSET_BASE}player-marker.png`,
	npcMarkers: [
		`${NOTEBOOK_ASSET_BASE}npc-marker-1.png`,
		`${NOTEBOOK_ASSET_BASE}npc-marker-2.png`,
		`${NOTEBOOK_ASSET_BASE}npc-marker-3.png`,
	],
} as const;

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
	...NOTEBOOK_ASSETS.portraits,
	NOTEBOOK_ASSETS.activeIntentsCard,
	NOTEBOOK_ASSETS.mapCard,
	NOTEBOOK_ASSETS.timeCard,
	NOTEBOOK_ASSETS.mapIcon,
	NOTEBOOK_ASSETS.timeIcon,
	NOTEBOOK_ASSETS.paperExitLabel,
	NOTEBOOK_ASSETS.npcSelectionRing,
	NOTEBOOK_ASSETS.playerMarker,
	...NOTEBOOK_ASSETS.npcMarkers,
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

function approved(
	entry: NotebookPersonArtManifestEntry | undefined,
): entry is NotebookPersonArtManifestEntry {
	return entry?.approval_status === 'approved';
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
	const personArt = manifest?.assets.personArt;
	const fallback = approved(personArt?.fallback)
		? resolvedEntry(personArt.fallback, true)
		: {
				displayName: 'Unknown parish neighbour',
				portrait: NOTEBOOK_ASSETS.portraits[0],
				marker: NOTEBOOK_ASSETS.npcMarkers[0],
				fallback: true,
			};
	const byName = new Map<string, ResolvedNotebookPersonArt>();
	const assetUrls = new Set([fallback.portrait, fallback.marker]);
	for (const entry of personArt?.people ?? []) {
		if (!approved(entry) || !entry.real_name) continue;
		const resolved = resolvedEntry(entry, false);
		byName.set(normalizeNotebookPersonName(entry.real_name), resolved);
		assetUrls.add(resolved.portrait);
		assetUrls.add(resolved.marker);
	}
	return { byName, fallback, assetUrls: [...assetUrls] };
}

export function resolveNotebookPersonArt(
	personArt: LoadedNotebookPersonArt | null | undefined,
	realName: string | null | undefined,
): ResolvedNotebookPersonArt {
	if (!personArt || !realName) {
		return loadNotebookPersonArt(null).fallback;
	}
	return (
		personArt.byName.get(normalizeNotebookPersonName(realName)) ??
		personArt.fallback
	);
}
