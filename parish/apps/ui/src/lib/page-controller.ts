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
	getReconnectState,
	getUiConfig,
	getTheme,
	getDebugSnapshot,
	getDemoConfig,
	onGameContextReset,
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
import type {
	LanguageHint,
	MapData,
	NpcInfo,
	PlayerTaskSnapshot,
	ReconnectState,
	WorldSnapshot,
} from '$lib/types';

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

export interface ReconnectPresentationState {
	sceneDedup: SceneDeduplicator;
	contextEpoch: number | null;
	/** Invalidates canonical fetches that began against an older presentation. */
	generation: number;
	/** False after the owning page has begun teardown. */
	isActive: () => boolean;
	resetStream: () => void;
}

export type ReconnectStateFetcher = () => Promise<unknown>;

/**
 * Replaces UI state from one persistence-gated reconnect snapshot.
 *
 * Nothing is mutated until the complete envelope succeeds and passes runtime
 * validation. An epoch change (or the first reconnect when the epoch is
 * unknown) starts a fresh presentation context; an ordinary same-epoch
 * reconnect retains its transcript/dedup cursor while refreshing canonical
 * world, map, and NPC state.
 */
export async function resyncCanonicalStateAfterReconnect(
	presentation: ReconnectPresentationState,
	fetchState: ReconnectStateFetcher = getReconnectState,
): Promise<boolean> {
	if (!presentation.isActive()) return false;
	const requestGeneration = presentation.generation;
	let candidate: unknown;
	try {
		candidate = await fetchState();
	} catch (error) {
		console.warn('Reconnect resync failed:', error);
		return false;
	}

	if (!isReconnectState(candidate)) {
		console.warn('Reconnect resync failed: invalid aggregate payload');
		return false;
	}

	// A context reset landed while the aggregate was in flight. Its payload can
	// only describe the presentation that existed before that reset, so it must
	// not resurrect the old world even when the reset made the epoch unknown.
	if (
		!presentation.isActive() ||
		requestGeneration !== presentation.generation
	) {
		return false;
	}

	const contextChanged =
		presentation.contextEpoch === null ||
		presentation.contextEpoch !== candidate.context_epoch;

	// Commit point. Stream cancellation belongs inside the same non-failing
	// phase as the store replacement: a rejected/malformed aggregate must leave
	// even a half-streamed presentation exactly as it was.
	presentation.resetStream();
	if (contextChanged) {
		textLog.set([]);
		presentation.sceneDedup.reset();
	}

	applyCanonicalAggregate(candidate, presentation);
	streamingActive.set(Boolean(candidate.world.turn_in_flight));
	presentation.generation += 1;
	return true;
}

/**
 * Closes the startup gap between the first aggregate read and event-listener
 * registration.
 *
 * Unlike an actual reconnect, a same-context reconciliation does not reset a
 * stream that may have begun while listeners were being attached. An epoch
 * change still clears every old-context presentation artifact.
 */
export async function resyncCanonicalStateAfterSubscription(
	presentation: ReconnectPresentationState,
	fetchState: ReconnectStateFetcher = getReconnectState,
): Promise<boolean> {
	if (!presentation.isActive()) return false;
	const requestGeneration = presentation.generation;
	let candidate: unknown;
	try {
		candidate = await fetchState();
	} catch (error) {
		console.warn('Post-subscription resync failed:', error);
		return false;
	}
	if (!isReconnectState(candidate)) {
		console.warn('Post-subscription resync failed: invalid aggregate payload');
		return false;
	}
	if (
		!presentation.isActive() ||
		requestGeneration !== presentation.generation
	) {
		return false;
	}

	const contextChanged =
		presentation.contextEpoch === null ||
		presentation.contextEpoch !== candidate.context_epoch;
	if (contextChanged) {
		presentation.resetStream();
		streamingActive.set(false);
		loadingPhrase.set('');
		loadingColor.set([72, 199, 142]);
		textLog.set([]);
		presentation.sceneDedup.reset();
	}
	applyCanonicalAggregate(candidate, presentation);
	if (contextChanged || candidate.world.turn_in_flight) {
		streamingActive.set(Boolean(candidate.world.turn_in_flight));
	}
	presentation.generation += 1;
	return true;
}

