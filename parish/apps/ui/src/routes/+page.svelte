<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { goto } from '$app/navigation';
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

	import { worldState, mapData, npcsHere, textLog, streamingActive, loadingPhrase, loadingColor, languageHints, nameHints, uiConfig, fullMapOpen, focailOpen, addReaction, trimTextLog, messageHints, pushErrorLog, formatIpcError, syncFocailOnViewportChange } from '../stores/game';
	import { demoVisible, demoEnabled, demoConfig } from '../stores/demo';
	import { startDemoLoop, stopDemo } from '../lib/demo-player';
	import { SceneDeduplicator } from '../lib/scene-dedup';

	/** Which mobile-only panel is open (if any). Desktop ignores this. */
	let mobilePanel = $state<'none' | 'map' | 'sidebar'>('none');
	/** True on narrow viewports (<=768px). Desktop ignores focailOpen; on
	 * mobile the chat column becomes the Focail panel when that store
	 * is true. Fix for #355: without this gate both columns render the
	 * same Sidebar side-by-side on desktop. */
	let isMobile = $state(false);
	import { debugVisible, debugSnapshot, debugDockLeft } from '../stores/debug';
	import { savePickerVisible, modSelectorVisible } from '../stores/save';
	import { palette } from '../stores/theme';
	import { tiles } from '../stores/tiles';
	import { startTravel, cancelTravel } from '../stores/travel';
	import {
		getWorldSnapshot,
		getMap,
		getNpcsHere,
		getUiConfig,
		getTheme,
		getDebugSnapshot,
		getDemoConfig,
		onWorldUpdate,
		onStreamToken,
		onStreamTurnEnd,
		onStreamEnd,
		onTextLog,
		onLoading,
		onThemeUpdate,
		onThemeSwitch,
		onTilesSwitch,
		onDebugUpdate,
		onSavePicker,
		onToggleFullMap,
		onOpenDesigner,
		onNpcReaction,
		onTravelStart,
		onReconnect,
		submitInput,
		saveScreenshot,
		disposeTransport,
		toggleFullscreen,
		isTauri
	} from '$lib/ipc';
	import { captureScreen } from '$lib/screenshot';
	import { createAutoPauseTracker } from '$lib/auto-pause';
	import { createStreamManager } from '$lib/setup/stream-manager';
	import { applyAppIcon } from '$lib/app-icon';

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

	const MOUSEMOVE_THROTTLE_MS = 1000;

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
			const cleanup = await setupMount();
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

	async function setupMount(): Promise<() => void> {
		// Frontend auto-pause tracker — fires /pause after `auto_pause_timeout_seconds` of true UI
		// inactivity (no key/mouse/touch). The server-side tick_inactivity
		// backstop in parish-server still runs for the tab-close case.
		// TODO #6 / #31a — track OS-level window focus so the tracker
		// only fires /pause when the user is actively in the parish
		// window. Without this guard the idle timer fires every time
		// attention shifts to another app and produces a burst of
		// /pause + /resume toggles.
		let windowFocused = typeof document !== 'undefined' ? document.hasFocus() : true;
		const onWindowFocus = () => { windowFocused = true; };
		const onWindowBlur = () => { windowFocused = false; };
		window.addEventListener('focus', onWindowFocus);
		window.addEventListener('blur', onWindowBlur);

		const tracker = createAutoPauseTracker({
			idleMs: () => get(uiConfig).auto_pause_timeout_seconds * 1000,
			mousemoveThrottleMs: MOUSEMOVE_THROTTLE_MS,
			submitInput,
			isWorldPaused: () => get(worldState)?.paused ?? false,
			isWindowFocused: () => windowFocused
		});

		const onTrackerKey = () => tracker.recordActivity();
		const onTrackerMousedown = () => tracker.recordActivity();
		const onTrackerTouch = () => tracker.recordActivity();
		const onTrackerMousemove = () => tracker.recordMousemove();
		window.addEventListener('keydown', onTrackerKey);
		window.addEventListener('mousedown', onTrackerMousedown);
		window.addEventListener('touchstart', onTrackerTouch);
		window.addEventListener('mousemove', onTrackerMousemove);

		// Pause immediately when the tab is hidden; resume when it returns.
		// Only pauses if the game wasn't already paused, and only resumes if
		// this handler was the one that paused it.
		let visibilityPaused = false;
		const handleVisibilityChange = () => {
			if (document.hidden) {
				const alreadyPaused = get(worldState)?.paused ?? false;
				if (!alreadyPaused) {
					void submitInput('/pause');
					visibilityPaused = true;
				}
			} else if (visibilityPaused) {
				void submitInput('/resume');
				visibilityPaused = false;
			}
		};
		document.addEventListener('visibilitychange', handleVisibilityChange);

		// Scene deduplicator: tracks the last-seen location to prevent scene
		// descriptions from being appended on every world update when the
		// location hasn't changed.
		const sceneDedup = new SceneDeduplicator();

		// Initial data fetch (theme first to avoid color flash).
		//
		// Use `allSettled` so a single failed endpoint doesn't block the
		// rest of the UI from loading. Any failure is surfaced via
		// pushErrorLog so the user sees feedback instead of an indefinite
		// "Loading..." state — see #113.
		const [snapRes, mapRes, npcsRes, themeRes] = await Promise.allSettled([
			getWorldSnapshot(),
			getMap(),
			getNpcsHere(),
			getTheme()
		]);
		if (snapRes.status === 'fulfilled') {
			const snap = snapRes.value;
			worldState.set(snap);
			palette.applyGameHour(snap.hour);
			if (snap.name_hints) nameHints.set(snap.name_hints);
			if (snap.location_description && sceneDedup.shouldShowDescription(snap.location_name)) {
				textLog.update((log) => [
					...log,
					{ source: 'system', content: snap.location_description }
				]);
			}
		}
		if (mapRes.status === 'fulfilled') mapData.set(mapRes.value);
		if (npcsRes.status === 'fulfilled') npcsHere.set(npcsRes.value);
		if (themeRes.status === 'fulfilled') palette.applyServerPalette(themeRes.value);

		const failed: string[] = [];
		if (snapRes.status === 'rejected') failed.push(`world (${formatIpcError(snapRes.reason)})`);
		if (mapRes.status === 'rejected') failed.push(`map (${formatIpcError(mapRes.reason)})`);
		if (npcsRes.status === 'rejected') failed.push(`NPCs (${formatIpcError(npcsRes.reason)})`);
		if (themeRes.status === 'rejected') failed.push(`theme (${formatIpcError(themeRes.reason)})`);
		if (failed.length > 0) {
			pushErrorLog(`Failed to load initial game data: ${failed.join(', ')}.`);
			for (const r of [snapRes, mapRes, npcsRes, themeRes]) {
				if (r.status === 'rejected') console.warn('Initial fetch failed:', r.reason);
			}
		}

		// Fetch UI config from mod and show splash text
		try {
			const cfg = await getUiConfig();
			uiConfig.set(cfg);
			tiles.initFromUiConfig(cfg);
			applyAppIcon(cfg.app_icon_url, cfg.favicon_url);
			document.body.classList.toggle('blueprint-mode', cfg.map_overlay === 'grid');
			if (cfg.base_mod_required) {
				modSelectorVisible.set(true);
			}
			if (cfg.splash_text) {
				textLog.update((log) => [
					{ source: 'system', content: cfg.splash_text },
					...log
				]);
			}
		} catch (_) {}

		// Fetch initial debug snapshot
		try {
			const debugSnap = await getDebugSnapshot();
			debugSnapshot.set(debugSnap);
		} catch (_) {}

		const sm = createStreamManager();

		const listeners: Array<() => void> = [];
		try {
			listeners.push(await onWorldUpdate(async (snap) => {
				worldState.set(snap);
				tracker.onWorldStateChange(snap.paused);
				palette.applyGameHour(snap.hour);
				if (snap.name_hints) nameHints.set(snap.name_hints);
				// Append scene description only if location has changed
				if (snap.location_description && sceneDedup.shouldShowDescription(snap.location_name)) {
					textLog.update((log) => [
						...log,
						{ source: 'system', content: snap.location_description }
					]);
				}
				try {
					const [map, npcs] = await Promise.all([getMap(), getNpcsHere()]);
					mapData.set(map);
					npcsHere.set(npcs);
				} catch (_) {}
			}));

			listeners.push(await onTextLog((payload) => {
				if (
					payload.content === '' &&
					payload.source !== 'player' &&
					payload.source !== 'system' &&
					payload.stream_turn_id != null
				) {
					sm.queuePendingTurn(payload.stream_turn_id, payload.source, payload.id);
					return;
				}

				// Movement renders the full arrival scene here (with NPCs + exits).
				// Suppress the shorter scene line the imminent world-update would
				// otherwise append, so the location prints once, not twice.
				if (payload.subtype === 'location') {
					sceneDedup.suppressNextDescription();
				}

				// Strip "> " prefix from player messages — bubble alignment shows speaker
				const content =
					payload.source === 'player' && payload.content.startsWith('> ')
						? payload.content.slice(2)
						: payload.content;
				textLog.update((log) =>
					trimTextLog([
						...log,
						{
							id: payload.id,
							source: payload.source,
							content,
							stream_turn_id: payload.stream_turn_id ?? undefined,
							...(payload.subtype ? { subtype: payload.subtype } : {})
						}
					])
				);
			}));

			listeners.push(await onNpcReaction((payload) => {
				addReaction(payload.message_id, payload.emoji, payload.source);
			}));

			listeners.push(await onStreamToken((payload) => {
				const turn = sm.queuePendingTurn(payload.turn_id, payload.source);
				turn.buffer += payload.token;
				sm.startTurnPumpIfNeeded(turn);
			}));

			listeners.push(await onStreamTurnEnd((payload) => {
				const turn = sm.findPendingTurn(payload.turn_id);
				if (!turn) return;
				turn.complete = true;
				sm.startTurnPumpIfNeeded(turn);
			}));

			listeners.push(await onStreamEnd((payload) => {
				sm.setPendingEndHints(payload.hints);
				sm.maybeFinishNpcStream();
			}));

			listeners.push(await onLoading((payload) => {
				if (payload.active) {
					// Loading started: mark streaming active and update spinner UI.
					streamingActive.set(true);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
				} else if (
					!sm.isChainInProgress() &&
					sm.pendingTurnCount() === 0 &&
					!sm.hasPendingEndHints()
				) {
					// Loading ended with no NPC stream in flight — clear immediately.
					// When a stream IS in flight, the text pump is still dripping
					// characters; finishNpcStream() clears streamingActive after the
					// pump drains so the input field and demo loop wait for text to
					// finish displaying.
					//
					// `isChainInProgress()` suppresses the clear between addressed-NPC
					// turns within one handle_npc_conversation chain (#991): the
					// backend cancels and re-spawns the loading animation per turn,
					// so `loading {active:false}` fires multiple times before the
					// chain's terminal `stream-end`. Without this gate the demo
					// loop's waitForFalse(streamingActive) resolves mid-chain and
					// fires the next player turn over an unfinished reply.
					streamingActive.set(false);
				}
			}));

			listeners.push(await onThemeUpdate((p) => {
				palette.applyServerPalette(p);
			}));

			listeners.push(await onThemeSwitch((p) => {
				palette.setPreference({
					name: p.name as 'default' | 'solarized',
					mode: p.mode as 'light' | 'dark' | 'auto' | ''
				});
			}));

			listeners.push(await onTilesSwitch((p) => {
				tiles.setActiveId(p.id);
			}));

			listeners.push(await onDebugUpdate((snap) => {
				debugSnapshot.set(snap);
			}));

			listeners.push(await onToggleFullMap(() => {
				fullMapOpen.update((v) => !v);
			}));

			listeners.push(await onOpenDesigner(() => {
				goto('/editor');
			}));

			listeners.push(await onTravelStart((payload) => {
				startTravel(payload);
			}));

			listeners.push(await onSavePicker(() => {
				savePickerVisible.set(true);
			}));

			// Resync authoritative state after a WebSocket reconnect: events
			// emitted during the gap (e.g. a terminal stream-end) are lost, so
			// re-fetch snapshot/map/npcs and clear any stuck streaming flag so
			// the input field and demo loop don't hang (audit M4). No-op in
			// Tauri (the desktop transport never disconnects).
			listeners.push(onReconnect(async () => {
				// Discard the orphaned pre-reconnect stream SYNCHRONOUSLY, before
				// awaiting anything. The turn that was streaming when the socket
				// dropped lost its remaining tokens / stream-end during the gap,
				// so pendingTurnCount()/chainInProgress would otherwise stay
				// non-zero forever and leave the input disabled. Doing this before
				// the first await means it runs inside the onopen dispatch — ahead
				// of any onmessage on the new socket — so a stream the backend
				// resumes after reconnect queues a fresh turn that this reset can't
				// clobber (the late-reset race Codex flagged).
				sm.reset();
				streamingActive.set(false);

				// allSettled (matching the mount-time fetch): a transient failure
				// on one endpoint right after reconnect must not discard the
				// other successful updates.
				const [snapRes, mapRes, npcsRes] = await Promise.allSettled([
					getWorldSnapshot(),
					getMap(),
					getNpcsHere()
				]);
				if (snapRes.status === 'fulfilled') {
					const snap = snapRes.value;
					worldState.set(snap);
					palette.applyGameHour(snap.hour);
					if (snap.name_hints) nameHints.set(snap.name_hints);
				}
				if (mapRes.status === 'fulfilled') mapData.set(mapRes.value);
				if (npcsRes.status === 'fulfilled') npcsHere.set(npcsRes.value);
				for (const r of [snapRes, mapRes, npcsRes]) {
					if (r.status === 'rejected') console.warn('Reconnect resync partial failure:', r.reason);
				}
			}));

		} catch (e) {
			console.warn('Failed to set up some event listeners:', e);
		}

		// Fetch demo config after event listeners are registered so that
		// WebSocket is open before any potentially-slow optional API calls.
		// In web mode /api/demo-config returns 404 (not implemented), causing
		// a network round-trip that previously delayed listener registration
		// and caused the smoke test player-echo event to be dropped.
		try {
			const dc = await getDemoConfig();
			demoConfig.set({
				auto_start: dc.auto_start,
				extra_prompt: dc.extra_prompt,
				turn_pause_secs: dc.turn_pause_secs,
				max_turns: dc.max_turns
			});
			if (dc.auto_start) {
				startDemoLoop();
			}
		} catch (_) {}

		return () => {
			window.removeEventListener('keydown', onTrackerKey);
			window.removeEventListener('mousedown', onTrackerMousedown);
			window.removeEventListener('touchstart', onTrackerTouch);
			window.removeEventListener('mousemove', onTrackerMousemove);
			window.removeEventListener('focus', onWindowFocus);
			window.removeEventListener('blur', onWindowBlur);
			document.removeEventListener('visibilitychange', handleVisibilityChange);
			tracker.dispose();
			sm.dispose();
			listeners.forEach((fn) => fn());
		};
	}
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
					mobilePanel = 'none';
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
					mobilePanel = 'none';
					fullMapOpen.set(false);
					focailOpen.set(true);
				}
			}}
		>Language Hints</button>
	</div>

	<div class="main-area">
		<div class="chat-col" class:mobile-hidden={mobilePanel !== 'none'}>
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

		/* Hide chat when a mobile panel is open */
		.chat-col.mobile-hidden {
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
