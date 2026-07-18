/**
 * Page lifecycle + real-time event wiring for the main app shell (#1200 TD-052).
 *
 * Extracted verbatim from `routes/+page.svelte`'s `setupMount`. The route
 * component is a thin shell: it renders markup and calls `createPageController`
 * from its `onMount`, then runs the returned cleanup on teardown. All initial
 * data loading, WebSocket/Tauri event subscriptions, the auto-pause + scene-
 * dedup + stream managers, and reconnect resync live here, keeping the
 * component free of runtime orchestration. The controller closes over the same
 * shared stores / ipc bindings the component used, so behavior is unchanged.
 */

import { get } from 'svelte/store';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

import {
	worldState,
	mapData,
	npcsHere,
	textLog,
	streamingActive,
	loadingPhrase,
	loadingColor,
	nameHints,
	uiConfig,
	addReaction,
	trimTextLog,
	pushErrorLog,
	formatIpcError,
	flushStream,
	playerSubmittedCount,
	noteStreamingStarted,
	resetExternalDrive,
	replaceStreamEntryContent,
} from '../stores/game';
import { demoConfig } from '../stores/demo';
import { startDemoLoop } from './demo-player';
import { SceneDeduplicator } from './scene-dedup';
import { debugSnapshot } from '../stores/debug';
import {
	openNotebookOverlay,
	toggleNotebookOverlay,
} from '../stores/notebookOverlay';
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
	onDialogueCorrected,
	submitInput,
} from '$lib/ipc';
import { createAutoPauseTracker } from '$lib/auto-pause';
import { createStreamManager } from '$lib/setup/stream-manager';
import { applyAppIcon } from '$lib/app-icon';

const MOUSEMOVE_THROTTLE_MS = 1000;

/**
 * Safety ceiling for the loading spinner (#1536).
 *
 * If `loading {active:false}` never fires (e.g. a bridge-driven turn that
 * doesn't emit a `stream-end`), or fires while the stream-manager's guards
 * prevent the clear (pending turns / hints), the spinner would remain visible
 * forever.  This timeout unconditionally clears `streamingActive` after
 * LOADING_SAFETY_TIMEOUT_MS, guaranteeing the UI recovers regardless of the
 * event sequence from the backend.
 */
const LOADING_SAFETY_TIMEOUT_MS = 10_000;

/**
 * Wires up initial data load + real-time event subscriptions for the app
 * shell and returns a cleanup function that tears them all down.
 */
