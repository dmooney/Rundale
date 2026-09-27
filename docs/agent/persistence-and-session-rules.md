# Persistence and Session Rules

These rules cover canonical state, save/session lifecycle, restore behavior,
and continuity across runtimes. Stable numbers are retained from the root rule
block. See [engineering-rules.md](engineering-rules.md) for the complete map.

<a id="rule-9"></a>

## Rule 9 — **Resolve runtime paths from explicit config, not the cwd:**

Resolve saves/mods/data dirs once at startup and store on `AppState` /
`GlobalState`; never `current_dir()`, parent-walks, or marker-file searches
from request handlers (#771). Resolver APIs, platform roots, and env overrides:
[docs/agent/gotchas.md](gotchas.md).

<a id="rule-30"></a>

## Rule 30 — **Ground background simulation in canonical state (enforced):**

LLM simulation prompts must carry each participant's exact current location and
authored current activity. Every async result must retain immutable fingerprints
plus a monotonic participant-lineage revision from the canonical snapshot used
to generate it, then revalidate those anchors at the one public shared apply
seam; stale, restored-branch, ABA, missing-anchor, and
contradictory-known-location results must have zero side effects. Authored fallback schedules must
be semantically valid in every season and day type where they can resolve;
unscoped activity prose must remain season- and day-neutral (#1785, #1831).

<a id="rule-31"></a>

## Rule 31 — **Preserve live play signals and isolate replacement contexts:**

The first viewport must visibly retain player input, narration, streamed NPC
dialogue, location changes, and state-derived notebook content. Successful
new-game/branch/reconnect replacement clears prior prompt, presentation,
dedup, and retained-event state while preserving a lifetime-monotonic cursor;
failure preserves the old context (#1774, #1778, #1782, #1783).

<a id="rule-32"></a>

## Rule 32 — **Player-visible gameplay status comes from authoritative state:**

Consequential player actions must mutate canonical engine state, publish
semantic events, survive save/load and journal recovery, and flow through
shared IPC in every runtime. Local UI drafts and production placeholders must
not masquerade as progression (#1781).

<a id="rule-34"></a>

## Rule 34 — **Commit save/session identity before publishing a runtime:**

Acquire a candidate save lock before any SQLite open, persist a complete
candidate and atomic active-identity marker, then perform only infallible
live-state publication. Cold restore/create must single-flight and recheck
under one lifecycle gate; lock, marker, registry, or recovery failure must not
fall back to another ledger, publish a session, or start workers.

<a id="rule-42"></a>

## Rule 42 — **Player-earned knowledge is durable state:**

Introductions and other knowledge the player legitimately learns must survive
save/load, branch restore, reconnect, cold server restore, Tauri restore, and
headless restore. Only an explicit new-game/reset boundary may clear it. When
adding durable knowledge, cover serialization plus every production restore
lifecycle; compatibility repair may only infer facts from retained evidence
that satisfies the same canonical apply guard.
