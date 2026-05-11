<script lang="ts">
	/**
	 * macOS first-run fork:
	 *   "Run locally (vllm-mlx)"  |  "Use a hosted API (BYOK)"
	 *
	 * Mirrors ByokFork.svelte (the Ollama-vs-BYOK chooser on
	 * non-Mac hosts), but the local option is vllm-mlx with our
	 * bundled portable Python + Qwen2.5 4-bit weights pre-fetched
	 * over the existing setup-progress event surface.
	 *
	 * `choice = local-recommended` (Mac ≥ 16 GB): recommend the
	 * two-slot 14B + 1.5B loadout.
	 * `choice = local-low-mem`   (Mac < 16 GB): offer small-slot
	 * only (1.5B everywhere) with an honest "limited quality"
	 * warning so the user can pick BYOK without surprise.
	 */
	import ByokOnboarding from './ByokOnboarding.svelte';
	import {
		startLocalInferenceSetup,
		type LocalSetupArgs,
		type OnboardingChoice
	} from '$lib/ipc';

	let {
		onComplete,
		choice,
		ramGb
	}: {
		onComplete: () => void;
		choice: OnboardingChoice;
		ramGb: number;
	} = $props();

	let mode: 'fork' | 'byok' | 'local-confirming' = $state('fork');
	let localError = $state('');

	// Reactive so it tracks `choice` if the prop ever changes (Svelte 5
	// state_referenced_locally requires deriving rather than snapshotting).
	const variant: LocalSetupArgs['variant'] = $derived(
		choice === 'local-low-mem' ? 'small-only' : 'two-slot'
	);

	async function pickLocal() {
		mode = 'local-confirming';
		localError = '';
		try {
			await startLocalInferenceSetup({ variant });
			// Backend: HF download (with progress) → spawn vllm-mlx →
			// setup-done event → SetupOverlay fades.
			onComplete();
		} catch (e) {
			localError = String(e);
			mode = 'fork';
		}
	}
</script>

{#if mode === 'fork'}
	<div class="local-fork">
		<h1 class="local-fork__title">How do you want to power Rundale?</h1>
		<p class="local-fork__sub">
			Rundale runs on a large language model. On your Mac
			{#if ramGb > 0}({ramGb} GB unified memory){/if}
			we can run it locally with no API key required.
		</p>

		<div class="local-fork__cards">
			<button class="local-fork__card" onclick={pickLocal} type="button">
				{#if choice === 'local-low-mem'}
					<h2>Run locally (limited)</h2>
					<p class="local-fork__blurb">
						Below 16 GB unified memory the small model handles
						everything. Dialogue quality drops noticeably —
						consider BYOK for the best experience.
					</p>
					<p class="local-fork__detail">
						Downloads ~1.3 GB of weights, then runs offline. No
						API key, no per-message cost.
					</p>
				{:else}
					<h2>Run locally (recommended)</h2>
					<p class="local-fork__blurb">
						Free and private. Two MLX models — a 14B for player
						dialogue and a 1.5B for background simulation — run
						side-by-side on your Mac.
					</p>
					<p class="local-fork__detail">
						Downloads ~9 GB of weights once, then runs offline.
						No API key, no per-message cost.
					</p>
				{/if}
				<span class="local-fork__cta">Set up local inference</span>
			</button>

			<button
				class="local-fork__card"
				onclick={() => (mode = 'byok')}
				type="button"
			>
				<h2>Use a hosted API (BYOK)</h2>
				<p class="local-fork__blurb">
					Bring your own API key. Faster cold start; pay-per-use.
				</p>
				<p class="local-fork__detail">
					Anthropic, OpenAI, OpenRouter, Groq, Google, xAI, and
					more. Your key is stored in your OS keychain.
				</p>
				<span class="local-fork__cta">Choose a provider</span>
			</button>
		</div>

		{#if localError}
			<p class="local-fork__error">{localError}</p>
		{/if}
	</div>
{:else if mode === 'byok'}
	<ByokOnboarding {onComplete} onBack={() => (mode = 'fork')} />
{:else if mode === 'local-confirming'}
	<div class="local-fork__pending">Starting local inference setup…</div>
{/if}

<style>
	.local-fork {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1.5rem;
		padding: 2.5rem 1.5rem;
		max-width: 56rem;
		margin: 0 auto;
	}
	.local-fork__title {
		font-size: 2rem;
		margin: 0;
		text-align: center;
	}
	.local-fork__sub {
		font-size: 1.05rem;
		opacity: 0.85;
		text-align: center;
		margin: 0 0 0.5rem 0;
	}
	.local-fork__cards {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1.25rem;
		width: 100%;
	}
	@media (max-width: 720px) {
		.local-fork__cards {
			grid-template-columns: 1fr;
		}
	}
	.local-fork__card {
		text-align: left;
		padding: 1.5rem;
		border-radius: 0.75rem;
		border: 1px solid currentColor;
		background: transparent;
		color: inherit;
		font: inherit;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		transition: transform 80ms ease, box-shadow 80ms ease;
	}
	.local-fork__card:hover {
		transform: translateY(-2px);
		box-shadow: 0 4px 18px rgba(0, 0, 0, 0.2);
	}
	.local-fork__card h2 {
		margin: 0;
		font-size: 1.25rem;
	}
	.local-fork__blurb {
		margin: 0;
	}
	.local-fork__detail {
		margin: 0;
		font-size: 0.9rem;
		opacity: 0.75;
	}
	.local-fork__cta {
		margin-top: auto;
		font-weight: 600;
		text-decoration: underline;
	}
	.local-fork__error {
		color: #d33;
	}
	.local-fork__pending {
		font-size: 1.1rem;
		opacity: 0.85;
		padding: 2rem;
	}
</style>