/**
 * Clears every presentation artifact owned by the previous game context.
 *
 * The stream manager is reset before the transcript is cleared so pending
 * token pumps cannot append into the replacement context. The generation bump
 * invalidates reconnect/world aggregate requests already in flight.
 */
export function resetPresentationForNewContext(
	presentation: ReconnectPresentationState,
): void {
	if (!presentation.isActive()) return;
	presentation.resetStream();
	streamingActive.set(false);
	loadingPhrase.set('');
	loadingColor.set([72, 199, 142]);
	textLog.set([]);
	presentation.sceneDedup.reset();
	presentation.contextEpoch = null;
	presentation.generation += 1;
}

/**
 * Refreshes a pushed world event from one canonical aggregate.
 *
 * The event payload is deliberately not committed provisionally: it may have
 * been queued before a branch reset. Only the persistence-gated aggregate is
 * allowed to replace world, map, and NPC stores, and all presentation details
 * are derived from that accepted world.
 */
export async function refreshCanonicalStateAfterWorldUpdate(
	presentation: ReconnectPresentationState,
	refreshRevision: number,
	currentRefreshRevision: () => number,
	fetchState: ReconnectStateFetcher = getReconnectState,
): Promise<WorldSnapshot | null> {
	if (!presentation.isActive()) return null;
	const requestGeneration = presentation.generation;
	let candidate: unknown;
	try {
		candidate = await fetchState();
	} catch (_) {
		return null;
	}

	if (!isReconnectState(candidate)) {
		console.warn('World-update aggregate refresh ignored: invalid payload');
		return null;
	}
	if (
		!presentation.isActive() ||
		requestGeneration !== presentation.generation ||
		refreshRevision !== currentRefreshRevision()
	) {
		return null;
	}
	if (
		presentation.contextEpoch !== null &&
		presentation.contextEpoch !== candidate.context_epoch
	) {
		return null;
	}

	applyCanonicalAggregate(candidate, presentation);
	presentation.generation += 1;
	return candidate.world;
}

function applyCanonicalAggregate(
	candidate: ReconnectState,
	presentation: ReconnectPresentationState,
): void {
	const snap = candidate.world;
	worldState.set(snap);
	mapData.set(candidate.map);
	npcsHere.set(candidate.npcs);
	palette.applyGameHour(snap.hour);
	nameHints.set(snap.name_hints);
	if (
		snap.location_description &&
		presentation.sceneDedup.shouldShowDescription(snap.location_name)
	) {
		textLog.update((log) => [
			...log,
			{
				source: 'system',
				subtype: 'location',
				content: snap.location_description,
			},
		]);
	}
	presentation.contextEpoch = candidate.context_epoch;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
	return typeof value === 'string';
}

function isFiniteNumber(value: unknown): value is number {
	return typeof value === 'number' && Number.isFinite(value);
}

function isOptionalBoolean(value: unknown): boolean {
	return value === undefined || typeof value === 'boolean';
}

function isNullableString(value: unknown): boolean {
	return value === null || isString(value);
}

function isLanguageHint(value: unknown): value is LanguageHint {
	return (
		isRecord(value) &&
		isString(value.word) &&
		isString(value.pronunciation) &&
		isNullableString(value.meaning)
	);
}

function isPlayerTask(value: unknown): value is PlayerTaskSnapshot {
	return (
		isRecord(value) &&
		Number.isSafeInteger(value.id) &&
		isString(value.description) &&
		Number.isSafeInteger(value.assigned_by) &&
		Number.isSafeInteger(value.location_id) &&
		(value.status === 'assigned' ||
			value.status === 'in_progress' ||
			value.status === 'completed') &&
		isString(value.assigned_at) &&
		isNullableString(value.started_at) &&
		isNullableString(value.completed_at) &&
		isNullableString(value.last_matching_action)
	);
}

