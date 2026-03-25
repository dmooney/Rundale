# ADR-017: Per-Category Inference Providers

## Status

Accepted

## Context

Parish has three distinct inference categories with different requirements:

- **Dialogue** (Tier 1): Player-facing NPC conversation. Benefits from high-quality cloud models. Uses streaming.
- **Simulation** (Tier 2): Background NPC group interactions. Runs frequently, tolerates lower quality. Non-streaming JSON.
- **Intent**: Player input parsing. Needs low latency. Small structured JSON output.

The previous dual-client architecture (ADR-013) only supported two tiers: a single local provider and an optional cloud provider for dialogue. Simulation and intent were always locked to the local provider. Users wanted flexibility to:

- Run all categories on a remote server (no local GPU required)
- Use different models per category (e.g., large model for dialogue, small for intent)
- Mix local and cloud providers freely across categories

## Decision

Each inference category can independently configure its own provider, model, base URL, and API key. Unconfigured categories inherit from the base `[provider]` config.

### Configuration layers (per category, later overrides earlier):

1. Base `[provider]` config (fallback)
2. TOML `[provider.<category>]` section
3. `PARISH_<CATEGORY>_*` environment variables
4. `--<category>-*` CLI flags
5. Legacy `[cloud]` / `PARISH_CLOUD_*` / `--cloud-*` (dialogue only, lowest priority override)

### Runtime slash commands (all modes):

- `/model.<category> [name]` — Show or set model for a category
- `/provider.<category> [name]` — Show or set provider for a category
- `/key.<category> [value]` — Show or set API key for a category

Where `<category>` is `dialogue`, `simulation`, or `intent`. The base `/model`, `/provider`, `/key` commands also display per-category overrides.

### New types:

- `InferenceCategory` enum: `Dialogue`, `Simulation`, `Intent`
- `CategoryConfig`: Resolved per-category provider config
- `CliCategoryOverrides`: Per-category CLI flag overrides
- `InferenceClients`: Refactored from dual local/cloud fields to a `HashMap<InferenceCategory, (Client, Model)>` with a base fallback

## Consequences

- Full flexibility: any category can use any OpenAI-compatible provider
- Backward compatible: existing `[cloud]` config, `--cloud-*` flags, and `PARISH_CLOUD_*` env vars continue to work
- The `InferenceClients` struct is now generic over categories rather than hardcoded to local/cloud
- Adding new inference categories in the future requires only adding a variant to the enum
- Slightly more complex configuration surface, but all overrides are optional
