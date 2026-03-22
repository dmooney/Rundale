# ADR-013: Cloud LLM for Player Dialogue

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-22)

## Context

ADR-005 established Ollama local inference as the primary backend for all LLM calls. This works well for background NPC simulation (Tier 2 ticks, intent parsing) where structured JSON output from smaller models is sufficient. However, player-facing Tier 1 dialogue — the core roleplay experience — demands higher quality and speed than local models can reliably deliver:

- **Quality**: Cloud models (Claude, GPT-4) produce significantly richer, more nuanced dialogue for the 1820s Ireland roleplay setting.
- **Speed**: Cloud inference returns faster than local models on consumer hardware, especially for longer responses.
- **Hardware utilization**: Offloading dialogue to the cloud frees local GPU/CPU resources for background simulation, making the world richer overall.

Meanwhile, Tier 2 background simulation and intent parsing work well with local inference:

- Tier 2 produces structured JSON (mood changes, relationship deltas) — small models handle this fine.
- Intent parsing is structured classification — fast and reliable locally.
- Background simulation should continue running offline and at zero marginal cost.

## Decision

Add an optional cloud LLM provider alongside the existing local provider, with request routing based on inference purpose:

| Inference Type | Client | Rationale |
|---|---|---|
| Tier 1 dialogue (player-facing) | Cloud (if configured) | Quality + speed |
| Tier 2 simulation (NPC background) | Local (always) | Cost + offline + throughput |
| Intent parsing | Local (always) | Low latency, structured output |

When no cloud provider is configured, all inference uses the local provider (full backward compatibility with ADR-005).

### Configuration

Cloud provider is configured via a new `[cloud]` TOML section, `PARISH_CLOUD_*` environment variables, or `--cloud-*` CLI flags. The same layered resolution pattern from the local config is reused. Cloud defaults to OpenRouter (gateway to Claude, GPT-4, etc. via a single API key).

### Architecture

- `InferenceClients` struct holds both local and cloud `OpenAiClient` instances.
- `dialogue_client()` returns cloud if configured, local otherwise.
- `simulation_client()` and `intent_client()` always return local.
- The dialogue inference queue is backed by the cloud client; intent parsing calls the local client directly.
- Runtime commands (`/cloud model`, `/cloud key`, etc.) allow changing cloud settings without restart.

## Consequences

- **Quality**: Player dialogue improves significantly with cloud models.
- **Cost**: Cloud API calls cost money per token. Only Tier 1 dialogue uses cloud; background simulation remains free.
- **Network dependency**: Dialogue requires internet when cloud is configured. Game still works offline with local-only config.
- **Complexity**: Two clients instead of one, but cleanly separated by the `InferenceClients` routing struct.
- **ADR-005 compatibility**: Local inference remains the default. Cloud is opt-in.

## Alternatives Considered

1. **Cloud for everything**: Simpler routing but expensive and requires internet for all gameplay.
2. **Larger local models only**: Better hardware could help, but cloud models still outperform for roleplay quality.
3. **Hybrid with runtime fallback**: If cloud fails, retry on local. Deferred to a future enhancement — for now, static fallback (use local if no cloud configured) is sufficient.
