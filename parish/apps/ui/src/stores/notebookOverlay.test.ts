import { tick } from 'svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	bugReportContext,
	bugReportScreenshot,
	bugReportVisible,
	closeBugReport,
} from './bugReport';
import { debugVisible } from './debug';
import { fullMapOpen, uiConfig } from './game';
import { modSelectorVisible, savePickerVisible } from './save';
import {
	closeNotebookOverlay,
	notebookOverlay,
	notebookOverlayTransitioning,
	openNotebookOverlay,
	resetNotebookOverlayForTests,
} from './notebookOverlay';

const { captureScreen } = vi.hoisted(() => ({
	captureScreen: vi.fn(async () => 'data:image/png;base64,fresh'),
}));

vi.mock('$lib/ipc', () => ({
	getDebugSnapshot: vi.fn(async () => ({ request_id: 'debug-test' })),
}));

vi.mock('$lib/screenshot', () => ({
	captureScreen,
}));

describe('notebook overlay coordinator', () => {
	beforeEach(() => {
		captureScreen.mockReset();
		captureScreen.mockResolvedValue('data:image/png;base64,fresh');
		uiConfig.update((config) => ({ ...config, base_mod_required: false }));
		closeBugReport();
		resetNotebookOverlayForTests();
	});

	it('starts with no persistent secondary surface', () => {
		expect(get(notebookOverlay)).toBeNull();
		expect(get(fullMapOpen)).toBe(false);
		expect(get(savePickerVisible)).toBe(false);
		expect(get(debugVisible)).toBe(false);
		expect(get(modSelectorVisible)).toBe(false);
		expect(get(bugReportVisible)).toBe(false);
		expect(get(notebookOverlayTransitioning)).toBe(false);
	});

	it.each([
		'journal',
		'people',
		'focail',
		'map',
		'save',
		'debug',
		'mod',
	] as const)('routes the %s entry point', async (surface) => {
		expect(await openNotebookOverlay(surface, null)).toBe(true);
		expect(get(notebookOverlay)).toBe(surface);

		expect(get(fullMapOpen)).toBe(surface === 'map');
		expect(get(savePickerVisible)).toBe(surface === 'save');
		expect(get(debugVisible)).toBe(surface === 'debug');
		expect(get(modSelectorVisible)).toBe(surface === 'mod');
	});

	it('captures the clean viewport before routing Bug Report', async () => {
		await openNotebookOverlay('utility', null);
		expect(await openNotebookOverlay('bug', null)).toBe(true);

		expect(get(notebookOverlay)).toBe('bug');
		expect(get(bugReportVisible)).toBe(true);
		expect(get(notebookOverlayTransitioning)).toBe(false);
	});

	it('lets a newer route win over a delayed Bug capture', async () => {
		let releaseCapture!: (value: string) => void;
		captureScreen.mockImplementationOnce(
			() =>
				new Promise<string>((resolve) => {
					releaseCapture = resolve;
				}),
		);
		const openingBug = openNotebookOverlay('bug', null);
		await vi.waitFor(() => expect(captureScreen).toHaveBeenCalledOnce());
		expect(get(notebookOverlayTransitioning)).toBe(true);

		expect(await openNotebookOverlay('map', null)).toBe(true);
		releaseCapture('data:image/png;base64=late');
		expect(await openingBug).toBe(false);

		expect(get(notebookOverlay)).toBe('map');
		expect(get(fullMapOpen)).toBe(true);
		expect(get(bugReportVisible)).toBe(false);
		expect(get(notebookOverlayTransitioning)).toBe(false);
	});

	it.each(['older-first', 'newer-first'] as const)(
		'keeps the newer Bug capture when concurrent captures finish %s',
		async (completionOrder) => {
			const releases: Array<(value: string) => void> = [];
			captureScreen.mockImplementation(
				() =>
					new Promise<string>((resolve) => {
						releases.push(resolve);
					}),
			);
			const olderContext = {
				kind: 'event',
				label: 'older report',
				detail: { revision: 1 },
			};
			const newerContext = {
				kind: 'event',
				label: 'newer report',
				detail: { revision: 2 },
			};

			const olderOpen = openNotebookOverlay('bug', null, olderContext);
			await vi.waitFor(() => expect(captureScreen).toHaveBeenCalledTimes(1));
			const newerOpen = openNotebookOverlay('bug', null, newerContext);
			await vi.waitFor(() => expect(captureScreen).toHaveBeenCalledTimes(2));

			if (completionOrder === 'older-first') {
				releases[0]?.('data:image/png;base64,older');
				expect(await olderOpen).toBe(false);
				expect(get(bugReportVisible)).toBe(false);
				releases[1]?.('data:image/png;base64,newer');
				expect(await newerOpen).toBe(true);
			} else {
				releases[1]?.('data:image/png;base64,newer');
				expect(await newerOpen).toBe(true);
				releases[0]?.('data:image/png;base64,older');
				expect(await olderOpen).toBe(false);
			}

			expect(get(notebookOverlay)).toBe('bug');
			expect(get(bugReportVisible)).toBe(true);
			expect(get(bugReportContext)).toEqual(newerContext);
			expect(get(bugReportScreenshot)).toBe('data:image/png;base64,newer');
			expect(get(notebookOverlayTransitioning)).toBe(false);
		},
	);

	it('keeps legacy surfaces mutually exclusive', async () => {
		await openNotebookOverlay('map', null);
		await openNotebookOverlay('save', null);

		expect(get(notebookOverlay)).toBe('save');
		expect(get(fullMapOpen)).toBe(false);
		expect(get(savePickerVisible)).toBe(true);
	});

	it('cannot replace or dismiss a required Mod selector', async () => {
		uiConfig.update((config) => ({ ...config, base_mod_required: true }));
		await openNotebookOverlay('mod', null);

		expect(await openNotebookOverlay('map', null)).toBe(false);
		expect(closeNotebookOverlay('mod')).toBe(false);
		expect(get(notebookOverlay)).toBe('mod');
		expect(get(modSelectorVisible)).toBe(true);
	});

	it('restores focus to the invoking notebook control on close', async () => {
		const button = document.createElement('button');
		document.body.appendChild(button);
		button.focus();

		await openNotebookOverlay('people', button);
		closeNotebookOverlay('people');
		await tick();

		expect(document.activeElement).toBe(button);
		button.remove();
	});

	it('keeps the original viewport focus target across nested routing', async () => {
		const viewportButton = document.createElement('button');
		const nestedButton = document.createElement('button');
		document.body.append(viewportButton, nestedButton);

		await openNotebookOverlay('utility', viewportButton);
		await openNotebookOverlay('focail', nestedButton);
		closeNotebookOverlay('focail');
		await tick();

		expect(document.activeElement).toBe(viewportButton);
		viewportButton.remove();
		nestedButton.remove();
	});

	it('falls back to Player intent when body was active on open', async () => {
		const input = document.createElement('input');
		input.setAttribute('aria-label', 'Player intent');
		document.body.appendChild(input);
		document.body.tabIndex = -1;
		document.body.focus();

		await openNotebookOverlay('people');
		closeNotebookOverlay('people');
		await tick();

		expect(document.activeElement).toBe(input);
		input.remove();
		document.body.removeAttribute('tabindex');
	});
});
