<script lang="ts">
	import {
		editorValidation,
		editorTab,
		editorSelectedNpcId,
		editorSelectedLocationId
	} from '../../stores/editor';
	import { editorValidate } from '$lib/editor-ipc';
	import type { ValidationIssue } from '$lib/editor-types';

	let running = $state(false);

	async function runValidation() {
		running = true;
		try {
			const report = await editorValidate();
			editorValidation.set(report);
		} catch (e) {
			console.error('Validation failed:', e);
		} finally {
			running = false;
		}
	}

	function jumpToIssue(issue: ValidationIssue) {
		// Parse the field_path to determine what to select.
		const npcMatch = issue.field_path.match(/^npcs\[(\d+)\]/);
		const locMatch = issue.field_path.match(/^locations\[(\d+)\]/);
		if (npcMatch && issue.context) {
			editorTab.set('npcs');
			// context usually has the id; try to use it
			const id = parseInt(issue.context);
			if (!isNaN(id)) editorSelectedNpcId.set(id);
		} else if (locMatch && issue.context) {
			editorTab.set('locations');
			const id = parseInt(issue.context);
			if (!isNaN(id)) editorSelectedLocationId.set(id);
		}
	}

	const report = $derived($editorValidation);
	const allIssues = $derived(report
		? [...report.errors.map((e) => ({ ...e, _severity: 'error' as const })),
		   ...report.warnings.map((w) => ({ ...w, _severity: 'warning' as const }))]
		: []);
</script>

<div class="validator-panel">
	<div class="validator-header">
		<h3 class="validator-title">Validation</h3>
		<button class="run-btn" onclick={runValidation} disabled={running}>
			{running ? 'Running...' : 'Re-validate'}
		</button>
	</div>

	{#if report}
		<div class="summary">
			{#if report.errors.length === 0 && report.warnings.length === 0}
				<span class="summary-clean">All checks passed.</span>
			{:else}
				{#if report.errors.length > 0}
					<span class="summary-errors">{report.errors.length} error{report.errors.length > 1 ? 's' : ''}</span>
				{/if}
				{#if report.warnings.length > 0}
					<span class="summary-warnings">{report.warnings.length} warning{report.warnings.length > 1 ? 's' : ''}</span>
				{/if}
			{/if}
		</div>

		<div class="issue-list">
			{#each allIssues as issue}
				<button
					class="issue-row"
					class:is-error={issue._severity === 'error'}
					class:is-warning={issue._severity === 'warning'}
					onclick={() => jumpToIssue(issue)}
				>
					<span class="issue-severity">{issue._severity === 'error' ? 'ERR' : 'WARN'}</span>
					<span class="issue-cat">{issue.category}</span>
					<span class="issue-msg">{issue.message}</span>
					{#if issue.field_path}
						<span class="issue-path">{issue.field_path}</span>
					{/if}
				</button>
			{/each}
		</div>
	{:else}
		<div class="empty-state">
			<p>Click "Re-validate" to check the mod for issues.</p>
		</div>
	{/if}
</div>

<style>
	.validator-panel {
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.validator-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.validator-title {
		font-family: 'Cinzel', serif;
		font-size: 0.95rem;
		margin: 0;
		color: var(--color-accent);
	}

	.run-btn {
		padding: 0.25rem 0.6rem;
		border: 1px solid var(--color-accent);
		border-radius: 3px;
		background: none;
		color: var(--color-accent);
		font-size: 0.7rem;
		font-family: 'IM Fell English', serif;
		cursor: pointer;
	}
	.run-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
	}
	.run-btn:disabled {
		opacity: 0.5;
		cursor: wait;
	}

	.summary {
		padding: 0.4rem 1rem;
		font-size: 0.8rem;
		border-bottom: 1px solid var(--color-border);
	}

	.summary-clean {
		color: #44cc44;
	}

	.summary-errors {
		color: #ff4444;
		margin-right: 0.75rem;
	}

	.summary-warnings {
		color: #ccaa44;
	}

	.issue-list {
		flex: 1;
		overflow-y: auto;
	}

	.issue-row {
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
		width: 100%;
		padding: 0.35rem 1rem;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: none;
		cursor: pointer;
		text-align: left;
		font-family: 'IM Fell English', serif;
		color: var(--color-fg);
		font-size: 0.75rem;
	}
	.issue-row:hover {
		background: var(--color-input-bg);
	}

	.issue-row.is-error {
		border-left: 3px solid #ff4444;
	}

	.issue-row.is-warning {
		border-left: 3px solid #ccaa44;
	}

	.issue-severity {
		font-size: 0.55rem;
		font-weight: 700;
		padding: 0.05rem 0.2rem;
		border-radius: 2px;
		text-transform: uppercase;
		min-width: 2.5em;
		text-align: center;
	}

	.is-error .issue-severity {
		background: color-mix(in srgb, #ff4444 20%, transparent);
		color: #ff4444;
	}

	.is-warning .issue-severity {
		background: color-mix(in srgb, #ccaa44 20%, transparent);
		color: #ccaa44;
	}

	.issue-cat {
		font-size: 0.6rem;
		color: var(--color-muted);
		text-transform: uppercase;
		min-width: 70px;
	}

	.issue-msg {
		flex: 1;
	}

	.issue-path {
		font-family: monospace;
		font-size: 0.6rem;
		color: var(--color-muted);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-muted);
		font-size: 0.85rem;
	}
</style>
