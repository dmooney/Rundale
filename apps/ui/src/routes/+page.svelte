<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import StatusBar from '../components/StatusBar.svelte';
	import ChatPanel from '../components/ChatPanel.svelte';
	import MapPanel from '../components/MapPanel.svelte';
	import FullMapOverlay from '../components/FullMapOverlay.svelte';
	import Sidebar from '../components/Sidebar.svelte';
	import InputField from '../components/InputField.svelte';
	import DebugPanel from '../components/DebugPanel.svelte';
	import SavePicker from '../components/SavePicker.svelte';

	import { worldState, mapData, npcsHere, textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor, languageHints, nameHints, uiConfig, fullMapOpen, addReaction, trimTextLog } from '../stores/game';

	/** Which mobile-only panel is open (if any). Desktop ignores this. */
	let mobilePanel = $state<'none' | 'map' | 'sidebar'>('none');
	import { debugVisible, debugSnapshot } from '../stores/debug';
	import { savePickerVisible } from '../stores/save';
	import { palette } from '../stores/theme';
	import { startTravel } from '../stores/travel';
	import {
		getWorldSnapshot,
		getMap,
		getNpcsHere,
		getUiConfig,
		getTheme,
		getDebugSnapshot,
		onWorldUpdate,
		onStreamToken,
		onStreamEnd,
		onTextLog,
		onLoading,
		onThemeUpdate,
		onDebugUpdate,
		onSavePicker,
		onToggleFullMap,
		onNpcReaction,
		onTravelStart,
		submitInput
	} from '$lib/ipc';
	import { createAutoPauseTracker } from '$lib/auto-pause';
	import { getStreamChunkDelayMs, takeNextStreamChunk } from '$lib/stream-pacing';
	import type { LanguageHint } from '$lib/types';

	const AUTO_PAUSE_MS = 60_000;
	const MOUSEMOVE_THROTTLE_MS = 1000;
	const STREAM_WAIT_FOR_WORD_MS = 70;

	// F5 toggle for save picker, F12 toggle for debug panel, M toggle for map
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'F5') {
			e.preventDefault();
			savePickerVisible.update((v) => !v);
		}
		if (e.key === 'F12') {
			e.preventDefault();
			debugVisible.update((v) => !v);
			// Fetch initial snapshot when opening
			if (!$debugVisible) {
				getDebugSnapshot()
					.then((s) => debugSnapshot.set(s))
					.catch(() => {});
			}
		}
		// Toggle full map with M key, but only when not typing in an input/contenteditable
		if ((e.key === 'm' || e.key === 'M') && document.activeElement?.tagName !== 'INPUT' && !(document.activeElement as HTMLElement)?.isContentEditable) {
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

	function appendStreamToken(token: string) {
		textLog.update((log) => {
			if (log.length > 0 && log[log.length - 1].streaming) {
				const last = log[log.length - 1];
				return [
					...log.slice(0, -1),
					{
						...last,
						content: last.content + token,
						latest_chunk: token,
						stream_chunk_id: (last.stream_chunk_id ?? 0) + 1
					}
				];
			}
			// Merge with the empty NPC name placeholder emitted by Rust
			const last = log.length > 0 ? log[log.length - 1] : null;
			if (
				last &&
				last.content === '' &&
				last.source !== 'player' &&
				last.source !== 'system'
			) {
				return [
					...log.slice(0, -1),
					{
						...last,
						content: token,
						streaming: true,
						latest_chunk: token,
						stream_chunk_id: 1
					}
				];
			}
			// Use the most recent NPC source name if available, otherwise fall back
			const npcSource =
				last && last.source !== 'player' && last.source !== 'system'
					? last.source
					: 'NPC';
			return trimTextLog([
				...log,
				{
					source: npcSource,
					content: token,
					streaming: true,
					latest_chunk: token,
					stream_chunk_id: 1
				}
			]);
		});
	}

	onMount(async () => {
		// Frontend auto-pause tracker — fires /pause after 60s of true UI
		// inactivity (no key/mouse/touch). The server-side tick_inactivity
		// backstop in parish-server still runs for the tab-close case.
		const tracker = createAutoPauseTracker({
			idleMs: AUTO_PAUSE_MS,
			mousemoveThrottleMs: MOUSEMOVE_THROTTLE_MS,
			submitInput,
			isWorldPaused: () => get(worldState)?.paused ?? false
		});

		const onTrackerKey = () => tracker.recordActivity();
		const onTrackerMousedown = () => tracker.recordActivity();
		const onTrackerTouch = () => tracker.recordActivity();
		const onTrackerMousemove = () => tracker.recordMousemove();
		window.addEventListener('keydown', onTrackerKey);
		window.addEventListener('mousedown', onTrackerMousedown);
		window.addEventListener('touchstart', onTrackerTouch);
		window.addEventListener('mousemove', onTrackerMousemove);

		// Initial data fetch (theme first to avoid color flash)
		try {
			const [snap, map, npcs, theme] = await Promise.all([
				getWorldSnapshot(),
				getMap(),
				getNpcsHere(),
				getTheme()
			]);
			worldState.set(snap);
			mapData.set(map);
			npcsHere.set(npcs);
			palette.apply(theme);
			if (snap.name_hints) nameHints.set(snap.name_hints);
			// Show initial location description in the chat panel
			if (snap.location_description) {
				textLog.update((log) => [
					...log,
					{ source: 'system', content: snap.location_description }
				]);
			}
		} catch (e) {
			console.warn('Initial fetch failed (expected in browser dev):', e);
		}

		// Fetch UI config from mod and show splash text
		try {
			const cfg = await getUiConfig();
			uiConfig.set(cfg);
			if (cfg.splash_text) {
				textLog.update((log) => [
					{ source: 'system', content: cfg.splash_text },
					...log
				]);
			}
		} catch (_) {}

		// Subscribe to events
		// Fetch initial debug snapshot
		try {
			const debugSnap = await getDebugSnapshot();
			debugSnapshot.set(debugSnap);
		} catch (_) {}

		let streamBuffer = '';
		let streamPumpHandle: ReturnType<typeof setTimeout> | null = null;
		let pendingStreamEndHints: LanguageHint[] | null = null;

		const finishNpcStream = () => {
			textLog.update((log) => {
				if (log.length > 0 && log[log.length - 1].streaming) {
					const last = log[log.length - 1];
					return [
						...log.slice(0, -1),
						{
							...last,
							streaming: false,
							latest_chunk: undefined,
							stream_chunk_id: undefined
						}
					];
				}
				return log;
			});
			languageHints.set(pendingStreamEndHints ?? []);
			pendingStreamEndHints = null;
			streamingActive.set(false);
		};

		const stopStreamPump = () => {
			if (streamPumpHandle !== null) {
				clearTimeout(streamPumpHandle);
				streamPumpHandle = null;
			}
		};

		const scheduleStreamPump = (delayMs: number) => {
			streamPumpHandle = setTimeout(() => {
				streamPumpHandle = null;
				pumpStream();
			}, delayMs);
		};

		const pumpStream = () => {
			if (streamBuffer.length === 0) {
				stopStreamPump();
				if (pendingStreamEndHints !== null) finishNpcStream();
				return;
			}

			const { chunk, rest } = takeNextStreamChunk(
				streamBuffer,
				pendingStreamEndHints !== null
			);

			if (chunk === null) {
				scheduleStreamPump(STREAM_WAIT_FOR_WORD_MS);
				return;
			}

			streamBuffer = rest;
			appendStreamToken(chunk);
			scheduleStreamPump(getStreamChunkDelayMs(chunk));
		};

		const startStreamPumpIfNeeded = () => {
			if (streamPumpHandle !== null) return;
			pumpStream();
		};

		const listeners: Array<() => void> = [];
		try {
			listeners.push(await onWorldUpdate(async (snap) => {
				worldState.set(snap);
				tracker.onWorldStateChange(snap.paused);
				if (snap.name_hints) nameHints.set(snap.name_hints);
				try {
					const [map, npcs] = await Promise.all([getMap(), getNpcsHere()]);
					mapData.set(map);
					npcsHere.set(npcs);
				} catch (_) {}
			}));

			listeners.push(await onTextLog((payload) => {
				// Strip "> " prefix from player messages — bubble alignment shows speaker
				const content =
					payload.source === 'player' && payload.content.startsWith('> ')
						? payload.content.slice(2)
						: payload.content;
				textLog.update((log) => trimTextLog([...log, { id: payload.id, source: payload.source, content }]));
			}));

			listeners.push(await onNpcReaction((payload) => {
				addReaction(payload.message_id, payload.emoji, payload.source);
			}));

			listeners.push(await onStreamToken((payload) => {
				streamBuffer += payload.token;
				startStreamPumpIfNeeded();
			}));

			listeners.push(await onStreamEnd((payload) => {
				pendingStreamEndHints = payload.hints;
				if (streamBuffer.length === 0 && streamPumpHandle === null) {
					finishNpcStream();
				}
			}));

			listeners.push(await onLoading((payload) => {
				const wasActive = get(streamingActive);
				streamingActive.set(payload.active);
				if (payload.active) {
					// Update animated loading phrase and spinner
					if (payload.spinner) loadingSpinner.set(payload.spinner);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
					// Only clean up stale streaming entries on the *first*
					// loading event (transition from inactive → active), not
					// on every animation tick, to avoid erasing in-progress text.
					if (!wasActive) {
						textLog.update((log) => {
							if (log.length > 0 && log[log.length - 1].streaming) {
								return log.slice(0, -1);
							}
							return log;
						});
					}
				}
			}));

			listeners.push(await onThemeUpdate((p) => {
				palette.apply(p);
			}));

			listeners.push(await onDebugUpdate((snap) => {
				debugSnapshot.set(snap);
			}));

			listeners.push(await onToggleFullMap(() => {
				fullMapOpen.update((v) => !v);
			}));

			listeners.push(await onTravelStart((payload) => {
				startTravel(payload);
			}));

			listeners.push(await onSavePicker(() => {
				savePickerVisible.set(true);
			}));
		} catch (e) {
			console.warn('Failed to set up some event listeners:', e);
		}

		return () => {
			window.removeEventListener('keydown', onTrackerKey);
			window.removeEventListener('mousedown', onTrackerMousedown);
			window.removeEventListener('touchstart', onTrackerTouch);
			window.removeEventListener('mousemove', onTrackerMousemove);
			tracker.dispose();
			stopStreamPump();
			listeners.forEach((fn) => fn());
		};
	});
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $fullMapOpen}
	<FullMapOverlay onclose={() => fullMapOpen.set(false)} />
{/if}

