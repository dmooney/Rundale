<script lang="ts">
	/**
	 * BYOK wizard: provider grid → key entry → validate → save.
	 *
	 * Used in two modes:
	 *  - first-run via ByokFork (full overlay)
	 *  - settings re-edit via DebugPanel (modal)
	 */
	import { onMount } from 'svelte';
	import {
		setProviderConfig,
		validateProviderConfig,
		listByokEnvKeys,
		listPresetModels,
		listAvailableProviders,
		type ValidationOutcome,
		type SetProviderConfigArgs,
		type ProviderPresetOption,
	} from '$lib/ipc';
	import {
		toByokMeta,
		findProvider,
		FALLBACK_FEATURED,
		type ByokProviderMeta,
	} from '$lib/byokProviders';

	let {
		onComplete,
		onBack,
		mode = 'fullscreen',
	}: {
		onComplete: () => void;
		onBack?: () => void;
		mode?: 'fullscreen' | 'modal';
	} = $props();

	type Step = 'pick' | 'key' | 'validating' | 'saving' | 'error';

	let step: Step = $state('pick');
	let chosenId = $state('');
	let apiKey = $state('');
	let baseUrl = $state('');
	let modelName = $state('');
	let revealKey = $state(false);
	let allowInsecureHttp = $state(false);
	let validationError = $state<ValidationOutcome | null>(null);
	let saveError = $state('');

	// Provider lists fetched once on mount. Source of truth is the runtime
	// provider registry (parish-config builtins + mods/<id>/providers/*.toml);
	// the static byokProviders.ts arrays are no longer the picker's truth.
	let featured = $state<ByokProviderMeta[]>([]);
	let other = $state<ByokProviderMeta[]>([]);
	// True when the backend fetch failed and `featured` is the static
	// fallback set. The picker is still usable; the banner just tells
	// the user the dynamic list is stale (codex P2 regression fix).
	let providersFallback = $state(false);
	let chosen = $derived<ByokProviderMeta | undefined>(
		chosenId ? findProvider(chosenId, featured, other) : undefined,
	);

	// Map of {provider_id: has_env_key} fetched once on mount. The backend
	// never returns the env var value itself; we just surface a "leave blank
	// to use $ENV_VAR" hint on the key field. Critically, the lookup is keyed
	// by the *picked* provider id (not the current GameConfig provider) so
	// the hint shows during first-run too.
	let envKeys = $state<Record<string, boolean>>({});
	let presetModels = $state<Record<string, ProviderPresetOption[]>>({});
	onMount(() => {
		listAvailableProviders()
			.then((r) => {
				featured = r.featured.map(toByokMeta);
				other = r.other.map(toByokMeta);
				providersFallback = false;
			})
			.catch(() => {
				// Backend unreachable (transient web-mode network blip,
				// server cold-start, ad-blocker, ...). Fall back to a
				// minimal hand-picked set so onboarding is never blocked
				// by a single failed fetch.
				featured = FALLBACK_FEATURED;
				other = [];
				providersFallback = true;
			});
		listByokEnvKeys()
			.then((m) => (envKeys = m))
			.catch(() => (envKeys = {}));
		listPresetModels()
			.then((m) => (presetModels = m))
			.catch(() => (presetModels = {}));
	});
	let hasEnvKey = $derived(chosenId ? !!envKeys[chosenId] : false);

	function defaultModelFor(id: string): string {
		// First preset option's dialogue model is used to pre-fill the model name
		// field; other tiers fall back to their own presets via
		// fill_missing_models_from_presets after save.
		return presetModels[id]?.[0]?.dialogue ?? '';
	}

	function pick(p: ByokProviderMeta) {
		chosenId = p.id;
		apiKey = '';
		baseUrl = '';
		modelName = defaultModelFor(p.id);
		revealKey = false;
		validationError = null;
		saveError = '';
		step = 'key';
	}

	async function validateAndSave() {
		if (!chosen) return;
		if (!chosen.keyless && apiKey.trim().length === 0 && !hasEnvKey) {
			saveError = 'API key is required.';
			return;
		}
		if (chosen.needsBaseUrl && baseUrl.trim().length === 0) {
			saveError = 'Base URL is required.';
			return;
		}
		if (
			!chosen.keyless &&
			modelName.trim().length === 0 &&
			defaultModelFor(chosen.id) === ''
		) {
			saveError = 'A model name is required for this provider.';
			return;
		}

		validationError = null;
		saveError = '';
		step = 'validating';

		const outcome = await validateProviderConfig({
			provider: chosen.id,
			base_url: baseUrl.trim() || undefined,
			api_key: apiKey.trim() || undefined,
			allow_insecure_http: allowInsecureHttp,
		}).catch((e) => {
			return {
				kind: 'network',
				message: String(e),
			} satisfies ValidationOutcome;
		});

		if (outcome.kind !== 'ok') {
			validationError = outcome;
			step = 'error';
			return;
		}

		step = 'saving';
		const args: SetProviderConfigArgs = {
			provider: chosen.id,
			base_url: baseUrl.trim() || undefined,
			model: modelName.trim() || undefined,
			api_key: apiKey.trim() || undefined,
			allow_insecure_http: allowInsecureHttp,
		};
		try {
			await setProviderConfig(args);
			onComplete();
		} catch (e) {
			saveError = String(e);
			step = 'error';
		}
	}

	function describeError(o: ValidationOutcome): string {
		switch (o.kind) {
			case 'auth_failed':
				return `Authentication failed (HTTP ${o.status}). Double-check the key.`;
			case 'not_found':
				return `Endpoint not found (HTTP ${o.status}). Check the base URL.`;
			case 'rate_limited':
				return o.retry_after_secs
					? `Rate-limited. Wait ${o.retry_after_secs}s and retry.`
					: 'Rate-limited. Wait a bit and retry.';
			case 'network':
				return `Network error: ${o.message}`;
			case 'unexpected':
				return `Unexpected response (HTTP ${o.status}).`;
			default:
				return 'Unknown error.';
		}
	}

	function back() {
		if (step === 'pick') {
			onBack?.();
		} else {
			step = 'pick';
		}
	}
