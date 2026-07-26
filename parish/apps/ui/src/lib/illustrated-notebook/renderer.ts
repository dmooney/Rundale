import 'pixi.js/unsafe-eval';
import {
	Application,
	Assets,
	Container,
	Graphics,
	Rectangle,
	Sprite,
	Text,
	Texture,
	type TextStyleOptions,
} from 'pixi.js';
import type { NotebookAction } from '$lib/notebook/actions';
import { NOTEBOOK_ASSET_URLS, NOTEBOOK_ASSETS } from './assets';
import {
	activateNotebookTarget,
	notebookHitTarget,
	type NotebookHitTarget,
	type NotebookHitTargetKind,
	type NotebookTargetActivation,
} from './interactions';
import {
	computeNotebookLayout,
	NOTEBOOK_ACTIONS,
	pointFromAnchor,
	scaleForDepth,
	sortAnchorsByDepth,
} from './layout';
import { currentNotebookLocationId, selectVisualScene } from './scene';
import {
	notebookNpcLabel,
	notebookReactionSummary,
	selectVisibleNotebookLines,
} from './view-model';
import type {
	NotebookLayout,
	NotebookRect,
	NotebookRenderState,
	NotebookTab,
	SceneAnchor,
	VisualScene,
	VisualScenesFile,
} from './types';

const INK = 0x312316;
const INK_SOFT = 0x6f5836;
const INK_RED = 0x874534;
const PAPER_LIGHT = 0xf4e4bd;
const TAB_LABELS: NotebookTab[] = [
	'notes',
	'people',
	'places',
	'rumours',
	'journal',
];
const ACTION_LABELS: Record<NotebookAction, string> = {
	talk: 'Talk',
	ask: 'Ask',
	help: 'Help',
	observe: 'Observe',
	leave: 'Leave',
};
const FOCUS_INK = 0x5f2f24;

export interface IllustratedNotebookRendererOptions {
	onHitTargetsChanged?: (targets: NotebookHitTarget[]) => void;
}

export interface NotebookRendererAssetState {
	textures: Map<string, Texture>;
	scenes: VisualScene[];
	degraded: boolean;
}

export interface NearbyPortraitPlacement {
	frameRect: NotebookRect;
	portraitRect: NotebookRect;
	size: number;
}

/**
 * Returns the real Pixi/browser hit rectangle for an authored NPC marker.
 *
 * Marker art deliberately scales for depth and can visually overlap at the
 * mobile breakpoint. Interaction regions stay anchor-centred and narrow
 * enough for the closest authored crossroads anchors to remain disjoint.
 */
export function computeNpcMarkerHitRect(
	layout: NotebookLayout,
	anchor: SceneAnchor,
): NotebookRect {
	const point = pointFromAnchor(anchor, layout.width, layout.height);
	const width =
		layout.mode === 'desktop'
			? Math.max(44, Math.min(72, layout.width * 0.05))
			: Math.max(24, Math.min(44, layout.width * 0.075));
	const height = width * 1.35;
	const x = Math.max(0, Math.min(layout.width - width, point.x - width / 2));
	const y = Math.max(
		0,
		Math.min(layout.height - height, point.y + 10 - height),
	);
	return { x, y, width, height };
}

/**
 * Computes the actual interactive portrait frames used by the renderer.
 *
 * Desktop portraits form a vertical rail beneath the "Nearby" heading;
 * mobile portraits form one horizontal row. Both variants derive their size
 * from the available strip and use a positive gap, so the hit rectangles are
 * fully contained and pairwise disjoint at breakpoint edges.
 */
export function computeNearbyPortraitPlacements(
	layout: NotebookLayout,
	npcCount: number,
): NearbyPortraitPlacement[] {
	const count = Math.max(0, Math.min(4, Math.floor(npcCount)));
	if (count === 0) return [];
	const strip = layout.nearbyStrip;

	if (layout.mode === 'desktop') {
		const gap = 6;
		const topInset = 52;
		const bottomInset = 8;
		const availableHeight = Math.max(
			1,
			strip.height - topInset - bottomInset - gap * (count - 1),
		);
		const size = Math.max(
			1,
			Math.min(82, (strip.width - 16) / 1.12, availableHeight / count / 1.18),
		);
		const frameWidth = size * 1.12;
		const frameHeight = size * 1.18;
		const frameX = strip.x + (strip.width - frameWidth) / 2;
		return Array.from({ length: count }, (_, index) => {
			const frameY = strip.y + topInset + index * (frameHeight + gap);
			return {
				size,
				frameRect: {
					x: frameX,
					y: frameY,
					width: frameWidth,
					height: frameHeight,
				},
				portraitRect: {
					x: frameX + size * 0.21,
					y: frameY + size * 0.25,
					width: size * 0.68,
					height: size * 0.72,
				},
			};
		});
	}

	const gap = 4;
	const availableWidth = Math.max(1, strip.width - 16 - gap * (count - 1));
	const size = Math.max(
		1,
		Math.min(58, availableWidth / count / 1.12, (strip.height - 8) / 1.18),
	);
	const frameWidth = size * 1.12;
	const frameHeight = size * 1.18;
	const rowWidth = count * frameWidth + (count - 1) * gap;
	const frameX = strip.x + (strip.width - rowWidth) / 2;
	const frameY = strip.y + (strip.height - frameHeight) / 2;
	return Array.from({ length: count }, (_, index) => {
		const x = frameX + index * (frameWidth + gap);
		return {
			size,
			frameRect: {
				x,
				y: frameY,
				width: frameWidth,
				height: frameHeight,
			},
			portraitRect: {
				x: x + size * 0.21,
				y: frameY + size * 0.25,
				width: size * 0.68,
				height: size * 0.72,
			},
		};
	});
}

