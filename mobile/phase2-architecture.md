# Phase 2 integration decisions

This record describes the implementation boundaries selected on 2026-09-07.
Verification and physical-device acceptance must be recorded separately; this
document is not evidence that a gate has passed.

## Runtime ownership

The existing SwiftUI screen and RundaleKit presentation reducer remain the
client. Shared orchestration belongs in the portable mobile feature of
parish-core. The existing default Rust feature set retains desktop behavior.
The mobile dependency graph excludes local-model setup, editor, diagnostics,
and desktop entry points.

The runtime reuses Parish domain types and its NPC candidate validator. The
Phase 2 world has one location and one NPC. A completed conversation records
the accepted exchange locally; it does not enable the legacy task, travel, or
background simulation systems.

Swift owns platform networking, credentials, lifecycle, draft, and viewport
storage. Rust owns command interpretation, inference context, candidate
validation, request terminality, authoritative state, and durable transcript.
The presentation snapshot is a projection, never a second gameplay save.

## Durable boundaries

The existing parish-persistence crate owns the mobile SQLite store. Its
existing snapshot and task journal API lacks a transaction combining logical
requests, transcript identities, and final world state, so the mobile store
adds that explicit boundary within the same persistence crate.

Acceptance persists the original command and its identities before clearing
the draft. Completion persists the resulting state, final output, terminal
request, and revision together before delivery. Inference runs outside a
transaction. Stop and final validation return through one serialized mutation
lane; only one terminal decision can win.

The first mobile save format does not import legacy desktop saves or Phase 1
fixture snapshots. Those stores use separate paths. Existing invalid,
unsupported, or content-incompatible mobile saves must produce a recoverable
error, never an implicit new game. Provisional token updates are bounded in
memory and are not permanently journaled.

## Presentation and inference

Rust events use the Phase 1 semantic contract, including stable session,
request, attempt, transcript item, and event identities. Final NPC dialogue
passes the canonical validator before it becomes committed history or
conversation memory. Phase 2's explicit streaming requirement permits clearly
provisional text before validation; this is not the legacy runtime policy of
quarantining all candidate text. Rejected or interrupted output must remain
visibly uncommitted and have no canonical effects.

Production inference uses Parish Endpoints. A simulated transport is permitted
only for deterministic verification and cannot count as evidence of real
streaming. The integrated service accepts direct Firebase Auth plus mandatory
App Check mobile principals, authorizes them through a strict app-to-tenant and
Endpoint binding, and provides a versioned Google SSE stream. Existing consumer
API-key and creator authorization remain separate. No shared invocation or
provider key enters the iOS app. Deployment, published-contract, live Firebase,
and live Google evidence were recorded separately on 2026-09-09; see
[the current handoff](endpoint/phase2-handoff.md). Cottage is the user-selected
reference for Firebase authentication. Physical App Attest remains separate.

## Build and evidence

Use the repository-pinned Rust toolchain explicitly: the host shell's
Homebrew compiler can differ from rustup's selected compiler. Device and
simulator libraries must be built with the same compiler as their installed
target standard libraries.

Binding tests, Rust fault-injection tests, simulator tests, real Endpoint
delivery, and physical-iPhone acceptance are distinct evidence. A passing
mock transport or simulator suite does not establish live or physical gates.
The phase concludes with a native demonstration and an explicit account of
which acceptance gates were exercised.

## Batch delivery regression analysis

A synchronously completed command could remain in the composer, and a
finalized response could retain its previous provisional display.

1. Draft clearing depended on the controller's most recent event still being
   the acceptance event; transcript updates refreshed only the last row.
2. A Rust operation can return acceptance, output, and completion together,
   and completion can change an earlier row while appending another.
3. A state publisher does not promise that its subscriber observes every
   intermediate assignment as a separate event.
4. The fixture adapter's paced delivery hid that assumption, while the
   projection treated stable identity as evidence of immutable content.
5. The presentation boundary had not exercised batched durable completion.

Use the durable submission receipt with the existing draft revision guard,
and reconcile changed rows even when their identities are unchanged. The
prevention is a batch-delivery regression test. No additional AGENTS.md rule
is needed: the existing requirements for meaningful behavioral tests and
shared semantic-contract proof already cover this obligation.
