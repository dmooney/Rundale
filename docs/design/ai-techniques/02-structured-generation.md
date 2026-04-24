# Structured Generation & Constrained Decoding

**Target crate:** `crates/parish-inference/` (new `schema` module), consumers in
`crates/parish-npc/ticks.rs`.

## Problem

Today the LLM emits free text, a `---` separator, then JSON (ADR-008). Parsing
relies on the model "remembering" the contract. Failures surface as:

- Missing `---` → everything treated as prose, no state updates.
- Invalid JSON keys → Tier 2 tick silently drops mood changes.
- Hallucinated enum values (`action: "pray_for_rain"` when only `move|talk|look|
  interact|examine` is permitted).

The inference pipeline already has retries, but retries cost tokens and break
streaming.

## SOTA techniques

### 1. Grammar-based decoding (GBNF, Outlines, XGrammar)

- **llama.cpp GBNF:** ships natively; enforces a context-free grammar at
  sampling time. Zero-overhead on CPU, very small on GPU.
- **Outlines / LM Format Enforcer:** Python-side; mask invalid tokens per step.
- **XGrammar:** newer (2024), near-zero overhead even for complex JSON schemas.

Ollama supports GBNF via `format: json` + `grammar` field in the raw API.
OpenAI-compatible `response_format: json_schema` gives the same guarantee on
cloud Tier 1.

### 2. JSON Schema as the single source of truth

- Define `NpcTickOutput` as a `#[derive(JsonSchema)]` Rust struct via
  `schemars`.
- Emit the schema both to the prompt (for model steering) and to the decoder
  (for guarantee).
- Downstream parsing is `serde_json::from_str::<NpcTickOutput>(...)`; no
  tolerant parsing, no regex rescue.

### 3. Streaming + structured

Dialogue wants streaming; structure wants all-at-once. Resolve with a two-
segment contract already hinted at by ADR-008:

```
<free-text dialogue streamed token by token>
<|meta|>
<grammar-constrained JSON object>
```

Use a custom stop sequence / sentinel (`<|meta|>`). Stream segment A to the UI;
switch the sampler to grammar mode for segment B. llama.cpp supports this via
runtime grammar swap; Ollama needs a tiny wrapper.

### 4. Speculative structured decoding

For Tier 2/3 (JSON-heavy, not streamed to a player) we can run:

- A **draft** from the 3B intent-parse model.
- A **verify** pass by the 9B tier-2 model against the grammar.

Grammar mask + speculative decoding gives ~2× throughput on tier-2 sim batches.

### 5. Typed tool calling

Instead of one mega-JSON blob, expose discrete typed tools
(`set_mood`, `update_relationship`, `add_knowledge`). Modern function-calling
APIs (OpenAI, Anthropic, Ollama ≥0.4) already enforce schemas. This also feeds
directly into `04-agent-planning`.

## Minimal first cut

1. Add `crates/parish-schema` with `NpcTickOutput`, `DialogueTurn`,
   `IntentResult` types + `schemars` schemas.
2. Extend `parish-inference::provider` with an optional `grammar: Option<Grammar>`
   field on `InferenceRequest`.
3. Generate GBNF from the schemars schema at build time (cache as `OnceLock`).
4. Wire Tier 2 ticks to the grammar path first (no streaming, pure win).
5. Tier 1: add grammar swap at `<|meta|>` sentinel; fall back to unconstrained
   if the provider doesn't support it (cloud routes through `response_format`).

## Risks

- Grammar + Ollama interaction is version-sensitive — pin a known-good build
  and integration-test.
- Over-constraining kills creativity. Keep dialogue prose *outside* the
  grammar; only constrain metadata.
- Mode parity: OpenRouter models vary wildly in schema support. Provider
  capability flag already exists; extend it.

## Papers / references

- Willard & Louf, *Efficient Guided Generation for LLMs* (Outlines, 2023).
- Geng et al., *Grammar-Constrained Decoding for Structured NLP Tasks* (2023).
- Dong et al., *XGrammar: Flexible and Efficient Structured Generation Engine* (2024).
- OpenAI, *Structured Outputs* blog post (Aug 2024).
