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
import { PARISH_ASSETS, PARISH_ASSET_URLS, PARISH_PLATE_SIZES } from './assets';
import { activateParishTarget } from './interactions';
import {
	computeParishLayout,
	mapPlatePointToViewport,
	PARISH_ACTIONS,
} from './layout';
import { parishProfilePlaceholder } from './profile';
import type {
	NotebookSurface,
	ParishHitTarget,
	ParishLayout,
	ParishRect,
	ParishRenderState,
	ParishTab,
	ParishTargetActivation,
	ParishTargetKind,
} from './types';

const INK = 0x35362f;
const INK_SOFT = 0x5b5545;
const PAPER = 0xdfceb0;
const PAPER_LIGHT = 0xeadbbc;
const PAPER_SHADOW = 0x5a4935;
const MOOD_RED = 0xa1543a;
const TRUST_OLIVE = 0x84954e;
const WASH_GRAY = 0x72736d;
const HAND_FONT = 'Kalam, "Bradley Hand", "Segoe Print", cursive';
const BOOKEND_PAPER_OPTIONS = {
	pale: true,
	alpha: 0.93,
	shadow: true,
} as const;

const TAB_LABELS: ParishTab[] = [
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

const SURFACE_LABELS: Record<NotebookSurface, string> = {
	journal: 'Journal',
	people: 'People',
	focail: 'Focail',
	map: 'Map',
	save: 'Save / Load',
	debug: 'Debug',
	mod: 'Mod',
	bug: 'Bug Report',
	shortcuts: 'Shortcuts',
	utility: 'Notebook tools',
	time: 'Time and weather',
	intents: 'Active intents',
	rumours: 'Rumours',
};

export interface IllustratedParishRendererOptions {
	onHitTargetsChanged?: (targets: ParishHitTarget[]) => void;
}

export class IllustratedParishRenderer {
	private app: Application | null = null;
	private readonly textures = new Map<string, Texture>();
	private lastState: ParishRenderState | null = null;
	private hitTargets: ParishHitTarget[] = [];
	private hoveredTargetId: string | null = null;
	private focusedTargetId: string | null = null;
	private hoverFrame: number | null = null;
	private readonly layers = {
		scene: new Container(),
		sceneInk: new Container(),
		chrome: new Container(),
		intent: new Container(),
		treatment: new Container(),
	};

	constructor(
		private readonly host: HTMLElement,
		private readonly options: IllustratedParishRendererOptions = {},
	) {}

	async init(): Promise<void> {
		const app = new Application();
		await app.init({
			resizeTo: this.host,
			backgroundColor: 0x302b22,
			antialias: true,
			// The player-triggered screenshot path and visual proof both read the
			// presented canvas after composition. Pixi/WebGL defaults this to false,
			// which permits the browser to clear texture-backed regions before that
			// later read and produces intermittent black captures.
			preserveDrawingBuffer: true,
			autoDensity: true,
			resolution: Math.min(window.devicePixelRatio || 1, 2),
		});
		this.app = app;
		this.host.appendChild(app.canvas);
		app.stage.addChild(
			this.layers.scene,
			this.layers.sceneInk,
			this.layers.chrome,
			this.layers.intent,
			this.layers.treatment,
		);
		await Promise.all([
			this.loadAssets(),
			document.fonts?.load(`20px ${HAND_FONT}`) ?? Promise.resolve([]),
		]);
	}

	destroy(): void {
		if (this.hoverFrame !== null) {
			window.cancelAnimationFrame(this.hoverFrame);
			this.hoverFrame = null;
		}
		this.clearAll();
		this.app?.destroy(true);
		this.app = null;
	}

	render(state: ParishRenderState): void {
		if (!this.app) return;
		const width = Math.max(1, this.host.clientWidth || this.app.renderer.width);
		const height = Math.max(
			1,
			this.host.clientHeight || this.app.renderer.height,
		);
		const layout = computeParishLayout(width, height);
		this.lastState = state;
		this.hitTargets = [];
		this.clearAll();
		this.drawScene(layout);
		this.drawSceneInk(layout, state);
		this.drawTop(layout, state);
		this.drawNearby(layout, state);
		this.drawNotebook(layout, state);
		this.drawActions(layout, state);
		this.drawIntent(layout, state);
		this.drawBottomCards(layout, state);
		this.drawTreatments();
		this.emitTargets();
	}

	resize(): void {
		if (this.lastState) this.render(this.lastState);
	}

	setFocusedTarget(id: string | null): void {
		if (this.focusedTargetId === id) return;
		this.focusedTargetId = id;
		this.drawTreatments();
	}

	activateTarget(id: string): boolean {
		if (!this.lastState) return false;
		return activateParishTarget(
			this.hitTargets.find((target) => target.id === id),
			this.lastState.callbacks,
		);
	}

	private async loadAssets(): Promise<void> {
		const loaded = await Assets.load(PARISH_ASSET_URLS);
		for (const url of PARISH_ASSET_URLS) {
			this.textures.set(url, loaded[url] ?? Texture.from(url));
		}
	}

	private clearAll(): void {
		for (const layer of Object.values(this.layers)) this.clear(layer);
	}

	private clear(layer: Container): void {
		for (const child of layer.removeChildren()) {
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

	private sprite(url: string): Sprite {
		return new Sprite(this.texture(url));
	}

	private place(sprite: Sprite, rect: ParishRect): Sprite {
		sprite.x = rect.x;
		sprite.y = rect.y;
		sprite.width = rect.width;
		sprite.height = rect.height;
		return sprite;
	}

	private contain(
		layer: Container,
		url: string,
		rect: ParishRect,
		padding = 0,
	): Sprite {
		const sprite = this.sprite(url);
		const availableWidth = Math.max(1, rect.width - padding * 2);
		const availableHeight = Math.max(1, rect.height - padding * 2);
		const scale = Math.min(
			availableWidth / Math.max(1, sprite.texture.width),
			availableHeight / Math.max(1, sprite.texture.height),
		);
		sprite.width = sprite.texture.width * scale;
		sprite.height = sprite.texture.height * scale;
		sprite.x = rect.x + (rect.width - sprite.width) / 2;
		sprite.y = rect.y + (rect.height - sprite.height) / 2;
		layer.addChild(sprite);
		return sprite;
	}

	private text(
		layer: Container,
		value: string,
		x: number,
		y: number,
		size: number,
		options: Partial<TextStyleOptions> = {},
	): Text {
		const display = new Text({
			text: value,
			style: {
				fontFamily: HAND_FONT,
				fontSize: size,
				fill: INK,
				fontWeight: '400',
				lineHeight: size * 1.14,
				...options,
			},
		});
		display.x = x;
		display.y = y;
		layer.addChild(display);
		return display;
	}

	private paper(
		layer: Container,
		rect: ParishRect,
		options: { alpha?: number; shadow?: boolean; pale?: boolean } = {},
	): Graphics {
		const { x, y, width, height } = rect;
		const inset = Math.min(5, width * 0.025, height * 0.08);
		const points = [
			x + inset,
			y,
			x + width * 0.34,
			y + 1.5,
			x + width - inset * 0.45,
			y,
			x + width,
			y + inset,
			x + width - 1.2,
			y + height * 0.55,
			x + width,
			y + height - inset,
			x + width - inset,
			y + height,
			x + width * 0.58,
			y + height - 1.1,
			x + inset,
			y + height,
			x,
			y + height - inset,
			x + 1.1,
			y + height * 0.39,
			x,
			y + inset,
		];
		if (options.shadow !== false) {
			const shadow = new Graphics();
			shadow
				.poly(points.map((value, index) => value + (index % 2 === 0 ? 3 : 4)))
				.fill({ color: PAPER_SHADOW, alpha: 0.28 });
			layer.addChild(shadow);
		}
		const paper = new Graphics();
		paper
			.poly(points)
			.fill({
				color: options.pale ? PAPER_LIGHT : PAPER,
				alpha: options.alpha ?? 0.93,
			})
			.stroke({ color: INK, width: 1.2, alpha: 0.72 });
		layer.addChild(paper);
		return paper;
	}

	private inkLine(
		layer: Container,
		x1: number,
		y1: number,
		x2: number,
		y2: number,
		alpha = 0.65,
	): void {
		const line = new Graphics();
		line
			.moveTo(x1, y1)
			.quadraticCurveTo((x1 + x2) / 2, (y1 + y2) / 2 + 0.8, x2, y2)
			.stroke({ color: INK, width: 1.1, alpha });
		layer.addChild(line);
	}

	private target(
		id: string,
		kind: ParishTargetKind,
		label: string,
		rect: ParishRect,
		activation: ParishTargetActivation,
		order: number,
		disabled = false,
	): ParishHitTarget {
		return { id, kind, label, rect: { ...rect }, activation, order, disabled };
	}

	private bind<T extends Container | Sprite | Graphics>(
		display: T,
		target: ParishHitTarget,
	): T {
		if (!this.hitTargets.some((existing) => existing.id === target.id)) {
			this.hitTargets.push(target);
		}
		display.eventMode = 'static';
		display.cursor = target.disabled ? 'default' : 'pointer';
		const bounds = display.getLocalBounds();
		display.hitArea = new Rectangle(
			bounds.x,
			bounds.y,
			bounds.width,
			bounds.height,
		);
		display.on('pointerdown', () => this.activateTarget(target.id));
		display.on('pointerover', () => this.setHoveredTarget(target.id));
		display.on('pointerout', () => {
			if (this.hoveredTargetId === target.id) this.setHoveredTarget(null);
		});
		return display;
	}

	private setHoveredTarget(id: string | null): void {
		if (this.hoveredTargetId === id) return;
		this.hoveredTargetId = id;
		if (this.hoverFrame !== null) window.cancelAnimationFrame(this.hoverFrame);
		this.hoverFrame = window.requestAnimationFrame(() => {
			this.hoverFrame = null;
			this.drawTreatments();
		});
	}

	private emitTargets(): void {
		this.options.onHitTargetsChanged?.(
			this.hitTargets.map((target) => ({
				...target,
				rect: { ...target.rect },
				activation: { ...target.activation },
			})),
		);
	}

	private drawTreatments(): void {
		this.clear(this.layers.treatment);
		for (const target of this.hitTargets) {
			const focused = target.id === this.focusedTargetId;
			const hovered = target.id === this.hoveredTargetId;
			if (!focused && !hovered) continue;
			const pad = focused ? 4 : 2;
			const g = new Graphics();
			g.roundRect(
				target.rect.x - pad,
				target.rect.y - pad,
				target.rect.width + pad * 2,
				target.rect.height + pad * 2,
				5,
			).stroke({
				color: focused ? MOOD_RED : INK,
				width: focused ? 2 : 1.2,
				alpha: focused ? 0.9 : 0.62,
			});
			this.layers.treatment.addChild(g);
		}
	}

	private drawScene(layout: ParishLayout): void {
		const url =
			layout.mode === 'mobile'
				? PARISH_ASSETS.sceneMobile
				: PARISH_ASSETS.sceneDesktop;
		const scene = this.sprite(url);
		const scale = Math.max(
			layout.width / Math.max(1, scene.texture.width),
			layout.height / Math.max(1, scene.texture.height),
		);
		scene.width = scene.texture.width * scale;
		scene.height = scene.texture.height * scale;
		scene.x = (layout.width - scene.width) / 2;
		scene.y = (layout.height - scene.height) / 2;
		this.layers.scene.addChild(scene);

		const wash = new Graphics();
		wash
			.rect(0, 0, layout.width, layout.height)
			.fill({ color: WASH_GRAY, alpha: 0.035 });
		wash
			.rect(0, 0, layout.width, layout.height * 0.085)
			.fill({ color: PAPER_LIGHT, alpha: 0.09 });
		this.layers.scene.addChild(wash);
	}

	private drawSceneInk(layout: ParishLayout, state: ParishRenderState): void {
		if (layout.mode === 'desktop') {
			for (const exit of layout.exitLabels) this.drawExitLabel(exit);
		}

		const anchors =
			layout.mode === 'mobile'
				? [
						{ x: 0.443, y: 0.437 },
						{ x: 0.457, y: 0.378 },
						{ x: 0.707, y: 0.542 },
						{ x: 0.487, y: 0.657 },
					]
				: [
						{ x: 0.491, y: 0.57 },
						{ x: 0.585, y: 0.391 },
						{ x: 0.664, y: 0.58 },
						{ x: 0.505, y: 0.727 },
					];
		const plateSize =
			layout.mode === 'mobile'
				? PARISH_PLATE_SIZES.mobile
				: PARISH_PLATE_SIZES.desktop;
		const markerScale = layout.mode === 'mobile' ? 0.82 : 1;
		state.npcs.slice(0, anchors.length).forEach((npc, index) => {
			const { x, y } = mapPlatePointToViewport(
				layout.width,
				layout.height,
				plateSize.width,
				plateSize.height,
				anchors[index],
			);
			const selected = npc.real_name === state.selectedRealName;
			const targetRect = {
				x: x - 24 * markerScale,
				y: y - 30 * markerScale,
				width: 48 * markerScale,
				height: 54 * markerScale,
			};
			const hit = new Graphics();
			hit
				.rect(targetRect.x, targetRect.y, targetRect.width, targetRect.height)
				.fill({ color: 0xffffff, alpha: 0.001 });
			this.layers.sceneInk.addChild(hit);
			this.bind(
				hit,
				this.target(
					`scene-person:${safeId(npc.real_name)}`,
					'scene-person',
					`Select ${npc.name} in the parish scene`,
					targetRect,
					{ type: 'select-npc', realName: npc.real_name },
					120 + index,
				),
			);
			this.drawEye(this.layers.sceneInk, x + 18, y - 26, 0.7 * markerScale);
			if (selected) {
				const selection = new Graphics();
				selection
					.ellipse(x, y + 12, 25 * markerScale, 10 * markerScale)
					.stroke({ color: 0xf4ead4, width: 2, alpha: 0.95 });
				this.layers.sceneInk.addChild(selection);
				if (layout.mode === 'desktop') {
					const labelWidth = Math.max(100, npc.name.length * 7.2);
					const label = {
						x: x + 12,
						y: y - 44,
						width: labelWidth,
						height: 27,
					};
					this.paper(this.layers.sceneInk, label, {
						alpha: 0.88,
						shadow: false,
						pale: true,
					});
					this.text(
						this.layers.sceneInk,
						npc.name,
						label.x + 10,
						label.y + 4,
						15,
					);
				}
			}
		});
	}

	private drawExitLabel(exit: ParishRect & { label: string }): void {
		this.layers.sceneInk.addChild(
			this.place(this.sprite(PARISH_ASSETS.label), exit),
		);
		const arrow = new Graphics();
		arrow
			.moveTo(exit.x + 5, exit.y + exit.height / 2)
			.lineTo(exit.x + 12, exit.y + exit.height / 2 - 5)
			.moveTo(exit.x + 5, exit.y + exit.height / 2)
			.lineTo(exit.x + 12, exit.y + exit.height / 2 + 5)
			.stroke({ color: INK, width: 1.2, alpha: 0.75 });
		this.layers.sceneInk.addChild(arrow);
		this.text(
			this.layers.sceneInk,
			exit.label,
			exit.x + 16,
			exit.y + exit.height * 0.18,
			Math.max(12, exit.height * 0.48),
		);
	}

	private drawTop(layout: ParishLayout, state: ParishRenderState): void {
		this.paper(this.layers.chrome, layout.logoCard, BOOKEND_PAPER_OPTIONS);
		if (layout.mode === 'desktop') {
			this.paper(this.layers.chrome, layout.compass, BOOKEND_PAPER_OPTIONS);
		}
		const compactLogo = layout.mode === 'mobile';
		this.text(
			this.layers.chrome,
			compactLogo ? 'RUNDALE' : 'R U N D A L E',
			layout.logoCard.x + layout.logoCard.width * 0.075,
			layout.logoCard.y + layout.logoCard.height * 0.12,
			compactLogo
				? Math.min(18, layout.logoCard.height * 0.34)
				: Math.max(18, Math.min(32, layout.logoCard.height * 0.39)),
			{ letterSpacing: compactLogo ? 0.5 : 2 },
		);
		if (layout.mode === 'desktop') {
			this.text(
				this.layers.chrome,
				'Parish Notebook',
				layout.logoCard.x + layout.logoCard.width * 0.09,
				layout.logoCard.y + layout.logoCard.height * 0.54,
				Math.max(13, layout.logoCard.height * 0.19),
				{ fill: INK_SOFT },
			);
			this.drawChurch(
				this.layers.chrome,
				layout.logoCard.x + layout.logoCard.width * 0.84,
				layout.logoCard.y + layout.logoCard.height * 0.52,
				layout.logoCard.height * 0.34,
			);
		}

		this.layers.chrome.addChild(
			this.place(this.sprite(PARISH_ASSETS.topRibbon), layout.statusRibbon),
		);
		const location = shortText(
			state.world?.location_name ?? 'The Crossroads',
			28,
		);
		const weather =
			[state.world?.time_label, state.world?.weather]
				.filter(Boolean)
				.join(' · ') || 'Afternoon clearing';
		const time = `${String(state.world?.hour ?? 15).padStart(2, '0')}:${String(
			state.world?.minute ?? 40,
		).padStart(2, '0')}`;
		if (layout.mode === 'mobile') {
			this.text(
				this.layers.chrome,
				location,
				layout.statusRibbon.x + 10,
				layout.statusRibbon.y + 6,
				14,
			);
			this.text(
				this.layers.chrome,
				`${shortText(weather, 27)}  ·  ${time}`,
				layout.statusRibbon.x + 10,
				layout.statusRibbon.y + layout.statusRibbon.height * 0.53,
				11,
				{ fill: INK_SOFT },
			);
			const north = this.text(
				this.layers.chrome,
				'N',
				layout.compass.x + layout.compass.width / 2,
				layout.compass.y + 3,
				8,
			);
			north.anchor.set(0.5, 0);
			this.contain(
				this.layers.chrome,
				PARISH_ASSETS.compassIcon,
				{
					x: layout.compass.x,
					y: layout.compass.y + 8,
					width: layout.compass.width,
					height: layout.compass.height - 8,
				},
				1,
			);
		} else {
			const centerY = layout.statusRibbon.y + layout.statusRibbon.height * 0.48;
			const centeredText = (
				value: string,
				xRatio: number,
				size: number,
				options: Partial<TextStyleOptions> = {},
			) => {
				const display = this.text(
					this.layers.chrome,
					value,
					layout.statusRibbon.x + layout.statusRibbon.width * xRatio,
					centerY,
					size,
					options,
				);
				display.anchor.set(0.5);
				return display;
			};
			centeredText(
				location,
				0.23,
				Math.max(18, layout.statusRibbon.height * 0.38),
			);
			centeredText('•', 0.45, Math.max(15, layout.statusRibbon.height * 0.32));
			centeredText(
				shortText(weather.replaceAll(' · ', ' '), 22),
				0.61,
				Math.max(14, layout.statusRibbon.height * 0.29),
				{ fill: INK_SOFT },
			);
			this.drawWeather(
				this.layers.chrome,
				layout.statusRibbon.x + layout.statusRibbon.width * 0.77,
				centerY,
				layout.statusRibbon.height * 0.2,
			);
			centeredText(time, 0.9, Math.max(14, layout.statusRibbon.height * 0.29));
			const compassSize = Math.min(52, layout.compass.height * 0.68);
			const compassRect = {
				x:
					layout.compass.x +
					layout.compass.width -
					compassSize -
					layout.compass.height * 0.18,
				y: layout.compass.y + layout.compass.height * 0.23,
				width: compassSize,
				height: compassSize,
			};
			this.contain(this.layers.chrome, PARISH_ASSETS.compassIcon, compassRect);
			const north = this.text(
				this.layers.chrome,
				'N',
				compassRect.x + compassRect.width / 2,
				layout.compass.y + 3,
				Math.max(9, layout.compass.height * 0.15),
			);
			north.anchor.set(0.5, 0);
		}
	}

	private drawNearby(layout: ParishLayout, state: ParishRenderState): void {
		this.layers.chrome.addChild(
			this.place(this.sprite(PARISH_ASSETS.nearbyRail), layout.nearbyRail),
		);
		if (layout.mode === 'desktop') {
			this.text(
				this.layers.chrome,
				'Nearby',
				layout.nearbyRail.x + layout.nearbyRail.width * 0.22,
				layout.nearbyRail.y + 10,
				Math.max(15, layout.nearbyRail.width * 0.15),
			);
			this.inkLine(
				this.layers.chrome,
				layout.nearbyRail.x + 12,
				layout.nearbyRail.y + 34,
				layout.nearbyRail.x + layout.nearbyRail.width - 12,
				layout.nearbyRail.y + 34,
			);
			const available = layout.moreButton.y - (layout.nearbyRail.y + 40);
			const rowHeight =
				available / Math.max(1, Math.min(4, state.npcs.length || 1));
			state.npcs.slice(0, 4).forEach((npc, index) => {
				const row = {
					x: layout.nearbyRail.x + 6,
					y: layout.nearbyRail.y + 39 + index * rowHeight,
					width: layout.nearbyRail.width - 12,
					height: rowHeight - 2,
				};
				this.drawNearbyPerson(layout, state, npc, index, row);
			});
		} else {
			const people = state.npcs.slice(0, 3);
			const labelWidth = Math.min(64, layout.nearbyRail.width * 0.18);
			this.text(
				this.layers.chrome,
				'Nearby',
				layout.nearbyRail.x + 9,
				layout.nearbyRail.y + 8,
				13,
			);
			this.inkLine(
				this.layers.chrome,
				layout.nearbyRail.x + 8,
				layout.nearbyRail.y + 27,
				layout.nearbyRail.x + labelWidth - 6,
				layout.nearbyRail.y + 27,
				0.55,
			);
			const peopleStart = layout.nearbyRail.x + labelWidth;
			const peopleWidth = layout.moreButton.x - peopleStart - 4;
			const cellWidth = peopleWidth / Math.max(1, people.length);
			if (people.length === 0) {
				this.text(
					this.layers.chrome,
					'No one nearby',
					peopleStart + 9,
					layout.nearbyRail.y + layout.nearbyRail.height * 0.39,
					12,
					{ fill: INK_SOFT },
				);
			} else {
				people.forEach((npc, index) => {
					const row = {
						x: peopleStart + index * cellWidth,
						y: layout.nearbyRail.y + 3,
						width: cellWidth - 3,
						height: layout.nearbyRail.height - 6,
					};
					this.drawNearbyPerson(layout, state, npc, index, row);
				});
			}
		}

		const morePaper = this.paper(this.layers.chrome, layout.moreButton, {
			alpha: 0.86,
			shadow: false,
			pale: true,
		});
		this.bind(
			morePaper,
			this.target(
				'more',
				'more',
				'Open notebook tools',
				layout.moreButton,
				{ type: 'open-surface', surface: 'utility' },
				35,
			),
		);
		this.text(
			this.layers.chrome,
			layout.mode === 'mobile' ? 'More…' : 'More… ⌄',
			layout.moreButton.x + (layout.mode === 'mobile' ? 7 : 9),
			layout.moreButton.y +
				layout.moreButton.height * (layout.mode === 'mobile' ? 0.39 : 0.22),
			layout.mode === 'mobile'
				? Math.min(15, Math.max(12, layout.moreButton.width * 0.23))
				: Math.max(13, layout.moreButton.height * 0.4),
		);
	}

	private drawNearbyPerson(
		layout: ParishLayout,
		state: ParishRenderState,
		npc: ParishRenderState['npcs'][number],
		index: number,
		row: ParishRect,
	): void {
		const selected = npc.real_name === state.selectedRealName;
		const portraitRect =
			layout.mode === 'desktop'
				? {
						x: row.x + row.width * 0.18,
						y: row.y + 2,
						width: row.width * 0.56,
						height: row.height * 0.63,
					}
				: {
						x: row.x + 2,
						y: row.y + 2,
						width: row.width * 0.56,
						height: row.height - 4,
					};
		this.drawPortraitPlaceholder(
			this.layers.chrome,
			portraitRect,
			npc.name,
			selected,
		);
		const target = this.target(
			`nearby:${safeId(npc.real_name)}`,
			'nearby-portrait',
			`Select nearby person ${npc.name}`,
			row,
			{ type: 'select-npc', realName: npc.real_name },
			10 + index,
		);
		const hit = new Graphics();
		hit
			.rect(row.x, row.y, row.width, row.height)
			.fill({ color: 0xffffff, alpha: 0.001 });
		this.layers.chrome.addChild(hit);
		this.bind(hit, target);
		if (layout.mode === 'desktop') {
			const profile = parishProfilePlaceholder();
			this.drawEye(
				this.layers.chrome,
				row.x + row.width * 0.86,
				row.y + row.height * 0.31,
				0.62,
			);
			this.text(
				this.layers.chrome,
				shortText(npc.name, 18),
				row.x + 6,
				row.y + row.height * 0.66,
				Math.max(11, row.width * 0.105),
			);
			for (let dot = 0; dot < profile.nearbyTrustSlots; dot++) {
				const trust = new Graphics();
				trust
					.circle(
						row.x + row.width * 0.31 + dot * 12,
						row.y + row.height * 0.9,
						3.3,
					)
					.fill({
						color: dot < profile.filledTrustSlots ? TRUST_OLIVE : PAPER_LIGHT,
						alpha: 0.9,
					})
					.stroke({ color: INK, width: 0.8, alpha: 0.7 });
				this.layers.chrome.addChild(trust);
			}
		} else {
			this.text(
				this.layers.chrome,
				shortText(npc.name.split(' ')[0] ?? npc.name, 9),
				row.x + row.width * 0.52,
				row.y + row.height * 0.35,
				11,
				{ wordWrap: true, wordWrapWidth: row.width * 0.46 },
			);
		}
	}

	private drawNotebook(layout: ParishLayout, state: ParishRenderState): void {
		// Tabs sit behind the sewn page so only their right-hand handles protrude.
		this.drawTabs(layout);
		const shadow = new Graphics();
		shadow
			.roundRect(
				layout.notebookPage.x + 7,
				layout.notebookPage.y + 9,
				layout.notebookPage.width,
				layout.notebookPage.height,
				9,
			)
			.fill({ color: 0x201b14, alpha: 0.3 });
		this.layers.chrome.addChild(shadow);
		this.layers.chrome.addChild(
			this.place(this.sprite(PARISH_ASSETS.sewnPage), layout.notebookPage),
		);

		const page = layout.notebookPage;
		const scale = page.width / 440;
		const selected = state.selectedNpc;
		const profile = parishProfilePlaceholder();
		const pageLeft = page.x + page.width * 0.17;
		const pageRight = page.x + page.width * 0.88;
		const name = selected?.name ?? 'Parish Notes';
		this.text(
			this.layers.chrome,
			shortText(name, layout.mode === 'mobile' ? 18 : 26),
			pageLeft,
			page.y + page.height * 0.105,
			Math.max(13, 23 * scale),
		);
		this.inkLine(
			this.layers.chrome,
			pageLeft,
			page.y + page.height * 0.16,
			pageRight,
			page.y + page.height * 0.16,
			0.28,
		);

		if (selected) {
			const portraitRect = {
				x: pageLeft,
				y: page.y + page.height * 0.18,
				width: page.width * 0.25,
				height: page.height * 0.13,
			};
			this.drawPortraitPlaceholder(
				this.layers.chrome,
				portraitRect,
				selected.name,
				true,
			);
			this.text(
				this.layers.chrome,
				shortText(selected.mood || 'watchful', 13),
				page.x + page.width * 0.58,
				page.y + page.height * 0.22,
				Math.max(12, 18 * scale),
				{ fill: MOOD_RED },
			);
		}

		const trustY = page.y + page.height * 0.37;
		this.text(
			this.layers.chrome,
			'Trust',
			pageLeft,
			trustY,
			Math.max(13, 19 * scale),
		);
		for (let index = 0; index < profile.profileTrustSlots; index++) {
			const dot = new Graphics();
			dot
				.circle(
					page.x + page.width * (0.48 + index * 0.075),
					trustY + Math.max(8, 10 * scale),
					Math.max(3, 6 * scale),
				)
				.fill({
					color: index < profile.filledTrustSlots ? TRUST_OLIVE : PAPER_LIGHT,
					alpha: 0.9,
				})
				.stroke({ color: INK, width: 1, alpha: 0.8 });
			this.layers.chrome.addChild(dot);
		}

		const factsY = page.y + page.height * 0.47;
		this.text(
			this.layers.chrome,
			'They know',
			pageLeft,
			factsY,
			Math.max(13, 18 * scale),
		);
		const facts = profile.knowledgeNotes;
		facts.forEach((fact, index) => {
			this.text(
				this.layers.chrome,
				`• ${shortText(fact, layout.mode === 'mobile' ? 22 : 31)}`,
				pageLeft + 5,
				factsY + page.height * (0.075 + index * 0.07),
				Math.max(10, 15 * scale),
				{ wordWrap: true, wordWrapWidth: page.width * 0.65 },
			);
		});
		this.drawCartSketch(
			this.layers.chrome,
			page.x + page.width * 0.34,
			page.y + page.height * 0.76,
			page.width * 0.42,
		);
	}

	private drawTabs(layout: ParishLayout): void {
		this.layers.chrome.addChild(
			this.place(this.sprite(PARISH_ASSETS.indexRail), layout.tabRail),
		);
		const pageRight = layout.notebookPage.x + layout.notebookPage.width;
		layout.tabs.forEach((rect, index) => {
			const tab = TAB_LABELS[index];
			const visibleRight = Math.min(layout.width, rect.x + rect.width);
			const visibleWidth = Math.max(1, visibleRight - pageRight);
			const visibleCenterX = pageRight + visibleWidth / 2;
			const iconOnly = layout.mode === 'mobile' || rect.height < 38;
			const hit = new Graphics();
			hit
				.rect(rect.x, rect.y, rect.width, rect.height)
				.fill({ color: 0xffffff, alpha: 0.001 });
			this.layers.chrome.addChild(hit);
			this.bind(
				hit,
				this.target(
					`tab:${tab}`,
					'tab',
					`Open ${titleCase(tab)} notebook tab`,
					rect,
					{ type: 'open-tab', tab },
					40 + index,
				),
			);
			if (!iconOnly) {
				const label = this.text(
					this.layers.chrome,
					titleCase(tab),
					visibleCenterX,
					rect.y + rect.height * 0.19,
					Math.max(9, Math.min(11, rect.height * 0.23)),
					{ fill: INK_SOFT },
				);
				label.anchor.set(0.5);
			}

			const iconSize = iconOnly
				? Math.max(14, Math.min(22, visibleWidth - 5, rect.height - 8))
				: Math.min(36, Math.max(28, visibleWidth * 0.65), rect.height * 0.78);
			this.contain(this.layers.chrome, PARISH_ASSETS.tabIcons[tab], {
				x: visibleCenterX - iconSize / 2,
				y: iconOnly
					? rect.y + (rect.height - iconSize) / 2
					: rect.y + rect.height * 0.32,
				width: iconSize,
				height: iconSize,
			});
		});
	}

	private drawActions(layout: ParishLayout, state: ParishRenderState): void {
		const paper = this.place(
			this.sprite(PARISH_ASSETS.actionStrip),
			layout.actionStrip,
		);
		this.layers.chrome.addChild(paper);
		this.bind(
			paper,
			this.target(
				'action-strip',
				'action',
				'Notebook action strip',
				layout.actionStrip,
				{ type: 'focus-input' },
				69,
			),
		);
		layout.actionCells.forEach((cell, index) => {
			const action = PARISH_ACTIONS[index];
			const target = this.target(
				`action:${action}`,
				'action',
				`${ACTION_LABELS[action]} action`,
				cell,
				{ type: 'action', action },
				70 + index,
				state.busy,
			);
			const hit = new Graphics();
			hit
				.rect(cell.x, cell.y, cell.width, cell.height)
				.fill({ color: 0xffffff, alpha: 0.001 });
			this.layers.chrome.addChild(hit);
			this.bind(hit, target);
			this.contain(this.layers.chrome, PARISH_ASSETS.actionIcons[action], {
				x: cell.x + cell.width * 0.22,
				y: cell.y + cell.height * 0.08,
				width: cell.width * 0.56,
				height: cell.height * 0.52,
			});
			this.text(
				this.layers.chrome,
				ACTION_LABELS[action],
				cell.x + cell.width * 0.22,
				cell.y + cell.height * 0.67,
				Math.max(10, Math.min(16, cell.height * 0.22)),
				{ fill: state.busy ? INK_SOFT : INK },
			);
		});
	}

	private drawIntent(layout: ParishLayout, state: ParishRenderState): void {
		const paper = this.place(
			this.sprite(PARISH_ASSETS.intentStrip),
			layout.intentStrip,
		);
		this.layers.intent.addChild(paper);
		this.bind(
			paper,
			this.target(
				'intent',
				'intent',
				'Write player intent',
				layout.intentStrip,
				{ type: 'focus-input' },
				80,
			),
		);
		const labelWidth =
			layout.intentStrip.width * (layout.mode === 'mobile' ? 0.17 : 0.14);
		this.text(
			this.layers.intent,
			'Intent',
			layout.intentStrip.x + layout.intentStrip.width * 0.03,
			layout.intentStrip.y + layout.intentStrip.height * 0.3,
			Math.max(12, Math.min(19, layout.intentStrip.height * 0.28)),
		);
		const lineX = layout.intentStrip.x + labelWidth;
		const lineY = layout.intentStrip.y + layout.intentStrip.height * 0.61;
		const sendWidth = Math.max(42, layout.intentStrip.width * 0.095);
		this.inkLine(
			this.layers.intent,
			lineX,
			lineY,
			layout.intentStrip.x + layout.intentStrip.width - sendWidth,
			lineY,
			state.inputFocused ? 0.82 : 0.48,
		);
		this.text(
			this.layers.intent,
			shortText(
				state.intentText || 'write what you mean to do…',
				layout.mode === 'mobile' ? 33 : 58,
			),
			lineX + 7,
			layout.intentStrip.y + layout.intentStrip.height * 0.28,
			Math.max(12, Math.min(18, layout.intentStrip.height * 0.27)),
			{
				fill: state.intentText ? INK : INK_SOFT,
				wordWrap: false,
			},
		);
		const sendRect = {
			x: layout.intentStrip.x + layout.intentStrip.width - sendWidth,
			y: layout.intentStrip.y,
			width: sendWidth,
			height: layout.intentStrip.height,
		};
		const sendHit = new Graphics();
		sendHit
			.rect(sendRect.x, sendRect.y, sendRect.width, sendRect.height)
			.fill({ color: 0xffffff, alpha: 0.001 });
		this.layers.intent.addChild(sendHit);
		this.bind(
			sendHit,
			this.target(
				'send',
				'send',
				'Send player intent',
				sendRect,
				{ type: 'send' },
				81,
				state.busy || !state.intentText.trim(),
			),
		);
		this.contain(this.layers.intent, PARISH_ASSETS.quillIcon, {
			x: sendRect.x + sendRect.width * 0.12,
			y: sendRect.y + sendRect.height * 0.08,
			width: sendRect.width * 0.76,
			height: sendRect.height * 0.82,
		});
		if (state.busy) {
			const veil = new Graphics();
			veil
				.rect(sendRect.x, sendRect.y, sendRect.width, sendRect.height)
				.fill({ color: PAPER_LIGHT, alpha: 0.44 });
			this.layers.intent.addChild(veil);
		}
	}

	private drawBottomCards(
		layout: ParishLayout,
		state: ParishRenderState,
	): void {
		this.drawCard(
			layout.mapCard,
			'Map',
			'map',
			'Open parish map',
			90,
			PARISH_ASSETS.mapIcon,
		);
		this.drawCard(
			layout.timeCard,
			'Time',
			'time',
			'Open time and weather',
			91,
			PARISH_ASSETS.timeIcon,
		);
		this.text(
			this.layers.chrome,
			layout.mode === 'desktop' ? '×1' : '1',
			layout.timeCard.x + layout.timeCard.width * 0.71,
			layout.timeCard.y + layout.timeCard.height * 0.58,
			Math.max(10, layout.timeCard.height * 0.16),
		);

		const intents = this.place(
			this.sprite(PARISH_ASSETS.activeIntentsCard),
			layout.activeIntentsCard,
		);
		this.layers.chrome.addChild(intents);
		this.bind(
			intents,
			this.target(
				'active-intents',
				'card',
				'Open active intents',
				layout.activeIntentsCard,
				{ type: 'open-surface', surface: 'intents' },
				92,
			),
		);
		this.text(
			this.layers.chrome,
			'Active Intents',
			layout.activeIntentsCard.x + layout.activeIntentsCard.width * 0.12,
			layout.activeIntentsCard.y + layout.activeIntentsCard.height * 0.12,
			Math.max(11, Math.min(17, layout.activeIntentsCard.height * 0.19)),
		);
		this.text(
			this.layers.chrome,
			state.busy ? 'awaiting the parish…' : '(none)',
			layout.activeIntentsCard.x + layout.activeIntentsCard.width * 0.12,
			layout.activeIntentsCard.y + layout.activeIntentsCard.height * 0.43,
			Math.max(10, Math.min(15, layout.activeIntentsCard.height * 0.16)),
			{ fill: INK_SOFT },
		);
		this.inkLine(
			this.layers.chrome,
			layout.activeIntentsCard.x + layout.activeIntentsCard.width * 0.1,
			layout.activeIntentsCard.y + layout.activeIntentsCard.height * 0.72,
			layout.activeIntentsCard.x + layout.activeIntentsCard.width * 0.76,
			layout.activeIntentsCard.y + layout.activeIntentsCard.height * 0.72,
			0.42,
		);
		this.contain(this.layers.chrome, PARISH_ASSETS.quillIcon, {
			x: layout.activeIntentsCard.x + layout.activeIntentsCard.width * 0.77,
			y: layout.activeIntentsCard.y + layout.activeIntentsCard.height * 0.22,
			width: layout.activeIntentsCard.width * 0.18,
			height: layout.activeIntentsCard.height * 0.58,
		});
	}

	private drawCard(
		rect: ParishRect,
		label: string,
		surface: NotebookSurface,
		ariaLabel: string,
		order: number,
		iconUrl: string,
	): void {
		const paper = this.place(this.sprite(PARISH_ASSETS.smallCard), rect);
		this.layers.chrome.addChild(paper);
		this.bind(
			paper,
			this.target(
				`${surface}-card`,
				'card',
				ariaLabel,
				rect,
				{ type: 'open-surface', surface },
				order,
			),
		);
		this.text(
			this.layers.chrome,
			label,
			rect.x + rect.width * 0.29,
			rect.y + rect.height * 0.14,
			Math.max(11, Math.min(17, rect.height * 0.18)),
		);
		this.contain(this.layers.chrome, iconUrl, {
			x: rect.x + rect.width * 0.25,
			y: rect.y + rect.height * 0.34,
			width: rect.width * 0.5,
			height: rect.height * 0.48,
		});
	}

	private drawEye(layer: Container, x: number, y: number, scale = 1): void {
		const eye = new Graphics();
		eye
			.moveTo(x - 10 * scale, y)
			.quadraticCurveTo(x, y - 7 * scale, x + 10 * scale, y)
			.quadraticCurveTo(x, y + 7 * scale, x - 10 * scale, y)
			.stroke({ color: INK, width: 1.4 * scale, alpha: 0.9 });
		eye.circle(x, y, 2.5 * scale).fill({ color: INK, alpha: 0.88 });
		layer.addChild(eye);
	}

	private drawPortraitPlaceholder(
		layer: Container,
		rect: ParishRect,
		name: string,
		selected: boolean,
	): void {
		this.contain(layer, PARISH_ASSETS.portraitFrame, rect);
		if (selected) {
			const selection = new Graphics();
			selection
				.roundRect(rect.x - 1, rect.y - 1, rect.width + 2, rect.height + 2, 4)
				.stroke({ color: MOOD_RED, width: 1.5, alpha: 0.7 });
			layer.addChild(selection);
		}

		const initials = this.text(
			layer,
			npcInitials(name),
			rect.x + rect.width / 2,
			rect.y + rect.height / 2,
			Math.max(11, Math.min(rect.width, rect.height) * 0.38),
			{ fill: INK_SOFT, fontWeight: '600' },
		);
		initials.anchor.set(0.5);
	}

	private drawChurch(
		layer: Container,
		x: number,
		y: number,
		size: number,
	): void {
		const g = new Graphics();
		g.moveTo(x - size * 0.7, y + size * 0.75)
			.lineTo(x - size * 0.7, y)
			.lineTo(x, y - size * 0.55)
			.lineTo(x + size * 0.7, y)
			.lineTo(x + size * 0.7, y + size * 0.75)
			.moveTo(x - size * 0.2, y - size * 0.36)
			.lineTo(x - size * 0.2, y - size * 1.02)
			.lineTo(x + size * 0.2, y - size * 1.02)
			.lineTo(x + size * 0.2, y - size * 0.36)
			.moveTo(x, y - size * 1.25)
			.lineTo(x, y - size * 0.92)
			.moveTo(x - size * 0.12, y - size * 1.11)
			.lineTo(x + size * 0.12, y - size * 1.11)
			.stroke({ color: INK, width: 1.1, alpha: 0.72 });
		layer.addChild(g);
	}

	private drawWeather(
		layer: Container,
		x: number,
		y: number,
		size: number,
	): void {
		const g = new Graphics();
		g.arc(x, y, size * 0.45, Math.PI, Math.PI * 2)
			.moveTo(x - size * 0.65, y)
			.lineTo(x + size * 0.65, y)
			.stroke({ color: INK, width: 1, alpha: 0.65 });
		for (let index = 0; index < 5; index++) {
			const angle = Math.PI + (index * Math.PI) / 4;
			g.moveTo(
				x + Math.cos(angle) * size * 0.6,
				y + Math.sin(angle) * size * 0.6,
			)
				.lineTo(
					x + Math.cos(angle) * size * 0.82,
					y + Math.sin(angle) * size * 0.82,
				)
				.stroke({ color: INK, width: 0.8, alpha: 0.52 });
		}
		layer.addChild(g);
	}

	private drawCartSketch(
		layer: Container,
		x: number,
		y: number,
		width: number,
	): void {
		const g = new Graphics();
		const h = width * 0.3;
		g.circle(x, y + h, h * 0.36)
			.circle(x + width * 0.58, y + h, h * 0.36)
			.moveTo(x, y + h)
			.lineTo(x + width * 0.58, y + h)
			.lineTo(x + width * 0.48, y)
			.lineTo(x + width * 0.07, y)
			.closePath()
			.moveTo(x + width * 0.58, y + h * 0.2)
			.lineTo(x + width, y - h * 0.1)
			.stroke({ color: INK, width: 1, alpha: 0.45 });
		layer.addChild(g);
	}
}

function shortText(value: string, max: number): string {
	const normalized = value.replace(/\s+/g, ' ').trim();
	return normalized.length <= max
		? normalized
		: `${normalized.slice(0, max - 1)}…`;
}

function safeId(value: string): string {
	return value
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-|-$/g, '');
}

function npcInitials(value: string): string {
	return (
		value
			.trim()
			.split(/\s+/)
			.slice(0, 2)
			.map((part) => part.charAt(0).toLocaleUpperCase())
			.join('') || '?'
	);
}

function titleCase(value: string): string {
	return value.charAt(0).toUpperCase() + value.slice(1);
}

export function notebookSurfaceLabel(surface: NotebookSurface): string {
	return SURFACE_LABELS[surface];
}
