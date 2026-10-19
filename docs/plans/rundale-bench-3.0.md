# Rundale Benchmark 3.0 — repository handoff and interview archive

> Status: Planned · blocked pending verification of the full new-site launch
> (current results, archived results, and evidence links). Benchmark repository
> PRs 1 and 2 have merged; that foundation and provisional judging do not satisfy
> the launch evidence requirement. Updated: 2026-10-18 · [Docs Index](../index.md)

This file preserves the October 11, 2026 owner interview and subsequent scope
history. It is not the canonical implementation specification. Benchmark 3.0
belongs to the public [dmooney/rundale-bench repository](https://github.com/dmooney/rundale-bench).
Read its current instructions, specification, suite, and qualification evidence
for execution and publication readiness. The website and all benchmark behavior
belong there; this repository retains only historical tooling and routing guidance.

Keep the legacy site workflow enabled until the new site's current results,
archives, and evidence links are verified. Record that acceptance in the
follow-up PR that retires the workflow; this repository handoff PR
([dmooney/Rundale#2259](https://github.com/dmooney/Rundale/pull/2259)) does not
retire it. A successful foundation-only deployment does not satisfy the full
launch criteria.

The original interview and superseded broader design are retained below as
provenance, not as current requirements. In particular, only explicit requests
to inspect or reproduce pre-3.0 historical runs should use the legacy benchmark
instructions in this repository.

The removed pre-3.0 skill instructions are preserved at [their historical
revision](https://github.com/dmooney/Rundale/blob/3bf68689ea7f6e6717c8aec2f3a6869231742cf5/.agents/skills/rundale-bench/SKILL.md)
for reference when reproducing an explicitly requested legacy evaluation.

## Current scope — October 17 update

The owner narrowed the initial benchmark to dialogue inference. The required
scored evals are **dialogue**, **multiturn conversation**, and **Gaeilge**,
including fully Irish responses and Irish/English code-switching.

Each eval contributes one third of the overall quality score after normalization
to a shared scale. Normalization and within-eval scoring remain to be defined.
Cost and performance remain separate measurements, not overall-score components.

Intent inference now uses Jev in the game. Intent, reaction, Tier 2/3 simulation,
and full-session evals are deferred from the initial benchmark. No LLM player or
20-turn session is required for this initial scope. The $5 total budget still
covers candidate inference and judging. Irish judging remains mandatory.

This update takes precedence over the broader original design below. Its
full-session/world/player requirements and acceptance criteria apply only if
that scope is reactivated. Equal weighting is now confirmed for the three active
evals; earlier discussion of unsettled weights is superseded.

## Intended experience

For the initial 3.0 scope, the agent resolves a model's native provider API and
highest supported reasoning setting, then runs dialogue, multiturn conversation,
and objective Gaeilge checks within the $5 run budget. See the new repository's
README and benchmark skill for implementation details and current run gates.
This supersedes the later paragraphs in the interview archive that describe a
comprehensive all-slice suite.

The 3.0 implementation is hosted in the separate `rundale-bench` repository,
which owns the runner, test cases, judging, results, archives, and website.
Rundale/Limerick is a pinned, versioned dependency. The pull request and current
repository documentation are the source of implementation status; this archive
does not certify that CI passed, the PR merged, or the website is live.

## Superseded broader scope and evals

Cloud models only initially, using native provider APIs only. No OpenRouter
fallback. Each model and reasoning setting has a separate leaderboard entry;
providers do not create separate entries. Record the actual provider and resolved
model identity in the evidence.

Default to the highest supported reasoning setting. Additional settings are run
only when requested. A model without a reasoning control uses its supported
configuration; the runner must record that fact rather than invent a setting.

The following comprehensive suite was proposed in the original interview. It
is superseded by the three-eval October 17 scope above and is not the current
Benchmark 3.0 requirement:

| Eval                   | Intended coverage                                                                     | Scoring still to settle                                  |
| ---------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| Dialogue               | Character, historical authenticity, language, responsiveness, writing quality         | Anchored subjective rubric                               |
| Multiturn conversation | Memory, persona consistency, repeated introductions, wrong names, premature farewells | Objective checks where possible plus rubric              |
| Reaction               | Appropriate NPC reactions that stay in character                                      | Objective checks where possible plus rubric              |
| Intent                 | Interpretation of player actions                                                      | Expected answers and deterministic checks where possible |
| Tier 2 simulation      | Structured-output validity and plausible simulation decisions                         | Schema checks plus decision correctness/plausibility     |
| Tier 3 simulation      | Structured-output validity and plausible simulation decisions                         | Schema checks plus decision correctness/plausibility     |
| Gaeilge                | Fully Irish responses and natural Irish/English code-switching                        | Validated Irish judging; method unresolved               |
| Full session           | Quality of a complete game session                                                    | Multiple anchored judgment dimensions; rubric unresolved |
| Performance            | Time to first token, completion latency, generation speed, errors                     | Measured metrics                                         |

Also publish projected gameplay cost and actual run cost. The projection's
workload assumptions must be defined before implementation is accepted.

Existing benchmark assets are candidates for reuse, not authority over this
specification. Audit cases and scoring before bringing them into 3.0.

## Full-session eval

Run one session of 20 player turns initially. The candidate powers all game
inference roles. A separate, fixed LLM player follows a basic scenario skeleton
but can adapt to improvised NPC responses and take different directions.

Use a purpose-built eval world, not the production world. There is no fixed win
condition. Judge the session through a rubric covering several dimensions.
World design and scenario coverage remain open. A small village with three NPCs
and a disputed problem was suggested, but has not been accepted as a requirement.

The player model and configuration are pinned and shared across candidates.
Its identity is still to be chosen. Publish the complete session transcript and
the evidence needed to distinguish generated claims from committed game facts.

## Judging and overall score

Publish individual eval scores and an overall quality score. Cost and performance
are reported separately under the provisional scoring proposal.

Equal weighting is confirmed for the three active quality evals. The score
scale, normalization,
critical-failure treatment, and treatment of missing judgments must be decided
before ranked results are published.

Use right/wrong checks where the task admits them. Subjective dialogue and
session quality need explicit rubrics. Irish judging is mandatory for a complete
overall score, including both Irish and code-switching coverage; reliable Irish
judging cannot be deferred out of the required suite.

Jev is a candidate judge, not a selected or validated judge. Establish whether
it is suitable for these tasks, especially Irish and full-session judgments.
Structured verdicts alone do not establish judgment quality. The owner is
willing to label a small calibration set later, but not now. Judge selection
and validation remain open until that work is done.

Pin subjective judge models and configurations. Changing them requires a new
benchmark version. Do not publish a generated strengths/weaknesses summary;
publish scores and their underlying evidence.

## Budget and completion

Target a maximum of $5 per requested model/configuration run, including candidate
inference, the LLM player, and judging. The owner wants all costs included if
possible; feasibility has not been demonstrated. Do not silently exceed $5.

If the budget prevents completion, publish whatever completed, mark the overall
score **DNF**, and state the reason. Do not quietly shrink the suite or sample
counts to manufacture a comparable overall score. Sample counts, execution order,
cost accounting, and budget reservation for judging remain design decisions.

Publish completed results automatically, including poor scores and incomplete
runs. Record which evals completed, which were partial or unattempted, and why.
Do not present partial coverage as a completed eval.

## Failures and investigation

Rejections and malformed output require evidence about the cause: a refusal may
have several causes, and invalid output may originate in the model or tooling.
An isolated event does not automatically terminate the run. If it happens more
than once, the run is DNF and needs investigation.

Publish available results and file an issue with the requests, responses, error
details, relevant configuration, and reproduction evidence, with credentials
excluded. Another agent handles investigation; the benchmark agent does not
automatically repair tooling and rerun the campaign.

The exact repeated-event rule—combined or per-category counts, and whether
transport failures count—remains to be specified. Do not attribute an unknown
cause to the model merely to choose a leaderboard status.

## Leaderboard, archives, and publication

Start 3.0 with an empty leaderboard. Preserve pre-3.0 results as historical
archives, outside the new ranking. Each benchmark version has its own leaderboard,
and old versions remain viewable.

Normally the latest run replaces the displayed result for its model/reasoning
entry; archive the previous run. This reflects model drift rather than choosing
the best historical result.

For a later DNF run:

- A model-caused failure replaces the displayed score with DNF.
- Infrastructure failure or budget exhaustion retains the previous score with
  a dated notice about the new incomplete run. Publish and archive that run too.
- Handling an unresolved cause needs a rule before implementation; do not imply
  that a retained score came from the latest attempt.

Every run publishes test prompts, model responses, full-session transcript,
individual judgments/check results, eval scores, completion status, and the
reason for DNF when applicable. Record the benchmark version, engine version,
player/judge configurations, candidate settings, dates, sample coverage, costs,
and timing measurements so the result can be inspected and reproduced as far
as provider behavior permits.

## Versioning

Freeze test cases and prompts within a suite version. Changes create a new suite
version. Pin the Limerick engine, player, and judges. Periodically accepting a new
engine version bumps the benchmark version, for example 3.0 to 3.1.

The exact version policy for rubric, aggregation, world, dependency, and runner
changes should be documented. Proposed rule: any change that affects score
comparability creates a new benchmark version; purely operational fixes retain
the version with a recorded runner revision.

## Open decisions before implementation acceptance

1. Anchored rubrics, objective answer keys, sample counts, and scenario skeleton.
2. Irish/code-switch judging method and evidence that it is reliable.
3. Judge selection, Jev validation, and the deferred owner calibration exercise.
4. Fixed player model/configuration and purpose-built world design.
5. Overall score scale, normalization, and critical-failure rules (equal weights confirmed).
6. $5 feasibility, budget enforcement, execution order, and cost projection.
7. Precise repeated-error and unknown-cause leaderboard rules.
8. Native-provider resolution and unsupported-task behavior.
9. Repository/site ownership, deployment, and migration/archive layout.

## Proposed acceptance criteria

These criteria belong to the superseded broader interview scope and apply only
if that scope is explicitly reactivated. They do not describe the current
three-eval benchmark or settle the open design choices above.

- A single model request completes native API setup, comprehensive execution,
  scoring, publication, and leaderboard update without routine owner input.
- The candidate drives every game inference role in one adaptive 20-turn session.
- All required evals, including validated Irish and code-switch judging, are
  present before a run can receive a complete overall score.
- Budget enforcement includes player and judge costs and publishes an honest
  DNF with completed evidence when the cap prevents completion.
- Repeated output/rejection failures publish DNF and produce an actionable issue.
- Poor results are published automatically; evidence remains inspectable.
- Repeat runs archive earlier results and apply the agreed latest-result and
  failure-cause rules without selecting the best score.
- Engine/player/judge/test changes preserve version-separated comparisons.
- The repository's site serves the updated leaderboard and run evidence;
  successful upload alone is not proof that the published result is visible.

Implementation, repository migration, and publication status are tracked in
the linked repository and PRs; this interview archive is not a status report.
