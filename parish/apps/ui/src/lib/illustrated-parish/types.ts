import type { NotebookAction } from '$lib/notebook/actions';
import type { MapData, NpcInfo, TextLogEntry, WorldSnapshot } from '$lib/types';

export type ParishTab = 'notes' | 'people' | 'places' | 'rumours' | 'journal';

export type NotebookCommandPhase =
	| 'idle'
	| 'focused'
	| 'typing'
	| 'busy'
	| 'disabled'
	| 'error';

export interface NotebookCommandState {
	text: string;
	focused: boolean;
	busy: boolean;
	disabled: boolean;
	error: string | null;
}

export interface NotebookCommandPresentation {
	phase: NotebookCommandPhase;
	displayText: string;
	statusText: string | null;
	showCaret: boolean;
	sendDisabled: boolean;
}

export type NotebookSurface =
	| 'journal'
	| 'people'
	| 'focail'
	| 'map'
	| 'save'
	| 'debug'
	| 'mod'
	| 'bug'
	| 'shortcuts'
	| 'utility'
	| 'time'
	| 'intents'
	| 'rumours';

export interface ParishRect {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface ParishPoint {
	x: number;
	y: number;
}

export interface ParishLayout {
	mode: 'desktop' | 'mobile';
	width: number;
	height: number;
	logoCard: ParishRect;
	statusRibbon: ParishRect;
	compass: ParishRect;
	nearbyRail: ParishRect;
	moreButton: ParishRect;
	notebookPage: ParishRect;
	tabRail: ParishRect;
	tabs: ParishRect[];
	actionStrip: ParishRect;
	actionCells: ParishRect[];
	intentStrip: ParishRect;
	mapCard: ParishRect;
	timeCard: ParishRect;
	activeIntentsCard: ParishRect;
	exitLabels: Array<ParishRect & { label: string }>;
}

export type ParishTargetKind =
	| 'nearby-portrait'
	| 'scene-person'
	| 'tab'
	| 'action'
	| 'intent'
	| 'send'
	| 'card'
	| 'more';

export type ParishTargetActivation =
	| { type: 'select-npc'; realName: string }
	| { type: 'open-tab'; tab: ParishTab }
	| { type: 'action'; action: NotebookAction }
	| { type: 'focus-input' }
	| { type: 'send' }
	| { type: 'open-surface'; surface: NotebookSurface };

export interface ParishHitTarget {
	id: string;
	kind: ParishTargetKind;
	label: string;
	rect: ParishRect;
	activation: ParishTargetActivation;
	order: number;
	disabled?: boolean;
}

export interface ParishRenderCallbacks {
	onAction: (action: NotebookAction) => void;
	onFocusInput: () => void;
	onOpenSurface: (surface: NotebookSurface) => void;
	onOpenTab: (tab: ParishTab) => void;
	onSelectNpc: (realName: string) => void;
	onSend: () => void;
}

export interface ParishRenderState {
	activeTab: ParishTab;
	world: WorldSnapshot | null;
	map: MapData | null;
	npcs: NpcInfo[];
	selectedNpc: NpcInfo | null;
	selectedRealName: string | null;
	journalEntries: TextLogEntry[];
	command: NotebookCommandState;
	callbacks: ParishRenderCallbacks;
}
