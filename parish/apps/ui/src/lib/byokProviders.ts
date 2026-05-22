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
		keyless: p.keyless
	};
}

export function findProvider(
	id: string,
	featured: ByokProviderMeta[],
	other: ByokProviderMeta[]
): ByokProviderMeta | undefined {
	return [...featured, ...other].find((p) => p.id === id);
}
