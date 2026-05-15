/**
 * Curated provider metadata for the BYOK onboarding wizard.
 *
 * The full provider list is shipped by parish-config as TOML mods; this table
 * hand-picks the most-recognized options for the front-of-grid display, with
 * an "Other…" expander revealing the long tail.
 *
 * Default model names live in the per-provider TOML presets. The wizard fetches
 * them at runtime via `list_preset_models` so we don't drift.
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

/** "Other..." expander — the long tail plus new providers. */
export const OTHER_PROVIDERS: ByokProviderMeta[] = [
	{
		id: 'vercel-ai',
		label: 'Vercel AI Gateway',
		blurb: 'Route to any model through your Vercel project.',
		signupUrl: 'https://vercel.com/docs/ai-gateway',
		needsBaseUrl: true,
		keyless: false
	},
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
		id: 'cohere',
		label: 'Cohere',
		blurb: 'Command R models, OpenAI-compatible.',
		signupUrl: 'https://dashboard.cohere.com/api-keys',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'qwen',
		label: 'Qwen (Alibaba)',
		blurb: 'Qwen family via DashScope international.',
		signupUrl: 'https://dashscope.aliyuncs.com',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'zhipu',
		label: 'Zhipu AI',
		blurb: 'GLM-4 and friends, OpenAI-compatible.',
		signupUrl: 'https://open.bigmodel.cn',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'moonshot',
		label: 'Moonshot AI',
		blurb: 'Kimi long-context models.',
		signupUrl: 'https://platform.moonshot.cn',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'siliconflow',
		label: 'SiliconFlow',
		blurb: 'Fast Chinese inference cloud.',
		signupUrl: 'https://cloud.siliconflow.cn',
		needsBaseUrl: false,
		keyless: false
	},
	{
		id: 'scaleway',
		label: 'Scaleway',
		blurb: 'European GPU cloud, OpenAI-compatible.',
		signupUrl: 'https://console.scaleway.com',
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
