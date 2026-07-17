import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { npcsHere, uiConfig, worldState } from '../../stores/game';
import {
	closeNotebookOverlay,
	notebookOverlay,
	notebookPersonSelection,
	openNotebookOverlay,
	resetNotebookOverlayForTests,
} from '../../stores/notebookOverlay';
import NotebookOverlayHost from './NotebookOverlayHost.svelte';

vi.mock('$lib/screenshot', () => ({
	captureScreen: vi.fn(async () => 'data:image/png;base64,fresh-notebook'),
}));

describe('NotebookOverlayHost', () => {
	beforeEach(() => {
		resetNotebookOverlayForTests();
		uiConfig.update((config) => ({ ...config, base_mod_required: false }));
		npcsHere.set([
			{
				npc_id: 4,
				name: 'Roisin Connolly',
				real_name: 'Roisin Connolly',
				occupation: 'shopkeeper',
				mood: 'wary',
				introduced: true,
				mood_emoji: '•',
			},
		]);
		worldState.set({
			location_name: 'Kilteevan Village',
			location_description: 'A whitewashed village by the bridge.',
			time_label: 'Afternoon',
			hour: 15,
			minute: 40,
			weather: 'clearing',
			season: 'Spring',
			festival: 'Bealtaine',
			paused: true,
			inference_paused: true,
			game_epoch_ms: Date.UTC(1820, 3, 1, 15, 40),
			speed_factor: 36,
			name_hints: [],
			day_of_week: 'Monday',
		});
	});

	it('renders no persistent overlay by default', () => {
		const { queryByTestId } = render(NotebookOverlayHost);
		expect(queryByTestId('notebook-overlay-backdrop')).toBeNull();
	});

	it('presents all notebook utility routes without dashboard chrome', async () => {
		const { getByRole, getByTestId } = render(NotebookOverlayHost);
		await openNotebookOverlay('utility', null);

		await waitFor(() =>
			expect(getByTestId('notebook-overlay-utility')).toBeTruthy(),
		);
		for (const name of [
			'Focail',
			'Save / Load',
			'Debug',
			'Mod',
			'Bug Report',
			'Shortcuts',
		]) {
			expect(
				getByRole('button', { name: new RegExp(`^${name}`) }),
			).toBeTruthy();
		}
	});

	it.each([
		['Focail', 'focail'],
		['Save / Load', 'save'],
		['Debug', 'debug'],
		['Mod', 'mod'],
		['Bug Report', 'bug'],
		['Shortcuts', 'shortcuts'],
	] as const)('routes the %s utility button to %s', async (name, surface) => {
		const { getByRole } = render(NotebookOverlayHost);
		await openNotebookOverlay('utility', null);
		const button = await waitFor(() =>
			getByRole('button', { name: new RegExp(`^${name}`) }),
		);

		await fireEvent.click(button);

		await waitFor(() => expect(get(notebookOverlay)).toBe(surface));
	});

	it('shows paused, inference, and festival state in the Time sheet', async () => {
		const { getByRole } = render(NotebookOverlayHost);
		await openNotebookOverlay('time', null);

		const timeSheet = await waitFor(() =>
			getByRole('dialog', { name: 'Time & Weather' }),
		);
		expect(timeSheet).toHaveTextContent('Clock statepaused');
		expect(timeSheet).toHaveTextContent('Parish repliespaused');
		expect(timeSheet).toHaveTextContent('FestivalBealtaine');
	});

	it('routes People selection back to the illustrated parish', async () => {
		const { getByRole } = render(NotebookOverlayHost);
		await openNotebookOverlay('people', null);
		const person = await waitFor(() =>
			getByRole('button', { name: 'Roisin Connolly shopkeeper · wary' }),
		);
		await fireEvent.click(person);

		expect(get(notebookPersonSelection)).toBe('Roisin Connolly');
		expect(get(notebookOverlay)).toBeNull();
	});

	it('closes notebook-native drawers with Escape', async () => {
		const { getByTestId } = render(NotebookOverlayHost);
		await openNotebookOverlay('time', null);
		await waitFor(() =>
			expect(getByTestId('notebook-overlay-time')).toBeTruthy(),
		);

		await fireEvent.keyDown(window, { key: 'Escape' });
		expect(get(notebookOverlay)).toBeNull();
	});

	it('dismisses a notebook sheet from the clear backdrop area', async () => {
		const { getByTestId } = render(NotebookOverlayHost);
		await openNotebookOverlay('people', null);
		const dismiss = await waitFor(() =>
			getByTestId('notebook-overlay-dismiss'),
		);
		expect(dismiss.getAttribute('aria-hidden')).toBe('true');
		expect(dismiss.getAttribute('tabindex')).toBe('-1');

		await fireEvent.click(dismiss);

		expect(get(notebookOverlay)).toBeNull();
	});

	it('exposes a notebook-styled wrapper around routed content', async () => {
		const { getByTestId } = render(NotebookOverlayHost);
		await openNotebookOverlay('rumours', null);
		const frame = await waitFor(() => getByTestId('notebook-overlay-rumours'));

		expect(frame.classList.contains('notebook-overlay-frame')).toBe(true);
		expect(frame.getAttribute('data-surface')).toBe('rumours');
		closeNotebookOverlay('rumours');
	});

	it('contains a reused Journal interior inside the notebook sheet', async () => {
		const { getByTestId } = render(NotebookOverlayHost);
		await openNotebookOverlay('journal', null);

		const frame = await waitFor(() => getByTestId('notebook-overlay-journal'));
		expect(frame.querySelector('[data-testid="chat-panel"]')).toBeTruthy();
		expect(frame.classList.contains('legacy-shell')).toBe(true);
	});

	it('leaves child-owned modals with one dialog and one visible header', async () => {
		const { getAllByRole, getByRole, getByTestId, queryByRole } =
			render(NotebookOverlayHost);
		await openNotebookOverlay('shortcuts', null);

		await waitFor(() =>
			expect(getByTestId('notebook-overlay-shortcuts')).toBeTruthy(),
		);
		expect(getAllByRole('dialog')).toHaveLength(1);
		const close = getByRole('button', { name: 'Close shortcuts' });
		expect(close).toBeTruthy();
		await waitFor(() => expect(document.activeElement).toBe(close));
		expect(
			queryByRole('button', { name: 'Close Notebook Shortcuts' }),
		).toBeNull();
	});

	it('does not include the hidden Focail header control in its focus loop', async () => {
		const { getByRole } = render(NotebookOverlayHost);
		await openNotebookOverlay('focail', null);
		const close = await waitFor(() =>
			getByRole('button', { name: 'Close Focail — Irish Words' }),
		);
		await waitFor(() => expect(document.activeElement).toBe(close));

		await fireEvent.keyDown(window, { key: 'Tab' });
		expect(document.activeElement).toBe(close);
	});

	it('recovers the focus loop when focus is on the frame or outside it', async () => {
		const { getByRole } = render(NotebookOverlayHost);
		await openNotebookOverlay('people', null);
		const sheet = await waitFor(() =>
			getByRole('dialog', { name: 'People of the Parish' }),
		);
		const close = getByRole('button', { name: 'Close People of the Parish' });
		const person = getByRole('button', {
			name: 'Roisin Connolly shopkeeper · wary',
		});

		sheet.focus();
		await fireEvent.keyDown(window, { key: 'Tab' });
		expect(document.activeElement).toBe(close);

		document.body.focus();
		await fireEvent.keyDown(window, { key: 'Tab', shiftKey: true });
		expect(document.activeElement).toBe(person);
	});
});