</script>

<div class="byok" class:byok--modal={mode === 'modal'}>
	{#if step === 'pick'}
		<div class="byok__inner">
			<h2>Choose a provider</h2>
			<p class="byok__sub">
				Pick the API you want Rundale to use for NPC dialogue.
			</p>

			{#if providersFallback}
				<p class="byok__fallback" role="status">
					Provider list unavailable; showing a minimal fallback set. Refresh to
					retry.
				</p>
			{/if}

			<div class="byok__grid">
				{#each featured as p (p.id + p.label)}
					<button class="byok__card" type="button" onclick={() => pick(p)}>
						<h3>{p.label}</h3>
						<p>{p.blurb}</p>
					</button>
				{/each}
			</div>

			<div class="byok__other-label">Other providers</div>
			<div class="byok__chips">
				{#each other as p (p.id + p.label)}
					<button
						class="byok__chip"
						type="button"
						onclick={() => pick(p)}
						title={p.blurb}
					>
						{p.label}
					</button>
				{/each}
			</div>

			{#if onBack}
				<button class="byok__back" type="button" onclick={back}>Back</button>
			{/if}
		</div>
	{:else if step === 'key' || step === 'validating' || step === 'saving' || step === 'error'}
		<div class="byok__inner">
			<button class="byok__back" type="button" onclick={back}>← Back</button>
			<h2>{chosen?.label}</h2>
			{#if chosen?.signupUrl}
				<p class="byok__sub">
					<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- chosen.signupUrl is an external provider signup URL, not a SvelteKit route -->
					<a href={chosen.signupUrl} target="_blank" rel="noopener noreferrer"
						>Get a key →</a
					>
				</p>
			{/if}

			{#if !chosen?.keyless}
				<label class="byok__field">
					<span>API key</span>
					<div class="byok__key">
						<input
							type={revealKey ? 'text' : 'password'}
							bind:value={apiKey}
							autocomplete="off"
							spellcheck="false"
							placeholder={hasEnvKey
								? '(env var detected — leave blank to use it)'
								: 'sk-...'}
							disabled={step === 'validating' || step === 'saving'}
						/>
						<button
							type="button"
							class="byok__eye"
							onclick={() => (revealKey = !revealKey)}
							aria-label={revealKey ? 'Hide key' : 'Show key'}
						>
							{revealKey ? '🙈' : '👁'}
						</button>
					</div>
					<small>Stored in your OS keychain on this machine.</small>
				</label>
			{/if}

			{#if chosen?.needsBaseUrl}
				<label class="byok__field">
					<span>Base URL</span>
					<input
						type="url"
						bind:value={baseUrl}
						placeholder="https://example.com/v1"
						disabled={step === 'validating' || step === 'saving'}
					/>
				</label>
			{/if}

			{#if chosen?.needsBaseUrl && baseUrl
					.trim()
					.startsWith('http://') && !/^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(?::|\/|$)/i.test(baseUrl.trim())}
				<label class="byok__field">
					<span
						><input type="checkbox" bind:checked={allowInsecureHttp} /> Allow insecure
						HTTP</span
					>
					<small
						>Credentials and model data will cross the network without TLS.</small
					>
				</label>
			{/if}

			<label class="byok__field">
				<span>Model (optional)</span>
				<input
					type="text"
					bind:value={modelName}
					placeholder={defaultModelFor(chosenId) || 'leave blank for default'}
					disabled={step === 'validating' || step === 'saving'}
				/>
				<small
					>Used for NPC dialogue. Other categories fall back to presets.</small
				>
			</label>

			{#if step === 'error' && validationError}
				<p class="byok__error">{describeError(validationError)}</p>
			{/if}
			{#if saveError}
				<p class="byok__error">{saveError}</p>
			{/if}

			<div class="byok__actions">
				<button
					type="button"
					class="byok__primary"
					onclick={validateAndSave}
					disabled={step === 'validating' || step === 'saving'}
				>
					{#if step === 'validating'}
						Validating…
					{:else if step === 'saving'}
						Saving…
					{:else}
						Validate &amp; continue
					{/if}
				</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.byok {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 1.5rem;
		color: inherit;
	}
	.byok--modal {
		max-width: 36rem;
		margin: 0 auto;
	}
	.byok__inner {
		width: 100%;
		max-width: 50rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.byok h2 {
		margin: 0;
		font-size: 1.4rem;
	}
	.byok__sub {
		opacity: 0.7;
		margin: 0;
	}
	.byok__sub a {
		color: inherit;
	}
	.byok__fallback {
		margin: 0.5rem 0 1rem;
		padding: 0.5rem 0.75rem;
		border-radius: 4px;
		background: rgba(220, 180, 80, 0.15);
		border: 1px solid rgba(220, 180, 80, 0.4);
		font-size: 0.9rem;
	}
	.byok__grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.75rem;
	}
	@media (max-width: 720px) {
		.byok__grid {
			grid-template-columns: 1fr;
		}
	}
	.byok__card {
		background: rgba(255, 255, 255, 0.03);
		border: 1px solid rgba(255, 255, 255, 0.12);
		border-radius: 0.5rem;
		padding: 0.9rem 1rem;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.byok__card:hover {
		background: rgba(255, 255, 255, 0.07);
		border-color: rgba(255, 255, 255, 0.25);
	}
	.byok__card h3 {
		margin: 0;
		font-size: 1rem;
	}
	.byok__card p {
		margin: 0;
		opacity: 0.7;
		font-size: 0.85rem;
	}
	.byok__other-label {
		margin-top: 0.5rem;
		font-size: 0.85rem;
		opacity: 0.6;
	}
	.byok__chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}
	.byok__chip {
		background: rgba(255, 255, 255, 0.04);
		border: 1px solid rgba(255, 255, 255, 0.18);
		border-radius: 999px;
		padding: 0.3rem 0.75rem;
		font: inherit;
		font-size: 0.85rem;
		color: inherit;
		cursor: pointer;
	}
	.byok__chip:hover,
	.byok__chip:focus-visible {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(255, 255, 255, 0.32);
	}
	.byok__field {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.byok__field span {
		font-weight: 600;
	}
	.byok__field input {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.18);
		border-radius: 0.35rem;
		padding: 0.5rem 0.75rem;
		color: inherit;
		font: inherit;
	}
	.byok__field small {
		opacity: 0.6;
		font-size: 0.8rem;
	}
	.byok__key {
		display: flex;
		gap: 0.4rem;
	}
	.byok__key input {
		flex: 1;
	}
	.byok__eye {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.18);
		border-radius: 0.35rem;
		color: inherit;
		cursor: pointer;
		font-size: 1rem;
		padding: 0 0.7rem;
	}
	.byok__error {
		color: var(--accent-warn, #f55);
		margin: 0;
	}
	.byok__primary {
		background: rgba(120, 200, 255, 0.18);
		border: 1px solid rgba(120, 200, 255, 0.45);
		color: inherit;
		border-radius: 0.4rem;
		padding: 0.6rem 1rem;
		cursor: pointer;
		font: inherit;
	}
	.byok__primary:disabled {
		opacity: 0.5;
		cursor: wait;
	}
	.byok__back {
		background: none;
		border: none;
		color: inherit;
		opacity: 0.7;
		cursor: pointer;
		font: inherit;
		align-self: flex-start;
	}
</style>