const FALLBACK_SCENE: VisualScene = {
	location_ids: [],
	plate_asset: null,
	written_visual_summary:
		'Neutral notebook paper; no location-specific illustration is available.',
	camera_hint: 'neutral notebook paper',
	background_generation_source: 'Code-native neutral fallback.',
	depth_bands: [],
	anchors: {
		player: null,
		npcs: [],
		exits: [],
	},
};

type NotebookAssetBundleLoader = (
	urls: readonly string[],
) => Promise<Record<string, Texture>>;
type NotebookSceneLoader = () => Promise<VisualScenesFile>;

async function loadVisualScenesFile(): Promise<VisualScenesFile> {
	const response = await fetch(NOTEBOOK_ASSETS.visualScenes);
	if (!response.ok) {
		throw new Error(`visual scene manifest returned ${response.status}`);
	}
	return (await response.json()) as VisualScenesFile;
}

/**
 * Loads renderer art without making renderer availability depend on it.
 *
 * A bundle rejection or incomplete bundle returns a structural neutral state:
 * code-native paper, no authored plate, and no scene markers. Scene-manifest
 * failure keeps successfully loaded UI chrome but still selects only the
 * neutral scene.
 */
export async function loadNotebookRendererAssets(
	loadBundle: NotebookAssetBundleLoader = async (urls) =>
		(await Assets.load([...urls])) as Record<string, Texture>,
	loadScenes: NotebookSceneLoader = loadVisualScenesFile,
): Promise<NotebookRendererAssetState> {
	let loaded: Record<string, Texture>;
	try {
		loaded = await loadBundle(NOTEBOOK_ASSET_URLS);
	} catch {
		return {
			textures: new Map(),
			scenes: [FALLBACK_SCENE],
			degraded: true,
		};
	}

	const textures = new Map<string, Texture>();
	for (const url of NOTEBOOK_ASSET_URLS) {
		const texture = loaded[url];
		if (!texture) {
			return {
				textures: new Map(),
				scenes: [FALLBACK_SCENE],
				degraded: true,
			};
		}
		textures.set(url, texture);
	}

	try {
		const file = await loadScenes();
		if (!Array.isArray(file.scenes) || file.scenes.length === 0) {
			throw new Error('visual scene manifest has no scenes');
		}
		return { textures, scenes: file.scenes, degraded: false };
	} catch {
		return { textures, scenes: [FALLBACK_SCENE], degraded: true };
	}
}

export class IllustratedNotebookRenderer {
	private app: Application | null = null;
	private readonly textures = new Map<string, Texture>();
	private scenes: VisualScene[] = [FALLBACK_SCENE];
	private scene: VisualScene = FALLBACK_SCENE;
	private lastState: NotebookRenderState | null = null;
	private hitTargets: NotebookHitTarget[] = [];
	private hoveredTargetId: string | null = null;
	private focusedTargetId: string | null = null;
	private hoverRenderFrame: number | null = null;
	private assetLoadDegraded = false;
	private readonly layers = {
		background: new Container(),
		wash: new Container(),
		exits: new Container(),
		markers: new Container(),
		ui: new Container(),
		intent: new Container(),
		treatment: new Container(),
	};

	constructor(
		private readonly host: HTMLElement,
		private readonly options: IllustratedNotebookRendererOptions = {},
	) {}

	async init(): Promise<void> {
		const app = new Application();
		await app.init({
			resizeTo: this.host,
			backgroundColor: 0x1f1a12,
			antialias: true,
			autoDensity: true,
			resolution: Math.min(window.devicePixelRatio || 1, 2),
		});
		this.app = app;
		this.host.appendChild(app.canvas);
		app.stage.sortableChildren = true;
		app.stage.addChild(
			this.layers.background,
			this.layers.wash,
			this.layers.exits,
			this.layers.markers,
			this.layers.ui,
			this.layers.intent,
			this.layers.treatment,
		);
		await this.loadAssets();
	}

	destroy(): void {
		if (this.hoverRenderFrame !== null) {
			window.cancelAnimationFrame(this.hoverRenderFrame);
			this.hoverRenderFrame = null;
		}
		this.clearAll();
		this.app?.destroy(true);
		this.app = null;
	}

	render(state: NotebookRenderState): void {
		if (!this.app) return;
		const width = Math.max(1, this.host.clientWidth || this.app.renderer.width);
		const height = Math.max(
			1,
			this.host.clientHeight || this.app.renderer.height,
		);
		const layout = computeNotebookLayout(width, height);
		this.lastState = state;
		const locationId = currentNotebookLocationId(state.map, state.world);
		this.scene = selectVisualScene(this.scenes, locationId, FALLBACK_SCENE);
		this.host.dataset.sceneLocationId =
			locationId === null ? '' : String(locationId);
		this.host.dataset.scenePlate = this.scene.plate_asset ?? '';
		this.host.dataset.sceneMode = this.scene.plate_asset
			? 'authored'
			: 'neutral';
		this.host.dataset.assetMode = this.assetLoadDegraded
			? 'degraded'
			: 'loaded';
		this.host.dataset.selectedRealName = state.selectedRealName ?? '';
		this.clearAll();
		this.beginHitTargetPass();
		this.drawBackground(width, height);
		this.drawSceneWash(width, height);
		this.drawExitLabels(layout, state);
		this.drawMarkers(layout, state);
		this.drawTopRibbon(layout, state);
		this.drawNearby(layout, state);
		this.drawNotebookPage(layout, state);
		this.drawLiveChronicle(layout, state);
		this.drawActionStamps(layout, state);
		this.drawIntentStrip(layout, state);
		this.drawLowerCards(layout, state);
		this.emitHitTargetsChanged();
	}

	resize(): void {
		if (this.lastState) this.render(this.lastState);
	}

