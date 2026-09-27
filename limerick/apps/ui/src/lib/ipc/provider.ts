/**
 * BYOK provider configuration + local-inference onboarding IPC (#1200 TD-054).
 */

import { command } from './transport';

export interface ByokCategoryOverride {
	provider?: string;
	model?: string;
	base_url?: string;
}

export interface SetProviderConfigArgs {
	provider: string;
	base_url?: string;
	model?: string;
	api_key?: string;
	allow_insecure_http?: boolean;
	category_overrides?: Record<string, ByokCategoryOverride>;
}

export interface ValidateProviderConfigArgs {
	provider: string;
	base_url?: string;
	api_key?: string;
	allow_insecure_http?: boolean;
}

export type ValidationOutcome =
	| { kind: 'ok' }
	| { kind: 'auth_failed'; status: number; body_excerpt: string }
	| { kind: 'not_found'; status: number; body_excerpt: string }
	| { kind: 'rate_limited'; status: number; retry_after_secs: number | null }
	| { kind: 'network'; message: string }
	| { kind: 'unexpected'; status: number; body_excerpt: string };

export interface GetProviderConfigResult {
	provider: string;
	model: string;
	base_url: string;
	has_api_key: boolean;
	has_env_key: boolean;
}

export const setProviderConfig = (args: SetProviderConfigArgs) =>
	command<void>('set_provider_config', { args });

export const validateProviderConfig = (args: ValidateProviderConfigArgs) =>
	command<ValidationOutcome>('validate_provider_config', { args });

export const getProviderConfig = () =>
	command<GetProviderConfigResult>('get_provider_config');

export const clearProviderConfig = () => command<void>('clear_provider_config');

export const listByokEnvKeys = () =>
	command<Record<string, boolean>>('list_byok_env_keys');

export interface ProviderPresetOption {
	key: string;
	label: string;
	dialogue: string | null;
	simulation: string | null;
	intent: string | null;
	reaction: string | null;
}

export const listPresetModels = () =>
	command<Record<string, ProviderPresetOption[]>>('list_preset_models');

export interface AvailableProviderInfo {
	id: string;
	display_name: string;
	blurb: string | null;
	signup_url: string | null;
	needs_base_url: boolean;
	keyless: boolean;
	featured: boolean;
}

export interface AvailableProvidersResponse {
	featured: AvailableProviderInfo[];
	other: AvailableProviderInfo[];
}

export const listAvailableProviders = () =>
	command<AvailableProvidersResponse>('list_available_providers');

/**
 * Bindings for the local-inference onboarding flow (vllm-mlx on macOS).
 *
 * `OnboardingChoice` is serialized kebab-case by the Rust enum:
 *   "configured" | "local-recommended" | "local-experimental" |
 *   "local-low-mem" | "local-unavailable"
 *
 * `LocalSetupArgs.variant`:
 *   - "two-slot"   — 14B Dialogue + 1.5B small-slot. Recommended on Mac ≥ 16 GB.
 *   - "small-only" — 1.5B for everything. Mac < 16 GB; degraded quality.
 */
export type OnboardingChoice =
	| 'configured'
	| 'local-recommended'
	| 'local-experimental'
	| 'local-low-mem'
	| 'local-unavailable';

export interface OnboardingOptions {
	choice: OnboardingChoice;
	ram_gb: number;
}

export interface LocalSetupArgs {
	variant: 'two-slot' | 'small-only';
}

export const getOnboardingOptions = () =>
	command<OnboardingOptions>('get_onboarding_options');

export const startLocalInferenceSetup = (args: LocalSetupArgs) =>
	command<void>('start_local_inference_setup', { args });
