import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import type {
	ParishHitTarget,
	ParishRenderState,
} from '$lib/illustrated-parish/types';
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
import {
	closeNotebookOverlay,
	notebookOverlay,
	resetNotebookOverlayForTests,
} from '../../stores/notebookOverlay';

let lastRenderState: ParishRenderState | null = null;
let lastRenderer: {
	activateTarget: Mock<(id: string) => boolean>;
	setFocusedTarget: Mock<(id: string | null) => void>;
} | null = null;
let mockHitTargets: ParishHitTarget[] = [];
let rendererConstructCount = 0;
const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});

vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
	getDebugSnapshot: vi.fn(async () => ({})),
}));

vi.mock('$lib/illustrated-parish/renderer', () => ({
	IllustratedParishRenderer: class {
		private readonly options?: {
			onHitTargetsChanged?: (targets: ParishHitTarget[]) => void;
		};

		constructor(
			_host: HTMLElement,
			options?: {
				onHitTargetsChanged?: (targets: ParishHitTarget[]) => void;
			},
		) {
			this.options = options;
			rendererConstructCount += 1;
			lastRenderer = {
				activateTarget: vi.fn((_id: string) => true),
				setFocusedTarget: vi.fn((_id: string | null) => undefined),
			};
		}

		async init() {}

		render(state: ParishRenderState) {
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
	npc_id: 6,
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};
const aoife = {
	npc_id: 7,
	name: 'Aoife Kelly',
	real_name: 'Aoife Kelly',
	occupation: 'weaver',
	mood: 'curious',
	introduced: true,
	mood_emoji: '•',
};

function target(id: string, label: string, order: number): ParishHitTarget {
	return {
		id,
		kind: 'action',
		label,
		rect: { x: order, y: order, width: 20, height: 20 },
		order,
		activation: { type: 'focus-input' },
	};
}

function tabTarget(
	tab: 'notes' | 'people' | 'places' | 'rumours' | 'journal',
	order: number,
): ParishHitTarget {
	return {
		id: `tab:${tab}`,
		kind: 'tab',
		label: `Open ${tab.charAt(0).toUpperCase()}${tab.slice(1)} notebook tab`,
		rect: { x: order, y: order, width: 44, height: 44 },
		order,
		activation: { type: 'open-tab', tab },
	};
}

function seedStores() {
	sessionStorage.clear();
	resetNotebookOverlayForTests();
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
	rendererConstructCount = 0;
	mockHitTargets = [
		target('nearby:roisin', 'Select nearby person Roisin Connolly', 10),
		target('action:ask', 'Ask action', 40),
		tabTarget('notes', 50),
		tabTarget('people', 51),
		tabTarget('places', 52),
		tabTarget('rumours', 53),
		tabTarget('journal', 54),
		target('time-card', 'Open time and weather', 60),
		target('active-intents', 'Open active intents', 70),
	];
	mockSubmitInput.mockReset();
	mockSubmitInput.mockResolvedValue(undefined);
}

describe('IllustratedNotebookGame fresh parish bridge', () => {
	beforeEach(seedStores);

	it('mounts one Pixi viewport without dashboard chrome', async () => {
		const { container, getByRole, getByTestId, queryByText } = render(
			IllustratedNotebookGame,
		);

		expect(
			getByRole('region', {
				name: 'Rundale illustrated parish notebook',
			}),
		).toBe(getByTestId('illustrated-notebook-game'));
		expect(getByTestId('illustrated-notebook-pixi-host')).toBeTruthy();
		expect(getByTestId('illustrated-notebook-pixi-host')).toHaveAttribute(
			'aria-hidden',
			'true',
		);
		expect(getByRole('status', { name: 'Parish status' })).toHaveTextContent(
			'Location: Kilteevan Village',
		);
		expect(getByRole('status', { name: 'Parish status' })).toHaveTextContent(
			'Selected person: Roisin Connolly, shopkeeper, mood wary',
		);
		expect(container.querySelector('.input-wrapper')).toBeNull();
		expect(container.querySelector('.input-form')).toBeNull();
		expect(container.querySelector('[data-testid="chat-panel"]')).toBeNull();
		expect(queryByText(/^Send$/i)).toBeNull();

		await waitFor(() =>
			expect(lastRenderState?.selectedNpc?.name).toBe('Roisin Connolly'),
		);
	});

	it('keeps intent editable while exposing the streaming state', async () => {
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		expect(input.disabled).toBe(false);
		expect(input).not.toHaveAttribute('aria-disabled');
		expect(input).toHaveAttribute('aria-busy', 'false');

		streamingActive.set(true);
		await waitFor(() => expect(input).toHaveAttribute('aria-busy', 'true'));
		expect(input.disabled).toBe(false);
		expect(input).not.toHaveAttribute('aria-disabled');
	});

	it('flushes streaming on the first character and keeps that draft', async () => {
		const flush = vi.fn(() => {
			streamingActive.set(false);
			return 1;
		});
		flushStream.set(flush);
		streamingActive.set(true);
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await fireEvent.keyDown(input, { key: 'x' });
		input.value = 'x';
		await fireEvent.input(input);

		expect(flush).toHaveBeenCalledOnce();
		await waitFor(() => expect(lastRenderState?.command.text).toBe('x'));
		expect(mockSubmitInput).not.toHaveBeenCalled();
	});

	it('flushes but does not submit when Enter ends a stream', async () => {
		const flush = vi.fn(() => {
			streamingActive.set(false);
			return 1;
		});
		flushStream.set(flush);
		streamingActive.set(true);
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;
		input.value = 'ask Roisin';
		await fireEvent.input(input);

		await fireEvent.keyDown(input, { key: 'Enter' });

		expect(flush).toHaveBeenCalledOnce();
		expect(mockSubmitInput).not.toHaveBeenCalled();
		expect(input.value).toBe('ask Roisin');
	});

	it.each(['ArrowUp', 'ArrowDown'])(
		'flushes streaming without recalling command history on %s',
		async (key) => {
			const { getByLabelText } = render(IllustratedNotebookGame);
			const input = getByLabelText('Player intent') as HTMLInputElement;

			await fireEvent.input(input, { target: { value: 'look around' } });
			await fireEvent.keyDown(input, { key: 'Enter' });
			await waitFor(() => expect(input.value).toBe(''));

			await fireEvent.input(input, {
				target: { value: 'draft during stream' },
			});
			const flush = vi.fn(() => {
				streamingActive.set(false);
				return 1;
			});
			flushStream.set(flush);
			streamingActive.set(true);

			await fireEvent.keyDown(input, { key });

			expect(flush).toHaveBeenCalledOnce();
			expect(input.value).toBe('draft during stream');
			expect(mockSubmitInput).toHaveBeenCalledTimes(1);
		},
	);

	it('keeps the selected person stable as nearby people change', async () => {
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

	it('seeds and submits through the hidden command input', async () => {
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

	it('recalls successful notebook commands and restores the in-progress draft', async () => {
		const { getByLabelText } = render(IllustratedNotebookGame);
		const input = getByLabelText('Player intent') as HTMLInputElement;

		await fireEvent.input(input, { target: { value: 'look around' } });
		await fireEvent.keyDown(input, { key: 'Enter' });
		await waitFor(() => expect(input.value).toBe(''));

		await fireEvent.input(input, { target: { value: 'talk to Roisin' } });
		await fireEvent.keyDown(input, { key: 'Enter' });
		await waitFor(() => expect(input.value).toBe(''));

		await fireEvent.input(input, { target: { value: 'my new draft' } });
		await fireEvent.keyDown(input, { key: 'ArrowUp' });
		expect(input.value).toBe('talk to Roisin');
		await fireEvent.keyDown(input, { key: 'ArrowUp' });
		expect(input.value).toBe('look around');
		await fireEvent.keyDown(input, { key: 'ArrowDown' });
		expect(input.value).toBe('talk to Roisin');
		await fireEvent.keyDown(input, { key: 'ArrowDown' });
		expect(input.value).toBe('my new draft');

		expect(mockSubmitInput).toHaveBeenNthCalledWith(1, 'look around');
		expect(mockSubmitInput).toHaveBeenNthCalledWith(2, 'talk to Roisin');
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
		const status = getByRole('status', { name: 'Command status' });
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

	it('turns tabs in place while cards use the overlay coordinator', async () => {
		const { getByRole, getByTestId } = render(IllustratedNotebookGame);
		await waitFor(() => expect(lastRenderState).not.toBeNull());
		const section = getByTestId('notebook-active-section');
		expect(section).toHaveAttribute('data-section', 'notes');
		expect(
			getByRole('button', { name: 'Open Notes notebook tab' }),
		).toHaveAttribute('aria-pressed', 'true');

		lastRenderState?.callbacks.onOpenTab('places');
		await waitFor(() => {
			expect(lastRenderState?.activeTab).toBe('places');
			expect(section).toHaveAttribute('data-section', 'places');
			expect(section).toHaveTextContent('Places in this Parish');
			expect(section).toHaveTextContent('The Crossroads');
		});
		expect(get(notebookOverlay)).toBeNull();
		expect(get(fullMapOpen)).toBe(false);
		expect(
			getByRole('button', { name: 'Open Places notebook tab' }),
		).toHaveAttribute('aria-pressed', 'true');

		lastRenderState?.callbacks.onOpenSurface('time');
		await waitFor(() => expect(get(notebookOverlay)).toBe('time'));

		lastRenderState?.callbacks.onOpenSurface('map');
		await waitFor(() => {
			expect(get(notebookOverlay)).toBe('map');
			expect(get(fullMapOpen)).toBe(true);
		});
		closeNotebookOverlay('map');
	});

	it('keeps the same Pixi host mounted and inert while an overlay is open', async () => {
		const { getByTestId } = render(IllustratedNotebookGame);
		const game = getByTestId('illustrated-notebook-game');
		const host = getByTestId('illustrated-notebook-pixi-host');
		await waitFor(() => expect(rendererConstructCount).toBe(1));

		lastRenderState?.callbacks.onOpenSurface('time');
		await waitFor(() => expect(game.getAttribute('aria-hidden')).toBe('true'));
		expect(game.classList.contains('overlay-open')).toBe(true);
		expect(getByTestId('illustrated-notebook-pixi-host')).toBe(host);
		expect(rendererConstructCount).toBe(1);

		closeNotebookOverlay('time');
		await waitFor(() => expect(game.getAttribute('aria-hidden')).toBe('false'));
		expect(game.classList.contains('overlay-open')).toBe(false);
		expect(getByTestId('illustrated-notebook-pixi-host')).toBe(host);
		expect(rendererConstructCount).toBe(1);
	});

	it('exposes fresh renderer hit targets for focus and activation', async () => {
		const { getByLabelText } = render(IllustratedNotebookGame);
		await waitFor(() => expect(getByLabelText('Ask action')).toBeTruthy());

		const targetButton = getByLabelText('Ask action');
		await fireEvent.focus(targetButton);
		expect(lastRenderer?.setFocusedTarget).toHaveBeenCalledWith('action:ask');

		await fireEvent.click(targetButton);
		expect(lastRenderer?.activateTarget).toHaveBeenCalledWith('action:ask');

		await fireEvent.blur(targetButton);
		expect(lastRenderer?.setFocusedTarget).toHaveBeenCalledWith(null);
	});
});
