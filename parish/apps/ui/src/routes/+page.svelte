<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import StatusBar from '../components/StatusBar.svelte';
	import ChatPanel from '../components/ChatPanel.svelte';
	import MapPanel from '../components/MapPanel.svelte';
	import FullMapOverlay from '../components/FullMapOverlay.svelte';
	import Sidebar from '../components/Sidebar.svelte';
	import InputField from '../components/InputField.svelte';
	import DebugPanel from '../components/DebugPanel.svelte';
	import DemoBanner from '../components/DemoBanner.svelte';
	import DemoPanel from '../components/DemoPanel.svelte';
	import SavePicker from '../components/SavePicker.svelte';
	import BugReportModal from '../components/BugReportModal.svelte';
	import SetupOverlay from '../components/SetupOverlay.svelte';
	import ModSelectorOverlay from '../components/ModSelectorOverlay.svelte';

	import { uiConfig, fullMapOpen, focailOpen, syncFocailOnViewportChange } from '../stores/game';
	import { demoVisible, demoEnabled } from '../stores/demo';
	import { stopDemo } from '../lib/demo-player';

	/** True on narrow viewports (<=768px). Desktop ignores focailOpen; on
	 * mobile the chat column becomes the Focail panel when that store
	 * is true. Fix for #355: without this gate both columns render the
	 * same Sidebar side-by-side on desktop. */
	let isMobile = $state(false);
	import { debugVisible, debugSnapshot, debugDockLeft } from '../stores/debug';
	import { savePickerVisible, modSelectorVisible } from '../stores/save';
	import { cancelTravel } from '../stores/travel';
	import {
		getDebugSnapshot,
		saveScreenshot,
		disposeTransport,
		toggleFullscreen,
		isTauri
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

	// F2 = capture screenshot, F5 toggle for save picker, F10 toggle for demo panel,
	// F11 toggle fullscreen (desktop), F12 toggle for debug panel, M toggle for map
	function handleKeydown(e: KeyboardEvent) {
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
			savePickerVisible.update((v) => !v);
		}
		if (e.key === 'F10') {
			e.preventDefault();
			demoVisible.update((v) => !v);
		}
		if (e.key === 'F11') {
			// Desktop: toggle the Tauri window's fullscreen. In the browser,
			// let the native F11 fullscreen behaviour stand.
			if (isTauri()) {
				e.preventDefault();
				toggleFullscreen().catch((err) => console.warn('Fullscreen toggle failed:', err));
			}
		}
		if (e.key === 'F12') {
			e.preventDefault();
			const nowVisible = !get(debugVisible);
			debugVisible.set(nowVisible);
			// Fetch initial snapshot when opening
			if (nowVisible) {
				getDebugSnapshot()
					.then((s) => debugSnapshot.set(s))
					.catch(() => {});
			}
		}
		// Toggle full map with M key, but only when not typing in an input/textarea/contenteditable
		if ((e.key === 'm' || e.key === 'M') && document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA' && !(document.activeElement as HTMLElement)?.isContentEditable) {
			e.preventDefault();
			fullMapOpen.update((v) => !v);
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
	let mobileMediaCleanup: (() => void) | null = null;
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
			}
		})();
		// Track the narrow-viewport media query live so a user who
		// resizes from mobile to desktop while focailOpen is true
		// doesn't end up with two Sidebars stacked in the chat column
		// and the right column (#355).
		if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
			const mq = window.matchMedia('(max-width: 768px)');
			isMobile = mq.matches;
			const onChange = (e: MediaQueryListEvent) => {
				isMobile = e.matches;
				// When transitioning from mobile→desktop, close the focail overlay so
				// the store doesn't stay true while the mobile Sidebar branch is hidden
				// (the desktop right-col always renders its own Sidebar unconditionally).
				syncFocailOnViewportChange(e.matches);
			};
			mq.addEventListener('change', onChange);
			mobileMediaCleanup = () => mq.removeEventListener('change', onChange);
		}
	});
	onDestroy(() => {
		cancelled = true;
		mountCleanup?.();
		mobileMediaCleanup?.();
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
	class="app-shell"
	class:debug-open-bottom={$debugVisible && !$debugDockLeft}
	class:debug-open-left={$debugVisible && $debugDockLeft}
>
	<StatusBar />

	<!-- Mobile-only toggle toolbar -->
	<div class="mobile-toolbar">
		<button
			type="button"
			class="mobile-btn"
			class:active={$fullMapOpen}
			aria-pressed={$fullMapOpen}
			aria-label="Toggle full map"
			onclick={() => {
				if ($fullMapOpen) {
					fullMapOpen.set(false);
				} else {
					focailOpen.set(false);
					fullMapOpen.set(true);
				}
			}}
		>Map</button>
		<button
			type="button"
			class="mobile-btn"
			class:active={$focailOpen}
			aria-pressed={$focailOpen}
			aria-label="Language Hints — toggle Irish words panel"
			onclick={() => {
				if ($focailOpen) {
					focailOpen.set(false);
				} else {
					fullMapOpen.set(false);
					focailOpen.set(true);
				}
			}}
		>Language Hints</button>
	</div>

	<div class="main-area">
		<div class="chat-col">
			{#if $focailOpen && isMobile}
				<Sidebar onclose={() => focailOpen.set(false)} />
			{:else}
				<ChatPanel />
				<InputField />
			{/if}
		</div>
		<div class="right-col">
			<MapPanel />
			<Sidebar />
		</div>
		{#if $fullMapOpen}
			<FullMapOverlay onclose={() => fullMapOpen.set(false)} />
		{/if}
	</div>

</div>

<DebugPanel />
<DemoBanner />
{#if $demoVisible}
	<DemoPanel />
{/if}
<SavePicker />
<BugReportModal />
<SetupOverlay />
{#if $modSelectorVisible}
	<ModSelectorOverlay onclose={() => modSelectorVisible.set(false)} required={$uiConfig?.base_mod_required} />
{/if}

{#if screenshotToast}
	<div class="screenshot-toast" role="status" aria-live="polite">{screenshotToast}</div>
{/if}

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100dvh;
		overflow: hidden;
		transition: height 0.15s ease;
		padding-bottom: env(safe-area-inset-bottom);
	}

	.app-shell.debug-open-bottom {
		height: 60vh;
	}

	@media (min-width: 1200px) {
		.app-shell.debug-open-left {
			margin-left: min(28rem, 36vw);
			width: calc(100vw - min(28rem, 36vw));
		}
	}

	.main-area {
		flex: 1;
		display: grid;
		grid-template-columns: 1fr 220px;
		overflow: hidden;
		min-height: 0;
		position: relative;
	}

	.chat-col {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
		position: relative;
	}

	.right-col {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	/* ── Mobile toolbar ── */
	.mobile-toolbar {
		display: none;
	}

	@media (max-width: 768px) {
		.main-area {
			grid-template-columns: 1fr;
		}

		/* Hide the desktop right column entirely on mobile */
		.right-col {
			display: none;
		}

		.mobile-toolbar {
			display: flex;
			gap: 0.5rem;
			padding: 0.35rem 0.75rem;
			background: var(--color-panel-bg);
			border-bottom: 1px solid var(--color-border);
			position: sticky;
			top: 0;
			z-index: 29;
		}

		.mobile-btn {
			background: none;
			border: 1px solid var(--color-border);
			color: var(--color-muted);
			font-family: var(--font-display);
			font-size: 0.65rem;
			letter-spacing: 0.1em;
			padding: 0.25rem 0.6rem;
			cursor: pointer;
			transition: color 0.2s, border-color 0.2s;
		}

		.mobile-btn:hover,
		.mobile-btn:focus-visible,
		.mobile-btn.active {
			color: var(--color-accent);
			border-color: var(--color-accent);
		}

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
