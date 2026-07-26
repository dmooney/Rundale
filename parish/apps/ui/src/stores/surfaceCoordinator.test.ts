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
	activeSurface,
	closeSurface,
	openSurface,
	resetSurfaceCoordinatorForTests,
	surfaceTransitioning,
} from './surfaceCoordinator';

const { captureScreen } = vi.hoisted(() => ({
	captureScreen: vi.fn(async () => 'data:image/png;base64,fresh'),
}));

vi.mock('$lib/ipc', () => ({
	getDebugSnapshot: vi.fn(async () => ({ request_id: 'debug-test' })),
}));

vi.mock('$lib/screenshot', () => ({
	captureScreen,
}));

describe('surface coordinator', () => {
	beforeEach(() => {
		captureScreen.mockReset();
		captureScreen.mockResolvedValue('data:image/png;base64,fresh');
		uiConfig.update((config) => ({ ...config, base_mod_required: false }));
		closeBugReport();
		resetSurfaceCoordinatorForTests();
	});

	it('starts with no persistent secondary surface', () => {
		expect(get(activeSurface)).toBeNull();
		expect(get(fullMapOpen)).toBe(false);
		expect(get(savePickerVisible)).toBe(false);
		expect(get(debugVisible)).toBe(false);
		expect(get(modSelectorVisible)).toBe(false);
		expect(get(bugReportVisible)).toBe(false);
		expect(get(surfaceTransitioning)).toBe(false);
	});

	it.each(['map', 'save', 'debug', 'mod'] as const)(
		'routes the %s entry point',
		async (surface) => {
			expect(await openSurface(surface, null)).toBe(true);
			expect(get(activeSurface)).toBe(surface);

			expect(get(fullMapOpen)).toBe(surface === 'map');
			expect(get(savePickerVisible)).toBe(surface === 'save');
			expect(get(debugVisible)).toBe(surface === 'debug');
			expect(get(modSelectorVisible)).toBe(surface === 'mod');
		},
	);

	it('captures the clean viewport before routing Bug Report', async () => {
		await openSurface('map', null);
		expect(await openSurface('bug', null)).toBe(true);

		expect(get(activeSurface)).toBe('bug');
		expect(get(bugReportVisible)).toBe(true);
		expect(get(surfaceTransitioning)).toBe(false);
	});

	it('lets a newer route win over a delayed Bug capture', async () => {
		let releaseCapture!: (value: string) => void;
		captureScreen.mockImplementationOnce(
			() =>
				new Promise<string>((resolve) => {
					releaseCapture = resolve;
				}),
		);
		const openingBug = openSurface('bug', null);
		await vi.waitFor(() => expect(captureScreen).toHaveBeenCalledOnce());
		expect(get(surfaceTransitioning)).toBe(true);

		expect(await openSurface('map', null)).toBe(true);
		releaseCapture('data:image/png;base64=late');
		expect(await openingBug).toBe(false);

		expect(get(activeSurface)).toBe('map');
		expect(get(fullMapOpen)).toBe(true);
		expect(get(bugReportVisible)).toBe(false);
		expect(get(surfaceTransitioning)).toBe(false);
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

			const olderOpen = openSurface('bug', null, olderContext);
			await vi.waitFor(() => expect(captureScreen).toHaveBeenCalledTimes(1));
			const newerOpen = openSurface('bug', null, newerContext);
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

			expect(get(activeSurface)).toBe('bug');
			expect(get(bugReportVisible)).toBe(true);
			expect(get(bugReportContext)).toEqual(newerContext);
			expect(get(bugReportScreenshot)).toBe('data:image/png;base64,newer');
			expect(get(surfaceTransitioning)).toBe(false);
		},
	);

	it('keeps legacy surfaces mutually exclusive', async () => {
		await openSurface('map', null);
		await openSurface('save', null);

		expect(get(activeSurface)).toBe('save');
		expect(get(fullMapOpen)).toBe(false);
		expect(get(savePickerVisible)).toBe(true);
	});

	it('cannot replace or dismiss a required Mod selector', async () => {
		uiConfig.update((config) => ({ ...config, base_mod_required: true }));
		await openSurface('mod', null);

		expect(await openSurface('map', null)).toBe(false);
		expect(closeSurface('mod')).toBe(false);
		expect(get(activeSurface)).toBe('mod');
		expect(get(modSelectorVisible)).toBe(true);
	});

	it('restores focus to the invoking control on close', async () => {
		const button = document.createElement('button');
		document.body.appendChild(button);
		button.focus();

		await openSurface('map', button);
		closeSurface('map');
		await tick();

		expect(document.activeElement).toBe(button);
		button.remove();
	});

	it('keeps the original shell focus target across nested routing', async () => {
		const viewportButton = document.createElement('button');
		const nestedButton = document.createElement('button');
		document.body.append(viewportButton, nestedButton);

		await openSurface('map', viewportButton);
		await openSurface('save', nestedButton);
		closeSurface('save');
		await tick();

		expect(document.activeElement).toBe(viewportButton);
		viewportButton.remove();
		nestedButton.remove();
	});

	it('falls back to Player intent when body was active on open', async () => {
		const input = document.createElement('input');
		input.setAttribute('aria-label', 'Player input');
		document.body.appendChild(input);
		document.body.tabIndex = -1;
		document.body.focus();

		await openSurface('map');
		closeSurface('map');
		await tick();

		expect(document.activeElement).toBe(input);
		input.remove();
		document.body.removeAttribute('tabindex');
	});
});
