# Engineering Rules

This is the canonical map for the numbered non-negotiable rules formerly
listed in the repository root instructions. The stable rule number is part of
each heading and each heading has an explicit anchor, so references remain
unambiguous when the documents are rearranged. The complete rule text lives in
exactly one topical file.

Rules marked **(enforced)** are checked mechanically by `cargo test` / CI — see
`limerick/crates/limerick-core/tests/architecture_fitness.rs`. The rest are still
convention.

## Canonical rule map

| Rule | Canonical text                                                                         |
| ---: | -------------------------------------------------------------------------------------- |
|    1 | [Module ownership](#rule-1)                                                            |
|    2 | [Mode parity](#rule-2)                                                                 |
|    3 | [Tests with behavior changes](#rule-3)                                                 |
|    4 | [Gameplay proof](#rule-4)                                                              |
|    5 | [No unexplained `#[allow]`](#rule-5)                                                   |
|    6 | [Feature flags](#rule-6)                                                               |
|    7 | [README maintenance](#rule-7)                                                          |
|    8 | [Five Whys](#rule-8)                                                                   |
|    9 | [Runtime paths](persistence-and-session-rules.md#rule-9)                               |
|   10 | [Truthful test automation](test-tooling-rules.md#rule-10)                              |
|   11 | [Scaling guardrails](#rule-11)                                                         |
|   12 | [Cross-runtime orchestration](#rule-12)                                                |
|   13 | [[REMOVED]](#rule-13)                                                                  |
|   14 | [Artifact content validation](generated-art-rules.md#rule-14)                          |
|   15 | [Dialogue prompt grounding](inference-rules.md#rule-15)                                |
|   16 | [External API payload caps](test-tooling-rules.md#rule-16)                             |
|   17 | [Existing tooling](test-tooling-rules.md#rule-17)                                      |
|   18 | [Truthful verification reporting](test-tooling-rules.md#rule-18)                       |
|   19 | [Production scope](#rule-19)                                                           |
|   20 | [Character-art identity](generated-art-rules.md#rule-20)                               |
|   21 | [Billable artifact persistence](generated-art-rules.md#rule-21)                        |
|   22 | [Generated-art visual contracts](generated-art-rules.md#rule-22)                       |
|   23 | [Character markers](generated-art-rules.md#rule-23)                                    |
|   24 | [Generated-file transactions](generated-art-rules.md#rule-24)                          |
|   25 | [Portable Node tools](legacy-client-rules.md#rule-25)                                  |
|   26 | [Shared-target artifacts](test-tooling-rules.md#rule-26)                               |
|   27 | [Crash-safe managed tests](test-tooling-rules.md#rule-27)                              |
|   28 | [Merged end-to-end contracts](test-tooling-rules.md#rule-28)                           |
|   29 | [Player-facing interaction models](test-tooling-rules.md#rule-29)                      |
|   30 | [Canonical background simulation](persistence-and-session-rules.md#rule-30)            |
|   31 | [Live play signals and replacement contexts](persistence-and-session-rules.md#rule-31) |
|   32 | [Authoritative gameplay status](persistence-and-session-rules.md#rule-32)              |
|   33 | [Canonical semantic model apply](inference-rules.md#rule-33)                           |
|   34 | [Save/session identity](persistence-and-session-rules.md#rule-34)                      |
|   35 | [HTTP harness continuity](legacy-client-rules.md#rule-35)                              |
|   36 | [Local-inference promotion](inference-rules.md#rule-36)                                |
|   37 | [Model termination](inference-rules.md#rule-37)                                        |
|   38 | [Serving-topology qualification](inference-rules.md#rule-38)                           |
|   39 | [Judge evidence](inference-rules.md#rule-39)                                           |
|   40 | [Independent judge families](inference-rules.md#rule-40)                               |
|   41 | [Production rendering support assets](generated-art-rules.md#rule-41)                  |
|   42 | [Durable player-earned knowledge](persistence-and-session-rules.md#rule-42)            |

The specialized documents preserve the same invariants for the systems they
cover. Their scope notes identify maintenance-only policies without weakening
requirements that apply to every runtime or client.

## General engineering rules

<a id="rule-1"></a>

## Rule 1 — **Module ownership (enforced):**

Shared logic belongs in a leaf crate; `limerick-core` composes them. Never
duplicate leaf-crate logic in `limerick/crates/limerick-engine/src/`. Orphaned
source files (on disk but not declared as `mod`) are rejected. Crate map:
[docs/agent/architecture.md](architecture.md).

<a id="rule-2"></a>

## Rule 2 — **Mode parity (partially enforced):**

Scope note: this parity requirement applies to shared behavior across
maintained runtimes; it does not impose mobile legacy UI parity. Shared
engine/API semantics and every applicable entry point remain covered.

Tauri, headless CLI, and web server must share behavior. The fitness test
forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` /
`wry` / `tao`. Wiring parity (every IPC handler called from every entry point)
is still convention.

<a id="rule-3"></a>

## Rule 3 — **Tests with behavior changes:**

Add/adjust tests for every behavior change.

<a id="rule-4"></a>

## Rule 4 — **Gameplay proof:**

For gameplay features, run `/limerick-engine prove <feature>` — unit tests alone
are not sufficient.

<a id="rule-5"></a>

## Rule 5 — **No unexplained `#[allow]`:**

Only with explicit justification.

<a id="rule-6"></a>

## Rule 6 — **Feature flags for new engine/gameplay features:**

Gate with `config.flags.is_enabled("feature-name")`, default-on, document in
the PR.

<a id="rule-7"></a>

## Rule 7 — **Keep README.md up to date:**

feature list, repository structure, credits. Run `just notices` when
dependencies change.

<a id="rule-8"></a>

## Rule 8 — **Five Whys before patching:**

Diagnose bugs, regressions, and unexpected behavior with the `/five-whys`
skill (or the method) to reach root cause first.

<a id="rule-11"></a>

## Rule 11 — **Scaling guardrails:**

Any PR touching `AppState`, session persistence, real-time push, inference
calls, identity lookups, mod loading, or request-ID tracing must be reviewed
against the seam checklist in [docs/agent/scaling-rules.md](scaling-rules.md).

<a id="rule-12"></a>

## Rule 12 — **Cross-runtime orchestration belongs in `limerick-core`:**

Game-loop, IPC, and session handlers shared by the server, Tauri, and CLI
entry points — including their constants, payload structs, and helpers — are
defined once in a backend-agnostic crate, parameterized via traits (e.g.
`EventEmitter`); entry-point crates are thin wiring. Never copy an orchestration
body, constant, or payload struct into a second entry-point crate — the
divergence is invisible at review time and silently produces security drift
(#687, #696).

<a id="rule-13"></a>

## Rule 13 — **[REMOVED]**

This number is intentionally retained as a traceability marker for the rule
that was removed from the original numbered block.

<a id="rule-19"></a>

## Rule 19 — **Production scope means end-to-end, not minimum arguable plumbing:**

Rundale is a production-quality game, engine, and toolset. When an issue asks
for a production pipeline/workflow/tool, implement the full stated workflow
and proof path from source-of-truth inputs through generated/reviewed outputs
and runtime integration. Do not downgrade scope to cached artifacts, manual
intermediates, placeholders, or "assembly only" unless the issue explicitly
says that is acceptable. If provider access, credentials, or product
constraints block the true end-to-end workflow, mark the work incomplete/blocked
instead of calling it done.
