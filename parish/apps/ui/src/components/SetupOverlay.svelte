<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import {
		getSetupSnapshot,
		isTauri,
		onSetupStatus,
		onSetupProgress,
		onSetupDone,
		type SetupSnapshot,
		type SetupStatusPayload,
		type SetupProgressPayload,
		type SetupDonePayload
	} from '$lib/ipc';
	import { LONG_WAIT_MESSAGES } from '$lib/setupWaitMessages';

	const tauri = isTauri();
	const INITIAL_SETUP_MESSAGE = 'Preparing the storyteller...';
	const SETUP_COMPLETE_SESSION_KEY = 'rundale-setup-complete';
	const SETUP_ACTIVITY_SESSION_KEY = 'rundale-setup-activity';
	const SETUP_START_MESSAGE = 'Starting inference provider setup...';
	const SETUP_HISTORY_LIMIT = 80;
	const RATE_SAMPLE_WINDOW_MS = 30_000;
	const RATE_UPDATE_INTERVAL_MS = 750;
	const MIN_RATE_SAMPLE_SPAN_MS = 750;
	const RATE_NEW_SAMPLE_WEIGHT = 0.15;
	const WAIT_MESSAGE_FIRST_DELAY_MS = 2_500;
	const WAIT_MESSAGE_INTERVAL_MS = 7_000;

	type StoredSetupActivity = {
		currentPhrase: string;
		messages: string[];
		completed: number;
		total: number;
	};

	let setupComplete = readSetupCompleteFlag();
	const initialActivity = setupComplete ? null : readSetupActivity();
	let visible = $state(false);
	let fading = $state(false);
	let messages: string[] = $state(initialActivity?.messages ?? []);
	let currentPhrase = $state(initialActivity?.currentPhrase ?? INITIAL_SETUP_MESSAGE);
	let downloadCompleted = $state(initialActivity?.completed ?? 0);
	let downloadTotal = $state(initialActivity?.total ?? 0);
	let hasError = $state(false);
	let errorMsg = $state('');
	let messagesEl = $state<HTMLDivElement | undefined>(undefined);
	let elapsedSeconds = $state(0);
	let elapsedTimer: ReturnType<typeof setInterval> | null = null;
	let hideTimer: ReturnType<typeof setTimeout> | null = null;
	let waitMessageTimer: ReturnType<typeof setTimeout> | null = null;
	let progressSamples: Array<{ completed: number; timestamp: number }> = [];
	let lastRateUpdateAt = 0;
	let waitMessageIndex = 0;
	let downloadSpeedBps = $state<number | null>(null);
	let downloadEtaSeconds = $state<number | null>(null);
	let longSetupWaitActive = false;
	let receivedLiveSetupEvent = false;
	let receivedSetupDone = false;

	let downloadPct = $derived(
		downloadTotal > 0 ? Math.min(100, (downloadCompleted / downloadTotal) * 100) : null
	);
	let downloadStatsText = $derived(formatDownloadStats());
	let visibleMessages = $derived(messages.length > 0 ? messages : [INITIAL_SETUP_MESSAGE]);

	let cleanupFns: Array<() => void> = [];

	function readSetupCompleteFlag() {
		try {
			return sessionStorage.getItem(SETUP_COMPLETE_SESSION_KEY) === 'true';
		} catch {
			return false;
		}
	}

	function readSetupActivity(): StoredSetupActivity | null {
		try {
			const raw = sessionStorage.getItem(SETUP_ACTIVITY_SESSION_KEY);
			if (!raw) return null;
			const parsed = JSON.parse(raw) as Partial<StoredSetupActivity>;
			const storedMessages = Array.isArray(parsed.messages)
				? parsed.messages.filter((message): message is string => typeof message === 'string')
				: [];
			const displayMessages = storedMessages.map(formatSetupStatusMessage);
			return {
				currentPhrase:
					typeof parsed.currentPhrase === 'string'
						? formatSetupStatusMessage(parsed.currentPhrase)
						: displayMessages.at(-1) ?? INITIAL_SETUP_MESSAGE,
				messages: compactSetupMessages(displayMessages),
				completed: typeof parsed.completed === 'number' ? parsed.completed : 0,
				total: typeof parsed.total === 'number' ? parsed.total : 0
			};
		} catch {
			return null;
		}
	}

	function markSetupComplete() {
		setupComplete = true;
		try {
			sessionStorage.setItem(SETUP_COMPLETE_SESSION_KEY, 'true');
		} catch {
			// Ignore storage failures; the backend snapshot is still authoritative.
		}
		clearSetupActivity();
	}

	function clearSetupComplete() {
		setupComplete = false;
		try {
			sessionStorage.removeItem(SETUP_COMPLETE_SESSION_KEY);
		} catch {
			// Ignore storage failures; this only affects remount flash suppression.
		}
	}

	function persistSetupActivity() {
		if (setupComplete) return;
		try {
			sessionStorage.setItem(
				SETUP_ACTIVITY_SESSION_KEY,
				JSON.stringify({
					currentPhrase,
					messages,
					completed: downloadCompleted,
					total: downloadTotal
				})
			);
		} catch {
			// Ignore storage failures; this only preserves activity across remounts.
		}
	}

	function clearSetupActivity() {
		try {
			sessionStorage.removeItem(SETUP_ACTIVITY_SESSION_KEY);
		} catch {
			// Ignore storage failures.
		}
	}

	function clearHideTimer() {
		if (hideTimer !== null) {
			clearTimeout(hideTimer);
			hideTimer = null;
		}
	}

	function clearWaitMessageTimer() {
		if (waitMessageTimer !== null) {
			clearTimeout(waitMessageTimer);
			waitMessageTimer = null;
		}
	}

	function compactSetupMessages(nextMessages: string[]) {
		return nextMessages
			.filter((message) => message.trim().length > 0)
			.slice(-SETUP_HISTORY_LIMIT);
	}

	function formatSetupStatusMessage(message: string) {
		const trimmed = message.trim();
		if (message === SETUP_START_MESSAGE) {
			return 'Opening the parish ledger (starting inference provider setup)...';
		}
		if (trimmed === 'pulling manifest') {
			return 'Reading the storyteller\'s table of contents (Ollama: pulling manifest). The first download can take a few minutes.';
		}
		if (trimmed === 'verifying sha256 digest') {
			return 'Checking the wax seals (Ollama: verifying sha256 digest)...';
		}
		if (trimmed === 'writing manifest') {
			return 'Filing the table of contents in the parish ledger (Ollama: writing manifest)...';
		}
		if (trimmed === 'success') {
			return 'The parcels are in order (Ollama: success).';
		}

		const forced = message.match(/^Forcing a fresh download of '(.+)'\.$/);
		if (forced) {
			return `A clean slate it is: forcing a fresh download of '${forced[1]}'. This big fetch is a one-time thing unless you force it again.`;
		}

		const clearing = message.match(/^Clearing the local copy of '(.+)' before fetching it again\.\.\.$/);
		if (clearing) {
			return `Sweeping out the old local copy of '${clearing[1]}' before fetching it again...`;
		}

		const removed = message.match(/^Local copy of '(.+)' removed\. Fetching a fresh copy\.\.\.$/);
		if (removed) {
			return `The old '${removed[1]}' copy has left the parish. Fetching a fresh one now...`;
		}

		const missing = message.match(/^No local copy of '(.+)' was present\. Fetching it now\.\.\.$/);
		if (missing) {
			return `No local copy of '${missing[1]}' turned up in the ledger. Fetching it now...`;
		}

		const fetching = message.match(/^Fetching the storyteller's book of tales \('(.+)'\)\.\.\.$/);
		if (fetching) {
			return `Fetching the storyteller's book of tales ('${fetching[1]}'). This is the big one-time model download; later starts should be much quicker...`;
		}

		return message;
	}

	function sentenceBreakParts(message: string) {
		const parts = message.split(/(?<=\.)\s+/).filter((part) => part.length > 0);
		return parts.map((text, index) => ({
			text,
			breakAfter: index < parts.length - 1
		}));
	}

	function isLongSetupWaitMessage(message: string) {
		const lower = message.toLowerCase();
		return (
			lower.includes('pulling manifest') ||
			lower.includes('book of tales') ||
			lower.includes('fresh download') ||
			lower.includes('fresh copy') ||
			lower.includes('one-time model download') ||
			lower.includes('local copy') ||
			lower.includes('download')
		);
	}

	function mergeSetupMessages(snapshotMessages: string[]) {
		const incoming = compactSetupMessages(snapshotMessages);
		if (messages.length === 0) {
			return incoming.length > 0 ? incoming : [INITIAL_SETUP_MESSAGE];
		}
		if (incoming.length === 0) return messages;
		if (incoming.length === 1 && incoming[0] === INITIAL_SETUP_MESSAGE) {
			return messages;
		}

		const merged = [...messages];
		for (const message of incoming) {
			if (!merged.includes(message)) {
				merged.push(message);
			}
		}
		return compactSetupMessages(merged);
	}

	function showSetupOverlay() {
		if (setupComplete) return;
		clearHideTimer();
		fading = false;
		visible = true;
	}

	function scrollMessages() {
		tick().then(() => {
			if (messagesEl) {
				messagesEl.scrollTop = messagesEl.scrollHeight;
			}
		});
	}

	function appendStatusMessage(message: string) {
		const displayMessage = formatSetupStatusMessage(message);
		if (message === SETUP_START_MESSAGE) {
			messages = [];
			downloadCompleted = 0;
			downloadTotal = 0;
			resetDownloadRate();
			elapsedSeconds = 0;
			longSetupWaitActive = false;
		}
		currentPhrase = displayMessage;
		messages =
			messages.at(-1) === displayMessage
				? messages
				: compactSetupMessages([...messages, displayMessage]);
		if (isLongSetupWaitMessage(message) || isLongSetupWaitMessage(displayMessage)) {
			startLongSetupWait();
		} else if (message === SETUP_START_MESSAGE) {
			stopLongSetupWait();
		}
		persistSetupActivity();
		scrollMessages();
	}

	function startLongSetupWait(delay = WAIT_MESSAGE_FIRST_DELAY_MS) {
		longSetupWaitActive = true;
		if (waitMessageTimer !== null) return;

		waitMessageTimer = setTimeout(() => {
			waitMessageTimer = null;
			if (!visible || hasError || setupComplete || receivedSetupDone || !longSetupWaitActive) {
				return;
			}
			appendWaitMessage();
			startLongSetupWait(WAIT_MESSAGE_INTERVAL_MS);
		}, delay);
	}

	function stopLongSetupWait() {
		longSetupWaitActive = false;
		clearWaitMessageTimer();
	}

	function appendWaitMessage() {
		const message = LONG_WAIT_MESSAGES[waitMessageIndex % LONG_WAIT_MESSAGES.length];
		waitMessageIndex += 1;
		currentPhrase = message;
		messages =
			messages.at(-1) === message
				? messages
				: compactSetupMessages([...messages, message]);
		persistSetupActivity();
		scrollMessages();
	}

	function applySetupProgress(completed: number, total: number, sampleSpeed = false) {
		const switchedTransfer =
			total > 0 &&
			downloadTotal > 0 &&
			(total < downloadTotal || completed < downloadCompleted);
		if (total > 0 && completed < total) {
			startLongSetupWait();
		} else if (total > 0 && completed >= total) {
			stopLongSetupWait();
		}

		if (!sampleSpeed || total <= 0 || switchedTransfer) {
			resetDownloadRate(sampleSpeed && total > 0 ? completed : undefined);
		} else {
			recordDownloadSample(completed, total);
		}

		downloadCompleted = completed;
		downloadTotal = total;
		persistSetupActivity();
	}

	function resetDownloadRate(initialCompleted?: number) {
		downloadSpeedBps = null;
		downloadEtaSeconds = null;
		lastRateUpdateAt = 0;
		progressSamples =
			initialCompleted === undefined
				? []
				: [{ completed: initialCompleted, timestamp: performance.now() }];
	}

	function updateEtaFromDisplayedSpeed(completed: number, total: number) {
		if (downloadSpeedBps === null || downloadSpeedBps <= 0 || total <= completed) {
			downloadEtaSeconds = null;
			return;
		}

		downloadEtaSeconds = (total - completed) / downloadSpeedBps;
	}

	function recordDownloadSample(completed: number, total: number) {
		const now = performance.now();
		progressSamples = [
			...progressSamples.filter((sample) => sample.timestamp >= now - RATE_SAMPLE_WINDOW_MS),
			{ completed, timestamp: now }
		];

		const first = progressSamples[0];
		const last = progressSamples.at(-1);
		if (!first || !last || last.completed <= first.completed) {
			updateEtaFromDisplayedSpeed(completed, total);
			return;
		}

		const spanMs = last.timestamp - first.timestamp;
		if (spanMs < MIN_RATE_SAMPLE_SPAN_MS) {
			updateEtaFromDisplayedSpeed(completed, total);
			return;
		}
		if (lastRateUpdateAt > 0 && now - lastRateUpdateAt < RATE_UPDATE_INTERVAL_MS) {
			updateEtaFromDisplayedSpeed(completed, total);
			return;
		}

		const windowBps = (last.completed - first.completed) / (spanMs / 1000);
		downloadSpeedBps =
			downloadSpeedBps === null
				? windowBps
				: downloadSpeedBps * (1 - RATE_NEW_SAMPLE_WEIGHT) + windowBps * RATE_NEW_SAMPLE_WEIGHT;

		updateEtaFromDisplayedSpeed(completed, total);
		lastRateUpdateAt = now;
	}

	function applySetupDone(success: boolean, error: string) {
		stopLongSetupWait();
		if (success) {
			markSetupComplete();
			if (!visible) {
				clearHideTimer();
				visible = false;
				fading = false;
				return;
			}

			appendStatusMessage('The storyteller is ready.');
			fading = true;
			clearHideTimer();
			hideTimer = setTimeout(() => {
				visible = false;
				hideTimer = null;
			}, 650);
		} else {
			clearSetupComplete();
			showSetupOverlay();
			hasError = true;
			errorMsg = error;
			appendStatusMessage(error ? `Setup failed: ${error}` : 'Setup failed.');
		}
	}

	function applySetupSnapshot(snapshot: SetupSnapshot) {
		if (snapshot.done && snapshot.success === true) {
			markSetupComplete();
			stopLongSetupWait();
			clearHideTimer();
			fading = false;
			visible = false;
			return;
		}

		const rawSnapshotMessages =
			snapshot.messages.length > 0
				? snapshot.messages
				: [snapshot.current_message || INITIAL_SETUP_MESSAGE];
		const snapshotMessages = rawSnapshotMessages.map(formatSetupStatusMessage);
		const staleFallbackSnapshot =
			receivedLiveSetupEvent &&
			!snapshot.done &&
			snapshot.success === null &&
			snapshot.completed === 0 &&
			snapshot.total === 0 &&
			snapshotMessages.length === 1 &&
			snapshotMessages[0] === INITIAL_SETUP_MESSAGE;
		if (staleFallbackSnapshot) {
			return;
		}

		clearSetupComplete();
		showSetupOverlay();
		const ignoreDefaultSnapshot =
			messages.length > 0 &&
			snapshotMessages.length === 1 &&
			snapshotMessages[0] === INITIAL_SETUP_MESSAGE;
		messages = mergeSetupMessages(snapshotMessages);
		if (!ignoreDefaultSnapshot) {
			currentPhrase =
				formatSetupStatusMessage(snapshot.current_message) ||
				snapshotMessages.at(-1) ||
				currentPhrase ||
				INITIAL_SETUP_MESSAGE;
		}
		applySetupProgress(snapshot.completed, snapshot.total);
		const snapshotHasLongWait =
			(snapshot.completed > 0 && snapshot.completed < snapshot.total) ||
			rawSnapshotMessages.some(isLongSetupWaitMessage) ||
			snapshotMessages.some(isLongSetupWaitMessage) ||
			isLongSetupWaitMessage(snapshot.current_message);
		if (snapshotHasLongWait) {
			startLongSetupWait();
		} else {
			stopLongSetupWait();
		}

		if (snapshot.done && snapshot.success !== null) {
			if (!snapshot.success) {
				hasError = true;
				errorMsg = snapshot.error;
			} else {
				stopLongSetupWait();
			}
		}
		scrollMessages();
	}

	function formatElapsed(seconds: number) {
		const mins = Math.floor(seconds / 60);
		const secs = seconds % 60;
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	function formatBytes(bytes: number) {
		const units = ['B', 'KB', 'MB', 'GB'];
		let value = Math.max(0, bytes);
		let unitIndex = 0;
		while (value >= 1024 && unitIndex < units.length - 1) {
			value /= 1024;
			unitIndex += 1;
		}
		const digits = value >= 100 || unitIndex === 0 ? 0 : value >= 10 ? 1 : 2;
		return `${value.toFixed(digits)} ${units[unitIndex]}`;
	}

	function formatDuration(seconds: number) {
		const totalSeconds = Math.max(0, Math.round(seconds));
		const mins = Math.floor(totalSeconds / 60);
		const secs = totalSeconds % 60;
		if (mins >= 60) {
			const hours = Math.floor(mins / 60);
			const restMins = mins % 60;
			return `${hours}h ${restMins}m`;
		}
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	function formatDownloadStats() {
		if (downloadTotal <= 0) return '';

		const parts = [`${formatBytes(downloadCompleted)} of ${formatBytes(downloadTotal)}`];
		if (downloadSpeedBps !== null && downloadSpeedBps > 0) {
			parts.push(`${formatBytes(downloadSpeedBps)}/s`);
		}
		if (downloadEtaSeconds !== null) {
			parts.push(`${formatDuration(downloadEtaSeconds)} left`);
		}
		return parts.join(' • ');
	}

	onMount(async () => {
		if (!tauri) return;

		elapsedTimer = setInterval(() => {
			elapsedSeconds += 1;
		}, 1000);

		const [statusCleanup, progressCleanup, doneCleanup] = await Promise.all([
			onSetupStatus((p: SetupStatusPayload) => {
				receivedLiveSetupEvent = true;
				if (setupComplete) clearSetupComplete();
				showSetupOverlay();
				appendStatusMessage(p.message);
			}),
			onSetupProgress((p: SetupProgressPayload) => {
				receivedLiveSetupEvent = true;
				if (setupComplete) clearSetupComplete();
				showSetupOverlay();
				applySetupProgress(p.completed, p.total, true);
			}),
			onSetupDone((p: SetupDonePayload) => {
				receivedLiveSetupEvent = true;
				receivedSetupDone = true;
				applySetupDone(p.success, p.error);
			})
		]);
		cleanupFns.push(statusCleanup, progressCleanup, doneCleanup);

		try {
			const snapshot = await getSetupSnapshot();
			if (!receivedSetupDone) {
				applySetupSnapshot(snapshot);
			}
		} catch {
			if (!receivedLiveSetupEvent) {
				showSetupOverlay();
				appendStatusMessage('Waiting for setup updates...');
			}
		}
	});

	onDestroy(() => {
		if (elapsedTimer !== null) {
			clearInterval(elapsedTimer);
			elapsedTimer = null;
		}
		clearHideTimer();
		clearWaitMessageTimer();
		cleanupFns.forEach((fn) => fn());
	});
</script>

{#if visible}
	<div class="setup-overlay" class:fading>
		<div class="setup-box">
			<h1 class="game-title">Rundale</h1>

			<svg
				class="triquetra-spinner"
				viewBox="0 0 100 100"
				xmlns="http://www.w3.org/2000/svg"
				aria-hidden="true"
			>
				<circle
					class="knot-circle"
					pathLength="120"
					cx="50"
					cy="50"
					r="16"
					fill="none"
					stroke="currentColor"
					stroke-width="3"
					stroke-linecap="round"
				/>
				<path
					class="triquetra-path"
					pathLength="120"
					d="M 50 22
					   A 28 28 0 0 0 74.25 64
					   A 28 28 0 0 0 25.75 64
					   A 28 28 0 0 0 50 22 Z"
					fill="none"
					stroke="currentColor"
					stroke-width="3"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>

			{#if currentPhrase && !hasError}
				<p class="current-phrase">
					{#key currentPhrase}
						<span class="phrase-text message-fade">
							{#each sentenceBreakParts(currentPhrase) as part}
								{part.text}{#if part.breakAfter}<wbr />{' '}{/if}
							{/each}
						</span>
					{/key}
				</p>
			{/if}

			<div
				class="progress-track"
				class:indeterminate={downloadPct === null}
				role="progressbar"
				aria-label="Setup progress"
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={downloadPct === null ? undefined : Math.round(downloadPct)}
			>
				<div
					class="progress-fill"
					style={downloadPct === null ? undefined : `width: ${downloadPct}%`}
				></div>
			</div>
			{#if downloadPct !== null}
				<p class="progress-label" aria-label={`${downloadPct.toFixed(1)}%`}>
					{#each `${downloadPct.toFixed(1)}%`.split('') as char}
						<span
							class="progress-char"
							class:digit={char >= '0' && char <= '9'}
							class:dot={char === '.'}
							class:percent={char === '%'}
						>{char}</span>
					{/each}
				</p>
			{/if}
			{#if downloadStatsText}
				<p class="download-stats" aria-label={downloadStatsText}>
					{#each downloadStatsText.split('') as char}
						<span
							class="stat-char"
							class:digit={char >= '0' && char <= '9'}
							class:dot={char === '.'}
							class:colon={char === ':'}
							class:space={char === ' '}
							class:separator={char === '•'}
						>{char === ' ' ? '\u00a0' : char}</span>
					{/each}
				</p>
			{/if}

			<div class="activity-panel">
				<div class="activity-header">
					<span>Activity</span>
					<span class="elapsed">{formatElapsed(elapsedSeconds)}</span>
				</div>
				<div class="messages" bind:this={messagesEl}>
					{#each visibleMessages as msg, i}
						<p class="msg message-fade" class:latest={i === visibleMessages.length - 1}>
							{#each sentenceBreakParts(msg) as part}
								{part.text}{#if part.breakAfter}<wbr />{' '}{/if}
							{/each}
						</p>
					{/each}
				</div>
			</div>

			{#if hasError}
				<div class="error-box">
					<p class="error-title">Something went wrong.</p>
					<p class="error-msg">{errorMsg}</p>
					<p class="error-hint">Close the app and check the terminal for details.</p>
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.setup-overlay {
		position: fixed;
		inset: 0;
		z-index: 200;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--color-bg);
		opacity: 1;
		transition: opacity 0.6s ease;
	}

	.setup-overlay.fading {
		opacity: 0;
		pointer-events: none;
	}

	.setup-box {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1.25rem;
		max-width: 39rem;
		width: min(94vw, 39rem);
		text-align: center;
	}

	.game-title {
		margin: 0 0 -0.3rem;
		color: var(--color-fg);
		font-family: var(--font-display);
		font-size: 4.75rem;
		font-weight: 400;
		line-height: 0.95;
		letter-spacing: 0;
		text-shadow:
			0 2px 0 color-mix(in srgb, var(--color-accent) 28%, transparent),
			0 0 18px rgba(176, 133, 49, 0.16);
	}

	.triquetra-spinner {
		width: 6rem;
		height: 6rem;
		animation: triquetra-rotate 6s linear infinite;
	}

	.triquetra-path {
		stroke: rgb(72, 199, 142);
		stroke-dasharray: 80 40;
		stroke-dashoffset: 0;
		filter: drop-shadow(0 0 10px rgba(72, 199, 142, 0.38));
		animation:
			triquetra-draw 2.4s linear infinite,
			setup-spinner-stroke-cycle 18s ease-in-out infinite;
	}

	.knot-circle {
		stroke: rgb(72, 199, 142);
		stroke-dasharray: 0 120;
		stroke-dashoffset: 0;
		filter: drop-shadow(0 0 10px rgba(72, 199, 142, 0.38));
		animation:
			circle-draw 3s ease-in-out infinite,
			setup-spinner-stroke-cycle 18s ease-in-out infinite;
		animation-delay: 0.4s, 0s;
	}

	@keyframes triquetra-draw {
		to {
			stroke-dashoffset: -120;
		}
	}

	@keyframes circle-draw {
		0%   { stroke-dasharray: 0 120;   stroke-dashoffset: 0; }
		30%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		70%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		100% { stroke-dasharray: 0 120;   stroke-dashoffset: -120; }
	}

	@keyframes triquetra-rotate {
		to {
			transform: rotate(360deg);
		}
	}

	/* Mirrors parish-core/src/loading.rs SPINNER_COLORS. */
	@keyframes setup-spinner-stroke-cycle {
		0%,
		100% {
			stroke: rgb(72, 199, 142);
			filter: drop-shadow(0 0 10px rgba(72, 199, 142, 0.38));
		}
		16.666% {
			stroke: rgb(255, 200, 87);
			filter: drop-shadow(0 0 10px rgba(255, 200, 87, 0.34));
		}
		33.333% {
			stroke: rgb(100, 149, 237);
			filter: drop-shadow(0 0 10px rgba(100, 149, 237, 0.34));
		}
		50% {
			stroke: rgb(255, 160, 100);
			filter: drop-shadow(0 0 10px rgba(255, 160, 100, 0.32));
		}
		66.666% {
			stroke: rgb(180, 130, 255);
			filter: drop-shadow(0 0 10px rgba(180, 130, 255, 0.32));
		}
		83.333% {
			stroke: rgb(120, 220, 180);
			filter: drop-shadow(0 0 10px rgba(120, 220, 180, 0.36));
		}
	}

	.current-phrase {
		color: var(--color-accent);
		font-size: 1.3rem;
		font-style: italic;
		line-height: 1.35;
		margin: 0;
		max-width: 37rem;
		min-height: 1.4em;
	}

	.phrase-text {
		display: inline-block;
	}

	.message-fade {
		animation: setup-message-fade 360ms ease-out both;
	}

	@keyframes setup-message-fade {
		from {
			opacity: 0;
			transform: translateY(0.18rem);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.progress-track {
		width: min(100%, 32rem);
		height: 0.65rem;
		padding: 2px;
		background:
			linear-gradient(180deg, rgba(255, 255, 255, 0.12), rgba(255, 255, 255, 0.03)),
			var(--color-border, rgba(255, 255, 255, 0.1));
		border: 1px solid var(--color-border, rgba(255, 255, 255, 0.16));
		border-radius: 999px;
		box-shadow:
			inset 0 1px 4px rgba(0, 0, 0, 0.28),
			0 0 18px rgba(72, 199, 142, 0.12);
		overflow: hidden;
	}

	.progress-fill {
		width: 0;
		height: 100%;
		background:
			linear-gradient(90deg, var(--color-accent), #f0c66e, var(--color-accent));
		background-size: 200% 100%;
		border-radius: inherit;
		box-shadow:
			0 0 10px rgba(240, 198, 110, 0.35),
			0 0 18px rgba(72, 199, 142, 0.18);
		animation: progress-sheen 2.4s linear infinite;
		transition: width 0.3s ease;
	}

	.progress-track.indeterminate .progress-fill {
		width: 42%;
		animation:
			progress-sweep 1.55s ease-in-out infinite,
			progress-sheen 2.4s linear infinite;
	}

	@keyframes progress-sheen {
		to {
			background-position: 200% 0;
		}
	}

	@keyframes progress-sweep {
		0% {
			transform: translateX(-120%);
		}
		50% {
			transform: translateX(70%);
		}
		100% {
			transform: translateX(260%);
		}
	}

	.progress-label {
		display: inline-flex;
		align-items: baseline;
		justify-content: center;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		padding: 0.12rem 0.55rem;
		font-family: var(--font-body);
		font-size: 1.18rem;
		font-weight: 400;
		font-variant-numeric: tabular-nums lining-nums;
		font-feature-settings: 'tnum' 1, 'lnum' 1;
		letter-spacing: 0;
		color: var(--color-fg);
		margin: -0.55rem 0 0;
	}

	.progress-char {
		display: inline-block;
		text-align: center;
	}

	.progress-char.digit {
		width: 0.58em;
	}

	.progress-char.dot {
		width: 0.22em;
	}

	.progress-char.percent {
		width: 0.55em;
		color: var(--color-muted);
		font-size: 0.86em;
	}

	.download-stats {
		display: inline-flex;
		align-items: baseline;
		justify-content: center;
		margin: -0.75rem 0 0;
		color: var(--color-muted);
		font-family: var(--font-body);
		font-size: 0.86rem;
		font-weight: 400;
		font-variant-numeric: tabular-nums lining-nums;
		font-feature-settings: 'tnum' 1, 'lnum' 1;
		line-height: 1.3;
	}

	.stat-char {
		display: inline-block;
		text-align: center;
	}

	.stat-char.digit {
		width: 0.55em;
	}

	.stat-char.dot,
	.stat-char.colon {
		width: 0.22em;
	}

	.stat-char.space {
		width: 0.24em;
	}

	.stat-char.separator {
		width: 0.72em;
		color: var(--color-border);
		font-size: 0.86em;
	}

	.activity-panel {
		width: min(100%, 37rem);
		padding: 0.75rem;
		background:
			linear-gradient(180deg, rgba(255, 255, 255, 0.055), rgba(255, 255, 255, 0.02)),
			rgba(0, 0, 0, 0.12);
		border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
		border-radius: 8px;
		box-shadow: 0 12px 34px rgba(0, 0, 0, 0.14);
	}

	.activity-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 0.55rem;
		color: var(--color-muted);
		font-size: 0.72rem;
		font-weight: 700;
		letter-spacing: 0;
		text-transform: uppercase;
	}

	.elapsed {
		color: var(--color-accent);
		font-family: var(--font-body);
		font-variant-numeric: tabular-nums lining-nums;
		font-feature-settings: 'tnum' 1, 'lnum' 1;
	}

	.messages {
		width: 100%;
		max-height: 8rem;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.msg {
		font-size: 0.8rem;
		color: var(--color-muted);
		margin: 0;
		text-align: left;
		line-height: 1.35;
	}

	.msg.latest {
		color: var(--color-fg);
	}

	.error-box {
		width: 100%;
		border-left: 3px solid #c0554a;
		padding: 0.75rem;
		text-align: left;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.error-title {
		color: #c0554a;
		font-size: 0.95rem;
		font-weight: 600;
		margin: 0;
	}

	.error-msg {
		color: var(--color-muted);
		font-size: 0.85rem;
		font-family: monospace;
		margin: 0;
		word-break: break-word;
	}

	.error-hint {
		color: var(--color-muted);
		font-size: 0.8rem;
		font-style: italic;
		margin: 0;
	}

	@media (max-width: 420px) {
		.game-title {
			font-size: 3.35rem;
		}
	}
</style>
