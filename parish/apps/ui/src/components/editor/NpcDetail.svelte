<script lang="ts">
	import {
		editorSelectedNpc,
		editorLocations,
		editorNpcs,
		editorSnapshot,
		editorDirty,
		editorValidation
	} from '../../stores/editor';
	import { editorUpdateNpcs, editorSave } from '$lib/editor-ipc';
	import type { NpcFileEntry, ScheduleVariantFileEntry } from '$lib/editor-types';

	$: npc = $editorSelectedNpc;
	$: locations = $editorLocations;
	$: allNpcs = $editorNpcs;

	function locationName(id: number): string {
		return locations.find((l) => l.id === id)?.name ?? `#${id}`;
	}

	function npcName(id: number): string {
		return allNpcs.find((n) => n.id === id)?.name ?? `#${id}`;
	}

	function formatHourRange(start: number, end: number): string {
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${pad(start)}:00–${pad(end)}:59`;
	}

	function variantLabel(v: ScheduleVariantFileEntry): string {
		const parts: string[] = [];
		if (v.season) parts.push(v.season);
		if (v.day_type) parts.push(v.day_type);
		return parts.length > 0 ? parts.join(' / ') : 'Default';
	}

	const intelligenceDimensions = [
		{ key: 'verbal', label: 'Verbal' },
		{ key: 'analytical', label: 'Analytical' },
		{ key: 'emotional', label: 'Emotional' },
		{ key: 'practical', label: 'Practical' },
		{ key: 'wisdom', label: 'Wisdom' },
		{ key: 'creative', label: 'Creative' }
	] as const;

	async function handleFieldChange(field: string, value: unknown) {
		if (!$editorSnapshot || !npc) return;
		const updated = { ...npc, [field]: value } as NpcFileEntry;
		const npcs = $editorSnapshot.npcs.npcs.map((n) => (n.id === updated.id ? updated : n));
		const npcFile = { npcs };
		try {
			const report = await editorUpdateNpcs(npcFile);
			editorSnapshot.update((s) => {
				if (!s) return s;
				return { ...s, npcs: npcFile, validation: report };
			});
			editorValidation.set(report);
			editorDirty.set(true);
		} catch (e) {
			console.error('Failed to update NPC:', e);
		}
	}

	async function handleSave() {
		try {
			const result = await editorSave(['npcs']);
			editorValidation.set(result.validation);
			if (result.saved) editorDirty.set(false);
		} catch (e) {
			console.error('Failed to save:', e);
		}
	}
</script>

<div class="npc-detail">
	{#if npc}
		<div class="detail-header">
			<h3 class="detail-title">{npc.name}</h3>
			<button class="save-btn" on:click={handleSave} disabled={!$editorDirty}>Save NPCs</button>
		</div>

		<div class="detail-scroll">
			<!-- Identity -->
			<section class="section">
				<h4 class="section-label">Identity</h4>
				<div class="field-row">
					<label class="field-label" for="npc-name">Name</label>
					<input
						id="npc-name"
						class="field-input"
						type="text"
						value={npc.name}
						on:change={(e) => handleFieldChange('name', e.currentTarget.value)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-age">Age</label>
					<input
						id="npc-age"
						class="field-input short"
						type="number"
						value={npc.age}
						on:change={(e) => handleFieldChange('age', parseInt(e.currentTarget.value))}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-occupation">Occupation</label>
					<input
						id="npc-occupation"
						class="field-input"
						type="text"
						value={npc.occupation}
						on:change={(e) => handleFieldChange('occupation', e.currentTarget.value)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-mood">Mood</label>
					<input
						id="npc-mood"
						class="field-input"
						type="text"
						value={npc.mood}
						on:change={(e) => handleFieldChange('mood', e.currentTarget.value)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-brief-desc">Brief Description</label>
					<input
						id="npc-brief-desc"
						class="field-input"
						type="text"
						value={npc.brief_description ?? ''}
						placeholder="(auto-generated from occupation)"
						on:change={(e) => {
							const val = e.currentTarget.value.trim();
							handleFieldChange('brief_description', val || null);
						}}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-personality">Personality</label>
					<textarea
						id="npc-personality"
						class="field-textarea"
						value={npc.personality}
						on:change={(e) => handleFieldChange('personality', e.currentTarget.value)}
					></textarea>
				</div>
			</section>

			<!-- Home / Workplace -->
			<section class="section">
				<h4 class="section-label">Home & Workplace</h4>
				<div class="field-row">
					<label class="field-label" for="npc-home">Home</label>
					<select
						id="npc-home"
						class="field-select"
						value={npc.home}
						on:change={(e) => handleFieldChange('home', parseInt(e.currentTarget.value))}
					>
						{#each locations as loc}
							<option value={loc.id}>{loc.name}</option>
						{/each}
					</select>
				</div>
				<div class="field-row">
					<label class="field-label" for="npc-workplace">Workplace</label>
					<select
						id="npc-workplace"
						class="field-select"
						value={npc.workplace ?? -1}
						on:change={(e) => {
							const v = parseInt(e.currentTarget.value);
							handleFieldChange('workplace', v === -1 ? null : v);
						}}
					>
						<option value={-1}>(none)</option>
						{#each locations as loc}
							<option value={loc.id}>{loc.name}</option>
						{/each}
					</select>
				</div>
			</section>

			<!-- Intelligence -->
			{#if npc.intelligence}
				<section class="section">
					<h4 class="section-label">Intelligence</h4>
					{#each intelligenceDimensions as dim}
						<div class="field-row">
							<label class="field-label" for="npc-intel-{dim.key}">{dim.label}</label>
							<input
								id="npc-intel-{dim.key}"
								class="field-range"
								type="range"
								min="1"
								max="5"
								value={npc.intelligence?.[dim.key] ?? 3}
								on:change={(e) => {
									if (!npc?.intelligence) return;
									const updated = {
										...npc.intelligence,
										[dim.key]: parseInt(e.currentTarget.value)
									};
									handleFieldChange('intelligence', updated);
								}}
							/>
							<span class="range-value">{npc.intelligence?.[dim.key] ?? 3}</span>
						</div>
					{/each}
				</section>
			{/if}

			<!-- Relationships -->
			<section class="section">
				<h4 class="section-label">Relationships ({npc.relationships.length})</h4>
				{#each npc.relationships as rel, i}
					<div class="rel-row">
						<span class="rel-target">{npcName(rel.target_id)}</span>
						<span class="rel-kind">{rel.kind}</span>
						<span class="rel-strength">{rel.strength.toFixed(2)}</span>
					</div>
				{/each}
				{#if npc.relationships.length === 0}
					<p class="empty-note">No relationships defined.</p>
				{/if}
			</section>

			<!-- Knowledge -->
			<section class="section">
				<h4 class="section-label">Knowledge ({npc.knowledge.length})</h4>
				{#each npc.knowledge as item, i}
					<div class="knowledge-item">{item}</div>
				{/each}
				{#if npc.knowledge.length === 0}
					<p class="empty-note">No knowledge entries.</p>
				{/if}
			</section>

			<!-- Schedule (read-only) -->
			{#if npc.seasonal_schedule}
				<section class="section">
					<h4 class="section-label">Schedule (read-only)</h4>
					{#each npc.seasonal_schedule as variant}
						<div class="schedule-variant">
							<span class="variant-label">{variantLabel(variant)}</span>
							{#each variant.entries as entry}
								<div class="schedule-entry">
									<span class="entry-time">{formatHourRange(entry.start_hour, entry.end_hour)}</span>
									<span class="entry-loc">{locationName(entry.location)}</span>
									<span class="entry-activity">{entry.activity}</span>
									{#if entry.cuaird}
										<span class="entry-cuaird" title="Cuaird (visiting round)">C</span>
									{/if}
								</div>
							{/each}
						</div>
					{/each}
				</section>
			{/if}
		</div>
	{:else}
		<div class="empty-state">
			<p>Select an NPC from the list to edit.</p>
		</div>
	{/if}
</div>

<style>
	.npc-detail {
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
		min-width: 100px;
		flex-shrink: 0;
	}

	.field-input,
	.field-select {
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
		max-width: 80px;
	}

	.field-textarea {
		flex: 1;
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
	}

	.field-range {
		flex: 1;
		accent-color: var(--color-accent);
	}

	.range-value {
		font-size: 0.75rem;
		min-width: 1.5em;
		text-align: center;
		color: var(--color-accent);
		font-weight: 600;
	}

	.rel-row {
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
		padding: 0.15rem 0;
		font-size: 0.75rem;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 50%, transparent);
	}

	.rel-target {
		font-weight: 600;
		min-width: 120px;
	}

	.rel-kind {
		color: var(--color-muted);
		font-size: 0.65rem;
		text-transform: lowercase;
	}

	.rel-strength {
		color: var(--color-accent);
		font-size: 0.7rem;
	}

	.knowledge-item {
		font-size: 0.72rem;
		padding: 0.15rem 0;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 50%, transparent);
	}

	.schedule-variant {
		margin-bottom: 0.5rem;
	}

	.variant-label {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--color-accent);
	}

	.schedule-entry {
		display: flex;
		gap: 0.5rem;
		font-size: 0.7rem;
		padding: 0.1rem 0;
		align-items: baseline;
	}

	.entry-time {
		font-family: monospace;
		min-width: 90px;
		color: var(--color-muted);
	}

	.entry-loc {
		font-weight: 600;
		min-width: 120px;
	}

	.entry-activity {
		color: var(--color-muted);
		font-style: italic;
	}

	.entry-cuaird {
		font-size: 0.55rem;
		padding: 0.05rem 0.2rem;
		border-radius: 2px;
		background: color-mix(in srgb, var(--color-accent) 20%, transparent);
		color: var(--color-accent);
		font-weight: 700;
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
