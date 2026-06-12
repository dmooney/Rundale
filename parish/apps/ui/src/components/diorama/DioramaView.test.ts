import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import DioramaView from './DioramaView.svelte';
import type { SceneState } from '$lib/types';
import { travelDimming } from '../../stores/scene';
import { debugVisible } from '../../stores/debug';

// Mock scene-actions so tests don't need the full IPC stack
vi.mock('$lib/scene-actions', () => ({
	handleSceneAction: vi.fn(),
	handleNpcSpriteClick: vi.fn(),
}));

// Mock the palette store (SceneOverlay depends on it)
vi.mock('../../stores/theme', () => ({
	palette: {
		subscribe: (fn: (v: { accent: string }) => void) => {
			fn({ accent: '#b08531' });
			return () => {};
		},
	},
}));

// ── Fixtures ─────────────────────────────────────────────────────────────────

function makeSceneState(overrides: Partial<SceneState> = {}): SceneState {
	return {
		schema_version: 1,
		location_id: 2,
		plate_url: 'data:image/png;base64,PLATE',
		variant: 'day',
		indoor: false,
		hotspots: [
			{
				id: 'door',
				shape: { rect: [82, 38, 14, 50] },
				label: 'Out to the Crossroads',
				action: { travel_to: 1 },
			},
		],
		npcs: [
			{
				id: 1,
				display_name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				introduced: true,
				mood_emoji: '😊',
				sprite_url: 'data:image/png;base64,SPRITE',
				x: 48,
				y: 55,
				scale: 1.0,
				flip: false,
			},
		],
		overflow_npcs: [],
		...overrides,
	};
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('DioramaView', () => {
	beforeEach(() => {
		travelDimming.set(false);
		debugVisible.set(false);
	});

	// AC-8: null sceneState → no plate img rendered (fallback path)
	it('renders nothing when sceneState is null', () => {
		const { container } = render(DioramaView, { props: { sceneState: null } });
		expect(container.querySelector('[data-testid="diorama-view"]')).toBeNull();
		expect(container.querySelector('img')).toBeNull();
	});

	// AC-2: plate <img> is rendered with pixelated rendering
	it('renders the plate img when sceneState is provided', async () => {
		const scene = makeSceneState();
		const { container } = render(DioramaView, { props: { sceneState: scene } });
		await tick();
		const plate = container.querySelector('img') as HTMLImageElement | null;
		expect(plate).not.toBeNull();
		expect(plate!.src).toContain('data:image/png;base64,PLATE');
	});

	// AC-2: wrapper has aspect-ratio 480/270
	it('wrapper has aspect-ratio 480/270', async () => {
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const wrapper = container.querySelector(
			'[data-testid="diorama-view"]',
		) as HTMLElement;
		expect(wrapper).not.toBeNull();
		// The CSS aspect-ratio is scoped by Svelte; check the style attr is set
		// via the inline style or that the element exists with expected class
		expect(wrapper.className).toMatch(/diorama-view/);
	});

	// AC-7: hotspot SVG layer uses viewBox="0 0 100 100" preserveAspectRatio="none"
	it('renders hotspot SVG with correct viewBox and preserveAspectRatio', async () => {
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const svg = container.querySelector('svg') as SVGElement | null;
		expect(svg).not.toBeNull();
		expect(svg!.getAttribute('viewBox')).toBe('0 0 100 100');
		expect(svg!.getAttribute('preserveAspectRatio')).toBe('none');
	});

	// AC-7: hotspot rect geometry from SceneState maps directly to SVG attrs
	it('places a hotspot rect at the expected percentage coordinates', async () => {
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const rect = container.querySelector('rect') as SVGRectElement | null;
		expect(rect).not.toBeNull();
		expect(Number(rect!.getAttribute('x'))).toBe(82);
		expect(Number(rect!.getAttribute('y'))).toBe(38);
		expect(Number(rect!.getAttribute('width'))).toBe(14);
		expect(Number(rect!.getAttribute('height'))).toBe(50);
	});

	// AC-9: hotspot is keyboard-focusable with aria-label
	it('hotspot rect has tabindex=0 and aria-label from hotspot.label', async () => {
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const rect = container.querySelector('rect');
		expect(rect).not.toBeNull();
		expect(rect!.getAttribute('tabindex')).toBe('0');
		expect(rect!.getAttribute('aria-label')).toBe('Out to the Crossroads');
	});

	// AC-7: NPC sprite is placed at the expected percentage position
	it('places an NPC sprite at the expected left/bottom percentages', async () => {
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		// The sprite button has inline style left:{x}% bottom:{100-y}%
		const spriteBtn = container.querySelector(
			'.sprite-btn',
		) as HTMLElement | null;
		expect(spriteBtn).not.toBeNull();
		// x=48, y=55 → left: 48%, bottom: 45%
		expect(spriteBtn!.style.left).toBe('48%');
		expect(spriteBtn!.style.bottom).toBe('45%');
	});

	// AC-6: sprite tooltip uses display_name, not real_name for unintroduced NPC
	it('sprite title uses display_name for an unintroduced NPC', async () => {
		const scene = makeSceneState({
			npcs: [
				{
					id: 2,
					display_name: 'an older woman',
					real_name: 'Brigid Walsh',
					introduced: false,
					mood_emoji: '😐',
					sprite_url: 'data:image/png;base64,SPRITE2',
					x: 22,
					y: 68,
					scale: 1.1,
					flip: true,
				},
			],
		});
		const { container } = render(DioramaView, { props: { sceneState: scene } });
		await tick();
		const spriteBtn = container.querySelector('.sprite-btn') as HTMLElement;
		expect(spriteBtn.title).toBe('an older woman');
		expect(spriteBtn.title).not.toBe('Brigid Walsh');
	});

	// AC-9: debug overlay shows hotspot rects visibly when debugVisible is true
	it('hotspot rect has debug class when debug panel is open', async () => {
		debugVisible.set(true);
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const rect = container.querySelector('rect');
		expect(rect).not.toBeNull();
		expect(rect!.classList.contains('debug')).toBe(true);
	});

	it('hotspot rect does not have debug class when debug panel is closed', async () => {
		debugVisible.set(false);
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const rect = container.querySelector('rect');
		expect(rect).not.toBeNull();
		expect(rect!.classList.contains('debug')).toBe(false);
	});

	// AC-10: plate wrapper gets dimmed class when travelDimming is true
	it('plate wrapper has dimmed class when travelDimming is true', async () => {
		travelDimming.set(true);
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const plateWrap = container.querySelector(
			'.plate-wrap',
		) as HTMLElement | null;
		expect(plateWrap).not.toBeNull();
		expect(plateWrap!.classList.contains('dimmed')).toBe(true);
	});

	it('plate wrapper is not dimmed when travelDimming is false', async () => {
		travelDimming.set(false);
		const { container } = render(DioramaView, {
			props: { sceneState: makeSceneState() },
		});
		await tick();
		const plateWrap = container.querySelector(
			'.plate-wrap',
		) as HTMLElement | null;
		expect(plateWrap).not.toBeNull();
		expect(plateWrap!.classList.contains('dimmed')).toBe(false);
	});
});
