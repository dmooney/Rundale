# Acceptance Criteria: snapshot-tier-preserve

## Task

`parish-persistence`'s `GameSnapshot::restore` rebuilds the
`NpcManager` from scratch via `*npc_manager = NpcManager::new()`
([snapshot.rs:424](../../../parish/crates/parish-persistence/src/snapshot.rs)),
which wipes the in-memory `tier_assignments` map. The next
`assign_tiers` call then sees every NPC's `old_tier` default to
`Tier4` and re-fires `GameEvent::NpcArrived` for every NPC that ends up
at Tier1.

Every consumer of `NpcArrived` is misled by this — character logs gain
a phantom "arrived" pulse on every session resume, `inflate_npc_context`
injects a recap memory for an NPC that didn't actually move, and the
debug event bus shows a flood of "Padraig arrived at Darcy's Pub"
right after `/load` even though Padraig was already there in the saved
state.

Fix: after `restore_npcs` repopulates the manager, **silently seed**
`tier_assignments` from the now-restored world+NPC state — same
distance/tier computation `assign_tiers` would perform, but without
firing any `GameEvent` or running inflate/deflate side effects. Tier
is derivable from `(world.player_location, npc.location, npc.state)`
so no new fields are needed in the on-disk snapshot.

## Criteria

- **C1 — `tier_assignments` is populated after `restore`.** A fresh
  `NpcManager` whose snapshot is restored has `tier_of(npc_id)` matching
  what `assign_tiers` would return for the same world. Observable via:
  Rust integration test that restores a snapshot and reads
  `npc_manager.tier_of(...)` for each NPC.

- **C2 — `restore` publishes no `NpcArrived` events.** A subscriber on
  `world.event_bus` sees zero `NpcArrived` events during the
  `snapshot.restore` call itself. Observable via: subscribe to
  `event_bus`, call `restore`, assert the receiver is empty.

- **C3 — The first `assign_tiers` after `restore` doesn't refire
  `NpcArrived` for already-Tier1 NPCs.** With tier state pre-seeded,
  `assign_tiers` sees no transitions and publishes nothing. Observable
  via: subscribe to `event_bus`, restore, then call `assign_tiers`;
  assert zero `NpcArrived` events.

- **C4 — Genuine post-restore tier changes still fire `NpcArrived`.**
  After restore, if the player moves and an NPC transitions Tier2 →
  Tier1, that NPC's `NpcArrived` event fires exactly once. Observable
  via: restore, move the player, call `assign_tiers`, assert one
  `NpcArrived` event for the promoted NPC.

- **C5 — Cold-start path is unchanged.** A brand-new `NpcManager` that
  has never been restored still fires `NpcArrived` for every Tier1
  NPC on the first `assign_tiers` call. The fix doesn't accidentally
  silence the initial game-start arrivals. Observable via: existing
  unit tests under `parish-npc` continue to pass without
  modification.

- **C6 — Schema is unchanged.** `GameSnapshot` adds no new
  serialised fields; saves written before this fix still load
  correctly. Observable via: existing
  `parish-persistence::snapshot::tests` deserialization tests
  continue to pass.

## Verification script

This is a backend-internal seam — the script harness's
`GameTestHarness::new` doesn't call `restore`, so a play-script can't
exercise it directly. Verification is via Rust integration test:

```sh
cargo test --manifest-path parish/Cargo.toml -p parish-persistence \
    snapshot::tests::tier_state_preserved_across_restore -- --nocapture
```

Plus a regression check that the previously-failing
`character_log` flood scenario stays clean:

```sh
cargo test --manifest-path parish/Cargo.toml -p parish-core --lib character_log
```

Expected signals in the captured transcript:

- C1 — assertion `tier_of(NpcId(X)) == Some(CogTier::Tier1)` passes
  after restore (for at least one NPC that was Tier1 pre-snapshot).
- C2 — assertion `rx.try_recv().is_err()` (or no `NpcArrived` event)
  immediately after `snapshot.restore` returns.
- C3 — assertion that `assign_tiers` returns an empty
  `transitions` vec post-restore for unchanged world.
- C4 — moving the player produces exactly one `NpcArrived` for the
  newly-Tier1 NPC.
- C5/C6 — existing test suites (`parish-npc`, `parish-persistence`)
  show 100% pass with no new ignored tests.