	setFocusedTarget(id: string | null): void {
		if (this.focusedTargetId === id) return;
		this.focusedTargetId = id;
		this.renderTargetTreatments();
	}

	activateTarget(id: string): boolean {
		const callbacks = this.lastState?.callbacks;
		if (!callbacks) return false;
		return activateNotebookTarget(
			this.hitTargets.find((target) => target.id === id),
			callbacks,
		);
	}

	private async loadAssets(): Promise<void> {
		const assets = await loadNotebookRendererAssets();
		this.textures.clear();
		for (const [url, texture] of assets.textures) {
			this.textures.set(url, texture);
		}
		this.scenes = assets.scenes;
		this.assetLoadDegraded = assets.degraded;
	}

	private clearAll(): void {
		for (const layer of Object.values(this.layers)) this.clear(layer);
	}

	private clear(container: Container): void {
		for (const child of container.removeChildren()) {
			child.destroy({
				children: true,
				texture: false,
				textureSource: false,
			} as never);
		}
	}

	private texture(url: string): Texture {
		return this.textures.get(url) ?? Texture.EMPTY;
	}

	private sprite(url: string, rect?: NotebookRect): Sprite {
		const sprite = new Sprite(this.texture(url));
		if (rect) this.place(sprite, rect);
		return sprite;
	}

	private place(sprite: Sprite, rect: NotebookRect): void {
		sprite.x = rect.x;
		sprite.y = rect.y;
		sprite.width = rect.width;
		sprite.height = rect.height;
	}

	private beginHitTargetPass(): void {
		this.hitTargets = [];
	}

	private emitHitTargetsChanged(): void {
		this.options.onHitTargetsChanged?.(
			this.hitTargets.map((target) => ({
				...target,
				rect: { ...target.rect },
				activation: { ...target.activation } as NotebookTargetActivation,
			})),
		);
	}

	private target(
		id: string,
		kind: NotebookHitTargetKind,
		label: string,
		rect: NotebookRect,
		activation: NotebookTargetActivation,
		order: number,
		disabled = false,
	): NotebookHitTarget {
		return notebookHitTarget({
			id,
			kind,
			label,
			rect,
			activation,
			order,
			disabled,
		});
	}

	private registerHitTarget(target: NotebookHitTarget): void {
		if (this.hitTargets.some((existing) => existing.id === target.id)) return;
		this.hitTargets.push(target);
	}

	private bindTarget<T extends Container | Sprite | Graphics>(
		display: T,
		target: NotebookHitTarget,
		hitArea?: Rectangle,
	): T {
		this.registerHitTarget(target);
		display.eventMode = 'static';
		display.cursor = target.disabled ? 'default' : 'pointer';
		if (hitArea) display.hitArea = hitArea;
		display.on('pointerdown', () => {
			this.activateTarget(target.id);
		});
		display.on('pointerover', () => {
			this.setHoveredTarget(target.id);
		});
		display.on('pointerout', () => {
			if (this.hoveredTargetId === target.id) this.setHoveredTarget(null);
		});
		return display;
	}

	private setHoveredTarget(id: string | null): void {
		if (this.hoveredTargetId === id) return;
		this.hoveredTargetId = id;
		if (this.hoverRenderFrame !== null) {
			window.cancelAnimationFrame(this.hoverRenderFrame);
		}
		this.hoverRenderFrame = window.requestAnimationFrame(() => {
			this.hoverRenderFrame = null;
			this.renderTargetTreatments();
		});
	}

	private renderTargetTreatments(): void {
		if (!this.app) return;
		this.clear(this.layers.treatment);
		for (const target of this.hitTargets) {
			this.drawTargetTreatment(target);
		}
	}

	private treatmentRadius(target: NotebookHitTarget): number {
		switch (target.kind) {
			case 'send':
				return 26;
			case 'npc-marker':
			case 'action-stamp':
			case 'intent-strip':
			case 'active-intents-card':
				return 18;
			case 'nearby-portrait':
			case 'map-card':
			case 'time-card':
				return 16;
			case 'tab':
				return 12;
		}
	}

	private drawTargetTreatment(target: NotebookHitTarget): void {
		const focused = this.focusedTargetId === target.id;
		const hovered = this.hoveredTargetId === target.id;
		if (!focused && !hovered) return;
		const rect = target.rect;
		const pad = focused ? 5 : 3;
		const radius = this.treatmentRadius(target);
		const g = new Graphics();
		g.roundRect(
			rect.x - pad,
			rect.y - pad,
			rect.width + pad * 2,
			rect.height + pad * 2,
			radius,
		)
			.fill({ color: PAPER_LIGHT, alpha: focused ? 0.16 : 0.1 })
			.stroke({
				color: focused ? FOCUS_INK : INK,
				width: focused ? 3 : 2,
				alpha: focused ? 0.92 : 0.58,
			});
		this.layers.treatment.addChild(g);
	}

	private addText(
		layer: Container,
		text: string,
		x: number,
		y: number,
		size: number,
		options: Partial<TextStyleOptions> = {},
	): Text {
		const display = new Text({
			text,
			style: {
				fontFamily: options.fontFamily ?? 'Georgia, "Times New Roman", serif',
				fontSize: size,
				fill: options.fill ?? INK,
				fontStyle: options.fontStyle ?? 'italic',
				fontWeight: options.fontWeight ?? '400',
				align: options.align ?? 'left',
				wordWrap: options.wordWrap ?? false,
				wordWrapWidth: options.wordWrapWidth,
				letterSpacing: 0,
				...options,
			},
		});
		display.x = x;
		display.y = y;
		layer.addChild(display);
		return display;
	}

