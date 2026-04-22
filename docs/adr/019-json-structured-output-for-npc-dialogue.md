# ADR 019: JSON Structured Output for NPC Dialogue

**Status:** Accepted
**Date:** 2026-04-22

## Context

Tier 1 NPC responses used a `---` separator convention: the LLM wrote dialogue
as free text, followed by `---`, followed by a JSON metadata block. This was
brittle for several reasons:

- LLMs occasionally produce `---` in dialogue (e.g. Markdown formatting).
- The separator could split across streaming chunks, requiring a holdback buffer.
- Parsing relied on prompt compliance — if the LLM ignored format instructions,
  metadata was silently lost.
- The fallback chain (separator → legacy NpcAction JSON → plain text) was complex
  and hard to test.

## Decision

Replace the separator approach with **JSON structured output** using the
provider's native `response_format: { "type": "json_object" }` mode. The Tier 1
response is now a single JSON object:

```json
{
  "dialogue": "(nods slowly) Aye, the road to Clifden is long...",
  "action": "nods slowly",
  "mood": "contemplative",
  "internal_thought": "This stranger asks too many questions",
  "language_hints": [{"word": "bóthar", "pronunciation": "BOH-her", "meaning": "road"}],
  "mentioned_people": []
}
```

During streaming, the `dialogue` field is extracted incrementally from the
partial JSON buffer using `extract_dialogue_from_partial_json()` in
`parish-types`. Metadata fields are parsed from the complete JSON after
streaming finishes.

## Consequences

**Positive:**
- Eliminates separator detection, holdback buffer, and multi-fallback parsing.
- JSON mode is natively supported by Ollama, OpenAI, and all major providers.
- Streaming UX is preserved — dialogue appears token-by-token.
- Metadata (mood, action, language hints) is reliably extracted.

**Negative:**
- Research suggests forcing JSON can degrade creative output by 17-26% for
  complex schemas. Our schema is simple (one free-text string field), so the
  impact is expected to be minimal.
- Anthropic has no native `response_format` equivalent; we augment the system
  prompt with a JSON-only instruction instead.

**Neutral:**
- The `NpcJsonResponse` struct and `NpcMetadata` struct coexist — the former is
  the deserialization target, the latter is the downstream-facing type used by
  the rest of the codebase.
