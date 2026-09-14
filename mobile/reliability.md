# Mobile reliability contract

Phase 4 hardens the canonical tiny world. The governing scope is
[Milestone 4](../docs/product-specs/product-technical-spec.md); the repeatable
[case matrix](../docs/test-plans/phase-4-test-cases.md) and
[acceptance record](acceptance.md) distinguish implementation from device proof.

## Interruption and retry

The app persists the composer on lifecycle transitions. Inactive/background
state blocks new submissions; backgrounding interrupts an uncommitted inference
through the same Rust Stop boundary used by the composer. A request accepted
while suspension races submission must also be resolved through that boundary.
Foregrounding does not silently resubmit work. The transcript reports an
interrupted or failed attempt and offers explicit retry. A completed action
remains committed, and retry must never repeat it.

Remote transport loss remains independent of local storage: the player can
recover the failed request without restarting the game, and deterministic local
commands remain available without inference. Streamed text stays provisional
until the engine validates and commits the final candidate. Late callbacks and
replayed events remain subject to attempt, revision and sequence validation.

## Long sessions and reading position

The native bridge returns a bounded presentation tail and pages durable history
by event sequence. The full request ledger and committed events remain in the
authoritative SQLite save. A long session must reopen within the bridge's response
limit; increasing that limit is not a substitute for bounded presentation data.

The viewport projection records an optional event cursor with its anchor and
session identity. Relaunch loads a bounded page containing that anchor before
publishing the transcript. Older projections without a cursor remain readable
and fall back to the latest tail when their anchor is no longer available there.
Reading an older window keeps it stable as new events arrive; the newest-content
control restores the current authoritative tail.

## Save compatibility and failure policy

Phase 4 preserves the existing SQLite mobile format version 1, canonical content
identity and Swift presentation projection. Updates must open existing Phase 3
saves without a reset. Newer/unsupported formats, a mismatched content identity,
and malformed existing databases must fail visibly without replacing the file.
An unreadable presentation projection must not be silently overwritten as a new
empty draft. SQLite commits request decisions, world state and durable events
atomically; the Swift projection stores only draft and viewport state.

No destructive reset, content migration or save-management surface is introduced
by this phase. Future format changes must carry a prior-format fixture, tested
atomic migration/recovery and an explicit rollback policy. A failed write must
never be presented as a successful committed action. Keep the prior committed
save available and report the storage failure to the player.

The existing persistence tests exercise transaction rollback, malformed/newer
schema preservation, interrupted initialization and cross-process locking.
Physical file-protection, lock/unlock, disk pressure and iOS termination checks
remain part of device acceptance; an unsigned build cannot establish them.

## Evidence and diagnostics

Use `./verify --phase 4` for the Phase 1–4 regression set or `./verify --phase all`
for release verification. JSON, JUnit, command logs and native xcresults are
stored under the selected ignored report directory. Native recovery tests attach
screenshots of the observed outcomes. The real Endpoint remains opt-in and must
be reported separately from transport fault injection.

Retain request/attempt identity and event sequence when diagnosing duplication;
never include authentication headers, tokens, private Firebase configuration or
provider credentials in receipts. Record an exact build and reproduction before
changing lifecycle policy. The app's current iOS 17 minimum remains provisional
until the primary/small-screen device matrix passes.
