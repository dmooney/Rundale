# ADR-020: NPC Function-Calling / Tool-Use Output

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Proposed (2026-04-26). No implementation yet.

## Context

Tier 1 NPC dialogue today is rendered as **prose followed by a `---` separator and a JSON sidecar** (see [ADR-008 Structured JSON LLM Output](008-structured-json-llm-output.md), [ADR-019 JSON Structured Output for NPC Dialogue](019-json-structured-output-for-npc-dialogue.md)). The system prompt at `mods/rundale/prompts/tier1_system.txt` instructs the model to emit:

```
<dialogue text>
---
{
  "action": "...",
  "mood": "...",
  "internal_thought": "...",
  "irish_words": [...]
}
```

This works, but has two costs that show up in the harness:

1. **Parse fragility.** The separator-based protocol depends on the model behaving. We've already shipped one fix for this — see commit `1b3c5bc refactor: replace separator-based LLM parsing with JSON structured output (#472)`. The post-fix parser is robust, but every new field (e.g. tier-2 `mood_changes` arrays, `relationship_changes`) costs prompt-engineering effort to keep the model emitting the right shape across providers.
2. **Action enumeration drift.** `"action"` is a freeform string in the schema; the engine's enum (`speak | move | trade | work | rest | observe`) is *implied* by the prompt. The model can emit an action the engine doesn't know how to handle, and we discover it at runtime.

Most modern provider APIs (Anthropic, OpenAI, Google) expose **structured tool-calling**: the client declares a JSON schema for each tool, the provider validates the model's call against the schema, and the response is a guaranteed-typed structured object instead of free text. This eliminates both costs above.

Two complications:

- **Local Ollama doesn't reliably support tool-calling** for the model sizes we target (14B, 9B). [ADR-005 Ollama Local Inference](005-ollama-local-inference.md) commits us to a local-first option. Any decision must keep the offline path viable.
- **Streaming.** Today Tier 1 streams the prose to the client token-by-token (see `parish-server/src/ws.rs`, `parish-inference/src/utf8_stream.rs`); the JSON sidecar is parsed at the end. Tool-calling typically returns a single structured response, defeating the streaming UX.

## Decision

**Deferred.** Capture the trade-offs here and come back to it after one of the following:

- The harness gains [LLM quality evals](../plans/llm-quality-evals.md) (separate plan) — once we can quantitatively measure prose vs tool-call output quality, the choice is no longer guesswork.
- A local model gains reliable tool-calling at 9B parameters or below.
- The JSON-sidecar schema grows by another two fields (signal that prompt-engineering cost is exceeding tool-call benefit).

When we revisit, the candidate decision is:

> **Cloud Tier 1 dialogue uses provider-native tool-calling; local Ollama Tier 1 keeps the JSON-sidecar protocol as a fallback. Tier 2 / Tier 3 unconditionally use tool-calling on cloud providers since they don't stream and have a simpler schema.**

## Consequences

### If accepted (when we revisit)

**Easier:**

- Schema validation moves from prompt engineering to API contract — the engine's action enum becomes the tool's `action` enum, and the provider rejects bad calls before we see them.
- Adding a field is a code change, not a prompt-tweak iteration.
- LLM quality evals can rubric on tool-args validity directly (no JSON-extraction step).

**Harder:**

- **Two code paths per tier.** Cloud Tier 1 uses tool-calling; local Tier 1 uses the existing protocol. Mode parity (AGENTS.md §2) requires both to produce the same `Tier1Response` struct. Likely materializes as a `Tier1ResponseAdapter` trait with `CloudToolCallAdapter` and `JsonSidecarAdapter` impls.
- **Streaming UX.** Need to design how a tool-call response surfaces dialogue tokens to the client. Options: (a) keep dialogue as a streamed prose field on the tool's argument (token-stream within structured args is supported by Anthropic/OpenAI), (b) accept non-streaming Tier 1 on cloud and rely on cloud latency being low enough.
- **Prompt rewrite.** `tier1_system.txt` must teach the cloud path to call the tool *and* the local path to emit the sidecar. Prompt-template branching, gated on provider category from `parish-config`'s provider routing.

### If rejected

We keep the current protocol and accept the two costs above. The escape hatch is to invest more in the JSON-sidecar parser robustness.

## Alternatives Considered

### A. Adopt tool-calling everywhere immediately, drop local-Ollama Tier 1

Cleanest code path. Violates [ADR-005 Ollama Local Inference](005-ollama-local-inference.md). Rejected.

### B. Build a Rust-side JSON-schema validator that retries on bad output

Strictly an improvement to the current protocol. Cheaper than tool-calling, but doesn't address the action-enum-drift problem (still need a closed enum in the prompt and runtime enforcement). Could ship in parallel as a defensive measure regardless of this ADR's outcome.

### C. Move all of Tier 1 to a single cloud provider, drop local

Out of scope — that's a product decision, not an output-format one.

## Related

- [ADR-005 Ollama Local Inference](005-ollama-local-inference.md) — local-first commitment that constrains this decision.
- [ADR-008 Structured JSON LLM Output](008-structured-json-llm-output.md) — current output protocol.
- [ADR-013 Cloud LLM for Player Dialogue](013-cloud-llm-dialogue.md) — provider routing this decision would key off.
- [ADR-017 Per-Category Inference Providers](017-per-category-inference-providers.md) — the category split (Dialogue / Simulation / Intent / Reaction) maps onto where tool-calling is and isn't viable.
- [ADR-019 JSON Structured Output for NPC Dialogue](019-json-structured-output-for-npc-dialogue.md) — recent reinforcement of the current protocol.
- [LLM Quality Evals Plan](../plans/llm-quality-evals.md) — measurement framework that should land before this decision, so the trade-off is data-driven.
- [docs/design/npc-system.md](../design/npc-system.md) — current NPC pipeline this would change.
- `mods/rundale/prompts/tier1_system.txt` — the prompt to refactor.
