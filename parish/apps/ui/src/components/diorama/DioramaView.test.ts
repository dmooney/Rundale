import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import DioramaView from './DioramaView.svelte';
import { mapData, npcsHere, streamingActive, textLog } from '../../stores/game';
import { sceneNpcFocusRequest } from '../../stores/scene';
import type { SceneState } from '$lib/types';

const mockSubmitInput = vi.fn();

vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
}));

function scene(): SceneState {
	return {
		schema_version: 1,
		location_id: 2,
		location_name: "Darcy's Pub",
		indoor: true,
		slug: 'darcys-pub',
		native_size: [1280, 720],
		underlay_url: '/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1',
		plate_url: '/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1',
		variant: 'day',
		weather_overlay: null,
		layers: [
			{
				id: 'underlay',
				asset_id: 'pub-underlay',
				kind: 'underlay',
				asset_url: '/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1',
				x: 50,
				y: 50,
				z: 0,
				scale: 1,
				opacity: 1,
				flip: false,
				anchor: [50, 100],
				labels: [],
			},
		],
		hotspots: [
			{
				id: 'front-door',
				label: 'Out to the Crossroads',
				shape: { rect: [82, 38, 14, 50] },
				action: { travel_to: 1 },
				activation: {
					kind: 'travel',
					target_location_id: 1,
					target_label: 'The Crossroads',
					command: 'go to The Crossroads',
				},
			},
			{
				id: 'hearth',
				label: 'The hearth',
				shape: { rect: [5, 30, 18, 40] },
				action: { inspect: 'A turf fire smoulders in the wide hearth.' },
				activation: {
					kind: 'inspect',
					text: 'A turf fire smoulders in the wide hearth.',
				},
			},
		],
		slots: [
			{
				id: 'behind-bar',
				x: 50,
				y: 52,
				scale: 1,
				prefer_npc: 1,
				occupied_npc_id: 1,
			},
		],
		npcs: [
			{
				npc_id: 1,
				slot_id: 'behind-bar',
				display_name: 'an older man behind the bar',
				real_name: null,
				introduced: false,
				mood: 'content',
				mood_emoji: '🙂',
				sprite_url:
					'/api/scene-asset/assets/scenes/sprites/generic-villager.png?v=1',
				x: 50,
				y: 52,
				scale: 1,
				flip: false,
			},
		],
		overflow_npcs: [],
	};
}

beforeEach(() => {
	mockSubmitInput.mockReset();
	mockSubmitInput.mockResolvedValue(undefined);
	mapData.set({
		locations: [
			{
				id: '1',
				name: 'The Crossroads',
				lat: 0,
				lon: 0,
				adjacent: true,
				hops: 1,
			},
		],
		edges: [['1', '2']],
		player_location: '2',
		transport_label: 'on foot',
		transport_id: 'walking',
	});
	npcsHere.set([
		{
			name: 'an older man behind the bar',
			real_name: 'Padraig Darcy',
			occupation: 'Publican',
			mood: 'content',
			introduced: false,
			mood_emoji: '🙂',
		},
	]);
	textLog.set([]);
	streamingActive.set(false);
	sceneNpcFocusRequest.set(null);
});

describe('DioramaView', () => {
	it('renders the scene plate, hotspots, and NPC sprite', () => {
		const { getByTestId, getByAltText, getByRole } = render(DioramaView, {
			props: { scene: scene() },
		});
		expect(getByTestId('diorama-view')).toBeTruthy();
		expect(getByAltText("Darcy's Pub scene plate")).toBeTruthy();
		expect(getByRole('button', { name: 'Out to the Crossroads' })).toBeTruthy();
		expect(
			getByRole('button', { name: 'Speak to an older man behind the bar' }),
		).toBeTruthy();
	});

	it('submits travel when a travel hotspot is clicked', async () => {
		const { getByRole } = render(DioramaView, { props: { scene: scene() } });
		await fireEvent.click(
			getByRole('button', { name: 'Out to the Crossroads' }),
		);
		expect(mockSubmitInput).toHaveBeenCalledWith('go to The Crossroads');
	});

	it('adds inspect text to the local log', async () => {
		const { getByRole } = render(DioramaView, { props: { scene: scene() } });
		await fireEvent.click(getByRole('button', { name: 'The hearth' }));
		let entries: unknown[] = [];
		textLog.subscribe((value) => {
			entries = value;
		})();
		expect(entries).toContainEqual({
			source: 'system',
			subtype: 'scene-inspect',
			content: 'A turf fire smoulders in the wide hearth.',
		});
	});

	it('requests an addressed input chip without exposing real_name in the scene label', async () => {
		const { getByRole } = render(DioramaView, { props: { scene: scene() } });
		await fireEvent.click(
			getByRole('button', { name: 'Speak to an older man behind the bar' }),
		);
		let request = null;
		sceneNpcFocusRequest.subscribe((value) => {
			request = value;
		})();
		expect(request).toMatchObject({
			display_name: 'an older man behind the bar',
			real_name: 'Padraig Darcy',
		});
	});

	it('shows debug hotspot labels only when debugHotspots is true', () => {
		const { queryByText, rerender } = render(DioramaView, {
			props: { scene: scene(), debugHotspots: false },
		});
		expect(queryByText('Out to the Crossroads')).toBeNull();
		rerender({ scene: scene(), debugHotspots: true });
		expect(queryByText('Out to the Crossroads')).toBeTruthy();
	});
});
