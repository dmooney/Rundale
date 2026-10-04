# Rundale Agent Guide

Rundale is the game. Limerick is the Rust game engine.

## Start here

1. Read the [product specification](docs/product-specs/README.md) for the task's milestone.
2. Use [docs/agent/README.md](docs/agent/README.md) to select the engineering references
   relevant to the files and behavior you will change.
3. Follow the `AGENTS.md` in each directory you change. Claude Code and Codex load
   them when you work there; other agents should read them before editing.
4. Search [LEARNINGS.md](LEARNINGS.md) for the subsystem you are changing; do not read
   it end to end.

Record a reusable, non-obvious trap in the nearest directory `AGENTS.md`, beside the
code it concerns. Use LEARNINGS.md only for traps that span several areas.
Do not load every reference for every task.

## Current product direction

- Mobile-first text adventure, with iPhone as the primary client.
- SwiftUI player UI: compact status header, transcript, native composer.
- Limerick Rust gameplay runs on device; local saves remain authoritative.
- Production remote inference goes through Limerick Endpoints. No provider credentials
  or shared invocation secrets ship in the app.
- Start with fixtures, then one location/one NPC, then the canonical three-location,
  three-NPC world. Expand only after the applicable milestone gates pass.
- Existing UI and content are reference material, not mobile feature-parity requirements.

The specifications describe the target experience, not evidence that it is implemented.
Do not restore legacy surfaces or content merely because the engine supports them.

## Sources of truth

| Question                                                         | Read                                                                  |
| ---------------------------------------------------------------- | --------------------------------------------------------------------- |
| Product scope, six milestones, acceptance and Definition of Done | [Product specs](docs/product-specs/README.md)                         |
| Target mobile architecture and open integration decisions        | [Mobile architecture](docs/agent/mobile-architecture.md)              |
| Existing engine layout and ownership                             | [Architecture](docs/agent/architecture.md)                            |
| Build, tests, and available verification commands                | [Build/test](docs/agent/build-test.md)                                |
| Detailed invariants, selected by affected subsystem              | [Engineering rules](docs/agent/engineering-rules.md)                  |
| Debugging and known pitfalls                                     | [Gotchas](docs/agent/gotchas.md), [LEARNINGS.md](LEARNINGS.md)        |
| Repository navigation                                            | [Codebase map](docs/agent/codebase-map.md), [docs hub](docs/index.md) |

Versioned product specs govern the reset. Technical recommendations remain proposals
until validated or recorded as decisions. Existing subsystem docs describe reusable
implementation; historical roadmaps do not override current product requirements.
Keep source provenance and requirement changes reviewable in the repository.

## Core engineering invariants

- Shared domain logic belongs in leaf crates; `limerick-core` composes them and owns
  shared application orchestration. Entry-point crates remain thin wiring.
- Keep shared orchestration backend-agnostic. Preserve existing runtime contracts
  when changing shared behavior; mobile scope does not require legacy UI parity.
- Resolve runtime paths from explicit startup configuration, never cwd discovery.
- Authoritative state changes pass through canonical validation and commit boundaries.
  Model output and provisional text are not committed game facts.
- Behavior changes require meaningful tests. Gameplay changes require production-path
  proof, not unit tests alone.
- Diagnose root causes before patching bugs; use Five Whys and existing tools.
- Preserve applicable tests and invariants. Do not weaken a gate to make work pass
  unless the task explicitly changes its underlying requirement and retains equivalent
  or stronger coverage.
- Report only verification actually run, with skips, failures, and unavailable gates
  distinguished from passes.
- Completion requires applicable acceptance criteria, Definition of Done, and Quality
  Gate evidence. Never claim physical-iPhone validation unless it actually occurred.

Read the [detailed rules](docs/agent/engineering-rules.md) before changing their
associated subsystem, especially persistence, inference, concurrency, or UI state.
Use the [mobile test plans](docs/test-plans/README.md) alongside the full milestone gates.

## Agent workflow

Inspect existing tooling before creating new tools. Carry authorized work through
implementation and relevant verification; distinguish a completed coding task from
a milestone that still requires live integration or physical-device acceptance.

When CI fails, fix the failure and work out why local gates did not catch it before
the push: a local recipe that does not run what CI runs, a gate that stopped before
finishing, or a gate you skipped for the files you changed. Say which in your report.
Close a missing-gate gap in its own tooling PR straight away, not as a later offer.
A local gate that failed or did not finish has not passed.

When instructions conflict (an issue criterion, plan order, a gate, and a delivery
rule cannot all be satisfied), stop and ask the owner which gives way, naming the
conflict. Do not circle between them. Standing answers:

- **Criterion blocked by a later issue:** if an issue's acceptance criterion cannot
  be met until a later plan issue lands (for example, a live app suite before the app
  is wired to the engine), move the criterion to that later issue, note it on both
  issues, and close the current one on the criteria it can meet.
