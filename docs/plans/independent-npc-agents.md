# Plan: Independent NPC Agents

> Status: Proposed | Design: [independent-npc-agents.md](../design/independent-npc-agents.md)

Implement depth-first. Each step is independently reviewable and leaves the
current simulation path available behind the `independent-npc-agents` kill
switch.

## 1. Agenda types and scheduler index

Commit: `feat(npc): add persisted NPC agendas and wake scheduler`

- Add typed goals, activities, plan steps, wake reasons, and agenda generation
  to `parish-npc`.
- Add a min-heap scheduler index and bounded/coalesced pending wakes to
  `NpcManager`.
- Seed default agendas from game time and authored schedules.
- Test heap ordering, stale generations, coalescing, fairness, pause behavior,
  and time-jump caps.
- Do not alter schedule or Tier 2/3 behavior yet.

## 2. Snapshot and restore

Commit: `feat(persistence): round-trip NPC agent agendas`

- Add `AgentAgenda` to `NpcSnapshot` with backward-compatible defaults.
- Update exhaustive `from_npc`/`into_npc` conversion.
- Rebuild the scheduler heap and clear in-flight work on restore, branch
  switch, and new game.
- Add old-save, round-trip, overdue-load, and no-duplicate tests.

## 3. Shared collect and commit seams

Commit: `feat(core): orchestrate autonomous NPC intent commits`

- Define immutable revision-stamped planning snapshots, typed intent envelopes,
  localized preconditions, and outcomes in `parish-npc`.
- Add bounded due-work collection and deterministic intent reduction in
  `parish-core::game_loop`.
- Keep all awaits outside world/NPC locks; collect each state domain in a
  separate lock scope so world and NPC locks are never held together.
- Publish existing semantic `GameEvent`s; add variants only for behavior that
  cannot be represented today.
- Test stale results, conflicting movement, cancellation, rejection reschedule,
  and deterministic ordering.

## 4. Diagnostics and fixture surface

Commit: `feat(engine): expose NPC agent agenda diagnostics`

- Add `/debug agents [npc]` to the shared debug command path and render stable
  agenda fields: actor, tier, goal, activity, generation, next wake, in-flight,
  and last outcome.
- Add queue depth and outcome counters to the debug snapshot.
- Ensure player-facing text does not include scheduler terminology.
- Turn `play_independent-npc-agents.txt` from a red fixture into a passing one.

## 5. Schedule intents in shadow mode

Commit: `feat(npc): shadow authored schedules through agent intents`

- Generate deterministic `FollowSchedule`/`MoveTo` intents at schedule
  boundaries.
- Compare proposed destinations and events with `tick_schedules` without
  committing the new path.
- Record mismatches with NPC, game time, weather, expected destination, and
  proposed destination.
- Add season/day/weather/shelter and large-time-jump parity tests.

## 6. Switch deterministic schedule ownership

Commit: `feat(npc): drive NPC schedules through the agent reducer`

- Under the default-on feature flag, commit schedule movement through the
  reducer and suppress the old schedule mutation pass.
- Keep the old path as the disabled-flag fallback.
- Verify identical `ScheduleEvent`/`GameEvent` behavior in every runtime.
- Run `just check` and the feature proof before continuing.

## 7. Migrate Tier 2 to independent wakes

Commit: `feat(npc): schedule nearby NPC scenes by individual deadlines`

- Replace the single Tier 2 last-tick gate with per-NPC due deadlines.
- Coalesce due, co-located NPCs into one scene request.
- Preserve the solo-NPC no-LLM rule, Background priority, player-input
  cancellation, balanced-brace JSON extraction, schema validation, bounded
  retry/defer handling, gossip minting, and request caps.
- Apply one typed intent/outcome per participant and assign separate next
  wakes.
- Add inference-count, scene-coherence, cancellation, and tier-transition
  tests.

## 8. Migrate Tier 3 and Tier 4

Commit: `feat(npc): plan distant NPC work through agent batches`

- Batch only due Tier 3 NPCs and return individual agendas and next wakes.
- Route Tier 4 rules through typed intents without introducing LLM calls.
- Preserve batch caps, life-event ordering, banshee behavior, and gossip flow.
- Add long-jump, mortality, batch-size, and starvation tests.

## 9. Runtime wiring and cleanup

Commit: `refactor(core): consolidate NPC agent dispatcher wiring`

- Put the shared async dispatch body and constants in `parish-core`.
- Keep server and Tauri pollers as thin per-session lifecycle wiring using
  existing cancellation/shutdown tokens.
- Make headless/script mode use the same collect and commit seams.
- Remove superseded tier tick state and duplicate dispatch loops only after
  mode-parity tests and live proof pass.
- Review every changed `AppState`, inference, persistence, and event-bus seam
  against `docs/agent/scaling-rules.md`.

## 10. Proof and documentation

Commit: `docs: document independent NPC agent scheduling`

- Update Cognitive LOD, Inference Pipeline, README feature text, config
  examples, and ADR-002 with the final semantics.
- Run the fixture and save `.proofs/independent-npc-agents/transcript.txt`.
- Map every acceptance criterion in `evidence.md`, obtain the independent
  judge verdict, and run `just agent-check`.
- Run `just check`, `just verify`, and
  `/parish-engine prove independent-npc-agents`.
- Attach the proof bundle to the PR body before review.

## Stop point for this design pass

Do not implement these steps until the design and acceptance criteria are
approved. The recommended first implementation PR covers steps 1-5 only; the
ownership switch and inference migrations should be separate PRs with their
own live proof bundles.
