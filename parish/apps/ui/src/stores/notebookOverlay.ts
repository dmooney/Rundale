import { tick } from 'svelte';
import { get, writable } from 'svelte/store';
import type { BugContext } from '$lib/types';
import type { NotebookSurface } from '$lib/illustrated-parish/types';
import { getDebugSnapshot } from '$lib/ipc';
import {
	bugReportVisible,
	closeBugReport,
	prepareBugReport,
	showPreparedBugReport,
} from './bugReport';
import { debugSnapshot, debugVisible } from './debug';
import { fullMapOpen, uiConfig } from './game';
import { modSelectorVisible, savePickerVisible } from './save';

export const notebookOverlay = writable<NotebookSurface | null>(null);
export const notebookOverlayTransitioning = writable(false);
export const notebookPersonSelection = writable<string | null>(null);

let restoreFocusTarget: HTMLElement | null = null;
let routeRevision = 0;

function defaultInvoker(): HTMLElement | null {
	if (typeof document === 'undefined') return null;
	const active = document.activeElement;
	if (
		!(active instanceof HTMLElement) ||
		active === document.body ||
		active === document.documentElement
	) {
		return null;
	}
	return active;
}

function requiredModIsOpen(): boolean {
	return (
		get(notebookOverlay) === 'mod' && Boolean(get(uiConfig)?.base_mod_required)
	);
}

function hideLegacySurfaces(except: NotebookSurface | null = null): void {
	if (except !== 'map') fullMapOpen.set(false);
	if (except !== 'save') savePickerVisible.set(false);
	if (except !== 'debug') debugVisible.set(false);
	if (except !== 'mod') modSelectorVisible.set(false);
	if (except !== 'bug' && get(bugReportVisible)) closeBugReport();
}

async function settleViewport(): Promise<void> {
	await tick();
	if (typeof window === 'undefined') return;
	await new Promise<void>((resolve) =>
		window.requestAnimationFrame(() => resolve()),
	);
}

export async function openNotebookOverlay(
	surface: NotebookSurface,
	invoker: HTMLElement | null = defaultInvoker(),
	bugContext?: BugContext,
): Promise<boolean> {
	if (requiredModIsOpen() && surface !== 'mod') return false;
	const revision = ++routeRevision;
	// A route opened from inside another notebook sheet replaces that sheet.
	// Keep the original viewport control as the restoration target because the
	// nested invoker is about to be unmounted.
	if (invoker && get(notebookOverlay) === null) restoreFocusTarget = invoker;

	if (surface === 'bug') {
		notebookOverlayTransitioning.set(true);
		notebookOverlay.set(null);
		hideLegacySurfaces();
		try {
			await settleViewport();
			if (revision !== routeRevision) return false;
			const preparedReport = await prepareBugReport(bugContext);
			if (revision !== routeRevision) return false;
			showPreparedBugReport(preparedReport);
			notebookOverlay.set('bug');
			return true;
		} finally {
			if (revision === routeRevision) {
				notebookOverlayTransitioning.set(false);
			}
		}
	}

	notebookOverlayTransitioning.set(false);
	hideLegacySurfaces(surface);
	notebookOverlay.set(surface);
	switch (surface) {
		case 'map':
			fullMapOpen.set(true);
			break;
		case 'save':
			savePickerVisible.set(true);
			break;
		case 'debug':
			debugVisible.set(true);
			void getDebugSnapshot()
				.then((snapshot) => debugSnapshot.set(snapshot))
				.catch(() => {});
			break;
		case 'mod':
			modSelectorVisible.set(true);
			break;
	}
	return true;
}

export async function toggleNotebookOverlay(
	surface: NotebookSurface,
	invoker?: HTMLElement | null,
): Promise<boolean> {
	if (get(notebookOverlay) === surface) return closeNotebookOverlay(surface);
	return openNotebookOverlay(surface, invoker);
}

export function closeNotebookOverlay(
	expected?: NotebookSurface,
	options: { force?: boolean; restoreFocus?: boolean } = {},
): boolean {
	const current = get(notebookOverlay);
	if (!current) {
		if (
			get(notebookOverlayTransitioning) &&
			(!expected || expected === 'bug')
		) {
			routeRevision += 1;
			notebookOverlayTransitioning.set(false);
			hideLegacySurfaces();
			restoreViewportFocus(options.restoreFocus !== false);
			return true;
		}
		return false;
	}
	if (expected && current !== expected) return false;
	if (current === 'mod' && requiredModIsOpen() && !options.force) return false;

	routeRevision += 1;
	notebookOverlayTransitioning.set(false);
	hideLegacySurfaces();
	notebookOverlay.set(null);
	restoreViewportFocus(options.restoreFocus !== false);
	return true;
}

function restoreViewportFocus(shouldRestore: boolean): void {
	const target = restoreFocusTarget;
	restoreFocusTarget = null;
	if (!shouldRestore) return;
	void tick().then(() => {
		if (target?.isConnected) {
			target.focus({ preventScroll: true });
			if (document.activeElement === target) return;
		}
		document
			.querySelector<HTMLElement>('[aria-label="Player intent"]')
			?.focus({ preventScroll: true });
	});
}

/**
 * Keeps direct legacy close actions (for example SavePicker finishing a load)
 * synchronized with the notebook frame without remounting the game viewport.
 */
export function legacyNotebookSurfaceClosed(surface: NotebookSurface): void {
	if (get(notebookOverlay) !== surface) return;
	if (surface === 'mod' && requiredModIsOpen()) return;
	closeNotebookOverlay(surface);
}

export function adoptLegacyNotebookSurface(surface: NotebookSurface): void {
	if (requiredModIsOpen() && surface !== 'mod') return;
	routeRevision += 1;
	notebookOverlayTransitioning.set(false);
	hideLegacySurfaces(surface);
	notebookOverlay.set(surface);
}

export function resetNotebookOverlayForTests(): void {
	routeRevision += 1;
	notebookOverlayTransitioning.set(false);
	restoreFocusTarget = null;
	hideLegacySurfaces();
	notebookOverlay.set(null);
	notebookPersonSelection.set(null);
}
