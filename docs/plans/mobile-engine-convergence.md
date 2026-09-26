# Plan: mobile engine convergence

> Status: Accepted · Created: 2026-09-25 · Decision: [ADR-025](../adr/025-mobile-runtime-on-shared-engine.md)

Put the iPhone app on the shared Limerick engine and remove the parallel mobile
runtime that grew on `ios-port`. Fix before adding: mobile feature work stays
frozen until stage 5 exits. Each step is one logical change with its own PR.
Desktop tests and the harness walkthrough stay green at every step.

## Where things stand

- `main` has the engine and the reset specs, but no `mobile/` or `endpoints/`
  directories.
- `ios-port` has a working SwiftUI app, the Swift/Rust boundary (FFI), the
  Endpoints service, and verification and release tooling. These sit on a
  mobile-only runtime:
  - `limerick-core/src/mobile`: turn pipeline, parser, world builder;
  - `limerick-persistence/src/mobile`: second save store;
  - `mobile/content/phase3-tiny-world.json`: third content format.
- Limerick Endpoints is deployed in the dedicated `limerick-prod` project. It
  serves `rundale-dialogue` v1 and v2 and `rundale-intent` v1, and the live
  simulator suite passes against it. TestFlight build 10 comes from the
  `ios-port` line.
- The owner's TestFlight save is test data and may be discarded. On the device,
  reinstalling the app starts fresh.

## Stage 0: stop the drift (now)

- Freeze mobile feature work, including Phase 4 and 5 and new Phase 2 audit
  items. Make no further changes to `ios-port`'s mobile runtime except to unblock
  testing.
- Discard `codex/ios-phase5-living-world-demo`. Its gossip is a hard-coded script
  (`490795f8f`).
- Record the decisions: ADR-025, this plan, and the spec updates.
- Do not carry forward `ios-port`'s single-location exception in
  `WorldGraph::validate`, which weakened a shared rule.

Exit: this plan and ADR-025 are merged, and no new work lands on the mobile
runtime.

## Stage 1: shared groundwork on `main` (S each)

1. **Portable dialogue and look extraction.** Port `dialogue_apply.rs` and
   `portable_look.rs` from `ios-port`. That was a near-verbatim move with
   re-exports. Fix the duplicated `detect_and_record_player_name` and the doc
   comment left on `RolledEncounter`.
2. **Shared input interpretation.** Land the `limerick-input` interpretation
   module from PR #2005 on its own:
   - `interpret_locally`, the intent system prompt, and strict structured-output
     parsing;
   - the `validated_intent` extraction;
   - the parity tests.

   Desktop behaviour must not change.

3. **Portable build seam.**
   - Add the `desktop` / `mobile` Cargo features.
   - Stop gating `limerick-mod` and `limerick-palette` behind `desktop`; they
     have no desktop dependencies.
   - Add a CI job that builds the mobile configuration
     (`--no-default-features --features mobile`).

Exit: CI builds the mobile configuration, and desktop suites and the harness
walkthrough pass unchanged.

## Stage 2: portable turn API (L)

- Add one turn entry point to the shared game loop:
  - submitting input returns committed events, an inference request, or a
    clarification;
  - the host resumes a request with a validated result, a failure, or Stop.
- Desktop drives the same API with in-process inference, so there is one turn
  pipeline.
- Move into shared core, following ADR-025 §2:
  - logical request and execution-attempt identities;
  - Stop and late-callback terminality;
  - durable acceptance;
  - idempotent transcript event identities;
  - clarification.
- Use `ios-port`'s request-lifecycle tests and `RundaleKit` lifecycle fixtures as
  the oracle.
- Use one parser and one addressee resolver: `limerick-input` plus the desktop
  target resolver, with clarification added and the role-vocative fallback kept.
  Remove `deterministic_capability`-style duplicates.
- Travel uses the full desktop movement path: weather-aware resolution,
  encounters, tier transitions, and arrival reactions.

Exit:

- New request-lifecycle tests pass headless with a scripted inference host.
- Desktop tests and the harness walkthrough are unchanged.
- `limerick-engine --script` output is unchanged for existing fixtures.

## Stage 3: one save system (L)

- Extend the existing `limerick-persistence` database with request and transcript
  tables and one transactional turn commit. Existing desktop saves keep opening
  (ADR-003 and ADR-004 behaviour, with a migration for the new tables).
- Apply the forward-compatibility rules in ADR-025 §4:
  - unknown event kinds are preserved and shown as a fallback line;
  - saves are refused only when authoritative state is unreadable, and a refusal
    keeps the file and offers a new game;
  - content compatibility uses stable content identity and version;
  - a test fails if saved data changes without a format version bump.
