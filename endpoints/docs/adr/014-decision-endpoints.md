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
- answers `{ choice, confidence, probabilities }` or `{ noul }` in TypeSafe's
  documentation. OpenRouter's schema requires only `type` and `choice` on a
  choice answer and makes `confidence` and `probabilities` optional, although
  all three candidates returned both in live calls on 2026-10-10;
- `usage` in input and output tokens (OpenRouter also reports `usage.cost`).

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
  1 to 64 letters, digits, `_` or `-`. `{id}` substitution inserts only the
  id; input fields reach a question only through `include`. Duplicate ids in an array, choices outside 2 to 255 options, and
  an input that expands to no questions or more than 64 are invalid
  input.
- **Instructions.** The definition's `instructions` travel as `background`
  beside every question (`{ question, background }`), so each question stays
  short. Question instructions and criteria may be text or JSON structure, as
  TypeSafe's "advanced structure" allows: for example an option described by
  `{ what, not_for, examples }`.
- **Options describe themselves.** `include` on a `criteriaFrom` source copies
  the named fields of the option's own element into its criterion, and on a
  `forEach` noul gives them as `about`. The model then reads what each option
  is, instead of an id it must look up in state, which TypeSafe documents as a
  weakness (indirection). An included field may hold player text, such as a
  span the engine cut from the line; the model only selects among options, and
  the runtime still checks every answer against them.
- **Output.** A choice gives `{ choice, confidence, probabilities }`, a noul
  gives its probability, and a `forEach` noul gives `{ <id>: p }`, each under
  its question name and validated by the output schema. An offered option
  the vendor leaves out of `probabilities` is 0. An answer that is missing,
  out of range, not an offered option, or a choice without `confidence` and
  `probabilities` fails the attempt with `OUTPUT_VALIDATION_FAILED`, keeping
  its usage for cost, so callers never see an option they did not offer.
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
