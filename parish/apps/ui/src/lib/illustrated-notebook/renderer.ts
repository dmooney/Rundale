import 'pixi.js/unsafe-eval';
import {
	Application,
	Assets,
	Container,
	Graphics,
	Sprite,
	Text,
	Texture,
	type FederatedPointerEvent,
	type TextStyleOptions,
} from 'pixi.js';
import type { NotebookAction } from '$lib/notebook/actions';
import { NOTEBOOK_ASSET_URLS, NOTEBOOK_ASSETS } from './assets';
import {
	computeNotebookLayout,
	NOTEBOOK_ACTIONS,
	pointFromAnchor,
	scaleForDepth,
	sortAnchorsByDepth,
} from './layout';
import type {
	NotebookLayout,
	NotebookRect,
	NotebookRenderState,
	NotebookTab,
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

const FALLBACK_SCENE: VisualScene = {
	location_ids: [1, 15],
	plate_asset: NOTEBOOK_ASSETS.scenePlate,
	written_visual_summary:
		'Rural Ireland in 1820 after rain, drawn as a wide elevated oblique illustrated storybook game scene.',
	camera_hint: 'wide elevated oblique illustrated storybook game scene',
	background_generation_source: 'Generated from written description only.',
	depth_bands: [
		{ name: 'far', min_depth: 0, max_depth: 0.35, marker_scale: 0.5 },
		{ name: 'mid', min_depth: 0.35, max_depth: 0.7, marker_scale: 0.72 },
		{ name: 'near', min_depth: 0.7, max_depth: 1, marker_scale: 0.95 },
	],
	anchors: {
		player: { x: 0.48, y: 0.55, depth: 0.72 },
		npcs: [
			{ id: 'nearby-1', x: 0.51, y: 0.55, depth: 0.72 },
			{ id: 'nearby-2', x: 0.43, y: 0.48, depth: 0.58 },
			{ id: 'nearby-3', x: 0.68, y: 0.58, depth: 0.66 },
			{ id: 'nearby-4', x: 0.33, y: 0.69, depth: 0.82 },
		],
		exits: [
			{ id: 'chapel', label: 'Chapel Lane', x: 0.16, y: 0.15, depth: 0.18 },
			{ id: 'shop', label: 'Shop Road', x: 0.68, y: 0.43, depth: 0.46 },
			{ id: 'bridge', label: 'Bridge', x: 0.77, y: 0.58, depth: 0.64 },
		],
	},
};

export class IllustratedNotebookRenderer {
	private app: Application | null = null;
	private readonly textures = new Map<string, Texture>();
	private scene: VisualScene = FALLBACK_SCENE;
	private lastState: NotebookRenderState | null = null;
	private readonly layers = {
		background: new Container(),
		wash: new Container(),
		exits: new Container(),
		markers: new Container(),
		ui: new Container(),
	};

	constructor(private readonly host: HTMLElement) {}

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
		);
		await this.loadAssets();
	}

	destroy(): void {
		this.clearAll();
		this.app?.destroy(true);
		this.app = null;
	}

	render(state: NotebookRenderState): void {
		this.lastState = state;
		if (!this.app) return;
		const width = Math.max(1, this.host.clientWidth || this.app.renderer.width);
		const height = Math.max(
			1,
			this.host.clientHeight || this.app.renderer.height,
		);
		const layout = computeNotebookLayout(width, height);
		this.clearAll();
		this.drawBackground(width, height);
		this.drawSceneWash(width, height);
		this.drawExitLabels(layout, state);
		this.drawMarkers(layout, state);
		this.drawTopRibbon(layout, state);
		this.drawNearby(layout, state);
		this.drawNotebookPage(layout, state);
		this.drawActionStamps(layout, state);
		this.drawIntentStrip(layout, state);
		this.drawLowerCards(layout, state);
	}

	resize(): void {
		if (this.lastState) this.render(this.lastState);
	}

	private async loadAssets(): Promise<void> {
		const loaded = await Assets.load(NOTEBOOK_ASSET_URLS);
		for (const url of NOTEBOOK_ASSET_URLS) {
			this.textures.set(url, loaded[url] ?? Texture.from(url));
		}
		try {
			const response = await fetch(NOTEBOOK_ASSETS.visualScenes);
			if (response.ok) {
				const file = (await response.json()) as VisualScenesFile;
				this.scene = file.scenes[0] ?? FALLBACK_SCENE;
			}
		} catch {
			this.scene = FALLBACK_SCENE;
		}
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

	private makeButton<T extends Container | Sprite | Graphics>(
		display: T,
		callback: (event: FederatedPointerEvent) => void,
	): T {
		display.eventMode = 'static';
		display.cursor = 'pointer';
		display.on('pointertap', callback);
		return display;
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
		const sprite = this.sprite(NOTEBOOK_ASSETS.scenePlate);
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
			{ kind: 'player' as const, anchor: this.scene.anchors.player, npc: null },
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
			const markerUrl = isPlayer
				? NOTEBOOK_ASSETS.playerMarker
				: NOTEBOOK_ASSETS.npcMarkers[
						state.npcs.indexOf(actor.actor.npc!) %
							NOTEBOOK_ASSETS.npcMarkers.length
					];
			const marker = this.sprite(markerUrl);
			marker.anchor.set(0.5, 1);
			marker.x = p.x;
			marker.y = p.y + 10;
			marker.scale.set(scale);
			this.layers.markers.addChild(marker);
			if (actor.actor.npc) {
				this.makeButton(marker, () =>
					state.callbacks.onSelectNpc(actor.actor.npc!.real_name),
				);
			}
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
					shortNpcName(actor.actor.npc.name, 24),
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
		const world = state.world;
		const location = world?.location_name ?? 'Rundale';
		const weather = world?.weather ?? 'weather turning';
		const time = world
			? `${String(world.hour).padStart(2, '0')}:${String(world.minute).padStart(2, '0')}`
			: '3:40 PM';
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
			const itemHeight = (layout.nearbyStrip.height - 70) / 4.6;
			state.npcs.slice(0, 4).forEach((npc, i) => {
				this.drawNearbyPerson(
					layout,
					state,
					npc,
					layout.nearbyStrip.x + 26,
					layout.nearbyStrip.y + 58 + i * itemHeight,
					82,
					i,
				);
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
			state.npcs.slice(0, 4).forEach((npc, i) => {
				this.drawNearbyPerson(
					layout,
					state,
					npc,
					layout.nearbyStrip.x + 58 + i * 78,
					layout.nearbyStrip.y + 18,
					58,
					i,
				);
			});
		}
	}

	private drawNearbyPerson(
		layout: NotebookLayout,
		state: NotebookRenderState,
		npc: NonNullable<NotebookRenderState['selectedNpc']>,
		x: number,
		y: number,
		size: number,
		index: number,
	): void {
		const selected = npc.real_name === state.selectedRealName;
		const frame = this.sprite(NOTEBOOK_ASSETS.nearbyPortraitCardFrame, {
			x: x - size * 0.55,
			y: y - size * 0.25,
			width: size * 1.12,
			height: size * 1.18,
		});
		frame.alpha = selected ? 1 : 0.86;
		this.makeButton(frame, () => state.callbacks.onSelectNpc(npc.real_name));
		this.layers.ui.addChild(frame);
		const portrait = this.sprite(
			NOTEBOOK_ASSETS.portraits[index % NOTEBOOK_ASSETS.portraits.length],
			{
				x: x - size * 0.34,
				y,
				width: size * 0.68,
				height: size * 0.72,
			},
		);
		this.makeButton(portrait, () => state.callbacks.onSelectNpc(npc.real_name));
		this.layers.ui.addChild(portrait);
		if (layout.mode === 'desktop') {
			this.addText(
				this.layers.ui,
				shortNpcName(npc.name, 20),
				x - size * 0.55,
				y + size * 0.73,
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
			const tab = this.sprite(NOTEBOOK_ASSETS.sideTabs[i], tabRect);
			this.makeButton(tab, () => state.callbacks.onOpenTab(TAB_LABELS[i]));
			this.layers.ui.addChild(tab);
			if (layout.mode === 'desktop') {
				this.addText(
					this.layers.ui,
					titleCase(TAB_LABELS[i]),
					tabRect.x + 18,
					tabRect.y + 19,
					13,
					{ fill: INK },
				);
			}
		});
		const npc = state.selectedNpc;
		const page = layout.notebookPage;
		const inset = layout.mode === 'mobile' ? 18 : 46;
		const titleSize = layout.mode === 'mobile' ? 16 : 25;
		const title = npc?.name ?? pageTitle(state);
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
			page.y + 42,
			adjustedTitleSize,
			{
				fill: INK,
				wordWrap: true,
				wordWrapWidth: page.width - inset * 1.4,
			},
		);
		if (npc) {
			const portraitSize = layout.mode === 'mobile' ? 66 : 112;
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.portraits[0], {
					x: page.x + inset + 6,
					y: page.y + (layout.mode === 'mobile' ? 82 : 108),
					width: portraitSize,
					height: portraitSize,
				}),
			);
			this.addText(
				this.layers.ui,
				npc.mood || 'watchful',
				page.x + page.width * 0.56,
				page.y + (layout.mode === 'mobile' ? 96 : 128),
				layout.mode === 'mobile' ? 15 : 20,
				{
					fill: INK_RED,
				},
			);
			this.addText(
				this.layers.ui,
				npc.occupation || 'parish neighbour',
				page.x + page.width * 0.55,
				page.y + (layout.mode === 'mobile' ? 122 : 158),
				layout.mode === 'mobile' ? 11 : 15,
				{
					fill: INK_SOFT,
					wordWrap: true,
					wordWrapWidth: page.width * 0.34,
				},
			);
			this.addText(
				this.layers.ui,
				'Trust',
				page.x + inset,
				page.y + (layout.mode === 'mobile' ? 166 : 245),
				layout.mode === 'mobile' ? 13 : 18,
				{
					fill: INK,
					fontStyle: 'normal',
				},
			);
			this.drawTrustDots(
				page.x + inset + (layout.mode === 'mobile' ? 52 : 76),
				page.y + (layout.mode === 'mobile' ? 176 : 257),
				layout.mode === 'mobile' ? 5 : 7,
			);
			if (layout.mode === 'desktop') {
				this.addText(
					this.layers.ui,
					'She knows',
					page.x + inset,
					page.y + 300,
					19,
					{ fill: INK },
				);
				this.addText(
					this.layers.ui,
					'- cart delayed\n- flour is short\n- saw who crossed the bridge',
					page.x + inset + 8,
					page.y + 337,
					16,
					{
						fill: INK,
						wordWrap: true,
						wordWrapWidth: page.width - inset * 1.7,
					},
				);
				this.addText(
					this.layers.ui,
					'Witness notes: watching the road.',
					page.x + inset,
					page.y + page.height - 78,
					14,
					{
						fill: INK_SOFT,
						wordWrap: true,
						wordWrapWidth: page.width - inset * 1.6,
					},
				);
			} else {
				this.addText(
					this.layers.ui,
					'Knows: cart delayed',
					page.x + inset,
					page.y + 202,
					11,
					{
						fill: INK,
						wordWrap: true,
						wordWrapWidth: page.width - inset * 1.5,
					},
				);
			}
		} else {
			this.addText(
				this.layers.ui,
				state.world?.location_description ??
					'The parish waits for your next line.',
				page.x + inset,
				page.y + 100,
				layout.mode === 'mobile' ? 12 : 16,
				{
					fill: INK,
					wordWrap: true,
					wordWrapWidth: page.width - inset * 1.5,
				},
			);
		}
	}

	private drawTrustDots(x: number, y: number, radius: number): void {
		const g = new Graphics();
		for (let i = 0; i < 5; i += 1) {
			g.circle(x + i * radius * 2.5, y, radius).stroke({
				color: INK,
				width: 1.4,
				alpha: 0.8,
			});
			if (i < 2)
				g.circle(x + i * radius * 2.5, y, radius - 1).fill({
					color: 0x8b9560,
					alpha: 0.9,
				});
		}
		this.layers.ui.addChild(g);
	}

	private drawActionStamps(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		NOTEBOOK_ACTIONS.forEach((action, i) => {
			const rect = layout.actionStamps[i];
			const group = new Container();
			group.x = rect.x;
			group.y = rect.y;
			this.makeButton(group, () => state.callbacks.onAction(action));
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
		const strip = this.sprite(
			NOTEBOOK_ASSETS.intentParchmentStrip,
			layout.intentStrip,
		);
		this.makeButton(strip, () => state.callbacks.onFocusInput());
		this.layers.ui.addChild(strip);
		this.addText(
			this.layers.ui,
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
		this.layers.ui.addChild(
			this.sprite(NOTEBOOK_ASSETS.handwrittenInputLine, {
				x: lineX,
				y: lineY + 13,
				width: lineW,
				height: layout.mode === 'mobile' ? 28 : 34,
			}),
		);
		const displayText =
			state.intentText ||
			(state.busy ? 'waiting on the parish...' : 'ask Roisin what she saw');
		this.addText(
			this.layers.ui,
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
			const caretX = Math.min(
				lineX + lineW - 18,
				lineX +
					18 +
					state.intentText.length * (layout.mode === 'mobile' ? 7.2 : 9.2),
			);
			caret
				.rect(caretX, lineY + 2, 2, layout.mode === 'mobile' ? 19 : 25)
				.fill({ color: INK, alpha: 0.75 });
			this.layers.ui.addChild(caret);
		}
		const sendSize = layout.mode === 'mobile' ? 52 : 68;
		const send = this.sprite(NOTEBOOK_ASSETS.inkStampSend, {
			x: layout.intentStrip.x + layout.intentStrip.width - sendSize - 34,
			y: layout.intentStrip.y + layout.intentStrip.height / 2 - sendSize / 2,
			width: sendSize,
			height: sendSize,
		});
		send.alpha = state.busy || !state.intentText.trim() ? 0.48 : 1;
		this.makeButton(send, () => state.callbacks.onSend());
		this.layers.ui.addChild(send);
	}

	private drawLowerCards(
		layout: NotebookLayout,
		state: NotebookRenderState,
	): void {
		if (layout.mapCard) {
			const map = this.sprite(NOTEBOOK_ASSETS.mapCard, layout.mapCard);
			this.makeButton(map, () => state.callbacks.onOpenMap());
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
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.timeCard, layout.timeCard),
			);
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
			this.layers.ui.addChild(
				this.sprite(
					NOTEBOOK_ASSETS.activeIntentsCard,
					layout.activeIntentsCard,
				),
			);
			this.addText(
				this.layers.ui,
				'Active Intents',
				layout.activeIntentsCard.x + 35,
				layout.activeIntentsCard.y + 20,
				16,
				{ fill: INK },
			);
			this.addText(
				this.layers.ui,
				'(none)',
				layout.activeIntentsCard.x + 42,
				layout.activeIntentsCard.y + 52,
				13,
				{ fill: INK },
			);
			this.layers.ui.addChild(
				this.sprite(NOTEBOOK_ASSETS.inkStampSend, {
					x: layout.activeIntentsCard.x + layout.activeIntentsCard.width - 68,
					y: layout.activeIntentsCard.y + 35,
					width: 42,
					height: 42,
				}),
			);
		}
	}
}

function titleCase(tab: NotebookTab): string {
	return tab.charAt(0).toUpperCase() + tab.slice(1);
}

function pageTitle(state: NotebookRenderState): string {
	return state.world?.location_name ?? 'Parish Notes';
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
