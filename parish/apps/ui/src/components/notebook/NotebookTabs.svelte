<script lang="ts">
	let {
		active = 'notes',
		onselect,
	}: {
		active?: string;
		onselect?: (tab: string) => void;
	} = $props();

	const tabs = [
		{ id: 'notes', label: 'Notes', icon: '✎' },
		{ id: 'people', label: 'People', icon: '♟' },
		{ id: 'places', label: 'Places', icon: '⌂' },
		{ id: 'rumours', label: 'Rumours', icon: '☰' },
		{ id: 'journal', label: 'Journal', icon: '□' },
	];
</script>

<nav class="notebook-tabs" aria-label="Notebook sections">
	{#each tabs as tab (tab.id)}
		<button
			type="button"
			class:active={tab.id === active}
			aria-pressed={tab.id === active}
			title={tab.label}
			onclick={() => onselect?.(tab.id)}
		>
			<span class="tab-icon" aria-hidden="true">{tab.icon}</span>
			<span class="tab-label">{tab.label}</span>
		</button>
	{/each}
</nav>

<style>
	.notebook-tabs {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: clamp(4.4rem, 5vw, 5.4rem);
		padding-top: 2.6rem;
		transform: translateX(-0.25rem);
	}

	button {
		display: grid;
		grid-template-columns: 1rem 1fr;
		align-items: center;
		gap: 0.35rem;
		width: 100%;
		min-height: clamp(3.35rem, 7vh, 4.6rem);
		padding: 0.3rem 0.55rem 0.3rem 0.7rem;
		border: 0;
		border-radius: 0;
		background: url('/notebook-ui/assets/notebook-tab.svg') center / 100% 100%;
		color: var(--notebook-ink-soft);
		font-family: var(--font-body);
		font-size: 0.72rem;
		filter: drop-shadow(2px 2px 4px rgba(35, 24, 13, 0.26));
		cursor: pointer;
	}

	button:hover,
	button:focus-visible,
	button.active {
		color: var(--notebook-ink);
		filter: drop-shadow(2px 2px 5px rgba(35, 24, 13, 0.34)) saturate(1.08);
	}

	.tab-icon {
		font-size: 0.95rem;
		line-height: 1;
	}

	.tab-label {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	@media (max-width: 900px) {
		.notebook-tabs {
			flex-direction: row;
			width: auto;
			padding: 0;
			order: -1;
			transform: none;
			gap: 0.35rem;
		}

		button {
			min-height: 2.45rem;
			min-width: 4.6rem;
		}
	}
</style>
