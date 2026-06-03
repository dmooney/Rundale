/**
 * UI-side types + adapters for the runtime-loaded provider registry.
 *
 * The provider list itself is no longer hand-curated here: the backend
 * exposes it via `list_available_providers` (Tauri command + HTTP route),
 * which reads from `parish-config` builtins plus every
 * `mods/<id>/providers/<id>.toml` discovered at startup. Adding a new
 * provider is a TOML drop under `mods/`, not a TS edit.
 *
 * `ByokProviderMeta` is the shape the wizard components consume; this
 * file converts `AvailableProviderInfo` from the IPC layer into it.
 */
import type { AvailableProviderInfo } from './ipc';

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

export function toByokMeta(p: AvailableProviderInfo): ByokProviderMeta {
	return {
		id: p.id,
		label: p.display_name,
		blurb: p.blurb ?? '',
		signupUrl: p.signup_url ?? '',
		needsBaseUrl: p.needs_base_url,
		keyless: p.keyless,
	};
}

export function findProvider(
	id: string,
	featured: ByokProviderMeta[],
	other: ByokProviderMeta[],
): ByokProviderMeta | undefined {
	return [...featured, ...other].find((p) => p.id === id);
}

/**
 * Tiny static fallback used by the wizard when `listAvailableProviders`
 * fails (transient network/server error in web mode). Keeps onboarding
 * unblocked — the user can still pick a recognisable cloud provider
 * even before the dynamic registry payload arrives.
 *
 * Intentionally minimal: five of the most-requested cloud providers, no
 * local-inference entries (those need the backend's live capability
 * info anyway). Provider ids match registered mod ids exactly.
 */
export const FALLBACK_FEATURED: ByokProviderMeta[] = [
	{
		id: 'anthropic',
		label: 'Anthropic',
		blurb: "Claude — the engine's native dialogue partner.",
		signupUrl: 'https://console.anthropic.com/settings/keys',
		needsBaseUrl: false,
		keyless: false,
	},
	{
		id: 'openai',
		label: 'OpenAI',
		blurb: 'GPT-class models, broadest tooling.',
		signupUrl: 'https://platform.openai.com/api-keys',
		needsBaseUrl: false,
		keyless: false,
	},
	{
		id: 'openrouter',
		label: 'OpenRouter',
		blurb: 'One key, dozens of model providers.',
		signupUrl: 'https://openrouter.ai/keys',
		needsBaseUrl: false,
		keyless: false,
	},
	{
		id: 'groq',
		label: 'Groq',
		blurb: 'Fast tokens, generous free tier.',
		signupUrl: 'https://console.groq.com/keys',
		needsBaseUrl: false,
		keyless: false,
	},
	{
		id: 'google',
		label: 'Google (Gemini)',
		blurb: 'Free tier with quota — Gemini family.',
		signupUrl: 'https://aistudio.google.com/app/apikey',
		needsBaseUrl: false,
		keyless: false,
	},
];
