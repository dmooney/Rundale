# Test and Tooling Rules

These rules cover truthful automation, tool reuse, verification reporting,
managed test processes, preserved end-to-end contracts, and player-facing UX
audits. See [engineering-rules.md](engineering-rules.md) for the complete
numbered map.

<a id="rule-10"></a>

## Rule 10 — **Truthful test automation:**

Anything named or scheduled as a test must execute the production behavior it
claims to cover, contain a machine-checkable oracle, and propagate failures to
its caller. Exploratory proof scripts belong outside regression test
directories. Every confirmed escaped bug gets a regression test at the lowest
production seam that would have caught it. Scheduled automation must be green,
explicitly paused with a tracking issue, or removed.

<a id="rule-16"></a>

## Rule 16 — **Cap external API payloads before sending:**

Validate payload size client-side against the provider's documented limit (e.g.
GitHub issue body ≤ 65536 chars) and truncate to ≤ 90% of it with a
`[truncated N chars]` marker — never rely on the provider's 4xx. A test
asserting `payload.len() <= budget` is required for every such code path
(#1375).

<a id="rule-17"></a>

## Rule 17 — **Survey existing tooling before building any:**

Check whether the repo already provides it — the `limerick-harness` binary
(built-in web server/dashboard), `justfile` recipes, `limerick/scripts/**`,
`.claude/skills/` — and run or extend the existing tool in place. Never
hand-roll a throwaway duplicate.

<a id="rule-18"></a>

## Rule 18 — **Report only verification you actually ran:**

Any claim that a test passed or a process ran must be backed by literal command
output from the current session. State skips and failures explicitly — never
estimate, extrapolate, or fabricate a result.

<a id="rule-26"></a>

## Rule 26 — **Keep shared-target artifacts worktree-coherent:**

When a build script embeds worktree-local files, parallel test tooling that
shares a Cargo target must key the build to those inputs and preserve and
validate the resulting executable before releasing its coordination lock.
Never launch the shared final binary after Cargo releases its own lock; use the
Playwright managed-server helper as the reference (#1717).

<a id="rule-27"></a>

## Rule 27 — **Make managed-test lifecycles crash-safe on every platform:**

Locks, candidates, and copied executables must have bounded startup recovery
and mechanically tested platform policy. A fresh active-use lease is a
capability: its heartbeat owner must fence and stop the child if ownership is
lost, while a pruner must preserve lazily read artifacts and fail closed on
malformed state. Retirement must atomically replace a stale lease with a
tombstone, preserve its artifacts for another full grace, and reclaim only in a
later pass. Test the real process-manager paths: POSIX group signals must leave
the launcher alive to stop/wait the child and release ownership; Windows
`taskkill /T /F` may skip hooks, so bounded expiry is the required fallback
(#1717).

<a id="rule-28"></a>

## Rule 28 — **Preserve merged end-to-end contracts when extending or extracting a shared spec:**

Retain every already-merged public-behavior assertion across API, IPC,
gameplay, and UI surfaces, and add new coverage alongside it; bounded behavior
must cover the cap boundary and overflow. A focused test for the new slice does
not prove earlier consumer contracts still exist.

<a id="rule-29"></a>

## Rule 29 — **Audit player-facing interaction models, not only controls:**

Scope note: the illustrated-notebook audit applies to the existing web/Tauri
surface. Mobile feature work follows its product physical-iPhone UX gates and
does not restore the notebook object; this note does not weaken semantic
interaction contracts for a mobile surface. In the preserved rule text below,
“desktop and mobile surface” means the existing web/Tauri client at desktop and
mobile viewport sizes, not the native SwiftUI client. Its native-iPhone
interaction audit is defined by the [product specifications](../product-specs/README.md).

Before declaring a material visual UI change complete, run [the
illustrated-notebook UX audit](../../.github/prompts/rundale-illustrated-notebook-ux-audit.prompt.md)
against the live desktop and mobile surface. A label must lead to its ordinary
semantic destination; persistent-object navigation (such as notebook tabs)
renders in that object, visibly distinct controls have visibly distinct
outcomes, and any deliberate overlay preserves the object’s visual and task
continuity. Record the interaction model, screenshots/recording, findings, and
semantic Playwright coverage. Functional tests that only assert that an overlay
opened are insufficient.
