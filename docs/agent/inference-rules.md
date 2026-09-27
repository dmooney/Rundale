# Inference Rules

These rules govern prompt grounding, model output application, provider
termination, and qualification evidence. See [engineering-rules.md](engineering-rules.md)
for the complete numbered map.

Rule 36 is a specialized maintenance policy for the existing local-inference
promotion and setup systems. It is not a requirement for mobile feature work.
When a local preset is promoted, the complete rule applies; this scope note
does not waive the cross-cutting inference, state, or evidence rules.

<a id="rule-15"></a>

## Rule 15 — **Dialogue prompts must ground the model in the actual world:**

Every NPC system prompt includes a `PEOPLE YOU KNOW` and a `PLACES IN THIS
LIMERICK` list with instructions to decline to confirm anyone or anywhere not on
them. Enforcement: `build_enhanced_system_prompt_with_config` in
`limerick-npc/src/ticks/prompt.rs` (`location_names` must be `Some(...)` in
production); test: `limerick-core/tests/dialogue_prompt_anchor.rs`; flag
`npc-dialogue-grounding`, default-on (#1394).

<a id="rule-33"></a>

## Rule 33 — **Validate semantic model output at the canonical apply seam:**

Prompt contracts and deterministic guards must share canonical constants. Treat
player-visible model dialogue as an effect: quarantine candidate tokens until
the one canonical apply validator accepts or replaces them, and never
pre-apply semantic guards or publish raw candidate text from a caller. Rejected
text and metadata have zero memory, event, UI, or state effects. Schema-valid
model fields and factual dialogue claims must not change or contradict
identity, relationships, occupations, workplaces, locations, calendar facts,
hints, recommendations, or other gameplay/UI metadata until cross-checked
against authored world data and the final delivered dialogue. Explicit
current-turn request facets must use one typed, ordered prompt/apply contract;
if the final transformed dialogue omits a recognized facet, replace the whole
response before effects. Multi-turn pronouns may carry an unresolved person/place
only through a bounded typed conversation context and only while the referent
is unambiguous (#1776, #1779, #1786, #1788–#1790, #1832, #1834, #1839–#1841).

<a id="rule-36"></a>

## Rule 36 — **Promote local-inference presets only from passing production evidence:**

A recommended model/backend/sampling/hardware profile requires a
content-addressed promotion receipt from the frozen production-prompt holdout
that passes dialogue and multiturn quality, hard-failure, parser-soak,
guard-intervention, p95 latency/throughput, and memory-headroom gates.
Development-split scores, preliminary leaderboard rows, hand-entered
summaries, and average quality alone must never change a shipped
recommendation.

<a id="rule-37"></a>

## Rule 37 — **Treat model termination as part of the response contract:**

Streaming provider clients must reject every non-success finish reason (for
example `length` / `MAX_TOKENS`) instead of parsing or displaying the partial
body. Mandatory-reasoning profiles must budget and measure reasoning-token
headroom separately from the player-visible response; a low effort label is
not a token ceiling.

<a id="rule-38"></a>

## Rule 38 — **Separate qualification policy by serving topology:**

Local promotion latency gates must not be reused for routed cloud models. A
promotion receipt is valid only for the exact provider and API route measured;
matching model identity through another gateway is not equivalent evidence.
Cloud screening hard-gates structural validity, guard intervention, evidence
completeness, and request reliability; latency and throughput rank qualified
profiles unless a separately measured product SLO explicitly makes one a
release gate.

<a id="rule-39"></a>

## Rule 39 — **Give judges the complete evidence needed by their rubric:**

Any quality axis that depends on system-prompt facts—character identity, mood
contract, known people, known places, or period rules—must receive those exact
production facts in the judge bundle. Persist every paid judge attempt before
validation, journal retries immutably, and trip a batch-wide circuit breaker on
authentication, billing, quota, or rate-limit failures.

<a id="rule-40"></a>

## Rule 40 — **Cloud dialogue qualification requires independent judge families:**

Never let a model family vote on itself. A promotable cloud profile needs at
least two eligible judge families, uses the policy-defined consensus statistic,
and routes split pass/fail votes or excessive score spread to an explicit
adjudication state. Same-family judgments may remain visible as diagnostic
evidence but cannot count toward promotion.
