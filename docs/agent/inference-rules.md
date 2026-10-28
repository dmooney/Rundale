# Inference Rules

These rules govern prompt grounding, model output application, provider
termination, and qualification evidence. See [engineering-rules.md](engineering-rules.md)
for the complete rule index.

[Local-inference promotion](#local-inference-promotion) is a specialized maintenance policy for the existing local-inference
promotion and setup systems. It is not a requirement for mobile feature work.
When a local preset is promoted, the complete rule applies; this scope note
does not waive the cross-cutting inference, state, or evidence rules.

[Dialogue prompt grounding](#dialogue-prompt-grounding) describes the desktop
in-process prompt assembly. The mobile Endpoint path (owner decision,
2026-09-28) builds its prompt in the Limerick Endpoint definition instead, and
the app streams reply text into the transcript as it arrives, marked
provisional.

[Committing model dialogue as written](#canonical-semantic-model-apply) applies
to every path: desktop, headless and mobile (owner decision, 2026-10-23, issue
2295). The engine commits the final reply after the structural checks only (a
complete, schema-valid, non-empty result), and no code inspects, rewrites or
replaces it. Grounding quality is fixed through the prompt, the facts it
supplies, the Endpoint definition and the model, never engine rewrites. Stop or
a failed call leaves the partial text visibly uncommitted, with no state
effects. [Model termination](#model-termination) still applies: only a
successful terminal result is committed.

<a id="dialogue-prompt-grounding"></a>
<a id="rule-15"></a>

## Dialogue prompts must ground the model in the actual world

Every NPC system prompt includes a `PEOPLE YOU KNOW` and a `PLACES IN THIS
LIMERICK` list with instructions to decline to confirm anyone or anywhere not on
them. Enforcement: `build_enhanced_system_prompt_with_config` in
`limerick-npc/src/ticks/prompt.rs` (`location_names` must be `Some(...)` in
production); test: `limerick-core/tests/dialogue_prompt_anchor.rs`; flag
`npc-dialogue-grounding`, default-on (#1394).

<a id="canonical-semantic-model-apply"></a>
<a id="rule-33"></a>

## Commit model dialogue as written; check structure only

The model does its best, for better or worse. The canonical apply seam
(`apply_npc_dialogue_turn_with_disposition` in
`limerick-core/src/dialogue_apply.rs`) checks only the structural response
contract, `limerick_npc::dialogue_candidate_is_complete`: the reply parsed as
the complete authored JSON object (on desktop this includes the Tier 1 metadata
block), its dialogue is non-empty, and it is within
`limerick_npc::MAX_DIALOGUE_CHARS` (4,096 characters, a runaway bound that
rejects whole and never truncates). A reply that fails the contract fails the
turn with no memory, event, UI, or state effects. A reply that passes is
committed exactly as written: there is no grounding, register, obligation,
anachronism, repetition, verbosity, or length rewrite, no replacement line, and
no flag that turns one back on. The host drains provider tokens internally and
publishes only the committed reply.

Model text still never changes game state by itself. Authoritative state moves
only through typed seams that read authored data and the player's own words:
the speaker becomes known from the delivered line plus the authored roster, a
task is assigned only when the delivered dialogue assigns it and authored
locations agree, and remembered player objects come from the player's input.
Test: `limerick-core/tests/dialogue_commits_unchanged.rs` (a reply naming a
person who does not exist commits unchanged on the desktop and phone paths).
Do not add a post-generation guard. Fix a bad reply with the prompt
([NPC dialogue prompt requirements](../design/npc-dialogue-prompt.md)), better
facts, or a better model.

<a id="local-inference-promotion"></a>
<a id="rule-36"></a>

## Promote local-inference presets only from passing production evidence

A recommended model/backend/sampling/hardware profile requires a
content-addressed promotion receipt from the frozen production-prompt holdout
that passes dialogue and multiturn quality, hard-failure, parser-soak,
p95 latency/throughput, and memory-headroom gates.
Development-split scores, preliminary leaderboard rows, hand-entered
summaries, and average quality alone must never change a shipped
recommendation.

<a id="model-termination"></a>
<a id="rule-37"></a>

## Treat model termination as part of the response contract

Streaming provider clients must reject every non-success finish reason (for
example `length` / `MAX_TOKENS`) instead of parsing or displaying the partial
body. Mandatory-reasoning profiles must budget and measure reasoning-token
headroom separately from the player-visible response; a low effort label is
not a token ceiling.

<a id="serving-topology-qualification"></a>
<a id="rule-38"></a>

## Separate qualification policy by serving topology

Local promotion latency gates must not be reused for routed cloud models. A
promotion receipt is valid only for the exact provider and API route measured;
matching model identity through another gateway is not equivalent evidence.
Cloud screening hard-gates structural validity, evidence
completeness, and request reliability; latency and throughput rank qualified
profiles unless a separately measured product SLO explicitly makes one a
release gate.

<a id="judge-evidence"></a>
<a id="rule-39"></a>

## Give judges the complete evidence needed by their rubric

Any quality axis that depends on system-prompt facts—character identity, mood
contract, known people, known places, or period rules—must receive those exact
production facts in the judge bundle. Persist every paid judge attempt before
validation, journal retries immutably, and trip a batch-wide circuit breaker on
authentication, billing, quota, or rate-limit failures.

<a id="independent-judge-families"></a>
<a id="rule-40"></a>

## Cloud dialogue qualification requires independent judge families

Never let a model family vote on itself. A promotable cloud profile needs at
least two eligible judge families, uses the policy-defined consensus statistic,
and routes split pass/fail votes or excessive score spread to an explicit
adjudication state. Same-family judgments may remain visible as diagnostic
evidence but cannot count toward promotion.
