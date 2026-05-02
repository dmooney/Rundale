<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import {
		isTauri,
		onSetupStatus,
		onSetupProgress,
		onSetupDone,
		type SetupStatusPayload,
		type SetupProgressPayload,
		type SetupDonePayload
	} from '$lib/ipc';

	const tauri = isTauri();

	let visible = $state(tauri);
	let fading = $state(false);
	let messages: string[] = $state([]);
	let currentPhrase = $state('');
	let downloadCompleted = $state(0);
	let downloadTotal = $state(0);
	let hasError = $state(false);
	let errorMsg = $state('');
	let messagesEl: HTMLDivElement;

	let downloadPct = $derived(
		downloadTotal > 0 ? Math.min(100, (downloadCompleted / downloadTotal) * 100) : null
	);

	let cleanupFns: Array<() => void> = [];

	onMount(async () => {
		if (!tauri) return;

		cleanupFns.push(
			await onSetupStatus((p: SetupStatusPayload) => {
				currentPhrase = p.message;
				messages = [...messages.slice(-49), p.message];
				tick().then(() => {
					if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
				});
			})
		);

		cleanupFns.push(
			await onSetupProgress((p: SetupProgressPayload) => {
				downloadCompleted = p.completed;
				downloadTotal = p.total;
			})
		);

		cleanupFns.push(
			await onSetupDone((p: SetupDonePayload) => {
				if (p.success) {
					fading = true;
					setTimeout(() => {
						visible = false;
					}, 650);
				} else {
					hasError = true;
					errorMsg = p.error;
				}
			})
		);
	});

	onDestroy(() => {
		cleanupFns.forEach((fn) => fn());
	});
</script>

{#if visible}
	<div class="setup-overlay" class:fading>
		<div class="setup-box">
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
					stroke="var(--color-accent)"
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
					stroke="var(--color-accent)"
					stroke-width="3"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>

			{#if currentPhrase && !hasError}
				<p class="current-phrase">{currentPhrase}</p>
			{/if}

			{#if downloadPct !== null}
				<div class="progress-track" role="progressbar" aria-valuenow={downloadPct} aria-valuemin={0} aria-valuemax={100}>
					<div class="progress-fill" style="width: {downloadPct}%"></div>
				</div>
				<p class="progress-label">{downloadPct.toFixed(1)}%</p>
			{/if}

			{#if messages.length > 0}
				<div class="messages" bind:this={messagesEl}>
					{#each messages as msg}
						<p class="msg">{msg}</p>
					{/each}
				</div>
			{/if}

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
		max-width: 26rem;
		width: 90%;
		text-align: center;
	}

	.triquetra-spinner {
		width: 6rem;
		height: 6rem;
		animation: triquetra-rotate 6s linear infinite;
	}

	.triquetra-path {
		stroke-dasharray: 80 40;
		stroke-dashoffset: 0;
		animation: triquetra-draw 2.4s linear infinite;
	}

	.knot-circle {
		stroke-dasharray: 0 120;
		stroke-dashoffset: 0;
		animation: circle-draw 3s ease-in-out infinite;
		animation-delay: 0.4s;
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

	.current-phrase {
		color: var(--color-accent);
		font-size: 0.95rem;
		font-style: italic;
		margin: 0;
		min-height: 1.4em;
	}

	.progress-track {
		width: 100%;
		height: 4px;
		background: var(--color-border, rgba(255, 255, 255, 0.1));
		border-radius: 2px;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--color-accent);
		border-radius: 2px;
		transition: width 0.3s ease;
	}

	.progress-label {
		font-size: 0.8rem;
		color: var(--color-muted);
		margin: -0.75rem 0 0;
	}

	.messages {
		width: 100%;
		max-height: 8rem;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		border-top: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
		padding-top: 0.75rem;
	}

	.msg {
		font-size: 0.8rem;
		color: var(--color-muted);
		margin: 0;
		text-align: left;
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
</style>
