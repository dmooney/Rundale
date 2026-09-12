<script lang="ts">
	import { demoEnabled, demoPaused, demoTurnCount, demoStatus } from '../stores/demo';
	import { stopDemo } from '../lib/demo-player';

	function togglePause() {
		demoPaused.update((v) => !v);
	}

	const statusLabel: Record<string, string> = {
		idle: 'idle',
		waiting: 'waiting...',
		thinking: 'thinking...',
		acting: 'acting'
	};
</script>

{#if $demoEnabled}
	<div class="demo-banner" role="status" aria-live="polite">
		<span class="demo-label">DEMO</span>
		<span class="demo-turn">Turn {$demoTurnCount}</span>
		<span class="demo-status">{statusLabel[$demoStatus] ?? $demoStatus}</span>
		<button type="button" class="demo-btn" onclick={togglePause}>
			{$demoPaused ? 'Resume' : 'Pause'}
		</button>
		<button type="button" class="demo-btn demo-stop" onclick={stopDemo}>Stop</button>
	</div>
{/if}

<style>
	.demo-banner {
		position: fixed;
		top: 0.5rem;
		left: 50%;
		transform: translateX(-50%);
		z-index: 200;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.3rem 0.9rem;
		background: var(--color-panel-bg, #1a1a2e);
		border: 1px solid var(--color-accent, #b08531);
		font-family: var(--font-display, monospace);
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		color: var(--color-fg, #ccc);
		pointer-events: all;
	}

	.demo-label {
		color: var(--color-accent, #b08531);
		font-weight: bold;
	}

	.demo-status {
		color: var(--color-muted, #888);
		min-width: 6rem;
	}

	.demo-btn {
		background: none;
		border: 1px solid var(--color-border, #444);
		color: var(--color-fg, #ccc);
		font-family: inherit;
		font-size: inherit;
		letter-spacing: inherit;
		padding: 0.15rem 0.5rem;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}

	.demo-btn:hover,
	.demo-btn:focus-visible {
		color: var(--color-accent, #b08531);
		border-color: var(--color-accent, #b08531);
	}

	.demo-stop:hover,
	.demo-stop:focus-visible {
		color: #c0392b;
		border-color: #c0392b;
	}
</style>
