import { tick } from 'svelte';
import { get, writable } from 'svelte/store';
import type { BugContext } from '$lib/types';
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

export type Surface = 'map' | 'save' | 'debug' | 'mod' | 'bug' | 'shortcuts';

export const activeSurface = writable<Surface | null>(null);
export const surfaceTransitioning = writable(false);

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
		get(activeSurface) === 'mod' && Boolean(get(uiConfig)?.base_mod_required)
	);
}

function hideLegacySurfaces(except: Surface | null = null): void {
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

export async function openSurface(
	surface: Surface,
	invoker: HTMLElement | null = defaultInvoker(),
	bugContext?: BugContext,
): Promise<boolean> {
	if (requiredModIsOpen() && surface !== 'mod') return false;
	const revision = ++routeRevision;
	// A route opened from inside another surface replaces that surface.
	// Keep the original viewport control as the restoration target because the
	// nested invoker is about to be unmounted.
	if (invoker && get(activeSurface) === null) restoreFocusTarget = invoker;

	if (surface === 'bug') {
		surfaceTransitioning.set(true);
		activeSurface.set(null);
		hideLegacySurfaces();
		try {
			await settleViewport();
			if (revision !== routeRevision) return false;
			const preparedReport = await prepareBugReport(bugContext);
			if (revision !== routeRevision) return false;
			showPreparedBugReport(preparedReport);
			activeSurface.set('bug');
			return true;
		} finally {
			if (revision === routeRevision) {
				surfaceTransitioning.set(false);
			}
		}
	}

	surfaceTransitioning.set(false);
	hideLegacySurfaces(surface);
	activeSurface.set(surface);
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

export async function toggleSurface(
	surface: Surface,
	invoker?: HTMLElement | null,
): Promise<boolean> {
	if (get(activeSurface) === surface) return closeSurface(surface);
	return openSurface(surface, invoker);
}

export function closeSurface(
	expected?: Surface,
	options: { force?: boolean; restoreFocus?: boolean } = {},
): boolean {
	const current = get(activeSurface);
	if (!current) {
		if (get(surfaceTransitioning) && (!expected || expected === 'bug')) {
			routeRevision += 1;
			surfaceTransitioning.set(false);
			hideLegacySurfaces();
			restoreViewportFocus(options.restoreFocus !== false);
			return true;
		}
		return false;
	}
	if (expected && current !== expected) return false;
	if (current === 'mod' && requiredModIsOpen() && !options.force) return false;

	routeRevision += 1;
	surfaceTransitioning.set(false);
	hideLegacySurfaces();
	activeSurface.set(null);
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
			.querySelector<HTMLElement>(
				'[data-testid="input-field"], [aria-label="Player input"]',
			)
			?.focus({ preventScroll: true });
	});
}

/**
 * Keeps direct legacy close actions (for example SavePicker finishing a load)
 * synchronized with the active presentation surface.
 */
export function legacySurfaceClosed(surface: Surface): void {
	if (get(activeSurface) !== surface) return;
	if (surface === 'mod' && requiredModIsOpen()) return;
	closeSurface(surface);
}

export function adoptLegacySurface(surface: Surface): void {
	if (requiredModIsOpen() && surface !== 'mod') return;
	routeRevision += 1;
	surfaceTransitioning.set(false);
	hideLegacySurfaces(surface);
	activeSurface.set(surface);
}

export function resetSurfaceCoordinatorForTests(): void {
	routeRevision += 1;
	surfaceTransitioning.set(false);
	restoreFocusTarget = null;
	hideLegacySurfaces();
	activeSurface.set(null);
}
