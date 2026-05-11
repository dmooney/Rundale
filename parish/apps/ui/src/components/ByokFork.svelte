<script lang="ts">
	/**
	 * Equal-weight chooser shown on first launch:
	 *   "Run locally (Ollama)"  |  "Use a hosted API (BYOK)"
	 * Modeled after opencode / openclaw fork screens. Neither side is marked
	 * as recommended — copy nudges based on the user's situation.
	 */
	import ByokOnboarding from './ByokOnboarding.svelte';
	import { setProviderConfig } from '$lib/ipc';

	let { onComplete }: { onComplete: () => void } = $props();

	let mode: 'fork' | 'byok' | 'ollama-confirming' = $state('fork');
	let ollamaError = $state('');

	async function pickOllama() {
		mode = 'ollama-confirming';
		ollamaError = '';
		try {
			await setProviderConfig({ provider: 'ollama' });
			// Backend bootstraps Ollama; the existing setup-status / setup-done
			// flow takes over and the parent SetupOverlay renders the spinner.
			onComplete();
		} catch (e) {
			ollamaError = String(e);
			mode = 'fork';
		}
	}
</script>

{#if mode === 'fork'}
	<div class="byok-fork">
		<h1 class="byok-fork__title">How do you want to power Rundale?</h1>
		<p class="byok-fork__sub">
			Rundale runs on a large language model. Pick where it should run.
		</p>

		<div class="byok-fork__cards">
			<button class="byok-fork__card" onclick={pickOllama} type="button">
				<h2>Run locally (Ollama)</h2>
				<p class="byok-fork__blurb">
					Free and private. Downloads a multi-GB model and runs it on your
					machine. Best with a discrete GPU.
				</p>
				<p class="byok-fork__detail">
					Auto-installs Ollama, picks a model that fits your hardware, and
					streams everything offline.
				</p>
				<span class="byok-fork__cta">Set up Ollama</span>
			</button>

			<button
				class="byok-fork__card"
				onclick={() => (mode = 'byok')}
				type="button"
			>
				<h2>Use a hosted API (BYOK)</h2>
				<p class="byok-fork__blurb">
					Bring your own API key. Faster on most laptops; pay-per-use.
				</p>
				<p class="byok-fork__detail">
					Anthropic, OpenAI, OpenRouter, Groq, Google, xAI, and more. Your
					key is stored in your OS keychain.
				</p>
				<span class="byok-fork__cta">Choose a provider</span>
			</button>
		</div>

		{#if ollamaError}
			<p class="byok-fork__error">{ollamaError}</p>
		{/if}
	</div>
{:else if mode === 'byok'}
	<ByokOnboarding {onComplete} onBack={() => (mode = 'fork')} />
{:else if mode === 'ollama-confirming'}
	<div class="byok-fork__pending">Starting Ollama setup…</div>
{/if}

<style>
	.byok-fork {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1.5rem;
		padding: 2rem;
		max-width: 60rem;
		margin: 0 auto;
	}
	.byok-fork__title {
		font-size: 1.75rem;
		text-align: center;
		margin: 0;
	}
	.byok-fork__sub {
		opacity: 0.7;
		text-align: center;
		margin: 0;
	}
	.byok-fork__cards {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1.5rem;
		width: 100%;
		margin-top: 1rem;
	}
	@media (max-width: 720px) {
		.byok-fork__cards {
			grid-template-columns: 1fr;
		}
	}
	.byok-fork__card {
		background: rgba(255, 255, 255, 0.03);
		border: 1px solid rgba(255, 255, 255, 0.12);
		border-radius: 0.75rem;
		padding: 1.5rem;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
		transition:
			background 120ms,
			border-color 120ms,
			transform 120ms;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}
	.byok-fork__card:hover {
		background: rgba(255, 255, 255, 0.07);
		border-color: rgba(255, 255, 255, 0.25);
		transform: translateY(-1px);
	}
	.byok-fork__card h2 {
		margin: 0;
		font-size: 1.1rem;
	}
	.byok-fork__blurb {
		margin: 0;
		font-weight: 600;
	}
	.byok-fork__detail {
		margin: 0;
		opacity: 0.7;
		font-size: 0.9rem;
	}
	.byok-fork__cta {
		margin-top: auto;
		opacity: 0.7;
		font-size: 0.85rem;
	}
	.byok-fork__error {
		color: var(--accent-warn, #f55);
	}
	.byok-fork__pending {
		text-align: center;
		padding: 2rem;
		opacity: 0.7;
	}
</style>
