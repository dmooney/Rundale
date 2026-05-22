<script lang="ts">
	import { getMods, switchMod } from '$lib/ipc';
	import type { ModEntry } from '$lib/types';

	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	let mods = $state<ModEntry[]>([]);
	let selected = $state<string>('');
	let loading = $state(true);
	let switching = $state(false);
	let error = $state('');
	let switched = $state(false);

	$effect(() => {
		getMods()
			.then((list) => {
				mods = list;
				const active = list.find((m) => m.active);
				if (active) selected = active.id;
			})
			.catch(() => {
				error = 'Could not load mod list.';
			})
			.finally(() => {
				loading = false;
			});
	});

	async function confirm() {
		if (!selected || switching) return;
		switching = true;
		error = '';
		try {
			const result = await switchMod(selected);
			if (!result.ok) {
				error = result.error ?? 'Switch failed.';
				switching = false;
				return;
			}
			switched = true;
		} catch (e) {
			error = String(e);
			switching = false;
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && !switching && !switched) {
			onclose();
		}
	}

	const activeId = $derived(mods.find((m) => m.active)?.id ?? '');
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay-backdrop" role="dialog" aria-modal="true" aria-label="Select mod">
	<div class="overlay-panel">
		<div class="overlay-header">
			<span class="overlay-title">Select Mod</span>
			{#if !switching && !switched}
				<button type="button" class="close-btn" aria-label="Close" onclick={onclose}>&times;</button>
			{/if}
		</div>

		{#if loading}
			<p class="status-text">Loading mods&hellip;</p>
		{:else if switched}
			<div class="switched-notice">
				<p>Mod-list updated to <strong>{selected}</strong>.</p>
				<p class="hint">Restart the server, then reload the page to apply.</p>
				<button type="button" class="action-btn" onclick={() => window.location.reload()}>
					Reload now
				</button>
			</div>
		{:else}
			{#if error}
				<p class="error-text">{error}</p>
			{/if}

			<ul class="mod-list" role="listbox" aria-label="Available mods">
				{#each mods as mod (mod.id)}
					<li
						class="mod-card"
						class:mod-card--selected={selected === mod.id}
						class:mod-card--active={mod.active}
						role="option"
						aria-selected={selected === mod.id}
						tabindex="0"
						onclick={() => (selected = mod.id)}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') {
								e.preventDefault();
								selected = mod.id;
							}
						}}
					>
						<div class="mod-card-header">
							<span class="mod-name">{mod.title ?? mod.name}</span>
							{#if mod.active}
								<span class="mod-badge">active</span>
							{/if}
						</div>
						<div class="mod-meta">v{mod.version} &mdash; {mod.id}</div>
						<div class="mod-desc">{mod.description}</div>
					</li>
				{/each}
			</ul>

			<div class="overlay-footer">
				<button type="button" class="cancel-btn" onclick={onclose}>Cancel</button>
				<button
					type="button"
					class="action-btn"
					disabled={!selected || selected === activeId || switching}
					onclick={confirm}
				>
					{switching ? 'Switching…' : 'Confirm'}
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.overlay-backdrop {
		position: fixed;
		inset: 0;
		z-index: 100;
		background: rgba(0, 0, 0, 0.65);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.overlay-panel {
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		width: min(480px, 92vw);
		max-height: 80vh;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.overlay-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.overlay-title {
		font-family: var(--font-display);
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.12em;
		color: var(--color-muted);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-muted);
		font-size: 1.3rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.25rem;
	}

	.close-btn:hover {
		color: var(--color-fg);
	}

	.mod-list {
		list-style: none;
		margin: 0;
		padding: 0.5rem;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.mod-card {
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		cursor: pointer;
		user-select: none;
		background: var(--color-input-bg);
		transition: border-color 0.1s;
	}

	.mod-card:hover {
		border-color: var(--color-muted);
	}

	.mod-card--selected {
		border-color: var(--color-accent);
		outline: none;
	}

	.mod-card--active {
		background: color-mix(in srgb, var(--color-accent) 8%, var(--color-input-bg));
	}

	.mod-card-header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.15rem;
	}

	.mod-name {
		font-weight: 600;
		font-size: 0.9rem;
		color: var(--color-fg);
	}

	.mod-badge {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-accent);
		border: 1px solid var(--color-accent);
		border-radius: 3px;
		padding: 0 4px;
	}

	.mod-meta {
		font-size: 0.7rem;
		color: var(--color-muted);
		margin-bottom: 0.25rem;
	}

	.mod-desc {
		font-size: 0.78rem;
		color: var(--color-fg);
		opacity: 0.8;
	}

	.overlay-footer {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--color-border);
	}

	.cancel-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		border-radius: 4px;
		padding: 0.35rem 0.9rem;
		font-size: 0.82rem;
		cursor: pointer;
	}

	.cancel-btn:hover {
		border-color: var(--color-muted);
		color: var(--color-fg);
	}

	.action-btn {
		background: var(--color-accent);
		border: 1px solid var(--color-accent);
		color: var(--color-panel-bg);
		border-radius: 4px;
		padding: 0.35rem 0.9rem;
		font-size: 0.82rem;
		font-weight: 600;
		cursor: pointer;
	}

	.action-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.action-btn:not(:disabled):hover {
		filter: brightness(1.15);
	}

	.status-text,
	.error-text {
		padding: 1.5rem 1rem;
		text-align: center;
		font-size: 0.85rem;
	}

	.error-text {
		color: var(--color-accent);
	}

	.switched-notice {
		padding: 1.5rem 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		align-items: center;
		text-align: center;
	}

	.switched-notice p {
		margin: 0;
		font-size: 0.88rem;
	}

	.hint {
		color: var(--color-muted);
		font-size: 0.78rem !important;
	}
</style>
