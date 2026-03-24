# Configuration Guide

> Back to [Docs Index](../index.md) | [Architecture Overview](overview.md)

Parish supports multiple LLM providers and lets you route different inference tasks to different backends. Configuration is resolved from three layers (highest priority first):

1. **CLI flags** (`--provider`, `--model`, etc.)
2. **Environment variables** (`PARISH_PROVIDER`, `PARISH_MODEL`, etc.)
3. **TOML config file** (`parish.toml`)

---

## Quick Start

### Option A: Local with Ollama (default, zero config)

```sh
# Install Ollama (https://ollama.ai), then just run:
cargo run
```

Parish auto-starts Ollama, detects your GPU, and pulls a model.

### Option B: Cloud via OpenRouter

```sh
# Set your API key and model:
export PARISH_PROVIDER=openrouter
export PARISH_API_KEY=sk-or-your-key-here
export PARISH_MODEL=google/gemma-3-1b-it:free
cargo run
```

Or copy `.env.example` to `.env` and fill in values.

### Option C: TOML config file

```sh
cp parish.toml.example parish.toml
# Edit parish.toml with your settings
cargo run
```

---

## Providers

| Provider | Name | Default URL | API Key | Model | Notes |
|----------|------|-------------|---------|-------|-------|
| Ollama | `ollama` | `localhost:11434` | No | Auto-selected | Auto-start, GPU detect, model pull |
| LM Studio | `lmstudio` | `localhost:1234` | No | Required | Must be running |
| OpenRouter | `openrouter` | `openrouter.ai/api` | **Required** | Required | Cloud gateway, many models |
| Custom | `custom` | Must set | Optional | Required | Any OpenAI-compatible endpoint |

---

## TOML Config File (`parish.toml`)

### Base Provider

The `[provider]` section configures the default backend for all inference:

```toml
[provider]
name = "ollama"
model = "qwen3:14b"
# base_url = "http://localhost:11434"
# api_key = "sk-..."
```

### Per-Category Overrides

Each inference category can use a different provider/model. Unconfigured categories inherit from `[provider]`:

```toml
# High-quality cloud model for player dialogue
[provider.dialogue]
name = "openrouter"
model = "anthropic/claude-sonnet-4"
api_key = "sk-or-..."

# Smaller local model for background NPC simulation
[provider.simulation]
name = "ollama"
model = "qwen3:4b"

# Tiny fast model for intent parsing
[provider.intent]
name = "ollama"
model = "qwen3:1.7b"
```

| Category | Purpose | Typical choice |
|----------|---------|----------------|
| `dialogue` | Player-facing NPC conversation (streamed) | Cloud for quality |
| `simulation` | Background NPC schedule ticks (JSON) | Local for speed/cost |
| `intent` | Parsing player input into actions (JSON) | Local, small & fast |

### Legacy Cloud Section

The `[cloud]` section still works and maps to `[provider.dialogue]`. If both are set, `[provider.dialogue]` takes precedence.

```toml
[cloud]
name = "openrouter"
model = "anthropic/claude-sonnet-4"
api_key = "sk-or-..."
```

---

## Environment Variables

### Base

| Variable | Description | Example |
|----------|-------------|---------|
| `PARISH_PROVIDER` | Provider backend | `ollama`, `openrouter` |
| `PARISH_MODEL` | Model name | `qwen3:14b` |
| `PARISH_BASE_URL` | API base URL | `http://localhost:11434` |
| `PARISH_API_KEY` | API key | `sk-or-...` |

### Per-Category

Each category has its own set of env vars. Pattern: `PARISH_{CATEGORY}_{FIELD}`.

