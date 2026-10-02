# Engineering Rules

This is the index of repository-wide engineering rules. Each rule's full text
lives in exactly one topical file under a stable named anchor such as
`#mode-parity`; cite rules by name and link that anchor. Read only the topical
files relevant to the change.

Rules marked **(enforced)** are checked mechanically by `cargo test` / CI — see
`limerick/crates/limerick-core/tests/architecture_fitness.rs`. The rest are
convention.

## Rules by topic

- **General (this file):** [Module ownership](#module-ownership), [Mode parity](#mode-parity), [Tests with behavior changes](#tests-with-behavior-changes), [Gameplay proof](#gameplay-proof), [No unexplained `#[allow]`](#no-unexplained-allow), [Feature flags](#feature-flags), [README maintenance](#readme-maintenance), [Five Whys](#five-whys), [Scaling guardrails](#scaling-guardrails), [Cross-runtime orchestration](#cross-runtime-orchestration), [Production scope](#production-scope).
- **[Persistence and sessions](persistence-and-session-rules.md):** [Runtime paths](persistence-and-session-rules.md#runtime-paths), [Canonical background simulation](persistence-and-session-rules.md#canonical-background-simulation), [Live play signals and replacement contexts](persistence-and-session-rules.md#live-play-signals), [Authoritative gameplay status](persistence-and-session-rules.md#authoritative-gameplay-status), [Save/session identity](persistence-and-session-rules.md#save-session-identity), [Durable player-earned knowledge](persistence-and-session-rules.md#durable-player-knowledge).
- **[Tests and tooling](test-tooling-rules.md):** [Truthful test automation](test-tooling-rules.md#truthful-test-automation), [External API payload caps](test-tooling-rules.md#external-api-payload-caps), [Existing tooling](test-tooling-rules.md#existing-tooling), [Truthful verification reporting](test-tooling-rules.md#truthful-verification-reporting), [Shared-target artifacts](test-tooling-rules.md#shared-target-artifacts), [Crash-safe managed tests](test-tooling-rules.md#crash-safe-managed-tests), [Merged end-to-end contracts](test-tooling-rules.md#merged-end-to-end-contracts), [Player-facing interaction models](test-tooling-rules.md#player-facing-interaction-models).
- **[Inference](inference-rules.md):** [Dialogue prompt grounding](inference-rules.md#dialogue-prompt-grounding), [Canonical semantic model apply](inference-rules.md#canonical-semantic-model-apply), [Local-inference promotion](inference-rules.md#local-inference-promotion), [Model termination](inference-rules.md#model-termination), [Serving-topology qualification](inference-rules.md#serving-topology-qualification), [Judge evidence](inference-rules.md#judge-evidence), [Independent judge families](inference-rules.md#independent-judge-families).
- **[Generated art](generated-art-rules.md):** [Artifact content validation](generated-art-rules.md#artifact-content-validation), [Character-art identity](generated-art-rules.md#character-art-identity), [Billable artifact persistence](generated-art-rules.md#billable-artifact-persistence), [Generated-art visual contracts](generated-art-rules.md#generated-art-visual-contracts), [Character markers](generated-art-rules.md#character-markers), [Generated-file transactions](generated-art-rules.md#generated-file-transactions), [Production rendering support assets](generated-art-rules.md#production-rendering-support-assets).
- **[Legacy clients](legacy-client-rules.md):** [Portable Node tools](legacy-client-rules.md#portable-node-tools), [HTTP harness continuity](legacy-client-rules.md#http-harness-continuity).

The specialized documents preserve the same invariants for the systems they
cover. Their scope notes identify maintenance-only policies without weakening
requirements that apply to every runtime or client.

## General engineering rules

<a id="module-ownership"></a>
<a id="rule-1"></a>

## Module ownership (enforced)

Shared logic belongs in a leaf crate; `limerick-core` composes them. Never
duplicate leaf-crate logic in `limerick/crates/limerick-engine/src/`. Orphaned
source files (on disk but not declared as `mod`) are rejected. Crate map:
[docs/agent/architecture.md](architecture.md).

<a id="mode-parity"></a>
<a id="rule-2"></a>

## Mode parity (partially enforced)

Scope note: this parity requirement applies to shared behavior across
maintained runtimes; it does not impose mobile legacy UI parity. Shared
engine/API semantics and every applicable entry point remain covered.

Tauri, headless CLI, and web server must share behavior. The fitness test
forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` /
`wry` / `tao`. Wiring parity (every IPC handler called from every entry point)
is still convention.

<a id="tests-with-behavior-changes"></a>
<a id="rule-3"></a>

## Tests with behavior changes

Add/adjust tests for every behavior change.

<a id="gameplay-proof"></a>
<a id="rule-4"></a>

## Gameplay proof

For gameplay features, run `/limerick-engine prove <feature>` — unit tests alone
are not sufficient.

<a id="no-unexplained-allow"></a>
<a id="rule-5"></a>

## No unexplained `#[allow]`

Only with explicit justification.

<a id="feature-flags"></a>
<a id="rule-6"></a>

## Feature flags for new engine/gameplay features

Gate with `config.flags.is_enabled("feature-name")`, default-on, document in
the PR.

<a id="readme-maintenance"></a>
<a id="rule-7"></a>

## Keep README.md up to date

feature list, repository structure, credits. Run `just notices` when
dependencies change.

<a id="five-whys"></a>
<a id="rule-8"></a>

## Five Whys before patching

Diagnose bugs, regressions, and unexpected behavior with the `/five-whys`
skill (or the method) to reach root cause first.

<a id="scaling-guardrails"></a>
<a id="rule-11"></a>

## Scaling guardrails

Any PR touching `AppState`, session persistence, real-time push, inference
calls, identity lookups, mod loading, or request-ID tracing must be reviewed
against the seam checklist in [docs/agent/scaling-rules.md](scaling-rules.md).

<a id="cross-runtime-orchestration"></a>
<a id="rule-12"></a>

## Cross-runtime orchestration belongs in `limerick-core`

Game-loop, IPC, and session handlers shared by the server, Tauri, and CLI
entry points — including their constants, payload structs, and helpers — are
defined once in a backend-agnostic crate, parameterized via traits (e.g.
`EventEmitter`); entry-point crates are thin wiring. Never copy an orchestration
body, constant, or payload struct into a second entry-point crate — the
divergence is invisible at review time and silently produces security drift
(#687, #696).

<a id="production-scope"></a>
<a id="rule-19"></a>

## Production scope means end-to-end, not minimum arguable plumbing

Rundale is a production-quality game, engine, and toolset. When an issue asks
for a production pipeline/workflow/tool, implement the full stated workflow
and proof path from source-of-truth inputs through generated/reviewed outputs
and runtime integration. Do not downgrade scope to cached artifacts, manual
intermediates, placeholders, or "assembly only" unless the issue explicitly
says that is acceptable. If provider access, credentials, or product
constraints block the true end-to-end workflow, mark the work incomplete/blocked
instead of calling it done.

## Legacy rule numbers

Older commits, code comments, ADRs, and issues cite rules by number. Each
old number still resolves through a `#rule-N` alias beside the named anchor.
Number 13 was removed and has no successor.

| Old number | Rule                                                                                                |
| ---------: | --------------------------------------------------------------------------------------------------- |
|          1 | [Module ownership](#module-ownership)                                                               |
|          2 | [Mode parity](#mode-parity)                                                                         |
|          3 | [Tests with behavior changes](#tests-with-behavior-changes)                                         |
|          4 | [Gameplay proof](#gameplay-proof)                                                                   |
|          5 | [No unexplained `#[allow]`](#no-unexplained-allow)                                                  |
|          6 | [Feature flags](#feature-flags)                                                                     |
|          7 | [README maintenance](#readme-maintenance)                                                           |
|          8 | [Five Whys](#five-whys)                                                                             |
|          9 | [Runtime paths](persistence-and-session-rules.md#runtime-paths)                                     |
|         10 | [Truthful test automation](test-tooling-rules.md#truthful-test-automation)                          |
|         11 | [Scaling guardrails](#scaling-guardrails)                                                           |
|         12 | [Cross-runtime orchestration](#cross-runtime-orchestration)                                         |
|         14 | [Artifact content validation](generated-art-rules.md#artifact-content-validation)                   |
|         15 | [Dialogue prompt grounding](inference-rules.md#dialogue-prompt-grounding)                           |
|         16 | [External API payload caps](test-tooling-rules.md#external-api-payload-caps)                        |
|         17 | [Existing tooling](test-tooling-rules.md#existing-tooling)                                          |
|         18 | [Truthful verification reporting](test-tooling-rules.md#truthful-verification-reporting)            |
|         19 | [Production scope](#production-scope)                                                               |
|         20 | [Character-art identity](generated-art-rules.md#character-art-identity)                             |
|         21 | [Billable artifact persistence](generated-art-rules.md#billable-artifact-persistence)               |
|         22 | [Generated-art visual contracts](generated-art-rules.md#generated-art-visual-contracts)             |
|         23 | [Character markers](generated-art-rules.md#character-markers)                                       |
|         24 | [Generated-file transactions](generated-art-rules.md#generated-file-transactions)                   |
|         25 | [Portable Node tools](legacy-client-rules.md#portable-node-tools)                                   |
|         26 | [Shared-target artifacts](test-tooling-rules.md#shared-target-artifacts)                            |
|         27 | [Crash-safe managed tests](test-tooling-rules.md#crash-safe-managed-tests)                          |
|         28 | [Merged end-to-end contracts](test-tooling-rules.md#merged-end-to-end-contracts)                    |
|         29 | [Player-facing interaction models](test-tooling-rules.md#player-facing-interaction-models)          |
|         30 | [Canonical background simulation](persistence-and-session-rules.md#canonical-background-simulation) |
|         31 | [Live play signals and replacement contexts](persistence-and-session-rules.md#live-play-signals)    |
|         32 | [Authoritative gameplay status](persistence-and-session-rules.md#authoritative-gameplay-status)     |
|         33 | [Canonical semantic model apply](inference-rules.md#canonical-semantic-model-apply)                 |
|         34 | [Save/session identity](persistence-and-session-rules.md#save-session-identity)                     |
|         35 | [HTTP harness continuity](legacy-client-rules.md#http-harness-continuity)                           |
|         36 | [Local-inference promotion](inference-rules.md#local-inference-promotion)                           |
|         37 | [Model termination](inference-rules.md#model-termination)                                           |
|         38 | [Serving-topology qualification](inference-rules.md#serving-topology-qualification)                 |
|         39 | [Judge evidence](inference-rules.md#judge-evidence)                                                 |
|         40 | [Independent judge families](inference-rules.md#independent-judge-families)                         |
|         41 | [Production rendering support assets](generated-art-rules.md#production-rendering-support-assets)   |
|         42 | [Durable player-earned knowledge](persistence-and-session-rules.md#durable-player-knowledge)        |
