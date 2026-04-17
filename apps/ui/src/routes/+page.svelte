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
	import SavePicker from '../components/SavePicker.svelte';

	import { worldState, mapData, npcsHere, textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor, languageHints, nameHints, uiConfig, fullMapOpen, focailOpen, addReaction, trimTextLog, messageHints, pushErrorLog, formatIpcError } from '../stores/game';

	/** Which mobile-only panel is open (if any). Desktop ignores this. */
	let mobilePanel = $state<'none' | 'map' | 'sidebar'>('none');
	import { debugVisible, debugSnapshot, debugDockLeft } from '../stores/debug';
	import { savePickerVisible } from '../stores/save';
	import { palette } from '../stores/theme';
	import { tiles } from '../stores/tiles';
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
		submitInput,
		disposeTransport
	} from '$lib/ipc';
	import { createAutoPauseTracker } from '$lib/auto-pause';
	import { getStreamChunkDelayMs, takeNextStreamChunk } from '$lib/stream-pacing';
	import type { LanguageHint } from '$lib/types';

	const AUTO_PAUSE_MS = 60_000;
	const MOUSEMOVE_THROTTLE_MS = 1000;
	const STREAM_WAIT_FOR_WORD_MS = 70;

	type PendingNpcTurn = {
		turnId: number;
		source: string;
		messageId?: string;
		buffer: string;
		placeholderInserted: boolean;
		complete: boolean;
		pumpHandle: ReturnType<typeof setTimeout> | null;
	};

	// F5 toggle for save picker, F12 toggle for debug panel, M toggle for map
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'F5') {
			e.preventDefault();
			savePickerVisible.update((v) => !v);
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

	function appendStreamToken(turnId: number, source: string, token: string, messageId?: string) {
		textLog.update((log) => {
			const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
			if (entryIndex >= 0) {
				const current = log[entryIndex];
				const nextEntry = {
					...current,
					id: current.id ?? messageId,
					source,
					content: current.content + token,
					stream_turn_id: turnId,
					streaming: true,
					latest_chunk: token,
					stream_chunk_id: (current.stream_chunk_id ?? 0) + 1
				};
				return [
					...log.slice(0, entryIndex),
					nextEntry,
					...log.slice(entryIndex + 1)
				];
			}
			return trimTextLog([
				...log,
				{
					id: messageId,
					source,
					content: token,
					stream_turn_id: turnId,
					streaming: true,
					latest_chunk: token,
					stream_chunk_id: 1
				}
			]);
		});
	}

	let mountCleanup: (() => void) | null = null;
	onMount(() => {
		(async () => {
			mountCleanup = await setupMount();
		})();
	});
	onDestroy(() => {
		mountCleanup?.();
		// In browser mode, also tear down the shared WebSocket and any
		// pending reconnect timer so navigation away doesn't leave an
		// orphan socket or a zombie reconnect queued.
		disposeTransport();
	});

	async function setupMount(): Promise<() => void> {
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
			if (snap.location_description) {
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

		let pendingNpcTurns = new Map<number, PendingNpcTurn>();
		let pendingStreamEndHints: LanguageHint[] | null = null;

		function findPendingTurn(turnId: number) {
			return pendingNpcTurns.get(turnId);
		}

		function queuePendingTurn(turnId: number, source: string, messageId?: string) {
			const existing = findPendingTurn(turnId);
			if (existing) {
				existing.source = source;
				existing.messageId = existing.messageId ?? messageId;
				if (messageId && existing.placeholderInserted) {
					textLog.update((log) => {
						const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
						if (entryIndex < 0) return log;
						return [
							...log.slice(0, entryIndex),
							{ ...log[entryIndex], id: log[entryIndex].id ?? messageId, source },
							...log.slice(entryIndex + 1)
						];
					});
				}
				return existing;
			}

			const turn: PendingNpcTurn = {
				turnId,
				source,
				messageId,
				buffer: '',
				placeholderInserted: false,
				complete: false,
				pumpHandle: null
			};
			pendingNpcTurns.set(turnId, turn);
			return turn;
		}

		function ensureTurnEntry(turn: PendingNpcTurn) {
			if (turn.placeholderInserted) return;

			textLog.update((log) =>
				trimTextLog([
					...log,
					{
						id: turn.messageId,
						source: turn.source,
						content: '',
						stream_turn_id: turn.turnId
					}
				])
			);
			turn.placeholderInserted = true;
		}

		function finalizeStreamingEntry(turnId: number) {
			textLog.update((log) => {
				const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
				if (entryIndex < 0) {
					return log;
				}

				const entry = log[entryIndex];
				if (entry.content === '') {
					return [...log.slice(0, entryIndex), ...log.slice(entryIndex + 1)];
				}

				return [
					...log.slice(0, entryIndex),
					{
						...entry,
						streaming: false,
						latest_chunk: undefined,
						stream_chunk_id: undefined
					},
					...log.slice(entryIndex + 1)
				];
			});
		}

		function finishNpcStream(hints: LanguageHint[] = []) {
			// Associate Irish hints with the last NPC message for inline highlighting
			if (hints.length > 0) {
				const log = get(textLog);
				for (let i = log.length - 1; i >= 0; i--) {
					if (log[i].id && log[i].source !== 'player' && log[i].source !== 'system') {
						messageHints.update((m) => { m.set(log[i].id!, hints); return m; });
						break;
					}
				}
			}
			languageHints.set(hints);
			streamingActive.set(false);
		}

		function maybeFinishNpcStream() {
			if (pendingStreamEndHints === null || pendingNpcTurns.size > 0) return;
			finishNpcStream(pendingStreamEndHints);
			pendingStreamEndHints = null;
		}

		function stopTurnPump(turn: PendingNpcTurn) {
			if (turn.pumpHandle !== null) {
				clearTimeout(turn.pumpHandle);
				turn.pumpHandle = null;
			}
		}

		function scheduleTurnPump(turn: PendingNpcTurn, delayMs: number) {
			turn.pumpHandle = setTimeout(() => {
				turn.pumpHandle = null;
				pumpTurn(turn.turnId);
			}, delayMs);
		}

		function finalizePendingTurn(turnId: number) {
			const turn = findPendingTurn(turnId);
			if (!turn) return;
			stopTurnPump(turn);
			finalizeStreamingEntry(turnId);
			pendingNpcTurns.delete(turnId);
			maybeFinishNpcStream();
		}

		function startTurnPumpIfNeeded(turn: PendingNpcTurn) {
			if (turn.pumpHandle !== null) return;
			pumpTurn(turn.turnId);
		}

		function pumpTurn(turnId: number) {
			const turn = findPendingTurn(turnId);
			if (!turn) return;

			if (turn.buffer.length === 0) {
				stopTurnPump(turn);
				if (turn.complete) {
					finalizePendingTurn(turnId);
				}
				return;
			}

			ensureTurnEntry(turn);

			const { chunk, rest } = takeNextStreamChunk(turn.buffer, turn.complete);

			if (chunk === null) {
				scheduleTurnPump(turn, STREAM_WAIT_FOR_WORD_MS);
				return;
			}

			turn.buffer = rest;
			appendStreamToken(
				turn.turnId,
				turn.source,
				chunk,
				turn.messageId
			);
			scheduleTurnPump(turn, getStreamChunkDelayMs(chunk));
		}

		const listeners: Array<() => void> = [];
		try {
			listeners.push(await onWorldUpdate(async (snap) => {
				worldState.set(snap);
				tracker.onWorldStateChange(snap.paused);
				palette.applyGameHour(snap.hour);
				if (snap.name_hints) nameHints.set(snap.name_hints);
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
					queuePendingTurn(payload.stream_turn_id, payload.source, payload.id);
					return;
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
				const turn = queuePendingTurn(payload.turn_id, payload.source);
				turn.buffer += payload.token;
				startTurnPumpIfNeeded(turn);
			}));

			listeners.push(await onStreamTurnEnd((payload) => {
				const turn = findPendingTurn(payload.turn_id);
				if (!turn) return;
				turn.complete = true;
				startTurnPumpIfNeeded(turn);
			}));

			listeners.push(await onStreamEnd((payload) => {
				pendingStreamEndHints = payload.hints;
				maybeFinishNpcStream();
			}));

			listeners.push(await onLoading((payload) => {
				streamingActive.set(payload.active);
				if (payload.active) {
					// Update animated loading phrase and spinner
					if (payload.spinner) loadingSpinner.set(payload.spinner);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
					// The loading animation ticks repeatedly while a turn is in
					// flight; don't mutate chat state on those frames.
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
		} catch (e) {
			console.warn('Failed to set up some event listeners:', e);
		}

		return () => {
			window.removeEventListener('keydown', onTrackerKey);
			window.removeEventListener('mousedown', onTrackerMousedown);
			window.removeEventListener('touchstart', onTrackerTouch);
			window.removeEventListener('mousemove', onTrackerMousemove);
			document.removeEventListener('visibilitychange', handleVisibilityChange);
			tracker.dispose();
			pendingNpcTurns.forEach((turn) => stopTurnPump(turn));
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
			class="mobile-btn"
			class:active={$fullMapOpen}
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
			class="mobile-btn"
			class:active={$focailOpen}
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
			{#if $focailOpen}
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
		.mobile-btn.active {
			color: var(--color-accent);
			border-color: var(--color-accent);
		}

	}
</style>