function isWorldSnapshot(value: unknown): value is WorldSnapshot {
	return (
		isRecord(value) &&
		Number.isSafeInteger(value.location_id) &&
		isString(value.location_name) &&
		isString(value.location_description) &&
		isString(value.time_label) &&
		isFiniteNumber(value.hour) &&
		isFiniteNumber(value.minute) &&
		isString(value.weather) &&
		isString(value.season) &&
		isNullableString(value.festival) &&
		typeof value.paused === 'boolean' &&
		typeof value.inference_paused === 'boolean' &&
		isFiniteNumber(value.game_epoch_ms) &&
		isFiniteNumber(value.speed_factor) &&
		Array.isArray(value.name_hints) &&
		value.name_hints.every(isLanguageHint) &&
		isString(value.day_of_week) &&
		Array.isArray(value.active_tasks) &&
		value.active_tasks.every(isPlayerTask) &&
		isOptionalBoolean(value.turn_in_flight)
	);
}

function isMapData(value: unknown): value is MapData {
	if (
		!isRecord(value) ||
		!Array.isArray(value.locations) ||
		!Array.isArray(value.edges) ||
		!isString(value.player_location) ||
		!isString(value.transport_label) ||
		!isString(value.transport_id)
	) {
		return false;
	}

	const locationsValid = value.locations.every(
		(location) =>
			isRecord(location) &&
			isString(location.id) &&
			isString(location.name) &&
			isFiniteNumber(location.lat) &&
			isFiniteNumber(location.lon) &&
			typeof location.adjacent === 'boolean' &&
			isFiniteNumber(location.hops) &&
			(location.indoor === undefined || typeof location.indoor === 'boolean') &&
			(location.travel_minutes === undefined ||
				isFiniteNumber(location.travel_minutes)) &&
			(location.visited === undefined || typeof location.visited === 'boolean'),
	);
	const edgesValid = value.edges.every(
		(edge) =>
			Array.isArray(edge) &&
			edge.length === 2 &&
			edge.every((endpoint) => isString(endpoint)),
	);
	const traversalsValid =
		value.edge_traversals === undefined ||
		(Array.isArray(value.edge_traversals) &&
			value.edge_traversals.every(
				(edge) =>
					Array.isArray(edge) &&
					edge.length === 3 &&
					isString(edge[0]) &&
					isString(edge[1]) &&
					isFiniteNumber(edge[2]),
			));
	return locationsValid && edgesValid && traversalsValid;
}

function isNpcInfo(value: unknown): value is NpcInfo {
	return (
		isRecord(value) &&
		isString(value.name) &&
		isString(value.real_name) &&
		isString(value.occupation) &&
		isString(value.mood) &&
		typeof value.introduced === 'boolean' &&
		isString(value.mood_emoji)
	);
}

export function isReconnectState(value: unknown): value is ReconnectState {
	return (
		isRecord(value) &&
		isWorldSnapshot(value.world) &&
		isMapData(value.map) &&
		Array.isArray(value.npcs) &&
		value.npcs.every(isNpcInfo) &&
		Number.isSafeInteger(value.context_epoch) &&
		(value.context_epoch as number) >= 0
	);
}

/**
 * Wires up initial data load + real-time event subscriptions for the app
 * shell and returns a cleanup function that tears them all down.
 */