export async function createPageController(): Promise<() => void> {
	// Frontend auto-pause tracker — fires /pause after `auto_pause_timeout_seconds` of true UI
	// inactivity (no key/mouse/touch). The server-side tick_inactivity
	// backstop in parish-server still runs for the tab-close case.
	// Track OS-level window focus so the tracker only fires /pause
	// when the user is actively in the parish window. Without this
	// guard the idle timer fires every time attention shifts to
	// another app and produces a burst of /pause + /resume toggles
	// (regression: demo-audit cycle 6).
	let windowFocused =
		typeof document !== 'undefined' ? document.hasFocus() : true;
	const onWindowFocus = () => {
		windowFocused = true;
	};
	const onWindowBlur = () => {
		windowFocused = false;
	};
	window.addEventListener('focus', onWindowFocus);
	window.addEventListener('blur', onWindowBlur);

	const tracker = createAutoPauseTracker({
		idleMs: () => get(uiConfig).auto_pause_timeout_seconds * 1000,
		mousemoveThrottleMs: MOUSEMOVE_THROTTLE_MS,
		submitInput,
		isWorldPaused: () => get(worldState)?.paused ?? false,
		isWindowFocused: () => windowFocused,
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
	//
	// Uses /pause-silent and /resume-silent so the clock freezes but
	// "The clocks of the parish stand still." / "Time stirs again in the
	// parish." are NOT printed.  Switching windows is not an idle pause —
	// the user-visible message should only appear when the player
	// explicitly types /pause or when the auto-idle timer fires (#1277).
	let visibilityPaused = false;
	const handleVisibilityChange = () => {
		if (document.hidden) {
			const alreadyPaused = get(worldState)?.paused ?? false;
			if (!alreadyPaused) {
				void submitInput('/pause-silent');
				visibilityPaused = true;
			}
		} else if (visibilityPaused) {
			void submitInput('/resume-silent');
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
		getTheme(),
	]);
	if (snapRes.status === 'fulfilled') {
		const snap = snapRes.value;
		worldState.set(snap);
		palette.applyGameHour(snap.hour);
		if (snap.name_hints) nameHints.set(snap.name_hints);
		if (
			snap.location_description &&
			sceneDedup.shouldShowDescription(snap.location_name)
		) {
			textLog.update((log) => [
				...log,
				{ source: 'system', content: snap.location_description },
			]);
		}
	}
	if (mapRes.status === 'fulfilled') mapData.set(mapRes.value);
	if (npcsRes.status === 'fulfilled') npcsHere.set(npcsRes.value);
	if (themeRes.status === 'fulfilled')
		palette.applyServerPalette(themeRes.value);

	const failed: string[] = [];
	if (snapRes.status === 'rejected')
		failed.push(`world (${formatIpcError(snapRes.reason)})`);
	if (mapRes.status === 'rejected')
		failed.push(`map (${formatIpcError(mapRes.reason)})`);
	if (npcsRes.status === 'rejected')
		failed.push(`NPCs (${formatIpcError(npcsRes.reason)})`);
	if (themeRes.status === 'rejected')
		failed.push(`theme (${formatIpcError(themeRes.reason)})`);
	if (failed.length > 0) {
		pushErrorLog(`Failed to load initial game data: ${failed.join(', ')}.`);
		for (const r of [snapRes, mapRes, npcsRes, themeRes]) {
			if (r.status === 'rejected')
				console.warn('Initial fetch failed:', r.reason);
		}
	}

	// Fetch UI config from mod and show splash text
	try {
		const cfg = await getUiConfig();
		uiConfig.set(cfg);
		tiles.initFromUiConfig(cfg);
		applyAppIcon(cfg.app_icon_url, cfg.favicon_url);
		document.body.classList.toggle(
			'blueprint-mode',
			cfg.map_overlay === 'grid',
		);
		if (cfg.base_mod_required) {
			void openNotebookOverlay('mod');
		}
		if (cfg.splash_text) {
			textLog.update((log) => [
				{ source: 'system', content: cfg.splash_text },
				...log,
			]);
		}
	} catch (_) {
		// ignore: best-effort, failure is non-fatal (mod config missing/unavailable)
	}

	// Fetch initial debug snapshot
	try {
		const debugSnap = await getDebugSnapshot();
		debugSnapshot.set(debugSnap);
	} catch (_) {
		// ignore: best-effort, debug snapshot is optional
	}

	const sm = createStreamManager();
	// Expose the flush so the input field can snap an in-flight reply fully into
	// view on the player's first keystroke before the next turn lands (#1379).
	flushStream.set(() => sm.flushAll());

	// ── Loading safety timeout (#1536) ──────────────────────────────────────
	// Tracks the active safety timer. Cleared on every `loading {active:false}`
	// or `stream-end` so normal turns don't trip it; fires only when no
	// terminal event arrives within LOADING_SAFETY_TIMEOUT_MS.
	let loadingSafetyTimer: ReturnType<typeof setTimeout> | null = null;

	function armLoadingSafetyTimer() {
		if (loadingSafetyTimer !== null) clearTimeout(loadingSafetyTimer);
		loadingSafetyTimer = setTimeout(() => {
			loadingSafetyTimer = null;
			// Force-clear: a bridge-driven or non-streaming turn may never emit
			// stream-end or a clean loading{active:false} path through the guards.
			if (get(streamingActive)) {
				sm.reset();
				streamingActive.set(false);
				resetExternalDrive();
			}
		}, LOADING_SAFETY_TIMEOUT_MS);
	}

	function disarmLoadingSafetyTimer() {
		if (loadingSafetyTimer !== null) {
			clearTimeout(loadingSafetyTimer);
			loadingSafetyTimer = null;
		}
	}

	// ── External-drive tracking (#1537) ─────────────────────────────────────
	// Snapshot the playerSubmittedCount at the start of each turn so we can
	// detect bridge-driven turns where the count doesn't change.
	let lastLocalSubmitCount = get(playerSubmittedCount);

	const listeners: Array<() => void> = [];
	try {
		listeners.push(
			await onWorldUpdate(async (snap) => {
				worldState.set(snap);
				tracker.onWorldStateChange(snap.paused);
				palette.applyGameHour(snap.hour);
				if (snap.name_hints) nameHints.set(snap.name_hints);
				// Append scene description only if location has changed
				if (
					snap.location_description &&
					sceneDedup.shouldShowDescription(snap.location_name)
				) {
					textLog.update((log) => [
						...log,
						{ source: 'system', content: snap.location_description },
					]);
				}
				try {
					const [map, npcs] = await Promise.all([getMap(), getNpcsHere()]);
					mapData.set(map);
					npcsHere.set(npcs);
				} catch (_) {
					// ignore: best-effort map/NPC refresh; stale data is acceptable
				}
			}),
		);

		listeners.push(
			await onTextLog((payload) => {
				if (
					payload.content === '' &&
					payload.source !== 'player' &&
					payload.source !== 'system' &&
					payload.stream_turn_id != null
				) {
					sm.queuePendingTurn(
						payload.stream_turn_id,
						payload.source,
						payload.id,
					);
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
							...(payload.subtype ? { subtype: payload.subtype } : {}),
						},
					]),
				);
			}),
		);

		listeners.push(
			await onNpcReaction((payload) => {
				addReaction(payload.message_id, payload.emoji, payload.source);
			}),
		);

		listeners.push(
			await onStreamToken((payload) => {
				// Re-assert the busy flag whenever tokens actually flow. Normally
				// loading{active:true} already set it, but after a mid-turn
				// reconnect (where we cleared it to recover from a possibly-dead
				// stream) a resumed stream would otherwise leave input enabled
				// mid-turn, allowing a duplicate send. finishNpcStream clears it
				// authoritatively once the pump drains. (Codex reconnect-resume.)
				streamingActive.set(true);
				// Pass message_id so a stream that resumed after a reconnect (whose
				// placeholder text-log — and its id — was discarded by sm.reset()
				// during the gap) rebinds to a reactable textLog entry (#1164).
				const turn = sm.queuePendingTurn(
					payload.turn_id,
					payload.source,
					payload.message_id,
				);
				turn.buffer += payload.token;
				sm.startTurnPumpIfNeeded(turn);
			}),
		);

		listeners.push(
			await onStreamTurnEnd((payload) => {
				const turn = sm.findPendingTurn(payload.turn_id);
				if (!turn) return;
				turn.complete = true;
				sm.startTurnPumpIfNeeded(turn);
			}),
		);

		listeners.push(
			await onDialogueCorrected((payload) => {
				// The backend has applied post-generation guards and found that the raw
				// model output differed from the canonical post-guard dialogue (#1552).
				// We must (a) clear the stream pump's remaining buffer so it stops
				// appending raw tokens and (b) replace the textLog entry content with
				// the corrected text so the player sees what is stored in the
				// conversation log and returned by /api/transcript.
				sm.clearTurnBuffer(payload.turn_id);
				replaceStreamEntryContent(payload.turn_id, payload.corrected_text);
			}),
		);

		listeners.push(
			await onStreamEnd((payload) => {
				// Terminal event for the turn — disarm the safety timeout so a normal
				// NPC stream doesn't trip it while the pump is draining (#1536).
				disarmLoadingSafetyTimer();
				sm.setPendingEndHints(payload.hints);
				sm.maybeFinishNpcStream();
			}),
		);

		listeners.push(
			await onLoading((payload) => {
				if (payload.active) {
					// Loading started: mark streaming active and update spinner UI.
					streamingActive.set(true);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
					// Detect bridge-driven turns: if playerSubmittedCount hasn't
					// changed since the last turn boundary, the input didn't come
					// from the local InputField (#1537).
					//
					// Only re-evaluate external/local at the START of a new chain
					// (when !sm.isChainInProgress()).  During a multi-turn NPC
					// conversation the backend cancels and re-spawns the loading
					// animation per NPC turn, firing loading{active:true} multiple
					// times within a single player-initiated interaction.  On those
					// re-spawns playerSubmittedCount has NOT incremented again, so a
					// naïve re-evaluation would falsely mark the whole chain as
					// external (#1538).  Instead we inherit the chain's existing
					// local/external classification for all re-spawned loadings.
					if (!sm.isChainInProgress()) {
						const currentCount = get(playerSubmittedCount);
						noteStreamingStarted(currentCount, lastLocalSubmitCount);
						lastLocalSubmitCount = currentCount;
					}
					// Arm the safety timeout so the spinner can't hang forever if
					// the terminal event (stream-end / loading{active:false}) is
					// lost (e.g. a bridge turn that emits no NPC stream, #1536).
					armLoadingSafetyTimer();
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
					disarmLoadingSafetyTimer();
					streamingActive.set(false);
				}
			}),
		);

		listeners.push(
			await onThemeUpdate((p) => {
				palette.applyServerPalette(p);
			}),
		);

		listeners.push(
			await onThemeSwitch((p) => {
				palette.setPreference({
					name: p.name as 'default' | 'solarized',
					mode: p.mode as 'light' | 'dark' | 'auto' | '',
				});
			}),
		);

		listeners.push(
			await onTilesSwitch((p) => {
				tiles.setActiveId(p.id);
			}),
		);

		listeners.push(
			await onDebugUpdate((snap) => {
				debugSnapshot.set(snap);
			}),
		);

		listeners.push(
			await onToggleFullMap(() => {
				void toggleNotebookOverlay('map');
			}),
		);

		listeners.push(
			await onOpenDesigner(() => {
				goto(resolve('/editor'));
			}),
		);

		listeners.push(
			await onTravelStart((payload) => {
				startTravel(payload);
			}),
		);

		listeners.push(
			await onSavePicker(() => {
				void openNotebookOverlay('save');
			}),
		);

		// Resync authoritative state after a WebSocket reconnect: events
		// emitted during the gap (e.g. a terminal stream-end) are lost, so
		// re-fetch snapshot/map/npcs and clear any stuck streaming flag so
		// the input field and demo loop don't hang (audit M4). No-op in
		// Tauri (the desktop transport never disconnects).
		listeners.push(
			onReconnect(async () => {
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
					getNpcsHere(),
				]);
				if (snapRes.status === 'fulfilled') {
					const snap = snapRes.value;
					worldState.set(snap);
					palette.applyGameHour(snap.hour);
					if (snap.name_hints) nameHints.set(snap.name_hints);
					// Re-assert busy state from authoritative server state: if a
					// turn was still in flight across the gap (slow model, or a
					// pause before the next stream-token), the reset above wrongly
					// cleared streamingActive, re-enabling the input field and
					// quick-travel chips → duplicate-turn window. A resumed
					// stream-token would re-set it, but only once tokens actually
					// flow; this closes the pre-token gap immediately (#1164).
					if (snap.turn_in_flight) streamingActive.set(true);
				}
				if (mapRes.status === 'fulfilled') mapData.set(mapRes.value);
				if (npcsRes.status === 'fulfilled') npcsHere.set(npcsRes.value);
				for (const r of [snapRes, mapRes, npcsRes]) {
					if (r.status === 'rejected')
						console.warn('Reconnect resync partial failure:', r.reason);
				}
			}),
		);
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
			max_turns: dc.max_turns,
		});
		if (dc.auto_start) {
			startDemoLoop();
		}
	} catch (_) {
		// ignore: best-effort, demo config is optional (web mode returns 404)
	}

	return () => {
		window.removeEventListener('keydown', onTrackerKey);
		window.removeEventListener('mousedown', onTrackerMousedown);
		window.removeEventListener('touchstart', onTrackerTouch);
		window.removeEventListener('mousemove', onTrackerMousemove);
		window.removeEventListener('focus', onWindowFocus);
		window.removeEventListener('blur', onWindowBlur);
		document.removeEventListener('visibilitychange', handleVisibilityChange);
		tracker.dispose();
		flushStream.set(() => 0);
		disarmLoadingSafetyTimer();
		resetExternalDrive();
		sm.dispose();
		listeners.forEach((fn) => fn());
	};
}