	private drawBackground(width: number, height: number): void {
		if (!this.scene.plate_asset) {
			const paper = new Graphics();
			paper.rect(0, 0, width, height).fill({ color: 0xe8d7ad, alpha: 1 });
			paper
				.rect(0, 0, width, height * 0.18)
				.fill({ color: 0xf4e7c6, alpha: 0.42 });
			paper
				.rect(0, height * 0.82, width, height * 0.18)
				.fill({ color: 0xcdbb91, alpha: 0.2 });
			this.layers.background.addChild(paper);
			return;
		}
		const sprite = this.sprite(this.scene.plate_asset);
		const texture = sprite.texture;
		const scale = Math.max(width / texture.width, height / texture.height);
		sprite.width = texture.width * scale;
		sprite.height = texture.height * scale;
		sprite.x = (width - sprite.width) / 2;
		sprite.y = (height - sprite.height) / 2;
		this.layers.background.addChild(sprite);
	}

	private drawSceneWash(width: number, height: number): void {
		const g = new Graphics();
		g.rect(0, 0, width, height).fill({ color: 0x20170d, alpha: 0.05 });
		g.rect(0, 0, width, height * 0.16).fill({ color: 0xf2dfb2, alpha: 0.12 });
		g.rect(0, height * 0.82, width, height * 0.18).fill({
			color: 0x171108,
			alpha: 0.16,
		});
		this.layers.wash.addChild(g);
	}

	private drawExitLabels(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		const maxLabels = layout.mode === 'mobile' ? 2 : 3;
		for (const anchor of this.scene.anchors.exits.slice(0, maxLabels)) {
			const p = pointFromAnchor(anchor, layout.width, layout.height);
			if (
				layout.mode === 'mobile' &&
				p.y < layout.nearbyStrip.y + layout.nearbyStrip.height + 8
			) {
				continue;
			}
			const labelWidth = layout.mode === 'mobile' ? 112 : 136;
			const labelHeight = layout.mode === 'mobile' ? 36 : 42;
			const card = this.sprite(NOTEBOOK_ASSETS.paperExitLabel, {
				x: p.x - labelWidth / 2,
				y: p.y - labelHeight / 2,
				width: labelWidth,
				height: labelHeight,
			});
			this.layers.exits.addChild(card);
			this.addText(
				this.layers.exits,
				shortText(anchor.label ?? 'Road', layout.mode === 'mobile' ? 14 : 18),
				card.x + labelWidth * 0.31,
				card.y + labelHeight * 0.27,
				layout.mode === 'mobile' ? 11 : 14,
				{ fill: INK, fontStyle: 'italic' },
			);
		}
		if (state.map?.locations.some((loc) => loc.adjacent)) {
			// The labels are authored art anchors; live map exits still drive the text
			// in secondary overlays and movement actions.
		}
	}

	private drawMarkers(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		const actors = [
			...(this.scene.anchors.player
				? [
						{
							kind: 'player' as const,
							anchor: this.scene.anchors.player,
							npc: null,
						},
					]
				: []),
			...state.npcs.slice(0, this.scene.anchors.npcs.length).map((npc, i) => ({
				kind: 'npc' as const,
				anchor: this.scene.anchors.npcs[i],
				npc,
			})),
		];
		for (const actor of sortAnchorsByDepth(
			actors.map((a) => ({ ...a.anchor, actor: a })),
		)) {
			const p = pointFromAnchor(actor, layout.width, layout.height);
			const scale =
				scaleForDepth(actor.depth, this.scene.depth_bands) *
				(layout.mode === 'mobile' ? 0.58 : 0.72);
			const isPlayer = actor.actor.kind === 'player';
			const selected = actor.actor.npc?.real_name === state.selectedRealName;
			if (selected) {
				const ring = this.sprite(NOTEBOOK_ASSETS.npcSelectionRing);
				ring.anchor.set(0.5, 0.5);
				ring.x = p.x;
				ring.y = p.y + 4;
				ring.scale.set(scale);
				this.layers.markers.addChild(ring);
			}
			const npcIndex = actor.actor.npc
				? state.npcs.indexOf(actor.actor.npc)
				: -1;
			const safeNpcIndex = npcIndex >= 0 ? npcIndex : 0;
			const markerUrl = isPlayer
				? NOTEBOOK_ASSETS.playerMarker
				: NOTEBOOK_ASSETS.npcMarkers[
						safeNpcIndex % NOTEBOOK_ASSETS.npcMarkers.length
					];
			const marker = this.sprite(markerUrl);
			marker.anchor.set(0.5, 1);
			marker.x = p.x;
			marker.y = p.y + 10;
			marker.scale.set(scale);
			if (actor.actor.npc) {
				const targetRect = computeNpcMarkerHitRect(layout, actor);
				const target = this.target(
					`marker:${targetIdPart(actor.actor.npc.real_name)}`,
					'npc-marker',
					`Select marker for ${notebookNpcLabel(actor.actor.npc)}`,
					targetRect,
					{
						type: 'select-npc',
						realName: actor.actor.npc.real_name,
					},
					100 + safeNpcIndex,
				);
				this.drawTargetTreatment(target);
				this.bindTarget(
					marker,
					target,
					new Rectangle(
						(targetRect.x - marker.x) / scale,
						(targetRect.y - marker.y) / scale,
						targetRect.width / scale,
						targetRect.height / scale,
					),
				);
			}
			this.layers.markers.addChild(marker);
			if (selected && actor.actor.npc && layout.mode === 'desktop') {
				const label = this.sprite(NOTEBOOK_ASSETS.paperExitLabel, {
					x: p.x + 10,
					y: p.y - 42,
					width: 128,
					height: 38,
				});
				this.layers.markers.addChild(label);
				this.addText(
					this.layers.markers,
					shortNpcName(notebookNpcLabel(actor.actor.npc), 24),
					label.x + 36,
					label.y + 10,
					12,
					{
						fill: INK,
						fontStyle: 'italic',
					},
				);
			}
		}
	}

