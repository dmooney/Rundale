<script lang="ts">
	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	const SHORTCUTS: Array<{ keys: string; desc: string }> = [
		{ keys: 'M', desc: 'Toggle the full parish map' },
		{ keys: 'F2', desc: 'Save a screenshot' },
		{ keys: 'F5', desc: 'Open the Ledger (save / load)' },
		{ keys: 'F10', desc: 'Toggle the demo panel' },
		{ keys: 'F11', desc: 'Toggle fullscreen (desktop)' },
		{ keys: 'F12', desc: 'Toggle the debug panel' },
		{ keys: '?', desc: 'Show this overlay' },
		{ keys: 'Tab', desc: 'Move through chat and surface controls' },
		{ keys: 'Enter', desc: 'Activate a control or send the current message' },
		{ keys: 'Esc', desc: 'Close dismissible overlays / stop the demo' },
	];

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === '?') {
			e.preventDefault();
			e.stopPropagation();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="shortcuts-container" data-testid="shortcuts-overlay">
	<button
		type="button"
		class="shortcuts-backdrop"
		aria-label="Dismiss shortcuts overlay"
		tabindex="-1"
		onclick={onclose}
	></button>
	<div
		class="shortcuts-card"
		role="dialog"
		aria-modal="true"
		aria-label="Keyboard shortcuts"
	>
		<div class="card-header">
			<span class="card-title">Keyboard Shortcuts</span>
			<button
				type="button"
				class="close-btn"
				aria-label="Close shortcuts"
				title="Close (Esc)"
				onclick={onclose}
			>
				<span aria-hidden="true">&times;</span>
			</button>
		</div>
		<dl class="shortcut-list">
			{#each SHORTCUTS as s (s.keys)}
				<dt><kbd>{s.keys}</kbd></dt>
				<dd>{s.desc}</dd>
			{/each}
		</dl>
	</div>
</div>

<style>
	.shortcuts-container {
		position: fixed;
		inset: 0;
		z-index: 90;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* The backdrop is a sibling button (not a clickable wrapper) so the
	   dialog card is never nested inside an interactive control. */
	.shortcuts-backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		background: rgba(0, 0, 0, 0.35);
		border: none;
		margin: 0;
		padding: 0;
		cursor: default;
	}

	.shortcuts-card {
		position: relative;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
		width: min(26rem, 92vw);
		max-height: 80vh;
		overflow-y: auto;
	}

	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.6rem 0.9rem;
		border-bottom: 1px solid var(--color-border);
	}

	.card-title {
		font-family: var(--font-display);
		font-size: 0.72rem;
		letter-spacing: 0.13em;
		text-transform: uppercase;
		color: var(--color-accent);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-muted);
		font-size: 1.3rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 4px;
	}

	.close-btn:hover,
	.close-btn:focus-visible {
		color: var(--color-fg);
	}

	.shortcut-list {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.45rem 0.9rem;
		margin: 0;
		padding: 0.9rem;
		align-items: baseline;
	}

	dt {
		text-align: right;
	}

	dd {
		margin: 0;
		font-size: 0.9rem;
		color: var(--color-fg);
	}

	kbd {
		font-family: var(--font-display);
		font-size: 0.68rem;
		letter-spacing: 0.06em;
		color: var(--color-muted);
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		padding: 0.12rem 0.4rem;
		white-space: nowrap;
	}
</style>
