<script lang="ts">
	import {
		editorSelectedLocation,
		editorLocations,
		editorNpcs,
		editorSnapshot,
		editorDirty,
		editorValidation
	} from '../../stores/editor';
	import { editorUpdateLocations, editorSave } from '$lib/editor-ipc';
	import type { LocationData } from '$lib/editor-types';

	$: loc = $editorSelectedLocation;
	$: locations = $editorLocations;
	$: npcs = $editorNpcs;

	function locationName(id: number): string {
		return locations.find((l) => l.id === id)?.name ?? `#${id}`;
	}

	function npcName(id: number): string {
		return npcs.find((n) => n.id === id)?.name ?? `#${id}`;
	}

	async function handleFieldChange(field: string, value: unknown) {
		if (!$editorSnapshot || !loc) return;
		const updated = { ...loc, [field]: value } as LocationData;
		const locs = $editorSnapshot.locations.map((l) => (l.id === updated.id ? updated : l));
		try {
			const report = await editorUpdateLocations(locs);
			editorSnapshot.update((s) => {
				if (!s) return s;
				return { ...s, locations: locs, validation: report };
			});
			editorValidation.set(report);
			editorDirty.set(true);
		} catch (e) {
			console.error('Failed to update location:', e);
		}
	}

	async function handleSave() {
		try {
			const result = await editorSave(['world']);
			editorValidation.set(result.validation);
			if (result.saved) editorDirty.set(false);
		} catch (e) {
			console.error('Failed to save:', e);
		}
	}
</script>

<div class="loc-detail">
	{#if loc}
		<div class="detail-header">
			<h3 class="detail-title">{loc.name}</h3>
			<button class="save-btn" on:click={handleSave} disabled={!$editorDirty}>Save World</button>
		</div>

		<div class="detail-scroll">
			<section class="section">
				<h4 class="section-label">Identity</h4>
				<div class="field-row">
					<label class="field-label">Name</label>
					<input
						class="field-input"
						type="text"
						value={loc.name}
						on:change={(e) => handleFieldChange('name', e.currentTarget.value)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label">Indoor</label>
					<input
						type="checkbox"
						checked={loc.indoor}
						on:change={(e) => handleFieldChange('indoor', e.currentTarget.checked)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label">Public</label>
					<input
						type="checkbox"
						checked={loc.public}
						on:change={(e) => handleFieldChange('public', e.currentTarget.checked)}
					/>
				</div>
			</section>

			<section class="section">
				<h4 class="section-label">Description Template</h4>
				<textarea
					class="field-textarea tall"
					value={loc.description_template}
					on:change={(e) => handleFieldChange('description_template', e.currentTarget.value)}
				></textarea>
				<p class="field-hint">Placeholders: {'{time}'}, {'{weather}'}, {'{npcs_present}'}</p>
			</section>

			<section class="section">
				<h4 class="section-label">Coordinates</h4>
				<div class="field-row">
					<label class="field-label">Lat</label>
					<input
						class="field-input short"
						type="number"
						step="0.001"
						value={loc.lat}
						on:change={(e) => handleFieldChange('lat', parseFloat(e.currentTarget.value))}
					/>
					<label class="field-label">Lon</label>
					<input
						class="field-input short"
						type="number"
						step="0.001"
						value={loc.lon}
						on:change={(e) => handleFieldChange('lon', parseFloat(e.currentTarget.value))}
					/>
				</div>
			</section>

			<section class="section">
				<h4 class="section-label">Connections ({loc.connections.length})</h4>
				{#each loc.connections as conn}
					<div class="conn-row">
						<span class="conn-target">{locationName(conn.target)}</span>
						<span class="conn-desc">{conn.path_description}</span>
					</div>
				{/each}
			</section>

			<section class="section">
				<h4 class="section-label">Associated NPCs</h4>
				{#each loc.associated_npcs as npc_id}
					<span class="assoc-npc">{npcName(npc_id)}</span>
				{/each}
				{#if loc.associated_npcs.length === 0}
					<p class="empty-note">None</p>
				{/if}
			</section>

			{#if loc.mythological_significance}
				<section class="section">
					<h4 class="section-label">Mythological Significance</h4>
					<textarea
						class="field-textarea"
						value={loc.mythological_significance}
						on:change={(e) =>
							handleFieldChange('mythological_significance', e.currentTarget.value || null)}
					></textarea>
				</section>
			{/if}

			{#if loc.aliases.length > 0}
				<section class="section">
					<h4 class="section-label">Aliases</h4>
					<div class="alias-list">
						{#each loc.aliases as alias}
							<span class="alias-tag">{alias}</span>
						{/each}
					</div>
				</section>
			{/if}
		</div>
	{:else}
		<div class="empty-state">
			<p>Select a location from the list to edit.</p>
		</div>
	{/if}
</div>

<style>
	.loc-detail {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.detail-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.5rem 0.8rem;
		border-bottom: 1px solid var(--color-border);
	}

	.detail-title {
		font-family: 'Cinzel', serif;
		font-size: 0.95rem;
		margin: 0;
		color: var(--color-accent);
	}

	.save-btn {
		padding: 0.25rem 0.6rem;
		border: 1px solid var(--color-accent);
		border-radius: 3px;
		background: none;
		color: var(--color-accent);
		font-size: 0.7rem;
		font-family: 'IM Fell English', serif;
		cursor: pointer;
	}
	.save-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
	}
	.save-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.detail-scroll {
		flex: 1;
		overflow-y: auto;
		padding: 0.5rem 0.8rem;
	}

	.section {
		margin-bottom: 1rem;
	}

	.section-label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-muted);
		margin: 0 0 0.3rem;
		border-bottom: 1px solid var(--color-border);
		padding-bottom: 0.15rem;
	}

	.field-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.25rem;
	}

	.field-label {
		font-size: 0.72rem;
		color: var(--color-muted);
		min-width: 60px;
		flex-shrink: 0;
	}

	.field-input {
		flex: 1;
		padding: 0.2rem 0.35rem;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		background: var(--color-input-bg);
		color: var(--color-fg);
		font-size: 0.75rem;
		font-family: 'IM Fell English', serif;
	}
	.field-input.short {
		max-width: 100px;
	}

	.field-textarea {
		width: 100%;
		min-height: 3rem;
		padding: 0.2rem 0.35rem;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		background: var(--color-input-bg);
		color: var(--color-fg);
		font-size: 0.75rem;
		font-family: 'IM Fell English', serif;
		resize: vertical;
		box-sizing: border-box;
	}
	.field-textarea.tall {
		min-height: 5rem;
	}

	.field-hint {
		font-size: 0.6rem;
		color: var(--color-muted);
		margin: 0.15rem 0 0;
	}

	.conn-row {
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
		padding: 0.15rem 0;
		font-size: 0.75rem;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 50%, transparent);
	}

	.conn-target {
		font-weight: 600;
		min-width: 120px;
	}

	.conn-desc {
		font-style: italic;
		color: var(--color-muted);
		font-size: 0.7rem;
	}

	.assoc-npc {
		display: inline-block;
		font-size: 0.7rem;
		padding: 0.1rem 0.3rem;
		margin: 0.1rem;
		border-radius: 3px;
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
		color: var(--color-accent);
	}

	.alias-list {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}

	.alias-tag {
		font-size: 0.7rem;
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-muted);
		font-size: 0.85rem;
	}

	.empty-note {
		color: var(--color-muted);
		font-size: 0.7rem;
		font-style: italic;
	}
</style>
