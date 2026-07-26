import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import NotebookShell from './NotebookShell.svelte';
import {
	mapData,
	npcsHere,
	textLog,
	worldState,
	streamingActive,
	loadingPhrase,
	loadingColor,
	intentDraft,
} from '../../stores/game';

vi.mock('maplibre-gl');

const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});
vi.mock('$lib/ipc', () => ({
	getAuthStatus: vi.fn(async () => null),
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
}));

const roisin = {
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

function seedNotebookStores() {
	worldState.set({
		location_id: 1,
		location_name: "Connolly's Crossroads",
		location_description: 'A muddy crossing by the shop and chapel road.',
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
		active_tasks: [],
	});
	npcsHere.set([roisin]);
	textLog.set([
		{
			source: 'system',
			subtype: 'location',
			content: 'You are at the crossroads.',
		},
		{ id: 'm1', source: 'Roisin Connolly', content: 'The cart is delayed.' },
	]);
	mapData.set({
		locations: [
			{
				id: 'crossroads',
				name: "Connolly's Crossroads",
				lat: 53.63,
				lon: -8.03,
				adjacent: false,
				hops: 0,
			},
			{
				id: 'chapel',
				name: 'Chapel Lane',
				lat: 53.631,
				lon: -8.031,
				adjacent: true,
				hops: 1,
			},
		],
		edges: [['crossroads', 'chapel']],
		player_location: 'crossroads',
		transport_label: 'on foot',
		transport_id: 'walking',
	});
	streamingActive.set(false);
	loadingPhrase.set('');
	loadingColor.set([72, 199, 142]);
	intentDraft.set(null);
}

describe('NotebookShell', () => {
	beforeEach(() => {
		seedNotebookStores();
		mockSubmitInput.mockClear();
	});

	it('renders the parish notebook regions instead of the old app grid', () => {
		const { getByTestId, getByText, getAllByText, queryByTestId } =
			render(NotebookShell);

		expect(getByTestId('parish-notebook-shell')).toBeTruthy();
		expect(getByTestId('notebook-top-ribbon')).toBeTruthy();
		expect(getByTestId('notebook-page')).toBeTruthy();
		expect(getByText('Rundale')).toBeTruthy();
		expect(getByText('Nearby')).toBeTruthy();
		expect(getAllByText('Roisin Connolly').length).toBeGreaterThan(0);
		expect(queryByTestId('chat-panel')).toBeNull();
	});

	it('seeds the shared intent field from an action stamp', async () => {
		const { getByRole } = render(NotebookShell);

		await fireEvent.click(
			getByRole('button', { name: /ask roisin connolly/i }),
		);

		await waitFor(() => {
			expect(getByRole('combobox').textContent).toBe('ask Roisin Connolly ');
		});
	});
});
