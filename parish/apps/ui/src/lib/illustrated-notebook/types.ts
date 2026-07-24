import type { MapData, NpcInfo, TextLogEntry, WorldSnapshot } from '$lib/types';
import type { NotebookAction } from '$lib/notebook/actions';

export type NotebookTab = 'notes' | 'people' | 'places' | 'rumours' | 'journal';

export interface DepthBand {
	name: string;
	min_depth: number;
	max_depth: number;
	marker_scale: number;
}

export interface SceneAnchor {
	id?: string;
	label?: string;
	x: number;
	y: number;
	depth: number;
}

export interface VisualScene {
	location_ids: number[];
	plate_asset: string;
	written_visual_summary: string;
	camera_hint: string;
	background_generation_source: string;
	depth_bands: DepthBand[];
	anchors: {
		player: SceneAnchor;
		npcs: SceneAnchor[];
		exits: SceneAnchor[];
	};
}

export interface VisualScenesFile {
	version: number;
	scenes: VisualScene[];
}

export interface NotebookRect {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface NotebookLayout {
	mode: 'desktop' | 'mobile';
	width: number;
	height: number;
	topRibbon: NotebookRect;
	nearbyStrip: NotebookRect;
	notebookPage: NotebookRect;
	tabs: NotebookRect[];
	actionStamps: NotebookRect[];
	intentStrip: NotebookRect;
	mapCard: NotebookRect | null;
	timeCard: NotebookRect | null;
	activeIntentsCard: NotebookRect | null;
}

export interface RenderCallbacks {
	onAction: (action: NotebookAction) => void;
	onFocusInput: () => void;
	onOpenActiveIntents: () => void;
	onOpenMap: () => void;
	onOpenTab: (tab: NotebookTab) => void;
	onOpenTime: () => void;
	onSelectNpc: (realName: string) => void;
	onSend: () => void;
}

export type NotebookLiveLineKind =
	| 'player'
	| 'npc'
	| 'location'
	| 'narration'
	| 'error';

export interface NotebookLiveLine {
	key: string;
	kind: NotebookLiveLineKind;
	speaker: string;
	content: string;
	streaming: boolean;
}

export interface NotebookPersonView {
	label: string;
	mood: string;
	detail: string;
	recentLines: NotebookLiveLine[];
	emptyNote: string;
}

export interface NotebookViewModel {
	locationName: string;
	locationDescription: string;
	weather: string;
	time: string;
	person: NotebookPersonView | null;
	liveTitle: string;
	liveEmpty: string;
	liveLines: NotebookLiveLine[];
	intentPlaceholder: string;
	draftSummary: string;
}

export interface NotebookViewModelInput {
	world: WorldSnapshot | null;
	npcs: NpcInfo[];
	selectedNpc: NpcInfo | null;
	textLog: TextLogEntry[];
	busy: boolean;
	intentText: string;
}

export interface NotebookRenderState {
	world: WorldSnapshot | null;
	map: MapData | null;
	npcs: NpcInfo[];
	selectedNpc: NpcInfo | null;
	selectedRealName: string | null;
	view: NotebookViewModel;
	intentText: string;
	inputFocused: boolean;
	busy: boolean;
	callbacks: RenderCallbacks;
}
