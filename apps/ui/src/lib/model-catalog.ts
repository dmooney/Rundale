/// Curated catalog of model identifiers used to power the `/model`
/// autocomplete dropdown. Names match what the backend forwards to each
/// provider verbatim (Anthropic Messages API model IDs, OpenAI model
/// names, Ollama tags, OpenRouter `vendor/model` slugs, etc.).
///
/// Keep this list focused on currently-shipping, well-known models —
/// it is a navigation aid, not an authoritative registry. Providers
/// continue to accept any string the user types directly.

export interface ModelSuggestion {
	/// The model identifier to send (e.g. `claude-opus-4-7`, `qwen3:14b`).
	name: string;
	/// Human-readable provider label shown in the dropdown.
	provider: string;
}

export const MODEL_CATALOG: ModelSuggestion[] = [
	// Anthropic — native Messages API
	{ name: 'claude-opus-4-7', provider: 'Anthropic' },
	{ name: 'claude-sonnet-4-6', provider: 'Anthropic' },
	{ name: 'claude-haiku-4-5', provider: 'Anthropic' },

	// OpenAI
	{ name: 'gpt-5.5', provider: 'OpenAI' },
	{ name: 'gpt-5.4-mini', provider: 'OpenAI' },
	{ name: 'gpt-5.4-nano', provider: 'OpenAI' },
	{ name: 'gpt-4o', provider: 'OpenAI' },
	{ name: 'gpt-4o-mini', provider: 'OpenAI' },

	// Google Gemini
	{ name: 'gemini-2.5-pro', provider: 'Google' },
	{ name: 'gemini-2.5-flash', provider: 'Google' },
	{ name: 'gemini-2.5-flash-lite', provider: 'Google' },

	// Groq (hosted open-source models)
	{ name: 'openai/gpt-oss-120b', provider: 'Groq' },
	{ name: 'llama-3.3-70b-versatile', provider: 'Groq' },
	{ name: 'llama-3.1-8b-instant', provider: 'Groq' },

	// xAI Grok
	{ name: 'grok-4.20-reasoning', provider: 'xAI' },
	{ name: 'grok-4.20-non-reasoning', provider: 'xAI' },
	{ name: 'grok-4.1-fast-non-reasoning', provider: 'xAI' },

	// Mistral — dated IDs; the -latest aliases still resolve to older builds
	{ name: 'mistral-large-2512', provider: 'Mistral' },
	{ name: 'mistral-medium-2508', provider: 'Mistral' },
	{ name: 'ministral-3-3b-2512', provider: 'Mistral' },

	// DeepSeek
	{ name: 'deepseek-v4-pro', provider: 'DeepSeek' },
	{ name: 'deepseek-v4-flash', provider: 'DeepSeek' },

	// Together AI
	{ name: 'Qwen/Qwen3.5-397B-A17B', provider: 'Together' },
	{ name: 'meta-llama/Llama-3.3-70B-Instruct-Turbo', provider: 'Together' },
	{ name: 'meta-llama/Llama-3.1-8B-Instruct-Turbo', provider: 'Together' },

	// OpenRouter (vendor-prefixed slugs)
	{ name: 'openrouter/auto', provider: 'OpenRouter' },
	{ name: 'anthropic/claude-opus-4-7', provider: 'OpenRouter' },
	{ name: 'anthropic/claude-sonnet-4-6', provider: 'OpenRouter' },
	{ name: 'anthropic/claude-haiku-4-5', provider: 'OpenRouter' },
	{ name: 'openai/gpt-4o', provider: 'OpenRouter' },
	{ name: 'google/gemini-2.5-flash', provider: 'OpenRouter' },
	{ name: 'meta-llama/llama-3.3-70b-instruct', provider: 'OpenRouter' },

	// Ollama (local tags) — Rundale's recommended tiers
	{ name: 'qwen3:32b', provider: 'Ollama' },
	{ name: 'qwen3:14b', provider: 'Ollama' },
	{ name: 'qwen3:8b', provider: 'Ollama' },
	{ name: 'qwen3:4b', provider: 'Ollama' },

	// LM Studio (local server)
	{ name: 'qwen3:32b', provider: 'LM Studio' },
	{ name: 'qwen3:14b', provider: 'LM Studio' },
	{ name: 'qwen3:4b', provider: 'LM Studio' },

	// vLLM (local inference server)
	{ name: 'Qwen/Qwen3-32B', provider: 'vLLM' },
	{ name: 'Qwen/Qwen3-14B', provider: 'vLLM' },
	{ name: 'Qwen/Qwen3-4B', provider: 'vLLM' }
];

/// Filter the catalog by a free-text query. Matches a substring against
/// either the model name or the provider label (case-insensitive).
/// Empty query returns the full catalog.
export function filterModels(query: string): ModelSuggestion[] {
	const trimmed = query.trim();
	if (trimmed === '') return MODEL_CATALOG;
	const q = trimmed.toLowerCase();
	return MODEL_CATALOG.filter(
		(m) => m.name.toLowerCase().includes(q) || m.provider.toLowerCase().includes(q)
	);
}

/// Per-category subcommand suffixes accepted after `/model.` (matches
/// `parish_config::InferenceCategory::from_name`).
export const MODEL_CATEGORIES = ['dialogue', 'simulation', 'intent', 'reaction'] as const;

/// If `text` matches `/model ` or `/model.<category> ` (with trailing
/// space), returns the leading `/model[.cat]` prefix and the remainder
/// the user has typed after the space. Otherwise returns `null`.
export function detectModelTrigger(
	text: string
): { prefix: string; query: string } | null {
	const match = /^\/model(\.[a-z]+)?\s(.*)$/i.exec(text);
	if (!match) return null;
	const dotted = match[1];
	if (dotted) {
		const cat = dotted.slice(1).toLowerCase();
		if (!(MODEL_CATEGORIES as readonly string[]).includes(cat)) return null;
		return { prefix: `/model.${cat}`, query: match[2] };
	}
	return { prefix: '/model', query: match[2] };
}
