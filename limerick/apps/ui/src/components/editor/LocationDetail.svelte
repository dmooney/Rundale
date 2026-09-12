<script lang="ts">
	import { onMount } from 'svelte';
	import {
		editorSelectedLocation,
		editorLocations,
		editorNpcs,
		editorSnapshot,
		editorDirty,
		editorValidation,
		editorSelectedLocationId
	} from '../../stores/editor';
	import { editorUpdateLocations, editorSave } from '$lib/editor-ipc';
	import type { GeoKind, LocationData } from '$lib/editor-types';
	import { normalizeLocationCaches, offsetLatLon } from '$lib/editor-map';
	import { EditorLocationMap } from '$lib/editor/location-map';

	let mapContainer: HTMLDivElement | undefined = $state(undefined);
	let componentDisposed = false;

	const loc = $derived($editorSelectedLocation);
	const locations = $derived($editorLocations);
	const npcs = $derived($editorNpcs);
	const selectedId = $derived($editorSelectedLocationId);

	function locationName(id: number): string {
		return locations.find((l) => l.id === id)?.name ?? `#${id}`;
	}

	function npcName(id: number): string {
		return npcs.find((n) => n.id === id)?.name ?? `#${id}`;
	}

	async function persistLocations(nextLocations: LocationData[]) {
		const normalizedLocations = normalizeLocationCaches(nextLocations);
		const report = await editorUpdateLocations(normalizedLocations);
		editorSnapshot.update((s) => {
			if (!s) return s;
			return { ...s, locations: normalizedLocations, validation: report };
		});
		editorValidation.set(report);
		editorDirty.set(true);
	}

	async function updateSelectedLocation(mutator: (location: LocationData) => LocationData) {
		if (!$editorSnapshot || !loc) return;
		const nextLocations = $editorSnapshot.locations.map((l) => (l.id === loc.id ? mutator(l) : l));
		try {
			await persistLocations(nextLocations);
		} catch (e) {
			console.error('Failed to update location:', e);
		}
	}

	// Like updateSelectedLocation but targets a specific location ID.
	// Used by the drag-commit path so that if the reactive `loc`/`selectedId`
	// changes between mousedown and mouseup (click-to-select race #408), the
	// dragged location — not the newly-selected one — receives the update.
	async function updateLocationById(id: number, mutator: (location: LocationData) => LocationData) {
		if (!$editorSnapshot) return;
		const nextLocations = $editorSnapshot.locations.map((l) => (l.id === id ? mutator(l) : l));
		try {
			await persistLocations(nextLocations);
		} catch (e) {
			console.error('Failed to update location:', e);
		}
	}

	async function handleFieldChange(field: string, value: unknown) {
		await updateSelectedLocation((current) => ({ ...current, [field]: value }));
	}

	async function setCoordinateMode(mode: 'absolute' | 'relative') {
		if (!loc) return;
		if (mode === 'absolute') {
			await handleFieldChange('relative_to', null);
			return;
		}
		const anchorCandidate = locations.find((l) => l.id !== loc.id);
		if (!anchorCandidate) return;
		await handleFieldChange('relative_to', {
			anchor: anchorCandidate.id,
			dnorth_m: 0,
			deast_m: 0
		});
	}

	async function applyRelativeField(field: 'anchor' | 'dnorth_m' | 'deast_m', raw: string) {
		if (!loc) return;
		const rel = loc.relative_to ?? { anchor: loc.id, dnorth_m: 0, deast_m: 0 };
		const value = field === 'anchor' ? Number(raw) : Number.parseFloat(raw);
		if (Number.isNaN(value)) return;
		await handleFieldChange('relative_to', { ...rel, [field]: value });
	}

	async function nudgeSelected(northM: number, eastM: number) {
		if (!loc) return;
		if (loc.relative_to) {
			await handleFieldChange('relative_to', {
				...loc.relative_to,
				dnorth_m: loc.relative_to.dnorth_m + northM,
				deast_m: loc.relative_to.deast_m + eastM
			});
			return;
		}
		const moved = offsetLatLon(loc.lat, loc.lon, northM, eastM);
		await updateSelectedLocation((current) => ({ ...current, ...moved }));
	}

	async function toggleConnection(targetId: number) {
		if (!$editorSnapshot || !loc || targetId === loc.id) return;
		const source = loc;
		const hasConnection = source.connections.some((c) => c.target === targetId);
		const nextLocations = $editorSnapshot.locations.map((entry) => {
			if (entry.id === source.id) {
				const connections = hasConnection
					? entry.connections.filter((c) => c.target !== targetId)
					: [...entry.connections, { target: targetId, path_description: 'an old lane between settlements' }];
				return { ...entry, connections };
			}
			if (entry.id === targetId) {
				const reverseHas = entry.connections.some((c) => c.target === source.id);
				const connections = hasConnection
					? entry.connections.filter((c) => c.target !== source.id)
					: reverseHas
						? entry.connections
						: [...entry.connections, { target: source.id, path_description: 'an old lane between settlements' }];
				return { ...entry, connections };
			}
			return entry;
		});
		try {
			await persistLocations(nextLocations);
		} catch (e) {
			console.error('Failed to toggle connection:', e);
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

	// MapLibre instance lifecycle + marker-drag handlers live in the
	// EditorLocationMap controller (#1200 TD-043); the component supplies
	// reactive state + actions through its hooks.
	const locationMap = new EditorLocationMap({
		getLocations: () => locations,
		getSelectedId: () => selectedId,
		getSelectedLocation: () => loc ?? null,
		isDisposed: () => componentDisposed,
		onSelect: (id) => editorSelectedLocationId.set(id),
		toggleConnection: (targetId) => toggleConnection(targetId),
		updateLocationById: (id, mutator) => updateLocationById(id, mutator)
	});

	onMount(() => {
		return () => {
			componentDisposed = true;
			locationMap.destroy();
		};
	});

	// Lifecycle management: ensure map exists when loc is selected,
	// and destroy it when deselected or the container is unmounted.
	// Background: the `{#if loc}` wrapper unmounts the map-frame div, but
	// Svelte's `bind:this` does not always reset `mapContainer` to
	// `undefined` in time — so we couple cleanup to `loc` directly (#409).
	// Without explicit teardown each deselect leaks a WebGL context
	// (MapLibre allocates one per Map instance) and after a few navigations
	// the browser aborts further WebGL contexts.
	// Also update map data when locations or selection changes.
	$effect(() => {
		if (loc && mapContainer) {
			if (!locationMap.exists) void locationMap.ensure(mapContainer);
		} else if (locationMap.exists) {
			locationMap.destroy();
		}
		locationMap.setMapData(locations, selectedId);
	});
</script>

<div class="loc-detail">
	{#if loc}
		<div class="detail-header">
			<h3 class="detail-title">{loc.name}</h3>
			<button class="save-btn" onclick={handleSave} disabled={!$editorDirty}>Save World</button>
		</div>

		<div class="detail-scroll">
			<section class="section">
				<h4 class="section-label">Map Designer</h4>
				<div class="map-frame" bind:this={mapContainer}></div>
				<p class="field-hint">Click to select, drag selected point to move. Shift-click another point to toggle a bidirectional link.</p>
			</section>

			<section class="section">
				<h4 class="section-label">Identity</h4>
				<div class="field-row">
					<label class="field-label" for="loc-name">Name</label>
					<input
						id="loc-name"
						class="field-input"
						type="text"
						value={loc.name}
						onchange={(e) => handleFieldChange('name', e.currentTarget.value)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="loc-indoor">Indoor</label>
					<input
						id="loc-indoor"
						type="checkbox"
						checked={loc.indoor}
						onchange={(e) => handleFieldChange('indoor', e.currentTarget.checked)}
					/>
				</div>
				<div class="field-row">
					<label class="field-label" for="loc-public">Public</label>
					<input
						id="loc-public"
						type="checkbox"
						checked={loc.public}
						onchange={(e) => handleFieldChange('public', e.currentTarget.checked)}
					/>
				</div>
			</section>

			<section class="section">
				<h4 class="section-label">Coordinates</h4>
				<div class="field-row">
					<label class="field-label" for="loc-geo-kind">Geo kind</label>
					<select
						id="loc-geo-kind"
						class="field-input"
						value={loc.geo_kind ?? 'fictional'}
						onchange={(e) => handleFieldChange('geo_kind', e.currentTarget.value as GeoKind)}
					>
						<option value="real">Real</option>
						<option value="manual">Manual</option>
						<option value="fictional">Fictional</option>
					</select>
				</div>
				<div class="field-row">
					<label class="field-label" for="loc-coord-mode">Mode</label>
					<select
						id="loc-coord-mode"
						class="field-input"
						value={loc.relative_to ? 'relative' : 'absolute'}
						onchange={(e) => setCoordinateMode(e.currentTarget.value as 'absolute' | 'relative')}
					>
						<option value="absolute">Absolute</option>
						<option value="relative">Relative</option>
					</select>
				</div>
				{#if loc.relative_to}
					<div class="field-row">
						<label class="field-label" for="loc-anchor">Anchor</label>
						<select
							id="loc-anchor"
							class="field-input"
							value={loc.relative_to.anchor}
							onchange={(e) => applyRelativeField('anchor', e.currentTarget.value)}
						>
							{#each locations.filter((l) => l.id !== loc.id) as option (option.id)}
								<option value={option.id}>{option.name}</option>
							{/each}
						</select>
					</div>
					<div class="field-row">
						<label class="field-label" for="loc-dnorth">dNorth m</label>
						<input
							id="loc-dnorth"
							class="field-input short"
							type="number"
							step="1"
							value={loc.relative_to.dnorth_m}
							onchange={(e) => applyRelativeField('dnorth_m', e.currentTarget.value)}
						/>
						<label class="field-label" for="loc-deast">dEast m</label>
						<input
							id="loc-deast"
							class="field-input short"
							type="number"
							step="1"
							value={loc.relative_to.deast_m}
							onchange={(e) => applyRelativeField('deast_m', e.currentTarget.value)}
						/>
					</div>
				{:else}
					<div class="field-row">
						<label class="field-label" for="loc-lat">Lat</label>
						<input
							id="loc-lat"
							class="field-input short"
							type="number"
							step="0.00001"
							value={loc.lat}
							onchange={(e) => handleFieldChange('lat', parseFloat(e.currentTarget.value))}
						/>
						<label class="field-label" for="loc-lon">Lon</label>
						<input
							id="loc-lon"
							class="field-input short"
							type="number"
							step="0.00001"
							value={loc.lon}
							onchange={(e) => handleFieldChange('lon', parseFloat(e.currentTarget.value))}
						/>
					</div>
				{/if}
				<div class="field-row">
					<label class="field-label" for="loc-geo-source">Geo source</label>
					<input
						id="loc-geo-source"
						class="field-input"
						type="text"
						value={loc.geo_source ?? ''}
						onchange={(e) => handleFieldChange('geo_source', e.currentTarget.value || null)}
					/>
				</div>
				<div class="nudge-row">
					<button class="nudge-btn" onclick={() => nudgeSelected(100, 0)}>N +100m</button>
					<button class="nudge-btn" onclick={() => nudgeSelected(-100, 0)}>S +100m</button>
					<button class="nudge-btn" onclick={() => nudgeSelected(0, 100)}>E +100m</button>
					<button class="nudge-btn" onclick={() => nudgeSelected(0, -100)}>W +100m</button>
				</div>
			</section>

			<section class="section">
				<h4 class="section-label">Connections ({loc.connections.length})</h4>
				{#each loc.connections as conn (conn.target)}
					<div class="conn-row">
						<span class="conn-target">{locationName(conn.target)}</span>
						<span class="conn-desc">{conn.path_description}</span>
						<button class="nudge-btn" onclick={() => toggleConnection(conn.target)}>Remove</button>
					</div>
				{/each}
			</section>

			<section class="section">
				<h4 class="section-label">Description Template</h4>
				<textarea
					class="field-textarea tall"
					aria-label="Description template"
					value={loc.description_template}
					onchange={(e) => handleFieldChange('description_template', e.currentTarget.value)}
				></textarea>
				<p class="field-hint">Placeholders: {'{time}'}, {'{weather}'}, {'{npcs_present}'}</p>
			</section>

			<section class="section">
				<h4 class="section-label">Associated NPCs</h4>
				{#each loc.associated_npcs as npc_id (npc_id)}
					<span class="assoc-npc">{npcName(npc_id)}</span>
				{/each}
				{#if loc.associated_npcs.length === 0}
					<p class="empty-note">None</p>
				{/if}
			</section>

			<section class="section">
				<h4 class="section-label">Mythological Significance</h4>
				<textarea
					class="field-textarea"
					aria-label="Mythological significance"
					value={loc.mythological_significance ?? ''}
					placeholder="Fairy fort, holy well, cursed ground…"
					onchange={(e) =>
						handleFieldChange(
							'mythological_significance',
							e.currentTarget.value.trim() === '' ? null : e.currentTarget.value
						)}
				></textarea>
			</section>

			<section class="section">
				<h4 class="section-label">Aliases</h4>
				{#if loc.aliases && loc.aliases.length > 0}
					<div class="alias-list">
						{#each loc.aliases as alias (alias)}
							<span class="alias-tag">{alias}</span>
						{/each}
					</div>
				{:else}
					<p class="empty-note">None</p>
				{/if}
			</section>
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
		font-family: var(--font-display);
		font-size: 0.95rem;
		margin: 0;
		color: var(--color-accent);
	}

	.save-btn,
	.nudge-btn {
		padding: 0.25rem 0.6rem;
		border: 1px solid var(--color-accent);
		border-radius: 3px;
		background: none;
		color: var(--color-accent);
		font-size: 0.7rem;
		font-family: var(--font-body);
		cursor: pointer;
	}
	.save-btn:hover:not(:disabled),
	.nudge-btn:hover {
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
		min-width: 70px;
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
		font-family: var(--font-body);
	}
	.field-input.short {
		max-width: 100px;
	}

	.map-frame {
		height: 640px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}

	.nudge-row {
		display: flex;
		gap: 0.4rem;
		flex-wrap: wrap;
		margin-top: 0.4rem;
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
		font-family: var(--font-body);
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
		align-items: center;
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
		flex: 1;
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
		gap: 0.2rem;
	}

	.alias-tag {
		display: inline-block;
		font-size: 0.7rem;
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		background: color-mix(in srgb, var(--color-muted) 18%, transparent);
		color: var(--color-muted);
		font-style: italic;
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
