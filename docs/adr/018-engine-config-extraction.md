# ADR 018: Extract Engine Tuning into Configuration

## Status

Accepted

## Context

After the engine/game-data separation (ADR/PR #119), game content lives in the mod system (`mods/rundale/`). However, ~50 engine-level numeric constants (inference timeouts, game speed factors, encounter probabilities, NPC memory/cognition tuning, palette contrast thresholds) remained hardcoded, requiring recompilation to tune.

## Decision

Extract engine tuning parameters into an `[engine]` section of `parish.toml` via an `EngineConfig` struct hierarchy. All fields use `#[serde(default)]` so existing deployments are unaffected.

### What was extracted

- **Inference**: 5 timeout values (request, streaming, reachability, download, loading) + log ring buffer capacity
- **Game speed**: 5 speed presets (Slow through Ludicrous)
- **Encounters**: 7 per-time-of-day probability thresholds
- **NPC**: Memory capacity, separator holdback, context count, 4 truncation lengths, cognitive tier distances, tier 2 tick interval, 5 relationship label thresholds
- **Palette**: 2 contrast thresholds
- **World**: Fuzzy name-matching threshold (Jaro-Winkler similarity)
- **Persistence**: Journal compaction threshold (reserved, not yet wired)

### What stays in the mod system

Loading animation, prompt templates, encounter text, anachronism data, festivals, game year, UI labels — all content that varies by setting.

### What stays hardcoded

Time-of-day hour ranges, season month ranges, luminance coefficients, protocol constants (`---` separator, JSON field names) — structural/algorithmic values.

### Pattern

Each module adds a `_with_config()` function variant. The original function becomes a thin wrapper calling `_with_config(&Default::default())`.

## Consequences

- Runtime tuning of engine parameters without recompilation
- Full backward compatibility — no config file needed
- Clear separation: mod system = content, `parish.toml` = engine tuning
- `parish.example.toml` documents all available settings
