import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NotebookHitTarget } from '$lib/illustrated-notebook/interactions';
import type { NotebookRenderState } from '$lib/illustrated-notebook/types';
import IllustratedNotebookGame from './IllustratedNotebookGame.svelte';
import {
	flushStream,
	fullMapOpen,
	intentDraft,
	mapData,
	npcsHere,
	streamingActive,
	textLog,
	worldState,
} from '../../stores/game';

let lastRenderState: NotebookRenderState | null = null;
let lastRenderer: {
	activateTarget: (id: string) => unknown;
	setFocusedTarget: (id: string | null) => unknown;
} | null = null;
let mockHitTargets: NotebookHitTarget[] = [];
const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});

vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
}));

vi.mock('$lib/illustrated-notebook/renderer', () => ({
	IllustratedNotebookRenderer: class {
		private readonly options?: {
			onHitTargetsChanged?: (targets: NotebookHitTarget[]) => void;
		};

		constructor(
			_host: HTMLElement,
			options?: {
				onHitTargetsChanged?: (targets: NotebookHitTarget[]) => void;
			},
		) {
			this.options = options;
			lastRenderer = {
				activateTarget: vi.fn((_id: string) => true),
				setFocusedTarget: vi.fn((_id: string | null) => undefined),
			};
		}

		async init() {}
		render(state: NotebookRenderState) {
			lastRenderState = state;
			this.options?.onHitTargetsChanged?.(mockHitTargets);
		}
		resize() {}
		destroy() {}
		activateTarget(id: string) {
			return lastRenderer?.activateTarget(id);
		}
		setFocusedTarget(id: string | null) {
			return lastRenderer?.setFocusedTarget(id);
		}
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
const aoife = {
	name: 'Aoife Kelly',
	real_name: 'Aoife Kelly',
	occupation: 'weaver',
	mood: 'curious',
	introduced: true,
	mood_emoji: '•',
};

function target(id: string, label: string, order: number): NotebookHitTarget {
	return {
		id,
		kind: 'action-stamp',
		label,
		rect: { x: order, y: order, width: 20, height: 20 },
		order,
		activation: { type: 'focus-input' },
	};
}

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
	fullMapOpen.set(false);
	lastRenderState = null;
	lastRenderer = null;
	mockHitTargets = [
		target('nearby:roisin', 'Select nearby person Roisin Connolly', 10),
		target('action:ask', 'Ask action stamp', 40),
		target('tab:people', 'Open People notebook tab', 50),
		target('time-card', 'Open time details', 60),
		target('active-intents-card', 'Open active intents', 70),
	];
	mockSubmitInput.mockReset();
	mockSubmitInput.mockResolvedValue(undefined);
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

	it('keeps default selected-person behavior stable when nearby people change', async () => {
		render(IllustratedNotebookGame);

		await waitFor(() =>
			expect(lastRenderState?.selectedNpc?.name).toBe('Roisin Connolly'),
		);

		npcsHere.set([roisin, aoife]);
		await waitFor(() =>
			expect(lastRenderState?.selectedNpc?.name).toBe('Roisin Connolly'),
		);

		lastRenderState?.callbacks.onSelectNpc('Aoife Kelly');
		await waitFor(() =>
			expect(lastRenderState?.selectedNpc?.name).toBe('Aoife Kelly'),
		);

		npcsHere.set([roisin]);
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

	it('renders focus and streaming as distinct command states while the hidden input stays editable', async () => {
		const { getByLabelText, getByRole } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await waitFor(() => expect(lastRenderState?.command.focused).toBe(true));
		expect(input.dataset.commandState).toBe('focused');
		expect(input.disabled).toBe(false);

		streamingActive.set(true);
		await waitFor(() => expect(lastRenderState?.command.busy).toBe(true));
		expect(lastRenderState?.command.disabled).toBe(false);
		expect(input.dataset.commandState).toBe('busy');
		expect(input.hasAttribute('aria-disabled')).toBe(false);
		expect(input.disabled).toBe(false);
		expect(input.readOnly).toBe(false);
		const status = getByRole('status');
		expect(status.hasAttribute('aria-live')).toBe(false);
	});

	it('renders the local pending submit as disabled without clearing the draft early', async () => {
		const deferred = { resolve: () => {} };
		mockSubmitInput.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					deferred.resolve = resolve;
				}),
		);
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await fireEvent.input(input, { target: { value: 'look around' } });
		await fireEvent.keyDown(input, { key: 'Enter' });

		await waitFor(() => expect(lastRenderState?.command.disabled).toBe(true));
		expect(lastRenderState?.command.busy).toBe(false);
		expect(input.dataset.commandState).toBe('disabled');
		expect(input.getAttribute('aria-disabled')).toBe('true');
		expect(input.readOnly).toBe(true);
		expect(input.value).toBe('look around');
		lastRenderState?.callbacks.onAction('ask');
		await waitFor(() => expect(input.value).toBe('look around'));

		deferred.resolve();
		await waitFor(() => expect(input.value).toBe(''));
		expect(input.hasAttribute('aria-disabled')).toBe(false);
		expect(input.readOnly).toBe(false);
	});

	it('renders a failed submit as an accessible Pixi error and preserves the draft for retry', async () => {
		mockSubmitInput.mockRejectedValueOnce(new Error('bridge unavailable'));
		const { container, getByLabelText, getByRole, getByText } = render(
			IllustratedNotebookGame,
		);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await fireEvent.input(input, { target: { value: 'look around' } });
		await fireEvent.keyDown(input, { key: 'Enter' });

		await waitFor(() => expect(input.dataset.commandState).toBe('error'));
		expect(lastRenderState?.command.error).toContain('bridge unavailable');
		expect(input.value).toBe('look around');
		expect(input.getAttribute('aria-invalid')).toBe('true');
		expect(getByText(/Ink blotted — Could not send input/)).toBeTruthy();
		expect(getByRole('alert').hasAttribute('aria-live')).toBe(false);
		expect(container.querySelector('.input-wrapper')).toBeNull();
		expect(container.querySelector('.input-form')).toBeNull();

		await fireEvent.input(input, { target: { value: 'look' } });
		await waitFor(() => expect(input.dataset.commandState).toBe('typing'));
		expect(input.getAttribute('aria-invalid')).toBe('false');
	});

	it('routes notebook tabs and cards through overlay state', async () => {
		const { getByLabelText, getByText } = render(IllustratedNotebookGame);

		await waitFor(() => expect(lastRenderState).not.toBeNull());

		lastRenderState?.callbacks.onOpenTab('people');
		await waitFor(() => expect(getByLabelText('people drawer')).toBeTruthy());

		lastRenderState?.callbacks.onOpenTime();
		await waitFor(() => expect(getByText('Clock')).toBeTruthy());

		lastRenderState?.callbacks.onOpenActiveIntents();
		await waitFor(() => expect(getByText('Current line')).toBeTruthy());

		lastRenderState?.callbacks.onOpenMap();
		expect(get(fullMapOpen)).toBe(true);
	});

	it('exposes renderer hit targets for keyboard focus and activation', async () => {
		const { getByLabelText } = render(IllustratedNotebookGame);

		await waitFor(() =>
			expect(getByLabelText('Ask action stamp')).toBeTruthy(),
		);

		const targetButton = getByLabelText('Ask action stamp');
		await fireEvent.focus(targetButton);
		expect(lastRenderer?.setFocusedTarget).toHaveBeenCalledWith('action:ask');

		await fireEvent.click(targetButton);
		expect(lastRenderer?.activateTarget).toHaveBeenCalledWith('action:ask');

		await fireEvent.blur(targetButton);
		expect(lastRenderer?.setFocusedTarget).toHaveBeenCalledWith(null);
	});
});
