# Judge — snapshot-tier-preserve

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

[C1 — `tier_assignments` populated after `restore`]:
`snapshot::tests::tier_state_preserved_across_restore` passes
(transcript line 29). The test asserts
`new_npcs.tier_of(NpcId(1)) == Some(CogTier::Tier1)` and the
analogous assertion for `NpcId(2)`, plus a same-world-reuse path. Met.

[C2 — `restore` publishes no `NpcArrived`]:
`snapshot::tests::restore_publishes_no_npc_arrived_events` passes
(transcript line 15). Subscriber is attached before `restore`; drain
afterwards counts zero `NpcArrived`. Met.

[C3 — First `assign_tiers` after restore is a no-op]:
Same two tests cover this. `tier_state_preserved_across_restore`
asserts the transitions vec is empty;
`restore_publishes_no_npc_arrived_events` keeps the subscriber alive
across the post-restore `assign_tiers` and re-drains, asserting zero
events again. Met.

[C4 — Genuine post-restore promotion still fires]:
`snapshot::tests::genuine_tier_promotion_after_restore_still_fires`
passes (transcript line 30). After restore + player move from
`LocationId(1)` to `LocationId(2)`, exactly one `NpcArrived` fires for
the NPC at `LocationId(2)` (Tier2 → Tier1). Met.

[C5 — Cold-start unchanged]: `parish-npc` test suite passes
(transcript lines 42-47, plus the per-target counts that flow through
`just check`). No existing tier-assignment test was modified; the new
`seed_tier_state` is an additive function. Met.

[C6 — Schema unchanged]: No new fields in `GameSnapshot`. All 21
pre-existing snapshot tests pass (transcript lines 10-28), including
`test_old_save_backward_compat_visited` (line 23) and
`test_old_save_backward_compat_introduced_npcs` (line 20). The fix is
runtime-only — tier is recomputed silently from world+NPC state which
is already in the snapshot. Met.

## Implementation notes

The fix is two additions, no deletions:

1. `parish-npc/src/tier_assign.rs::seed_tier_state` — same BFS-distance
   compute as `assign_tiers`, but writes only `tier_assignments` and
   publishes no events. ~25 LOC.
2. `parish-npc/src/manager.rs::NpcManager::seed_tier_state` — thin
   wrapper passing `self.npcs` / `self.tier_assignments` /
   `self.bfs_distances_cache` through to the free function.
3. `parish-persistence/src/snapshot.rs::GameSnapshot::restore` —
   single new call to `npc_manager.seed_tier_state(world)` at the end,
   after `restore_npcs` finishes rebuilding the manager.

## Technical debt

Clear. No new `#[allow]`, no `unsafe`, no TODOs. The new function
shares the distance-compute logic structurally with `assign_tiers` —
there's some duplication of the `match npc.state { Present / InTransit
}` distance arithmetic, but the original function is so tightly
coupled to its transitions/events emission that factoring out the
shared bit would obscure both halves. Worth revisiting only if a
third caller needs the same compute.

## Out of scope

A future PR could go further and persist `tier_assignments` directly
in the snapshot blob (with `#[serde(default)]` for back-compat),
trading a tiny on-disk cost for one fewer BFS pass on restore. Not
needed — the BFS is already paid by the *next* `assign_tiers` call
anyway; seeding now just moves the work earlier so the result is
useful before the broadcast subscriber wakes up.
