# ADR 014: Decision Endpoints

- Status: Accepted
- Date: 2026-10-10

## Context

Rundale needs to know whom a player's line is spoken to before any dialogue
call (Rundale design `docs/design/dynamic-npc-context.md` §4). That is a
classification with calibrated probabilities, not text generation. Decision
models such as TypeSafe's Jev, GPT-6 Luna Decisions and Microsoft-Decision-1
answer fixed questions with a probability per option. OpenRouter serves all
three through one alpha Decisions API (`POST /api/alpha/decisions`), which
accepts TypeSafe's System One request schema verbatim:

- a `state` (string, object or array) and a map of `questions`;
- `choice` questions with `criteria` per option (TypeSafe documents at most
  255 options), and `noul` yes/no questions with `true` and `false` criteria;
- answers `{ choice, confidence, probabilities }` or `{ noul }`, with `usage`
  in input and output tokens (OpenRouter also reports `usage.cost`).

OpenRouter's decision catalogue (`/api/v1/models?output_modalities=decisions`,
read 2026-10-10) prices these models per input token with free output, so the
existing per-token `FixedPriceCostCalculator` records their cost correctly.

## Decision

An Endpoint becomes a **decision Endpoint** when its `inferenceConfig` holds a
`decision` spec. The definition keeps its five keys and its input and output
schemas, so publishing, hashing, files (ADR 013), invocation envelopes and
accounting are unchanged, and no database migration is needed.

```json
"decision": {
  "version": 1,
  "stateFields": ["line", "present"],
  "idField": "optionId",
  "questions": {
    "recipient": {
      "type": "choice",
      "instructions": "…",
      "criteriaFrom": [{ "field": "present", "criterion": "… optionId {id} …" }],
      "criteria": { "no_one": "…" }
    },
    "mentioned": {
      "type": "noul",
      "instructions": "… optionId {id} …",
      "criteria": { "true": "…", "false": "…" },
      "forEach": "present"
    }
  }
}
```

- **State.** Only `stateFields` reach the model, so envelope identifiers stay
  behind.
- **Options from input.** A choice offers one option per element of each
  `criteriaFrom` array, in input order, then its fixed `criteria`. A `forEach`
  noul is asked once per element. The element's `idField` value is the option
  id and is the only input substituted into creator text (`{id}`); ids must be
  1 to 64 letters, digits, `_` or `-`, so untrusted input text never enters a
  question. Duplicate ids and choices outside 2 to 255 options are invalid
  input.
- **Instructions.** The definition's `instructions` precede every question's
  instructions; they hold rules shared by all questions.
- **Output.** `{ <choice>: { choice, confidence, probabilities }, <noul>: p,
  <forEach noul>: { <id>: p } }`, validated by the output schema. An answer
  that is missing, out of range, or not an offered option fails the attempt
  with `OUTPUT_VALIDATION_FAILED`, so callers never see an option they did not
  offer.
- **Providers.** `openrouter` (OpenRouter's Decisions API) and `typesafe`
  (TypeSafe's own `POST /v1/systemone`, the direct route for Jev) share one
  adapter, `DecisionsProvider`. They serve decision Endpoints only, and text
  providers never serve them. A decision Endpoint cannot stream and has no
  temperature.
- **Configuration.** `OPENROUTER_ALLOWED_MODELS` with `OPENROUTER_API_KEY`, and
  `TYPESAFE_ALLOWED_MODELS` with `TYPESAFE_API_KEY`, as for OpenAI: a provider
  with allowed models fails startup without its key, and every allowed model
  needs a `MODEL_PRICES_JSON` entry keyed `openrouter/<vendor>/<model>` or
  `typesafe/<model>`.

A direct route for GPT-6 Luna Decisions (OpenAI) or Microsoft-Decision-1
(Microsoft Foundry) is added only if Rundale's vendor comparison needs it.

## Consequences

This refines the MVP's "OpenAI and Google" provider scope (ADR 004) and the
Google-only rule for live deployments: a live deployment may also enable a
decision provider once its key is in Secret Manager. Scores are not supported
until an Endpoint needs them. A per-call price model is not needed while
decision models are billed per token.