<div class="app-shell" class:debug-open={$debugVisible}>
	<StatusBar />

	<!-- Mobile-only toggle toolbar -->
	<div class="mobile-toolbar">
		<button
			class="mobile-btn"
			class:active={mobilePanel === 'map'}
			onclick={() => mobilePanel = mobilePanel === 'map' ? 'none' : 'map'}
		>Map</button>
		<button
			class="mobile-btn"
			class:active={mobilePanel === 'sidebar'}
			onclick={() => mobilePanel = mobilePanel === 'sidebar' ? 'none' : 'sidebar'}
		>Hints</button>
	</div>

	<div class="main-area">
		<div class="chat-col" class:mobile-hidden={mobilePanel !== 'none'}>
			<ChatPanel />
			<InputField />
		</div>
		<div class="right-col">
			<MapPanel />
			<Sidebar />
		</div>
	</div>

	<!-- Mobile-only panel (replaces chat area when open) -->
	{#if mobilePanel !== 'none'}
		<div class="mobile-panel">
			{#if mobilePanel === 'map'}
				<div class="mobile-panel-inner">
					<MapPanel />
				</div>
			{:else}
				<div class="mobile-panel-inner">
					<Sidebar />
				</div>
			{/if}
		</div>
	{/if}
</div>

<DebugPanel />
<SavePicker />

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100dvh;
		overflow: hidden;
		transition: height 0.15s ease;
		padding-bottom: env(safe-area-inset-bottom);
	}

	.app-shell.debug-open {
		height: 60vh;
	}

	.main-area {
		flex: 1;
		display: grid;
		grid-template-columns: 1fr 220px;
		overflow: hidden;
		min-height: 0;
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

	:global(.status-bar) {
		position: sticky;
		top: 0;
		z-index: 30;
		flex-shrink: 0;
	}

	/* ── Mobile panel (shown when a toolbar button is active) ── */
	.mobile-panel {
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
		.mobile-btn.active {
			color: var(--color-accent);
			border-color: var(--color-accent);
		}

		.mobile-panel {
			display: flex;
			flex: 1;
			min-height: 0;
			overflow: hidden;
		}

		.mobile-panel-inner {
			flex: 1;
			display: flex;
			flex-direction: column;
			overflow-y: auto;
			background: var(--color-bg);
		}
	}
</style>
