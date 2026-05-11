/**
 * Curated provider metadata for the BYOK onboarding wizard.
 *
 * The full provider list (15 entries) is shipped by parish-config; this table
 * hand-picks the most-recognized options for the front-of-grid display, with
 * an "Other…" expander revealing the long tail. opencode zen is surfaced as
 * a labeled preset that resolves to `Provider::Custom` with a pre-filled
 * base URL — see `presetBaseUrl`.
 *
 * Default model names live in `parish-config/src/presets.rs` (the same table
 * `fill_missing_models_from_presets` reads). The wizard fetches them at
 * runtime via `list_preset_models` so we don't drift.
 */

export interface ByokProviderMeta {
	/** Lowercase provider id. Matches `Provider::from_str_loose` on the Rust side. */
	id: string;
	/** Display name. */
	label: string;
	/** Short tagline shown under the label in the grid. */
	blurb: string;
	/** Where to get an API key. */
	signupUrl: string;
	/** True if the provider needs an explicit base_url (Custom only). */
	needsBaseUrl: boolean;
	/** True if the provider does not require an API key (Ollama, LM Studio, vLLM, Simulator). */
	keyless: boolean;
	/**
	 * Pre-filled base URL when this preset resolves to `Provider::Custom`
	 * (e.g. opencode zen). When omitted, the provider's default base URL
	 * applies — leave `args.base_url` undefined when calling
	 * `set_provider_config`.
	 */
	presetBaseUrl?: string;
}

/** Top-of-grid curated cards. ~6 hits the opencode/openclaw sweet spot. */
export const FEATURED_PROVIDERS: ByokProviderMeta[] = [
	{
		id: 'anthropic',
		label: 'Anthropic',
		blurb: "Claude — the engine's native dialogue partner.",
		signupUrl: 'https://console.anthropic.com/settings/keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'openai',
		label: 'OpenAI',
		blurb: 'GPT-class models, broadest tooling.',
		signupUrl: 'https://platform.openai.com/api-keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'openrouter',
		label: 'OpenRouter',
		blurb: 'One key, dozens of model providers.',
		signupUrl: 'https://openrouter.ai/keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'groq',
		label: 'Groq',
		blurb: 'Fast tokens, generous free tier.',
		signupUrl: 'https://console.groq.com/keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'google',
		label: 'Google (Gemini)',
		blurb: 'Free tier with quota — Gemini family.',
		signupUrl: 'https://aistudio.google.com/app/apikey',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'xai',
		label: 'xAI (Grok)',
		blurb: 'OpenAI-compatible Grok API.',
		signupUrl: 'https://console.x.ai',
		needsBaseUrl: false,
		keyless: false
	}
];

/** "Other..." expander — the long tail and labeled presets. */
export const OTHER_PROVIDERS: ByokProviderMeta[] = [
	{
		id: 'mistral',
		label: 'Mistral',
		blurb: 'European, OpenAI-compatible.',
		signupUrl: 'https://console.mistral.ai/api-keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'deepseek',
		label: 'DeepSeek',
		blurb: 'Cost-efficient reasoning models.',
		signupUrl: 'https://platform.deepseek.com/api_keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'together',
		label: 'Together AI',
		blurb: 'Open-source models hosted.',
		signupUrl: 'https://api.together.xyz/settings/api-keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'nvidia-nim',
		label: 'NVIDIA NIM',
		blurb: 'NVIDIA-hosted OpenAI-compatible inference.',
		signupUrl: 'https://build.nvidia.com',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'lmstudio',
		label: 'LM Studio',
		blurb: 'Local desktop server. No key.',
		signupUrl: 'https://lmstudio.ai',
		needsBaseUrl: false,
		keyless: true
	},
	{
		id: 'vllm',
		label: 'vLLM',
		blurb: 'Self-hosted OpenAI-compatible server.',
		signupUrl: 'https://docs.vllm.ai',
		needsBaseUrl: false,
		keyless: true
	},
	{
		// opencode zen — sst's hosted gateway. Resolves to Provider::Custom on
		// the Rust side because there's no first-class enum variant; the
		// presetBaseUrl pre-fills the URL so the user only pastes a key.
		id: 'custom',
		label: 'opencode zen',
		blurb: "opencode's hosted gateway.",
		signupUrl: 'https://opencode.ai/zen',
		needsBaseUrl: true,
		keyless: false,
		presetBaseUrl: 'https://opencode.ai'
	},
	{
		id: 'custom',
		label: 'Custom (OpenAI-compatible)',
		blurb: 'Bring your own endpoint URL.',
		signupUrl: '',
		needsBaseUrl: true,
		keyless: false
	}
];

export function findProvider(id: string): ByokProviderMeta | undefined {
	return [...FEATURED_PROVIDERS, ...OTHER_PROVIDERS].find((p) => p.id === id);
}
