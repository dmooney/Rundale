<script lang="ts">
	import { submitInput } from '$lib/ipc';
	import type { DebugSnapshot } from '$lib/types';
	import { Key, Check, WarningCircle } from 'phosphor-svelte';
	import ByokOnboarding from './ByokOnboarding.svelte';
	import BugChip from './BugChip.svelte';

	const PRESET_PROVIDERS = [
		'anthropic',
		'openai',
		'google',
		'openrouter',
		'groq',
		'mistral',
		'ollama',
		'lmstudio',
		'nvidia-nim'
	] as const;

	function applyPreset(provider: string) {
		submitInput(`/preset ${provider}`).catch((err) => {
			console.error(`failed to apply preset ${provider}:`, err);
		});
	}

	function npcLabelFromEntry(entry: { system_prompt?: string | null }): string | null {
		if (!entry.system_prompt) return null;
		const m = entry.system_prompt.match(/^You are ([^,]+),/);
		return m ? m[1].trim() : null;
	}

	let { snap, logId, onSelectLog, onDeselectLog }: {
		snap: DebugSnapshot;
		logId: number | null;
		onSelectLog: (id: number) => void;
		onDeselectLog: () => void;
	} = $props();

	let byokOpen = $state(false);
	const selectedEntry = $derived(snap.inference.call_log.find(e => e.request_id === logId) ?? null);
</script>