	private drawTopRibbon(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		const ribbon = this.sprite(NOTEBOOK_ASSETS.topRibbon, layout.topRibbon);
		this.layers.ui.addChild(ribbon);
		const festival = state.world?.festival?.trim() ?? '';
		const location =
			layout.mode === 'mobile' && festival
				? `${state.view.locationName} · ${festival}`
				: state.view.locationName;
		const weather = festival
			? `${state.view.weather} · ${festival}`
			: state.view.weather;
		const time = state.world?.paused
			? `${state.view.time} · PAUSED`
			: state.view.time;
		const y = layout.topRibbon.y + 12;
		const titleSize = layout.mode === 'mobile' ? 20 : 28;
		this.addText(this.layers.ui, 'RUNDALE', 24, y - 2, titleSize, {
			fill: INK,
			fontStyle: 'normal',
			letterSpacing: 3,
		});
		if (layout.mode === 'desktop') {
			this.addText(this.layers.ui, 'Parish Notebook', 28, y + 32, 15, {
				fill: INK_SOFT,
			});
			this.addText(this.layers.ui, location, layout.width * 0.33, y + 5, 24, {
				fill: INK,
			});
			this.addText(this.layers.ui, weather, layout.width * 0.55, y + 5, 22, {
				fill: INK,
			});
			this.addText(this.layers.ui, time, layout.width * 0.72, y + 7, 18, {
				fill: INK,
				fontStyle: 'normal',
			});
			this.addText(this.layers.ui, 'N', layout.width - 48, y + 2, 16, {
				fill: INK,
				fontStyle: 'normal',
			});
			this.addText(this.layers.ui, '+', layout.width - 53, y + 18, 28, {
				fill: INK,
				fontStyle: 'normal',
			});
		} else {
			this.addText(this.layers.ui, location, 24, y + 33, 13, {
				fill: INK_SOFT,
				wordWrap: true,
				wordWrapWidth: layout.width - 150,
			});
			this.addText(this.layers.ui, time, layout.width - 78, y + 12, 14, {
				fill: INK,
				fontStyle: 'normal',
			});
			this.addText(this.layers.ui, 'N', layout.width - 28, y + 35, 12, {
				fill: INK,
				fontStyle: 'normal',
			});
		}
	}