export async function createPageController(
	isCancelled: () => boolean = () => false,
): Promise<() => void> {
	let disposed = false;
	const controllerActive = () => !disposed && !isCancelled();

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

	// Initial canonical data uses the same persistence-gated aggregate as
	// reconnect. Startup must not display world/map/NPCs captured from different
	// generations. Theme remains independently available and best-effort.
	const [stateRes, themeRes] = await Promise.allSettled([
		getReconnectState(),
		getTheme(),
	]);
	let initialContextEpoch: number | null = null;
	let initialStateFailure: unknown = null;
	if (
		stateRes.status === 'fulfilled' &&
		isReconnectState(stateRes.value) &&
		controllerActive()
	) {
		const state = stateRes.value;
		const snap = state.world;
		worldState.set(state.world);
		mapData.set(state.map);
		npcsHere.set(state.npcs);
		palette.applyGameHour(snap.hour);
		nameHints.set(snap.name_hints);
		streamingActive.set(Boolean(snap.turn_in_flight));
		initialContextEpoch = state.context_epoch;
		if (
			snap.location_description &&
			sceneDedup.shouldShowDescription(snap.location_name)
		) {
			textLog.update((log) => [
				...log,
				{
					source: 'system',
					subtype: 'location',
					content: snap.location_description,
				},
			]);
		}
	} else if (stateRes.status === 'rejected') {
		initialStateFailure = stateRes.reason;
	} else if (
		stateRes.status === 'fulfilled' &&
		!isReconnectState(stateRes.value)
	) {
		initialStateFailure = new Error('invalid aggregate payload');
	}
	if (themeRes.status === 'fulfilled' && controllerActive())
		palette.applyServerPalette(themeRes.value);

	const failed: string[] = [];
	if (initialStateFailure !== null)
		failed.push(`game state (${formatIpcError(initialStateFailure)})`);
	if (themeRes.status === 'rejected')
		failed.push(`theme (${formatIpcError(themeRes.reason)})`);
	if (failed.length > 0) {
		pushErrorLog(`Failed to load initial game data: ${failed.join(', ')}.`);
		if (initialStateFailure !== null)
			console.warn('Initial fetch failed:', initialStateFailure);
		if (themeRes.status === 'rejected')
			console.warn('Initial fetch failed:', themeRes.reason);
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
	const reconnectPresentation: ReconnectPresentationState = {
		sceneDedup,
		contextEpoch: initialContextEpoch,
		generation: 0,
		isActive: controllerActive,
		resetStream: () => sm.reset(),
	};
	let worldRefreshRevision = 0;
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
			await onGameContextReset(() => {
				if (!controllerActive()) return;
				disarmLoadingSafetyTimer();
				resetExternalDrive();
				resetPresentationForNewContext(reconnectPresentation);
				worldRefreshRevision += 1;
			}),
		);

		listeners.push(
			await onWorldUpdate(async (_snap) => {
				const refreshRevision = ++worldRefreshRevision;
				const acceptedWorld = await refreshCanonicalStateAfterWorldUpdate(
					reconnectPresentation,
					refreshRevision,
					() => worldRefreshRevision,
				);
				if (acceptedWorld) tracker.onWorldStateChange(acceptedWorld.paused);
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

		// Resync authoritative state after a WebSocket reconnect. The aggregate
		// command captures world/map/NPCs plus context epoch under one backend
		// persistence gate. Its helper validates the whole envelope before it
		// cancels an orphaned stream or mutates any presentation state.
		listeners.push(
			onReconnect(async () => {
				const applied = await resyncCanonicalStateAfterReconnect(
					reconnectPresentation,
				);
				if (applied && controllerActive()) {
					worldRefreshRevision += 1;
				}
			}),
		);
	} catch (e) {
		console.warn('Failed to set up some event listeners:', e);
	}

	// An aggregate may have completed before the reset/world listeners above
	// were attached. Re-read after subscription so a context switch in that gap
	// cannot strand the page on its initial world. Generation and disposal
	// guards reject a response overtaken by an event or unmount.
	if (controllerActive()) {
		const applied = await resyncCanonicalStateAfterSubscription(
			reconnectPresentation,
		);
		if (applied && controllerActive()) worldRefreshRevision += 1;
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
		disposed = true;
		reconnectPresentation.generation += 1;
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
