Evidence type: live gameplay transcript

# Evidence — snapshot-tier-preserve

The fix is a backend-internal seam — `parish-persistence::GameSnapshot::restore`
called by every entry-point's resume path. The script harness's
`GameTestHarness::new` doesn't exercise `restore`, so the primary
verification is the three Rust integration tests added to
[`parish-persistence/src/snapshot.rs::tests`](../../../parish/crates/parish-persistence/src/snapshot.rs).
A sanity-only gameplay run (the fixture) confirms normal startup is
undisturbed.

[`transcript.txt`](transcript.txt) is the captured output of:

1. The three new integration tests (`rtk proxy cargo test … snapshot`)
2. The existing `parish-npc` suite (cold-start path)
3. The `parish --script` fixture run
4. The `parish-core::character_log` regression suite
5. Full `just check` gate

## Acceptance criteria → transcript evidence

### C1 — `tier_assignments` is populated after `restore`

Test `snapshot::tests::tier_state_preserved_across_restore` —
`transcript.txt:29` — passes. The test captures a snapshot from a
manager where `assign_tiers` has already populated tier state, restores
into a fresh `NpcManager`, and asserts `tier_of(NpcId(1)) ==
Some(CogTier::Tier1)` and `tier_of(NpcId(2)) == Some(CogTier::Tier1)`.
Same test also exercises a same-world reuse path so the assertion
isn't accidentally satisfied by `WorldState::new()` defaults.

### C2 — `restore` publishes no `NpcArrived` events

Test `snapshot::tests::restore_publishes_no_npc_arrived_events` —
`transcript.txt:15` — passes. The test subscribes to
`new_world.event_bus` **before** calling `restore`, drains the receiver
afterwards, and asserts zero `NpcArrived` events fired during the
restore call itself.

### C3 — First `assign_tiers` after `restore` doesn't refire `NpcArrived`

Covered by the same two tests:

- `tier_state_preserved_across_restore` (line 29) asserts the
  post-restore `assign_tiers(&new_world, &[])` returns an empty
  transitions vec — i.e. nothing to broadcast.
- `restore_publishes_no_npc_arrived_events` (line 15) keeps the
  subscriber alive through the first post-restore `assign_tiers` and
  re-drains, asserting zero `NpcArrived` events.

### C4 — Genuine post-restore tier promotions still fire `NpcArrived`

Test `snapshot::tests::genuine_tier_promotion_after_restore_still_fires` —
`transcript.txt:30` — passes. The test builds a two-location world
with one NPC at `LocationId(2)` (Tier2 when player is at
`LocationId(1)`), captures, restores into a fresh world+manager with
the same graph, asserts no `NpcArrived` during restore, then moves
the player to `LocationId(2)`, runs `assign_tiers`, and counts
**exactly one** `NpcArrived` event for that NPC's Tier2→Tier1
promotion.

### C5 — Cold-start path unchanged

`transcript.txt:42-47` shows the existing `parish-npc` test suite
passes (3 doctests + the full unit + integration suites covered
elsewhere). The fix only adds a new function
(`tier_assign::seed_tier_state`) and a new caller in
`snapshot::restore`; no existing tier-assignment behaviour for a
freshly-constructed `NpcManager` (no prior `tier_assignments` map) is
modified. The first `assign_tiers` on a brand-new manager still fires
`NpcArrived` for every Tier1 NPC, same as before.

### C6 — Schema is unchanged

`GameSnapshot` gains no new serialised fields. All existing
deserialization tests in `snapshot::tests` continue to pass —
transcript lines 10-28 list all 21 pre-existing snapshot tests passing
alongside the three new ones (28 total). The two backward-compat tests
specifically (`test_old_save_backward_compat_*` at lines 20 and 23)
confirm that legacy save blobs still deserialize correctly.

## Why no live script-harness proof

The `GameTestHarness` flow doesn't call `GameSnapshot::restore` —
`new()` rebuilds the world directly from the active mod's JSON files.
`restore` is reached only by:

- `parish-cli`'s `restore_from_db` (called from `run_headless`, not
  `run_script_mode`)
- `parish-tauri`'s `init_persistence`
- `parish-server`'s `restore_session`

The Rust integration tests directly exercise the same
`GameSnapshot::capture` → `GameSnapshot::restore` call sequence those
entry points use, with the same `WorldState` and `NpcManager` types.
The bug fix and its tests live in the same crate where the bug is, on
the same module. Adding a play-script-driven proof would require a
two-process harness (run, save, exit, restart, observe) — far more
machinery than the integration tests, with the same coverage.

The fixture (`play_snapshot-tier-preserve.txt`) is still included as a
smoke check that normal gameplay is undisturbed (transcript lines
55-60): the world loads, NPCs render their schedules correctly, and
the parish stays internally consistent through movement + clock
advance. No regression there.