	private drawNearby(layout: NotebookLayout, state: NotebookRenderState): void {
		const npcs = state.npcs.slice(0, 4);
		const placements = computeNearbyPortraitPlacements(layout, npcs.length);
		if (layout.mode === 'desktop') {
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.nearbyPortraitStrip, layout.nearbyStrip),
			);
			this.addText(
				this.layers.ui,
				'Nearby',
				layout.nearbyStrip.x + 28,
				layout.nearbyStrip.y + 24,
				17,
				{
					fill: INK,
				},
			);
			npcs.forEach((npc, index) => {
				this.drawNearbyPerson(layout, state, npc, placements[index], index);
			});
		} else {
			const back = new Graphics();
			back
				.roundRect(
					layout.nearbyStrip.x,
					layout.nearbyStrip.y,
					layout.nearbyStrip.width,
					layout.nearbyStrip.height,
					8,
				)
				.fill({
					color: PAPER_LIGHT,
					alpha: 0.55,
				});
			this.layers.ui.addChild(back);
			this.addText(
				this.layers.ui,
				'Nearby',
				layout.nearbyStrip.x + 8,
				layout.nearbyStrip.y + 6,
				13,
				{ fill: INK },
			);
			npcs.forEach((npc, index) => {
				this.drawNearbyPerson(layout, state, npc, placements[index], index);
			});
		}
	}

	private drawNearbyPerson(
		layout: NotebookLayout,
		state: NotebookRenderState,
		npc: NonNullable<NotebookRenderState['selectedNpc']>,
		placement: NearbyPortraitPlacement,
		index: number,
	): void {
		const { frameRect, portraitRect, size } = placement;
		const selected = npc.real_name === state.selectedRealName;
		const target = this.target(
			`nearby:${targetIdPart(npc.real_name)}`,
			'nearby-portrait',
			`Select nearby person ${notebookNpcLabel(npc)}`,
			frameRect,
			{ type: 'select-npc', realName: npc.real_name },
			200 + index,
		);
		this.drawTargetTreatment(target);
		const frame = this.sprite(
			NOTEBOOK_ASSETS.nearbyPortraitCardFrame,
			frameRect,
		);
		frame.alpha = selected ? 1 : 0.86;
		this.bindTarget(frame, target);
		this.layers.ui.addChild(frame);
		const portrait = this.sprite(
			NOTEBOOK_ASSETS.portraits[index % NOTEBOOK_ASSETS.portraits.length],
			portraitRect,
		);
		this.bindTarget(portrait, target);
		this.layers.ui.addChild(portrait);
		if (layout.mode === 'desktop') {
			this.addText(
				this.layers.ui,
				shortNpcName(notebookNpcLabel(npc), 20),
				frameRect.x,
				portraitRect.y + size * 0.73,
				11,
				{
					fill: INK,
					wordWrap: true,
					wordWrapWidth: size * 1.18,
				},
			);
		}
	}

	private drawNotebookPage(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		const pageSprite = this.sprite(
			NOTEBOOK_ASSETS.spiralNotebookPage,
			layout.notebookPage,
		);
		this.layers.ui.addChild(pageSprite);
		if (layout.mode === 'desktop') {
			const rings = this.sprite(NOTEBOOK_ASSETS.notebookBindingRings, {
				x: layout.notebookPage.x - 18,
				y: layout.notebookPage.y + 8,
				width: 46,
				height: layout.notebookPage.height - 18,
			});
			this.layers.ui.addChild(rings);
		}
		layout.tabs.forEach((tabRect, i) => {
			const tabName = TAB_LABELS[i];
			const target = this.target(
				`tab:${tabName}`,
				'tab',
				`Open ${titleCase(tabName)} notebook tab`,
				tabRect,
				{ type: 'open-tab', tab: tabName },
				300 + i,
			);
			this.drawTargetTreatment(target);
			const tab = this.sprite(NOTEBOOK_ASSETS.sideTabs[i], tabRect);
			this.bindTarget(tab, target);
			this.layers.ui.addChild(tab);
			if (layout.mode === 'desktop') {
				this.addText(
					this.layers.ui,
					titleCase(tabName),
					tabRect.x + 18,
					tabRect.y + 19,
					13,
					{ fill: INK },
				);
			}
		});
		const person = state.view.person;
		const page = layout.notebookPage;
		const compactPage = layout.mode === 'mobile' && page.height < 180;
		const inset = compactPage ? 12 : layout.mode === 'mobile' ? 18 : 46;
		const titleSize = compactPage ? 13 : layout.mode === 'mobile' ? 16 : 25;
		const title = person?.label ?? state.view.locationName;
		const adjustedTitleSize =
			title.length > 36
				? Math.max(13, titleSize - 7)
				: title.length > 24
					? titleSize - 3
					: titleSize;
		this.addText(
			this.layers.ui,
			shortText(title, layout.mode === 'mobile' ? 58 : 72),
			page.x + inset,
			page.y + (compactPage ? 12 : 42),
			adjustedTitleSize,
			{
				fill: INK,
				wordWrap: true,
				wordWrapWidth: page.width - inset * 1.4,
			},
		);
		if (person) {
			const portraitSize = compactPage
				? 36
				: layout.mode === 'mobile'
					? 66
					: 112;
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.portraits[0], {
					x: page.x + inset + 6,
					y: page.y + (compactPage ? 38 : layout.mode === 'mobile' ? 82 : 108),
					width: portraitSize,
					height: portraitSize,
				}),
			);
			this.addText(
				this.layers.ui,
				person.mood,
				page.x + page.width * 0.56,
				page.y + (compactPage ? 40 : layout.mode === 'mobile' ? 96 : 128),
				compactPage ? 11 : layout.mode === 'mobile' ? 15 : 20,
				{
					fill: INK_RED,
				},
			);
			this.addText(
				this.layers.ui,
				shortText(person.detail, compactPage ? 32 : person.detail.length),
				page.x + page.width * 0.55,
				page.y + (compactPage ? 60 : layout.mode === 'mobile' ? 122 : 158),
				compactPage ? 9 : layout.mode === 'mobile' ? 11 : 15,
				{
					fill: INK_SOFT,
					wordWrap: true,
					wordWrapWidth: page.width * 0.34,
				},
			);
			if (compactPage) return;
			this.addText(
				this.layers.ui,
				'Recent exchange',
				page.x + inset,
				page.y + (layout.mode === 'mobile' ? 166 : 245),
				layout.mode === 'mobile' ? 12 : 18,
				{
					fill: INK,
					fontStyle: 'normal',
				},
			);
			const recentLine = person.recentLines.at(-1);
			const recentReactionSummary = recentLine
				? notebookReactionSummary(recentLine.reactions)
				: '';
			this.addText(
				this.layers.ui,
				recentLine
					? `${shortText(
							`${recentLine.content}${recentReactionSummary ? ` · ${recentReactionSummary}` : ''}`,
							layout.mode === 'mobile' ? 42 : 56,
						)}${recentLine.streaming ? ' …' : ''}`
					: person.emptyNote,
				page.x + inset,
				page.y + (layout.mode === 'mobile' ? 190 : 276),
				layout.mode === 'mobile' ? 11 : 14,
				{
					fill: recentLine ? INK : INK_SOFT,
					wordWrap: true,
					wordWrapWidth: page.width - inset * 1.55,
				},
			);
			if (layout.mode === 'desktop') {
				const placeY = page.y + page.height - 112;
				this.addText(this.layers.ui, 'Here', page.x + inset, placeY, 16, {
					fill: INK,
					fontStyle: 'normal',
				});
				this.addText(
					this.layers.ui,
					shortText(state.view.locationName, 44),
					page.x + inset,
					placeY + 25,
					14,
					{
						fill: INK,
						wordWrap: true,
						wordWrapWidth: page.width - inset * 1.7,
					},
				);
				this.addText(
					this.layers.ui,
					shortText(state.view.locationDescription, 92),
					page.x + inset,
					placeY + 47,
					12,
					{
						fill: INK_SOFT,
						wordWrap: true,
						wordWrapWidth: page.width - inset * 1.6,
					},
				);
			}
		} else {
			this.addText(
				this.layers.ui,
				compactPage
					? shortText(state.view.locationDescription, 72)
					: state.view.locationDescription,
				page.x + inset,
				page.y + (compactPage ? 38 : 100),
				compactPage ? 10 : layout.mode === 'mobile' ? 12 : 16,
				{
					fill: INK,
					wordWrap: true,
					wordWrapWidth: page.width - inset * 1.5,
				},
			);
		}
	}

	private drawLiveChronicle(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		const mobile = layout.mode === 'mobile';
		const {
			x: panelX,
			y: panelY,
			width: panelWidth,
			height: panelHeight,
		} = layout.liveChronicle;
		const panel = new Graphics();
		panel
			.roundRect(panelX, panelY, panelWidth, panelHeight, 12)
			.fill({ color: PAPER_LIGHT, alpha: 0.88 })
			.stroke({ color: INK, width: 1.5, alpha: 0.52 });
		this.layers.ui.addChild(panel);
		this.addText(
			this.layers.ui,
			state.view.liveTitle,
			panelX + 18,
			panelY + 12,
			mobile ? 13 : 16,
			{ fill: INK, fontStyle: 'normal', fontWeight: '600' },
		);

		const mobileHeightLimit =
			panelHeight >= 150 ? 3 : panelHeight >= 76 ? 2 : 1;
		const mobileWidthLimit = panelWidth >= 340 ? 3 : panelWidth >= 160 ? 2 : 1;
		const mobileLineLimit = Math.min(mobileHeightLimit, mobileWidthLimit);
		const lines = selectVisibleNotebookLines(
			state.view.liveLines,
			mobile ? mobileLineLimit : 4,
		);
		this.host.dataset.visibleLiveLineKeys = lines
			.map((line) => line.key)
			.join(',');
		this.host.dataset.visibleLiveLineKinds = lines
			.map((line) => line.kind)
			.join(',');
		if (lines.length === 0) {
			this.addText(
				this.layers.ui,
				state.view.liveEmpty,
				panelX + 18,
				panelY + 45,
				mobile ? 12 : 14,
				{
					fill: INK_SOFT,
					wordWrap: true,
					wordWrapWidth: panelWidth - 36,
				},
			);
			return;
		}

		const contentTop = panelY + 42;
		const lineHeight = (panelHeight - 50) / lines.length;
		lines.forEach((line, index) => {
			const reactionSummary = notebookReactionSummary(line.reactions);
			this.addText(
				this.layers.ui,
				`${line.speaker}: ${shortText(
					`${line.content}${reactionSummary ? ` · ${reactionSummary}` : ''}`,
					mobile ? 78 : 158,
				)}${line.streaming ? ' …' : ''}`,
				panelX + 18,
				contentTop + index * lineHeight,
				mobile ? 11 : 13,
				{
					fill:
						line.kind === 'error'
							? INK_RED
							: line.kind === 'npc'
								? INK
								: INK_SOFT,
					fontStyle: line.kind === 'player' ? 'italic' : 'normal',
					fontWeight: line.kind === 'command' ? '600' : 'normal',
					wordWrap: true,
					wordWrapWidth: panelWidth - 36,
				},
			);
		});
	}

	private drawActionStamps(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		NOTEBOOK_ACTIONS.forEach((action, i) => {
			const rect = layout.actionStamps[i];
			const target = this.target(
				`action:${action}`,
				'action-stamp',
				`${ACTION_LABELS[action]} action stamp`,
				rect,
				{ type: 'action', action },
				400 + i,
			);
			this.drawTargetTreatment(target);
			const group = new Container();
			group.x = rect.x;
			group.y = rect.y;
			this.bindTarget(
				group,
				target,
				new Rectangle(0, 0, rect.width, rect.height),
			);
			this.layers.ui.addChild(group);
			const frame = this.sprite(
				NOTEBOOK_ASSETS.actionStampFrames[
					i % NOTEBOOK_ASSETS.actionStampFrames.length
				],
				{
					x: 0,
					y: 0,
					width: rect.width,
					height: rect.height,
				},
			);
			this.bindTarget(frame, target);
			group.addChild(frame);
			const iconSize = rect.width * 0.38;
			group.addChild(
				this.sprite(NOTEBOOK_ASSETS.actionIcons[action], {
					x: rect.width / 2 - iconSize / 2,
					y: rect.height * 0.17,
					width: iconSize,
					height: iconSize,
				}),
			);
			const label = this.addText(
				group,
				ACTION_LABELS[action],
				rect.width * 0.5,
				rect.height * 0.66,
				layout.mode === 'mobile' ? 10 : 14,
				{
					fill: INK,
					align: 'center',
				},
			);
			label.anchor.set(0.5, 0);
			if (state.busy) group.alpha = 0.72;
		});
	}

	private drawIntentStrip(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		this.clear(this.layers.intent);
		const strip = this.sprite(
			NOTEBOOK_ASSETS.intentParchmentStrip,
			layout.intentStrip,
		);
		const inputTarget = this.target(
			'intent-strip',
			'intent-strip',
			'Focus handwritten intent line',
			layout.intentStrip,
			{ type: 'focus-input' },
			500,
		);
		this.drawTargetTreatment(inputTarget);
		this.bindTarget(strip, inputTarget);
		this.layers.intent.addChild(strip);
		this.addText(
			this.layers.intent,
			'Intent',
			layout.intentStrip.x + layout.intentStrip.width * 0.06,
			layout.intentStrip.y + layout.intentStrip.height * 0.34,
			layout.mode === 'mobile' ? 16 : 21,
			{
				fill: INK,
			},
		);
		const lineX = layout.intentStrip.x + layout.intentStrip.width * 0.22;
		const lineY = layout.intentStrip.y + layout.intentStrip.height * 0.43;
		const lineW = layout.intentStrip.width * 0.58;
		this.layers.intent.addChild(
			this.sprite(NOTEBOOK_ASSETS.handwrittenInputLine, {
				x: lineX,
				y: lineY + 13,
				width: lineW,
				height: layout.mode === 'mobile' ? 28 : 34,
			}),
		);
		const displayText = state.intentText || state.view.intentPlaceholder;
		const inputText = this.addText(
			this.layers.intent,
			displayText,
			lineX + 14,
			lineY,
			layout.mode === 'mobile' ? 15 : 20,
			{
				fill: state.intentText ? INK : INK_SOFT,
				wordWrap: true,
				wordWrapWidth: lineW - 24,
			},
		);
		if (state.inputFocused && !state.busy) {
			const caret = new Graphics();
			const typedWidth = state.intentText ? inputText.width : 0;
			const caretX = Math.min(
				lineX + lineW - 18,
				lineX + 16 + Math.min(typedWidth, lineW - 34),
			);
			caret
				.rect(caretX, lineY + 2, 2, layout.mode === 'mobile' ? 19 : 25)
				.fill({ color: INK, alpha: 0.75 });
			this.layers.intent.addChild(caret);
		}
		const sendSize = layout.mode === 'mobile' ? 52 : 68;
		const send = this.sprite(NOTEBOOK_ASSETS.inkStampSend, {
			x: layout.intentStrip.x + layout.intentStrip.width - sendSize - 34,
			y: layout.intentStrip.y + layout.intentStrip.height / 2 - sendSize / 2,
			width: sendSize,
			height: sendSize,
		});
		const sendDisabled = state.busy || !state.intentText.trim();
		const sendTarget = this.target(
			'send',
			'send',
			'Send intent',
			{
				x: send.x,
				y: send.y,
				width: send.width,
				height: send.height,
			},
			{ type: 'send' },
			510,
			sendDisabled,
		);
		send.alpha = sendDisabled ? 0.48 : 1;
		this.drawTargetTreatment(sendTarget);
		this.bindTarget(send, sendTarget);
		this.layers.intent.addChild(send);
	}

	private drawLowerCards(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		if (layout.mapCard) {
			const target = this.target(
				'map-card',
				'map-card',
				'Open parish map',
				layout.mapCard,
				{ type: 'open-map' },
				600,
			);
			this.drawTargetTreatment(target);
			const map = this.sprite(NOTEBOOK_ASSETS.mapCard, layout.mapCard);
			this.bindTarget(map, target);
			this.layers.ui.addChild(map);
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.mapIcon, {
					x: layout.mapCard.x + 31,
					y: layout.mapCard.y + 27,
					width: 42,
					height: 42,
				}),
			);
			this.addText(
				this.layers.ui,
				'Map',
				layout.mapCard.x + 34,
				layout.mapCard.y + 73,
				15,
				{ fill: INK },
			);
		}
		if (layout.timeCard) {
			const target = this.target(
				'time-card',
				'time-card',
				'Open time details',
				layout.timeCard,
				{ type: 'open-time' },
				610,
			);
			this.drawTargetTreatment(target);
			const time = this.sprite(NOTEBOOK_ASSETS.timeCard, layout.timeCard);
			this.bindTarget(time, target);
			this.layers.ui.addChild(time);
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.timeIcon, {
					x: layout.timeCard.x + 31,
					y: layout.timeCard.y + 25,
					width: 42,
					height: 42,
				}),
			);
			this.addText(
				this.layers.ui,
				'Time',
				layout.timeCard.x + 30,
				layout.timeCard.y + 73,
				15,
				{ fill: INK },
			);
			this.addText(
				this.layers.ui,
				`x${state.world?.speed_factor ? Math.round(state.world.speed_factor / 36) || 1 : 1}`,
				layout.timeCard.x + 64,
				layout.timeCard.y + 72,
				15,
				{
					fill: INK,
					fontStyle: 'normal',
				},
			);
		}
		if (layout.activeIntentsCard) {
			const card = layout.activeIntentsCard;
			const mobile = layout.mode === 'mobile';
			const target = this.target(
				'active-intents-card',
				'active-intents-card',
				'Open active tasks',
				card,
				{ type: 'open-active-intents' },
				620,
			);
			this.drawTargetTreatment(target);
			const active = this.sprite(NOTEBOOK_ASSETS.activeIntentsCard, card);
			this.bindTarget(active, target);
			this.layers.ui.addChild(active);
			const textX = card.x + (mobile ? 12 : 35);
			const titleY = card.y + (mobile ? card.height * 0.11 : 20);
			this.addText(
				this.layers.ui,
				'Active Task',
				textX,
				titleY,
				mobile ? 11 : 16,
				{ fill: INK },
			);
			const stampSize = mobile ? Math.min(32, card.height * 0.58) : 42;
			const taskTextLength = mobile
				? Math.max(12, Math.floor((card.width - stampSize - 26) / 5.4))
				: 34;
			this.addText(
				this.layers.ui,
				shortText(
					state.view.currentTask?.description ?? 'No active task',
					taskTextLength,
				),
				card.x + (mobile ? 12 : 42),
				card.y + (mobile ? card.height * 0.39 : 52),
				mobile ? 10 : 13,
				{ fill: INK },
			);
			if (state.view.currentTask) {
				this.addText(
					this.layers.ui,
					state.view.currentTask.statusLabel,
					card.x + (mobile ? 12 : 42),
					card.y + (mobile ? card.height * 0.68 : 72),
					mobile ? 9 : 11,
					{ fill: INK_SOFT, fontStyle: 'normal' },
				);
			}
			const stamp = this.sprite(NOTEBOOK_ASSETS.inkStampSend, {
				x: card.x + card.width - stampSize - (mobile ? 7 : 26),
				y: card.y + (card.height - stampSize) / 2,
				width: stampSize,
				height: stampSize,
			});
			this.bindTarget(stamp, target);
			this.layers.ui.addChild(stamp);
		}
	}
}

function titleCase(tab: NotebookTab): string {
	return tab.charAt(0).toUpperCase() + tab.slice(1);
}

function shortText(text: string, max: number): string {
	if (text.length <= max) return text;
	return `${text.slice(0, Math.max(0, max - 1)).trimEnd()}...`;
}

function shortNpcName(name: string, max: number): string {
	const cleaned = name.replace(/\s+/g, ' ').trim();
	if (cleaned.length <= max) return cleaned;
	const words = cleaned.split(' ');
	if (/^(an?|the)$/i.test(words[0]) && words.length >= 3) {
		return shortText(words.slice(0, 3).join(' '), max);
	}
	return shortText(cleaned, max);
}

function targetIdPart(value: string): string {
	return encodeURIComponent(value.trim().toLowerCase().replace(/\s+/g, '-'));
}
