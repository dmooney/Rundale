# ADR-025: Mobile Runtime on the Shared Limerick Engine

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-09-25). Supersedes the mobile portion of
[ADR-014](014-web-mobile-architecture.md): the iPhone client embeds the engine
on-device rather than acting as a thin client of a game server.

## Context

The mobile reset set out to keep the Limerick engine and rebuild the player UI as
a native iPhone app, verifying each feature as it lands. The
[technical vision](../product-specs/software-technical-vision.md) required an
inventory of existing simulation, persistence, content loading, and inference
before replacing any of them (§16.1), and it forbade a second authoritative
save store (§7.1 and the decision table in §15).

An architecture review of the `ios-port` branch (2026-09-25; merge base
`547373b3d`) found that the branch reused the engine's leaf crates (`WorldState`,
`NpcManager`, `WorldGraph`, movement resolution, schedules, `GameSnapshot`,
dialogue validation), but everything above them was rewritten inside a
mobile-only runtime, `limerick_core::mobile::MobileSession`. It adds about 7,000
lines of production Rust, and no inventory or decision record was produced. The
mobile-only runtime has these parts:

- its own turn and request pipeline, beside `game_loop` and `game_session`, which
  are compiled out of the mobile build;
- a second save format and lock (`limerick-persistence/src/mobile`, with tables
  `mobile_state`, `mobile_events`, `mobile_requests`, and `mobile_metadata`)
  beside the branch, snapshot, and journal store of
  [ADR-003](003-sqlite-wal-persistence.md) and
  [ADR-004](004-git-like-branching-saves.md);
- a third content format (a bundled JSON world converted by hand-written code
  that requires exactly three locations and three NPCs, and reuses `mods/rundale`
  numeric IDs for different places and people);
- its own command parser and addressee resolver, beside `limerick-input` (this
  caused #1993);
- its own NPC prompt, beside the `limerick-npc` prompt builders.

The divergence has already cost a save. An unmerged branch build added a
transcript event kind without bumping the save format version. The next build's
closed event enum then rejected the whole save, and the app opened with an empty
transcript and an inert composer. The same design also refuses to open a save
after any content edit, because it fingerprints content with `DefaultHasher`.

## Decision

1. **One engine.** Mobile gameplay runs through the shared engine in
   `limerick-core`. Add a portable turn API to the shared game loop. The host
   supplies inference: the engine yields an inference request, and the host
   resumes the turn with the validated result. Desktop keeps calling inference
   in-process through the same API. There is no mobile-only turn pipeline,
   parser, addressee resolver, or world builder.
2. **Request lifecycle in shared core.** Logical request and execution-attempt
   identities, Stop and late-callback terminality, durable acceptance, idempotent
   transcript event identities, and clarification are required by the product
   spec. They move into shared core so desktop gains them too. The `ios-port`
   implementation serves as a reference specification and test oracle, not as
   code to keep.
3. **One save system.** Extend the existing `limerick-persistence` database with
   request and transcript tables and a single transactional turn commit. The
   separate mobile schema is retired. Branching remains underneath and is not
   exposed in the mobile UI (product spec §12.3).
4. **Forward-compatible saves.**
   - A transcript event of an unknown kind never blocks opening a save. It is
     preserved verbatim and shown as a neutral fallback line, as the vision
     requires for unknown presentation items.
   - Opening is refused only when authoritative world or request state cannot be
     read. The refusal is a clear player-facing compatibility message, the file
     is kept, and the player is offered a new game.
   - Content compatibility uses stable content identity and version, not a hash
     whose value can change between compiler releases.
   - Every change to saved data bumps the save format version, enforced by a test
     over checked-in prior-format fixtures.
5. **Prompts are game data.** Endpoint definitions (instructions, input and
   output schemas, provider and inference settings) are source-controlled files
   inside the world's content, not rows authored in the Endpoints database. A
   publish step creates the immutable Endpoint version from the file and verifies
   its content hash. The engine owns which role and version to invoke and builds
   the structured inputs. This refines product spec §11 "prompt construction":
   the engine owns prompt content as authored game data, and the Endpoint
   executes a published copy.
6. **A new world, loaded as a mod.** The mobile world is the canonical tiny world
   from product spec §14, rebuilt from scratch as its own mod with its own ID
   space and loaded through the normal mod pipeline. It shares no IDs with
   `mods/rundale`. The old world stays available in git history.
7. **Fix before adding.** Mobile feature work (Phase 5 and beyond) is frozen
   until the convergence plan's engine stages pass. The unmerged
   `codex/ios-phase5-living-world-demo` branch is discarded, because its gossip
   was a hard-coded script rather than the engine's gossip module.
8. **Restart the Rust integration from `main`.** The mobile line is rebuilt from
   `main`, bringing over the SwiftUI app, the Swift/Rust boundary shape, the
   Endpoint client and service, and the verification and release tooling.
   `ios-port`'s Rust orchestration, mobile save store, and content bundle are not
   carried over.
9. **Scope and delivery confirmations.**
   - Milestone 2 may use the canonical three-location, three-NPC world; the
     one-location, one-NPC limit is retired, as confirmed by the owner in #1992.
   - Existing TestFlight saves may be discarded: they are test data, and no
     migrator is built for them.
   - Continuous TestFlight delivery to the owner's own device remains authorized.

## Alternatives considered

- **Salvage `ios-port` piece by piece** (content, then parser, then persistence,
  then orchestration). This is feasible, but every mobile change made during the
  migration grows the runtime being removed (PR #2005 alone added about 1,100
  lines to it). With features frozen, it costs more in total than rebuilding the
  orchestration on shared code.
- **Have desktop adopt the mobile save tables.** This would still leave two turn
  pipelines. Instead, the request and transcript tables join the existing
  database, so both front ends share one store.
- **Keep prompts in the Endpoints database.** This was rejected because it
  cannot be reviewed, diffed, or reproduced from the repository.

## Consequences

- The portable turn API is the largest piece of work, and it touches desktop
  code, so the existing desktop tests and harness must stay green at every step.
- Mobile regains every desktop fix automatically and stops needing parallel
  feature implementations.
- The Endpoints service gains a file-based publish flow, and its database holds
  published copies, not the source of truth.
- The Phase 2 audit (#1992) and PR #2005 were measured against the runtime being
  replaced. They are re-based on the new line; see the
  [convergence plan](../plans/mobile-engine-convergence.md).

## Revisit when

- A concrete iOS capability cannot be met by the shared engine, such as a
  measured performance or memory limit. Record the missing property and the
  narrowest adapter that meets it before adding mobile-only engine code.