| Variable | Category | Description |
|----------|----------|-------------|
| `PARISH_DIALOGUE_PROVIDER` | Dialogue | Provider override |
| `PARISH_DIALOGUE_MODEL` | Dialogue | Model override |
| `PARISH_DIALOGUE_BASE_URL` | Dialogue | Base URL override |
| `PARISH_DIALOGUE_API_KEY` | Dialogue | API key override |
| `PARISH_SIMULATION_PROVIDER` | Simulation | Provider override |
| `PARISH_SIMULATION_MODEL` | Simulation | Model override |
| `PARISH_SIMULATION_BASE_URL` | Simulation | Base URL override |
| `PARISH_SIMULATION_API_KEY` | Simulation | API key override |
| `PARISH_INTENT_PROVIDER` | Intent | Provider override |
| `PARISH_INTENT_MODEL` | Intent | Model override |
| `PARISH_INTENT_BASE_URL` | Intent | Base URL override |
| `PARISH_INTENT_API_KEY` | Intent | API key override |

### Legacy Cloud

| Variable | Description |
|----------|-------------|
| `PARISH_CLOUD_PROVIDER` | Cloud dialogue provider |
| `PARISH_CLOUD_MODEL` | Cloud dialogue model |
| `PARISH_CLOUD_BASE_URL` | Cloud dialogue URL |
| `PARISH_CLOUD_API_KEY` | Cloud dialogue API key |

You can also use a `.env` file (see `.env.example`).

---

## CLI Flags

### Base

```
--provider <NAME>     Provider backend (ollama, lmstudio, openrouter, custom)
--model <NAME>        Model name
--base-url <URL>      API base URL
--api-key <KEY>       API key
--config <PATH>       Path to TOML config file (default: parish.toml)
```

### Per-Category

```
--dialogue-provider <NAME>    Dialogue provider override
--dialogue-model <NAME>       Dialogue model override
--dialogue-base-url <URL>     Dialogue base URL override
--dialogue-api-key <KEY>      Dialogue API key override

--simulation-provider <NAME>  Simulation provider override
--simulation-model <NAME>     Simulation model override
--simulation-base-url <URL>   Simulation base URL override
--simulation-api-key <KEY>    Simulation API key override

--intent-provider <NAME>      Intent provider override
--intent-model <NAME>         Intent model override
--intent-base-url <URL>       Intent base URL override
--intent-api-key <KEY>        Intent API key override
```

### Legacy Cloud

```
--cloud-provider <NAME>       Cloud dialogue provider
--cloud-model <NAME>          Cloud dialogue model
--cloud-base-url <URL>        Cloud dialogue base URL
--cloud-api-key <KEY>         Cloud dialogue API key
```

### Other Flags

```
--headless            Plain stdin/stdout REPL (no UI)
--tui                 Terminal UI mode (default is GUI)
--script <FILE>       Run commands from file (JSON output, no LLM)
--improv              Enable improv craft mode for NPC dialogue
--screenshot [DIR]    Capture GUI screenshots (default: docs/screenshots)
```

---

## Examples

### All-local with Ollama (simplest)

```sh
cargo run
```

### OpenRouter for everything

```sh
cargo run --provider openrouter --model google/gemma-3-1b-it:free --api-key sk-or-...
```

### Local simulation, cloud dialogue

```toml
# parish.toml
[provider]
name = "ollama"

[provider.dialogue]
name = "openrouter"
model = "anthropic/claude-sonnet-4"
api_key = "sk-or-..."
```

### Different models per task

```toml
# parish.toml
[provider]
name = "ollama"
model = "qwen3:14b"

[provider.dialogue]
model = "qwen3:14b"   # largest for quality

[provider.simulation]
model = "qwen3:4b"    # medium for throughput

[provider.intent]
model = "qwen3:1.7b"  # smallest for speed
```

---

## Resolution Order

For each setting, the first non-empty value wins:

1. CLI flag (e.g. `--dialogue-model`)
2. Environment variable (e.g. `PARISH_DIALOGUE_MODEL`)
3. TOML per-category section (e.g. `[provider.dialogue].model`)
4. TOML base section (e.g. `[provider].model`)
5. Provider default (e.g. Ollama auto-selects model)

---

## Related

- [Architecture Overview](overview.md) — provider support and inference pipeline
- [ADR-005](../adr/005-ollama-local-inference.md) — Ollama as default local provider
- [ADR-013](../adr/013-cloud-llm-dialogue.md) — Cloud LLM for player dialogue
- [ADR-016](../adr/016-per-category-inference-providers.md) — Per-category inference providers