- Keep one save lock. Check the existing lock under the iOS sandbox and adopt the
  kernel lock only if the directory/PID lock is shown to be inadequate there.

Exit:

- Checked-in prior-format desktop fixtures open.
- A fixture with an unknown event kind opens with a fallback line.
- An unreadable save yields the compatibility message and is left untouched.
- Turn commits round-trip across relaunch.

## Stage 4: new world and prompts as game data (M)

- Author the canonical tiny world (product spec §14: Kilteevan Village, Letter
  Office, Connolly Cottage; Peig, Mícheál, Róisín) as a new mod with its own ID
  space, loaded through the normal mod pipeline. Choose a name such as
  `mods/kilteevan/`. There are no count checks and no reuse of `mods/rundale`
  IDs, and the canonical world sheet becomes a test oracle.
- Move Endpoint definitions into the mod as files, for example
  `endpoints/rundale-dialogue.v1.json` and `endpoints/rundale-intent.v1.json`.
  The engine reads each role's definition and version from the mod.
- Add a publish flow to Limerick Endpoints that creates immutable versions from
  those files and verifies content hashes. Republish `limerick-prod` from the
  files. The database then holds published copies only.

Exit:

- The desktop engine plays the tiny world from a headless script.
- Published hashes match the files.
- No mobile-specific content code remains.

## Stage 5: mobile line from `main` (M)

- Start a new mobile branch from `main` once stages 1–3 have landed. Stage 4 can
  proceed alongside.
- Bring over from `ios-port`:
  - `mobile/`: the SwiftUI app, `RundaleKit`, `RundaleBridge`, and the Endpoint
    client kit;
  - the `endpoints/` workspace;
  - the `limerick-mobile-ffi` boundary shape, re-pointed at the turn API and
    renamed from `parish_*` (#2007);
  - `mobile/scripts`: verify with pass reuse, release, UI recording, and
    stream-frame analysis;
  - the test plans, the Endpoint SSE fixtures, and the UI-test transcript trace.
- Do not bring over:
  - `limerick-core/src/mobile`;
  - `limerick-persistence/src/mobile`;
  - `mobile/content/`;
  - the `WorldGraph` single-location exception.
- Continue continuous TestFlight delivery from this line. Discarding pre-release
  saves is acceptable.

Exit:

- `./verify` automated gates for phases 1–3 pass on a large and a small (SE)
  simulator.
- The live Endpoint suite passes against `limerick-prod`.
- A TestFlight build from the new line is uploaded.

## Background inference seam (M, after stage 2)

Stage 2 routes only in-turn inference through the host. Post-turn NPC
reactions, idle banter, and tier-2/3/4 simulation still call inference
in-process, so mobile runs without them until this lands (#2025).

- Add a background-inference host seam in shared core, reusing the stage 2
  inference request and outcome types. Background work yields to player turns
  and can be cancelled.
- Apply results through the existing canonical seams with Rule 30
  revalidation.
- Desktop keeps in-process fulfilment, unchanged.

Exit: lifecycle tests pass with a scripted host, and mobile runs reactions,
banter, and background simulation through its host.

## Stage 6: re-audit, then resume features

- Re-run Milestones 1–3 on the new line: automated gates first, then the
  deferred physical-device checks.
- Re-base the Phase 2 audit (#1992) on the new line, and close findings that no
  longer apply.
- Tag `ios-port` as an archive (for example `archive/ios-port`) and stop
  updating it.
- Unfreeze feature work once the background inference seam has landed. The
  living world uses the engine's own systems, such as
  `limerick-npc` gossip, never mobile-only scripts.

## Open items

| Item                                                            | Disposition                                                                                                                                                        |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| PR #2005 (`ios-port` #1993 work)                                | Land the `limerick-input` module in stage 1.2. Port verify pass reuse, the transcript trace, and stream-frame tooling in stage 5. Close the mobile-runtime changes |
| #1993 intent inference                                          | Resolved on the new line by stages 1.2 and 2                                                                                                                       |
| #1992 Phase 2 audit                                             | Parked until stage 6                                                                                                                                               |
| #2007 leftover `parish_*` names                                 | Stage 5 boundary port                                                                                                                                              |
| PR #2008 (hooks, `.worktreeinclude`) and PR #2009 (clock flake) | Independent of this plan; merge on their own                                                                                                                       |
| `limerick-prod` Endpoints deployment                            | Keep; republish definitions from files in stage 4                                                                                                                  |
| `cottage-d6dc9` wind-down                                       | Separate; awaiting owner confirmation                                                                                                                              |
| `mods/rundale` and the desktop app                              | Unchanged by this plan; the mobile world does not depend on them                                                                                                   |
| `CLAUDE.md` / `AGENTS.md` agent policy                          | Owner will revise separately                                                                                                                                       |
