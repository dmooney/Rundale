import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NotebookRenderState } from '$lib/illustrated-notebook/types';
import IllustratedNotebookGame from './IllustratedNotebookGame.svelte';
import {
	flushStream,
	intentDraft,
	mapData,
	npcsHere,
	streamingActive,
	textLog,
	worldState,
} from '../../stores/game';

let lastRenderState: NotebookRenderState | null = null;
const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});

vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
}));

vi.mock('$lib/illustrated-notebook/renderer', () => ({
	IllustratedNotebookRenderer: class {
		async init() {}
		render(state: NotebookRenderState) {
			lastRenderState = state;
		}
		resize() {}
		destroy() {}
	},
}));

const roisin = {
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

function seedStores() {
	worldState.set({
		location_name: 'Kilteevan Village',
		location_description: 'A whitewashed village by the bridge.',
		time_label: 'Afternoon',
		hour: 15,
		minute: 40,
		weather: 'clearing',
		season: 'Spring',
		festival: null,
		paused: false,
		inference_paused: false,
		game_epoch_ms: Date.UTC(1820, 3, 1, 15, 40),
		speed_factor: 36,
		name_hints: [],
		day_of_week: 'Monday',
	});
	npcsHere.set([roisin]);
	mapData.set({
		locations: [
			{
				id: '15',
				name: 'Kilteevan Village',
				lat: 53.63,
				lon: -8.1,
				adjacent: false,
				hops: 0,
			},
			{
				id: '1',
				name: 'The Crossroads',
				lat: 53.64,
				lon: -8.11,
				adjacent: true,
				hops: 1,
			},
		],
		edges: [['15', '1']],
		player_location: '15',
		transport_label: 'on foot',
		transport_id: 'walking',
	});
	textLog.set([{ source: 'system', content: 'You are in Kilteevan.' }]);
	streamingActive.set(false);
	flushStream.set(() => 0);
	intentDraft.set(null);
	lastRenderState = null;
	mockSubmitInput.mockClear();
}

describe('IllustratedNotebookGame', () => {
	beforeEach(seedStores);

	it('mounts a Pixi host without old dashboard or InputField chrome', async () => {
		const { container, getByTestId, queryByText } = render(
			IllustratedNotebookGame,
		);

		expect(getByTestId('illustrated-notebook-game')).toBeTruthy();
		expect(getByTestId('illustrated-notebook-pixi-host')).toBeTruthy();
		expect(container.querySelector('.input-wrapper')).toBeNull();
		expect(container.querySelector('.input-form')).toBeNull();
		expect(container.querySelector('[data-testid="chat-panel"]')).toBeNull();
		expect(queryByText(/^Send$/i)).toBeNull();

		await waitFor(() =>
			expect(lastRenderState?.selectedNpc?.name).toBe('Roisin Connolly'),
		);
	});

	it('seeds and submits through the new hidden command input', async () => {
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await waitFor(() => expect(lastRenderState).not.toBeNull());
		lastRenderState?.callbacks.onAction('ask');
		await waitFor(() => expect(input.value).toBe('ask Roisin Connolly '));

		await fireEvent.keyDown(input, { key: 'Enter' });
		await waitFor(() =>
			expect(mockSubmitInput).toHaveBeenCalledWith('ask Roisin Connolly'),
		);
	});
});
