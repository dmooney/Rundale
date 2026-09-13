<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import DemoBanner from '../components/DemoBanner.svelte';
	import DemoPanel from '../components/DemoPanel.svelte';
	import SetupOverlay from '../components/SetupOverlay.svelte';
	import ChatGameShell from '../components/ChatGameShell.svelte';
	import SurfaceHost from '../components/SurfaceHost.svelte';

	import { demoVisible, demoEnabled } from '../stores/demo';
	import { stopDemo } from '../lib/demo-player';

	import { debugVisible, debugSnapshot } from '../stores/debug';
	import {
		activeSurface,
		closeSurface,
		surfaceTransitioning,
		toggleSurface,
	} from '../stores/surfaceCoordinator';
	import { cancelTravel } from '../stores/travel';
	import {
		getDebugSnapshot,
		saveScreenshot,
		disposeTransport,
		toggleFullscreen,
		isTauri,
	} from '$lib/ipc';
	import { captureScreen } from '$lib/screenshot';
	import { createPageController } from '$lib/page-controller';

	/** Transient toast text shown after a screenshot is saved (or fails). */
	let screenshotToast = $state<string | null>(null);
	let screenshotToastTimer: ReturnType<typeof setTimeout> | null = null;
	function flashScreenshotToast(message: string) {
		screenshotToast = message;
		if (screenshotToastTimer !== null) clearTimeout(screenshotToastTimer);
		screenshotToastTimer = setTimeout(() => {
			screenshotToast = null;
			screenshotToastTimer = null;
		}, 2500);
	}

	async function handleScreenshot() {
		try {
			const dataUrl = await captureScreen();
			const info = await saveScreenshot(dataUrl);
			flashScreenshotToast(`Screenshot saved: ${info.path}`);
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			flashScreenshotToast(`Screenshot failed: ${msg}`);
		}
	}

	/** True when the keystroke originated in a text-entry context where
	 *  single-letter shortcuts (M, ?) must not fire. */
	function isTypingContext(): boolean {
		const el = document.activeElement;
		return (
			el?.tagName === 'INPUT' ||
			el?.tagName === 'TEXTAREA' ||
			!!(el as HTMLElement | null)?.isContentEditable
		);
	}

	// F2 = capture screenshot, F5 toggle for save picker, F10 toggle for demo panel,
	// F11 toggle fullscreen (desktop), F12 toggle for debug panel, M toggle for map,
	// ? = keyboard-shortcuts overlay
	function handleKeydown(e: KeyboardEvent) {
		if ($surfaceTransitioning) {
			if (e.key === 'Escape') {
				e.preventDefault();
				closeSurface('bug');
			}
			return;
		}
		if (e.key === 'Escape' && get(demoEnabled)) {
			e.preventDefault();
			stopDemo();
			return;
		}
		if (e.key === 'F2') {
			e.preventDefault();
			void handleScreenshot();
		}
		if (e.key === 'F5') {
			e.preventDefault();
			void toggleSurface('save');
		}
		if (e.key === 'F10') {
			e.preventDefault();
			if ($activeSurface) return;
			demoVisible.update((v) => !v);
		}
		if (e.key === 'F11') {
			// Desktop: toggle the Tauri window's fullscreen. In the browser,
			// let the native F11 fullscreen behaviour stand.
			if (isTauri()) {
				e.preventDefault();
				toggleFullscreen().catch((err) =>
					console.warn('Fullscreen toggle failed:', err),
				);
			}
		}
		if (e.key === 'F12') {
			e.preventDefault();
			void toggleSurface('debug');
		}
		// Toggle full map with M key, but only when not typing in an input/textarea/contenteditable
		if ((e.key === 'm' || e.key === 'M') && !isTypingContext()) {
			e.preventDefault();
			void toggleSurface('map');
		}
		// `?` opens the shortcuts overlay (the overlay handles its own close)
		if (e.key === '?' && !isTypingContext()) {
			e.preventDefault();
			void toggleSurface('shortcuts');
		}
	}

	// Poll the debug snapshot while the debug panel is visible.
	//
	// The Tauri backend pushes `debug-update` events whenever state changes,
	// but the web server has no equivalent push channel for the snapshot —
	// so without polling, the panel only sees whatever was current at the
	// moment it was opened (e.g. an empty inference call_log). 1s polling
	// is cheap (the snapshot is just JSON over HTTP) and only runs while
	// the panel is actually visible.
	let debugPollHandle: ReturnType<typeof setInterval> | null = null;
	$effect(() => {
		if ($debugVisible) {
			debugPollHandle = setInterval(() => {
				getDebugSnapshot()
					.then((s) => debugSnapshot.set(s))
					.catch(() => {});
			}, 1000);
			return () => {
				if (debugPollHandle !== null) {
					clearInterval(debugPollHandle);
					debugPollHandle = null;
				}
			};
		}
	});

	let mountCleanup: (() => void) | null = null;
	let controllerReady = $state(false);
	// Disposed-before-mount-resolves flag for #348. setupMount is async
	// and onMount kicks it off in a detached IIFE, so a fast unmount
	// (HMR, navigate-away during initial fetch) can fire onDestroy
	// before mountCleanup is even assigned. Without this flag the
	// cleanup is silently dropped and every listener / timer
	// setupMount registers leaks indefinitely. We flip cancelled in
	// onDestroy and run cleanup() at the moment setupMount resolves
	// if the flag is set.
	let cancelled = false;
	onMount(() => {
		(async () => {
			const cleanup = await createPageController();
			if (cancelled) {
				cleanup();
			} else {
				mountCleanup = cleanup;
				controllerReady = true;
			}
		})();
	});
	onDestroy(() => {
		cancelled = true;
		mountCleanup?.();
		// Cancel any pending travel auto-clear so it doesn't fire
		// against a destroyed tree (#349).
		cancelTravel();
		// In browser mode, also tear down the shared WebSocket and any
		// pending reconnect timer so navigation away doesn't leave an
		// orphan socket or a zombie reconnect queued.
		disposeTransport();
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<div
	class="app-root"
	data-testid="app-root"
	data-controller-ready={controllerReady}
>
	<ChatGameShell />
	<SurfaceHost />
	{#if !$activeSurface && !$surfaceTransitioning}
		<DemoBanner />
		{#if $demoVisible}
			<DemoPanel />
		{/if}
	{/if}
	<SetupOverlay />

	{#if screenshotToast}
		<div class="screenshot-toast" role="status" aria-live="polite">
			{screenshotToast}
		</div>
	{/if}
</div>

<style>
	.app-root {
		position: relative;
		height: 100dvh;
		overflow: hidden;
	}

	/* ── Screenshot toast ── */
	.screenshot-toast {
		position: fixed;
		bottom: 1.5rem;
		left: 50%;
		transform: translateX(-50%);
		background: var(--color-panel-bg, rgba(20, 20, 20, 0.92));
		color: var(--color-text, #f4f4f4);
		border: 1px solid var(--color-border, #555);
		padding: 0.55rem 1rem;
		font-family: var(--font-display, sans-serif);
		font-size: 0.75rem;
		letter-spacing: 0.05em;
		border-radius: 4px;
		box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35);
		z-index: 1000;
		max-width: min(80vw, 50rem);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		pointer-events: none;
	}
</style>
