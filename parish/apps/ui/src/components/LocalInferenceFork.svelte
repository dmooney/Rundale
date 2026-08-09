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
	 * Variant selection is driven by `ramGb`, not `choice`:
	 *   - `ramGb >= 24` → `two-slot` (Qwen 14B Dialogue + 1.5B
	 *     small slot). 14B 4-bit needs ~12-14 GB resident with KV
	 *     cache; adding the 1.5B + the OS + Tauri/webview pushes
	 *     total working set to ~20-24 GB. A 16 GB Mac runs out of
	 *     unified memory mid-load.
	 *   - `ramGb < 24`  → `small-only` (1.5B everywhere). Fits in
	 *     ~4 GB resident; OK on any 16 GB+ Mac.
	 *
	 * `choice = local-low-mem` (Mac < 16 GB) drives the warning
	 * copy that nudges the user toward BYOK before they commit.
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

	let mode: 'fork' | 'byok' = $state('fork');

	// Reactive so it tracks `ramGb` if the prop ever changes. The
	// two-slot loadout's working set is ~20-24 GB once the 14B is
	// resident (weights + KV cache + activations) plus the 1.5B and
	// host overhead; below that we ship the small-slot-only variant
	// even though `choice` says local is recommended.
	const variant: LocalSetupArgs['variant'] = $derived(
		ramGb >= 24 ? 'two-slot' : 'small-only'
	);

	async function pickLocal() {
		// Flip the SetupOverlay to the progress UI *before* awaiting the
		// IPC. The backend awaits the full HF download (potentially many
		// minutes) before resolving, so if this component stayed mounted
		// until the promise resolved the user would stare at the fork
		// for the entire download with no progress bar, status messages,
		// or activity log. Switching to the progress UI now lets the
		// setup-status / setup-progress / setup-done event listeners
		// on SetupOverlay drive the visuals; setup-done
		// (emitted by record_setup_done in parish-tauri on both success
		// and failure) fades the overlay or surfaces the error.
		onComplete();
		try {
			await startLocalInferenceSetup({ variant });
		} catch (e) {
			// Synchronous IPC rejections (e.g. the wizard_in_flight
			// guard) won't have emitted a setup-done — the fork is
			// already unmounted, so log for diagnostics. The inner
			// bootstrap path emits setup-done on its own errors.
			console.error('startLocalInferenceSetup rejected:', e);
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
				{#if variant === 'small-only'}
					<h2>Run locally (limited)</h2>
					<p class="local-fork__blurb">
						{#if choice === 'local-low-mem'}
							Below 16 GB unified memory the small model handles
							everything. Dialogue quality drops noticeably —
							consider BYOK for the best experience.
						{:else}
							Your Mac has under 24 GB unified memory, so we
							run the small (1.5B) model only. The 14B
							dialogue tier needs ~24 GB to hold weights + KV
							cache without swapping. Quality drops noticeably
							versus the two-slot loadout — consider BYOK if
							that matters.
						{/if}
					</p>
					<p class="local-fork__detail">
						Downloads ~1.3 GB of weights, then runs offline. No
						API key, no per-message cost.
					</p>
				{:else}
					<h2>
						Run locally ({choice === 'local-recommended'
							? 'qualified'
							: 'experimental'})
					</h2>
					<p class="local-fork__blurb">
						{#if choice === 'local-recommended'}
							Free and private. This exact local profile has passed
							Rundale's dialogue quality, reliability, latency, and
							memory gates.
						{:else}
							Free and private, but this exact local profile has not
							passed Rundale's production dialogue gate. Expect
							occasional invented facts or rewritten replies.
						{/if}
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
				<h2>
					Use a hosted API
					{choice === 'local-recommended' ? '(BYOK)' : '(recommended)'}
				</h2>
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

	</div>
{:else if mode === 'byok'}
	<ByokOnboarding {onComplete} onBack={() => (mode = 'fork')} />
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
</style>
