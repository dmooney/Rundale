<script lang="ts">
	import {
		bugReportVisible,
		bugReportContext,
		bugReportScreenshot,
		closeBugReport
	} from '../stores/bugReport';
	import { submitBugReport } from '$lib/ipc';
	import type { BugReportResult } from '$lib/types';

	let title = $state('');
	let description = $state('');
	let submitting = $state(false);
	let error = $state('');
	let result = $state<BugReportResult | null>(null);

	// Reset the form each time the modal opens.
	$effect(() => {
		if ($bugReportVisible) {
			title = '';
			description = '';
			error = '';
			result = null;
			submitting = false;
		}
	});

	async function submit() {
		if (!title.trim() || submitting) return;
		submitting = true;
		error = '';
		try {
			result = await submitBugReport({
				title: title.trim(),
				description: description.trim(),
				screenshotDataUrl: $bugReportScreenshot ?? undefined,
				context: $bugReportContext ?? undefined
			});
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			submitting = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (!$bugReportVisible) return;
		if (e.key === 'Escape') closeBugReport();
		// Cmd/Ctrl+Enter submits.
		if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') void submit();
	}
</script>

<svelte:window onkeydown={onKeydown} />

{#if $bugReportVisible}
	<div
		class="overlay"
		role="dialog"
		aria-modal="true"
		aria-label="Report a bug"
		data-testid="bug-report-modal"
	>
		<div class="modal">
			<div class="modal-header">
				<span class="modal-title">🐛 Report a bug</span>
				<button class="close-x" aria-label="Close" onclick={closeBugReport}>✕</button>
			</div>

			<div class="modal-body">
				{#if result}
					<div class="result" data-testid="bug-report-result">
						<p class="result-message">{result.message}</p>
						{#if result.issue_url}
							<p>
								<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- result.issue_url is an external GitHub issue URL, not a SvelteKit route -->
								<a href={result.issue_url} target="_blank" rel="noreferrer noopener"
									>Open issue #{result.issue_number}</a
								>
							</p>
						{:else if result.bundle_path}
							<p class="muted">Saved locally (dry-run): <code>{result.bundle_path}</code></p>
						{/if}
					</div>
				{:else}
					<label class="field-label" for="bug-title">Title</label>
					<input
						id="bug-title"
						class="text-input"
						type="text"
						bind:value={title}
						placeholder="Short summary of the problem"
						maxlength="500"
						autocomplete="off"
					/>

					<label class="field-label" for="bug-description">Description</label>
					<textarea
						id="bug-description"
						class="text-input text-area"
						bind:value={description}
						placeholder="What happened? What did you expect?"
						rows="5"
					></textarea>

					<p class="attach-note">
						A screenshot{$bugReportScreenshot ? '' : ' (capture failed)'}, recent logs, and
						current game state will be attached automatically.
					</p>

					{#if $bugReportContext}
						<p class="attach-note context-note" data-testid="bug-report-context">
							Attached record: <strong>{$bugReportContext.kind}</strong> — {$bugReportContext.label}
						</p>
					{/if}

					{#if error}
						<p class="error" role="alert">{error}</p>
					{/if}
				{/if}
			</div>

			<div class="modal-footer">
				{#if result}
					<span class="footer-spacer"></span>
					<button class="footer-btn" onclick={closeBugReport}>Done</button>
				{:else}
					<span class="footer-spacer"></span>
					<button class="footer-btn" onclick={closeBugReport} disabled={submitting}>Cancel</button>
					<button
						class="footer-btn primary"
						onclick={submit}
						disabled={submitting || !title.trim()}
						data-testid="bug-report-submit"
					>
						{submitting ? 'Filing…' : 'File bug'}
					</button>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.modal {
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		max-width: 32rem;
		width: 90%;
		display: flex;
		flex-direction: column;
		border-radius: 2px;
	}

	.modal-header {
		padding: 0.6rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.modal-title {
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--color-accent);
	}

	.close-x {
		background: none;
		border: none;
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.8rem;
	}
	.close-x:hover {
		color: var(--color-fg);
	}

	.modal-body {
		flex: 1;
		overflow: auto;
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.field-label {
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-muted);
	}

	.text-input {
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		font-family: inherit;
		font-size: 0.8rem;
		padding: 0.35rem 0.5rem;
		border-radius: 2px;
		width: 100%;
		box-sizing: border-box;
	}
	.text-input:focus {
		outline: none;
		border-color: var(--color-accent);
	}
	.text-area {
		resize: vertical;
		min-height: 4rem;
	}

	.attach-note {
		font-size: 0.65rem;
		color: var(--color-muted);
		margin: 0.25rem 0 0;
	}
	.context-note {
		border-left: 2px solid var(--color-accent);
		padding-left: 0.4rem;
	}

	.error {
		color: #e06c75;
		font-size: 0.7rem;
	}

	.result-message {
		font-size: 0.8rem;
		color: var(--color-fg);
	}
	.muted {
		color: var(--color-muted);
		font-size: 0.7rem;
		word-break: break-all;
	}

	.modal-footer {
		padding: 0.4rem 0.75rem;
		border-top: 1px solid var(--color-border);
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.footer-spacer {
		flex: 1;
	}

	.footer-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.65rem;
		padding: 0.15rem 0.5rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.footer-btn:hover:not(:disabled),
	.footer-btn:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}
	.footer-btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.footer-btn.primary {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}
</style>