{#if selectedEntry}
	<button class="back-btn" onclick={onDeselectLog}>Back to list</button>
	{@const npcLabel = npcLabelFromEntry(selectedEntry)}
	<div class="log-detail-header">
		<span class="muted">[{selectedEntry.timestamp}]</span>
		<span class="log-id">#{selectedEntry.request_id}</span>
		{#if npcLabel}<span class="log-npc accent">{npcLabel}</span>{/if}
		<span class="muted">{selectedEntry.model}</span>
		{#if selectedEntry.streaming}<span class="log-badge stream">STREAM</span>{/if}
		{#if selectedEntry.error}<span class="log-badge error">ERROR</span>{:else}<span class="log-badge ok">OK</span>{/if}
		<span class="muted">{selectedEntry.duration_ms}ms · prompt {selectedEntry.prompt_len}ch · response {selectedEntry.response_len}ch{#if selectedEntry.ttft_ms != null} · ttft {selectedEntry.ttft_ms}ms{/if}{#if selectedEntry.output_tokens != null} · {selectedEntry.output_tokens} tok{#if selectedEntry.ttft_ms != null && selectedEntry.duration_ms > selectedEntry.ttft_ms} ({(selectedEntry.output_tokens / ((selectedEntry.duration_ms - selectedEntry.ttft_ms) / 1000)).toFixed(1)} tok/s){/if}{/if}</span>
	</div>
	{#if selectedEntry.error}
		<div class="log-error-msg">{selectedEntry.error}</div>
	{/if}
	{#if selectedEntry.system_prompt}
		<div class="prompt-label">System prompt ({selectedEntry.system_prompt.length}ch)</div>
		<pre class="prompt-block">{selectedEntry.system_prompt}</pre>
	{/if}
	<div class="prompt-label">Prompt ({selectedEntry.prompt_len}ch)</div>
	<pre class="prompt-block">{selectedEntry.prompt_text}</pre>
	<div class="prompt-label">Response ({selectedEntry.response_len}ch)</div>
	<pre class="prompt-block">{selectedEntry.response_text}</pre>
{:else}
	<div class="section">
		<h4>Provider</h4>
		<div class="field">{snap.inference.provider_name} · {snap.inference.model_name || '(auto)'} · {snap.inference.base_url || '(default)'}</div>
		<div class="field">Queue: {snap.inference.has_queue ? 'Active' : 'Inactive'}</div>
		<div class="field">Improv: {snap.inference.improv_enabled ? 'Active' : 'Inactive'}</div>
		{#if snap.inference.cloud_provider}
			<div class="field muted">Cloud: {snap.inference.cloud_provider} / {snap.inference.cloud_model || '(none)'}</div>
		{/if}
	</div>
	<div class="section">
		<h4>Quick Presets</h4>
		<div class="preset-row">
			{#each PRESET_PROVIDERS as provider}
				{@const isLocal = provider === 'ollama' || provider === 'lmstudio'}
				{@const isConfigured = snap.inference.configured_providers.includes(provider)}
				<button class="preset-btn" type="button" onclick={() => applyPreset(provider)}>
					{#if isLocal}
						{#if isConfigured}
							<span class="configured-icon" title="Local provider available" aria-hidden="true">
								<Check size={14} weight="bold" />
							</span>
						{/if}
					{:else if isConfigured}
						<span class="configured-icon" title="API key configured" aria-hidden="true">
							<Key size={14} weight="bold" />
						</span>
					{:else}
						<span class="missing-icon" title="API key missing" aria-hidden="true">
							<WarningCircle size={14} weight="bold" />
						</span>
					{/if}
					{provider}
				</button>
			{/each}
		</div>
		<div class="field muted">Sets a sensible model per inference role; API keys are not changed.</div>
		<div class="byok-row">
			<button class="byok-btn" type="button" onclick={() => (byokOpen = true)}>
				Change provider or key…
			</button>
		</div>
	</div>
	{#if snap.inference.categories.length > 0}
		<div class="section">
			<h4>Per-role</h4>
			<table class="role-table">
				<thead>
					<tr>
						<th>Role</th>
						<th>Provider</th>
						<th>Model</th>
					</tr>
				</thead>
				<tbody>
					{#each snap.inference.categories as cat}
						<tr>
							<td>{cat.role}</td>
							<td class:muted={!cat.provider}>{cat.provider ?? `(${snap.inference.provider_name})`}</td>
							<td class:muted={!cat.model}>{cat.model ?? `(${snap.inference.model_name || '(auto)'})`}</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<div class="field muted">Values in parentheses are inherited from the base provider/model.</div>
		</div>
	{/if}
	<div class="section">
		<div class="field muted">Reaction req id: {snap.inference.reaction_req_id}</div>
	</div>
	<div class="section">
		<h4>Call Log ({snap.inference.call_log.length})</h4>
		{#if snap.inference.call_log.length === 0}
			<div class="field muted">(no calls yet)</div>
		{:else}
			{@const avgMs = Math.round(snap.inference.call_log.reduce((s, e) => s + e.duration_ms, 0) / snap.inference.call_log.length)}
			{@const errorCount = snap.inference.call_log.filter(e => e.error).length}
			<div class="field muted">Avg latency: {avgMs}ms | Errors: {errorCount}</div>
			{#each [...snap.inference.call_log].reverse() as entry}
				{@const npcLabel = npcLabelFromEntry(entry)}
				<div class="log-row-wrap">
					<button class="log-row" class:log-row-error={entry.error} onclick={() => onSelectLog(entry.request_id)}>
						<span class="muted">[{entry.timestamp}]</span>
						<span class="log-id">#{entry.request_id}</span>
						{#if npcLabel}<span class="log-npc accent">{npcLabel}</span>{:else}<span class="log-model">{entry.model}</span>{/if}
						{#if entry.streaming}<span class="log-badge stream">STREAM</span>{/if}
						{#if entry.error}<span class="log-badge error">ERROR</span>{:else}<span class="log-badge ok">OK</span>{/if}
						<span class="muted">{entry.duration_ms}ms{#if entry.ttft_ms != null} · ttft {entry.ttft_ms}ms{/if}{#if entry.output_tokens != null && entry.ttft_ms != null && entry.duration_ms > entry.ttft_ms} · {(entry.output_tokens / ((entry.duration_ms - entry.ttft_ms) / 1000)).toFixed(1)} tok/s{/if}</span>
					</button>
					<BugChip kind="inference" label={`call #${entry.request_id} (${entry.model})`} detail={entry} />
				</div>
			{/each}
		{/if}
	</div>
{/if}

{#if byokOpen}
	<div
		class="byok-modal-backdrop"
		role="dialog"
		aria-modal="true"
		aria-label="Change provider or key"
	>
		<div class="byok-modal">
			<button
				type="button"
				class="byok-modal__close"
				onclick={() => (byokOpen = false)}
				aria-label="Close"
			>
				✕
			</button>
			<ByokOnboarding
				mode="modal"
				onComplete={() => (byokOpen = false)}
				onBack={() => (byokOpen = false)}
			/>
		</div>
	</div>
{/if}

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }

	.back-btn {
		align-self: flex-start;
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		padding: 0.15rem 0.5rem;
		font-size: 0.65rem;
		margin-bottom: 0.5rem;
	}

	.back-btn:hover,
	.back-btn:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.preset-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		margin-bottom: 0.25rem;
	}

	.byok-row {
		margin-top: 0.4rem;
	}
	.byok-btn {
		background: none;
		border: 1px solid var(--color-accent);
		color: var(--color-accent);
		cursor: pointer;
		padding: 0.25rem 0.6rem;
		font-size: 0.7rem;
	}
	.byok-btn:hover,
	.byok-btn:focus-visible {
		background: var(--color-accent);
		color: var(--color-bg);
	}

	.byok-modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}
	.byok-modal {
		position: relative;
		background: var(--color-bg);
		border: 1px solid var(--color-accent);
		border-radius: 0.5rem;
		max-width: 40rem;
		width: 95%;
		max-height: 90vh;
		overflow-y: auto;
		padding: 1rem;
	}
	.byok-modal__close {
		position: absolute;
		top: 0.5rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--color-muted);
		cursor: pointer;
		font-size: 1rem;
	}
	.byok-modal__close:hover,
	.byok-modal__close:focus-visible {
		color: var(--color-fg);
	}

	.preset-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		cursor: pointer;
		padding: 0.15rem 0.5rem;
		font-size: 0.7rem;
		font-family: inherit;
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}

	.preset-btn:hover,
	.preset-btn:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
		background: color-mix(in srgb, var(--color-accent) 10%, transparent);
	}

	.configured-icon {
		display: flex;
		align-items: center;
		color: var(--color-name);
	}

	.missing-icon {
		display: flex;
		align-items: center;
		color: var(--color-muted);
		opacity: 0.5;
	}

	.role-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.7rem;
		margin-bottom: 0.25rem;
	}

	.role-table th {
		text-align: left;
		padding: 0.1rem 0.5rem 0.1rem 0;
		font-weight: 600;
		color: var(--color-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.role-table td {
		padding: 0.1rem 0.5rem 0.1rem 0;
		vertical-align: top;
	}

	.role-table td.muted {
		color: var(--color-muted);
	}

	.log-row-wrap {
		display: flex;
		align-items: center;
		gap: 0.1rem;
	}
	.log-row-wrap .log-row {
		flex: 1;
		min-width: 0;
	}

	.log-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: baseline;
		width: 100%;
		padding: 0.3rem 0.5rem;
		background: none;
		border: none;
		border-bottom: 1px solid var(--color-border);
		cursor: pointer;
		text-align: left;
		font-size: 0.72rem;
		color: var(--color-fg);
	}

	.log-row:hover {
		background: var(--color-input-bg);
	}

	.log-row.log-row-error {
		background: color-mix(in srgb, #ff4444 8%, transparent);
	}

	.log-detail-header {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: baseline;
		font-size: 0.72rem;
		margin-bottom: 0.5rem;
	}

	.prompt-block {
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		padding: 0.4rem 0.5rem;
		font-size: 0.65rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--color-fg);
		margin: 0.1rem 0 0.4rem;
	}

	.prompt-label {
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-muted);
		margin-top: 0.4rem;
	}

	.log-id {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.log-model {
		color: var(--color-accent);
		font-weight: 600;
	}

	.log-npc {
		font-weight: 600;
	}

	.log-badge {
		font-size: 0.55rem;
		padding: 0.05rem 0.3rem;
		border-radius: 2px;
		text-transform: uppercase;
		font-weight: 700;
		letter-spacing: 0.05em;
	}

	.log-badge.stream {
		background: color-mix(in srgb, var(--color-accent) 20%, transparent);
		color: var(--color-accent);
	}

	.log-badge.ok {
		background: color-mix(in srgb, #44cc44 20%, transparent);
		color: #44cc44;
	}

	.log-badge.error {
		background: color-mix(in srgb, #ff4444 20%, transparent);
		color: #ff4444;
	}

	.log-error-msg {
		color: #ff4444;
		font-size: 0.65rem;
		margin-top: 0.1rem;
		word-break: break-word;
	}
</style>
