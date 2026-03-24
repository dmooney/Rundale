<script lang="ts">
	import { streamingActive } from '../stores/game';
	import { submitInput } from '$lib/ipc';

	let inputEl: HTMLInputElement;
	let text = $state('');

	$effect(() => {
		if (!$streamingActive && inputEl) {
			inputEl.focus();
		}
	});

	async function handleSubmit(e: Event) {
		e.preventDefault();
		const trimmed = text.trim();
		if (!trimmed || $streamingActive) return;
		text = '';
		await submitInput(trimmed);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			handleSubmit(e);
		}
	}
</script>

<form class="input-form" onsubmit={handleSubmit}>
	<input
		bind:this={inputEl}
		bind:value={text}
		onkeydown={handleKeydown}
		disabled={$streamingActive}
		placeholder={$streamingActive ? 'Waiting…' : 'What do you do?'}
		class="input-field"
		autocomplete="off"
		spellcheck="false"
	/>
	<button type="submit" disabled={$streamingActive || !text.trim()} class="send-btn">
		Send
	</button>
</form>

<style>
	.input-form {
		display: flex;
		gap: 0.5rem;
		padding: 0.6rem 0.75rem;
		background: var(--color-panel-bg);
		border-top: 1px solid var(--color-border);
	}

	.input-field {
		flex: 1;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.5rem 0.75rem;
		font-size: 0.95rem;
		font-family: inherit;
		border-radius: 4px;
		outline: none;
	}

	.input-field:focus {
		border-color: var(--color-accent);
	}

	.input-field:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.input-field::placeholder {
		color: var(--color-muted);
	}

	.send-btn {
		background: var(--color-accent);
		color: var(--color-bg);
		border: none;
		padding: 0.5rem 1rem;
		font-size: 0.85rem;
		font-family: inherit;
		font-weight: 600;
		border-radius: 4px;
		cursor: pointer;
		transition: opacity 0.15s;
	}

	.send-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.send-btn:hover:not(:disabled) {
		opacity: 0.85;
	}
</style>