- **Gate misclassification:** if a gate fails because it classifies a change wrongly
  (for example, Markdown under `mods/` treated as runtime code), fix the gate's
  classification with a regression test in its own PR. Do not satisfy it with
  mislabeled evidence, and do not route around it.
- **Mobile-plan PRs:** once the gatekeeper merges the PR (see
  [Gatekeeper review](#gatekeeper-review)), start the next plan issue.
- **`limerick-prod`:** the game is unreleased and has no users, so publishing and
  promoting Endpoints there is authorized. So is live Endpoint testing: run the live
  suites against it, including App Check debug-token setup, without asking first.
  Its Endpoints use an inexpensive Gemini Flash Lite model.

## Standard commands

These are existing repository commands, not proof of mobile milestone completion.

```sh
just build          # existing default engine build
just check          # existing pre-commit quality gates
just verify         # existing checks plus harness walkthrough
just mobile-verify --phase N  # iPhone app gates for spec Milestone N (macOS + Xcode)
just agent-check    # lints; CI also requires the evidence-page link for runtime changes
just ui-test        # existing Svelte frontend tests
just ui-e2e         # existing browser Playwright contracts
bash limerick/scripts/check-doc-paths.sh  # documentation links and paths
```

For gameplay changes, use `/limerick-engine prove <feature>` on the affected production
path. Read [build/test](docs/agent/build-test.md) for mobile verification requirements.
`just mobile-verify` is the phase-selectable iPhone gate the technical vision
requires; `just verify` is the engine's gate. Neither stands in for the other.
Use [runtime driving](docs/agent/runtime-driving-reference.md) for existing MCP/CLI
commands and [harness.md](docs/agent/harness.md) to diagnose gate failures.

## Changes and delivery

Use conventional commits and one logical change per commit. PRs explain changed
behavior, link requirements/issues, and state in ordinary prose the acceptance
criteria, what shows each one, the verification actually run, and remaining gates.
A PR touching runtime-shipping code (`mobile/**`, `mods/**`, engine runtime crates)
must link its evidence page, `https://dmooney.github.io/rundale-pages/pr/<number>/`,
in the body; CI's agent-check fails without it. Pure docs, CI, tooling, and
test-only changes need no page. For visible changes include appropriate visual and
interaction evidence; pace recordings for a human viewer as the [phase demo plan](docs/product-specs/phase-demo-plan.md#recordings) describes.
PR recordings live outside this repository, in the public
[`dmooney/rundale-pages`](https://github.com/dmooney/rundale-pages) repository, which
GitHub Pages serves at `https://dmooney.github.io/rundale-pages/pr/<number>/`. Put an
`index.html`, the video as H.264 `.mp4`, and a short `.gif` in one directory and run
`bash limerick/scripts/publish-pr-page.sh <pr-number> <directory>`; it copies them into
`pr/<number>/`, refreshes the site's index, and pushes. Link that page from the PR body
and embed the GIF so it plays inline. Do not use Claude Artifacts (private to the
owner's account) or GitHub release assets (videos download instead of playing) for PR
evidence, and never commit videos to this repository.
Follow [git workflow](docs/agent/git-workflow.md) and the [evidence gate](docs/agent/agent-check.md).
Keep the README, documentation, and canonical world sheet consistent with changes
where applicable. Run `just notices` when dependencies change.

### Gatekeeper review

Every PR an agent opens is reviewed by a gatekeeper before it merges. Do not merge
your own PR. Once its required checks pass, spawn a fresh-context subagent (never a
fork of your session) and give it only the PR number and the instruction to run the
[`gatekeeper`](.agents/skills/gatekeeper/SKILL.md) skill on that PR. The gatekeeper
squash-merges it, sends it back with review comments, or hands it to the owner.
Address its comments with new commits, wait for checks, and spawn a new gatekeeper
for the next round. A hand-off waits for the owner: report it, and move on to work
that does not depend on it. The one exception is the owner asking to land a PR
directly, which the [`land`](.agents/skills/land/SKILL.md) skill does.

### Keep the iPhone beta current

The owner wants to try new features and fixes on their iPhone. Whenever a change
provides something new to try in the iOS app, publish an updated internal
TestFlight build after the applicable checks pass. This is standing authorization
to build, sign, upload, and distribute to the existing Internal Beta group; do
not ask for routine release confirmation. Documentation-only changes do not need
a build.

Use `just testflight-update` and its [runbook](mobile/testflight.md); it runs
`just mobile-verify --phase all` before uploading. Complete export compliance, confirm the build is **Testing** in **Internal Beta** (upload
success alone is not delivery), and report the build number App Store Connect
received, what to try, checks run, and any remaining device gates. If signing,
Apple processing, or another blocker prevents delivery, report it explicitly.

### Phase-end demonstrations

Conclude every phase with a demonstration in the running app, following the
[phase demo plan](docs/product-specs/phase-demo-plan.md). Never present a
simulator demo as physical-iPhone validation or a fixture as live gameplay.
