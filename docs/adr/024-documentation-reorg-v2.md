# ADR-024: Documentation Reorganization v2

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted

## Date

2026-05-25

## Context

[ADR-012](012-documentation-hierarchy.md) established a layered documentation
hierarchy at the end of Phase 3. By mid-2026 the tree had drifted again:

1. **`docs/index.md` was badly stale** — it described the project as "Phases 1–3
   complete, Phase 4 next," its ADR table stopped at 017, and it linked only
   about half of the design docs and a third of the plans. The actual project
   had shipped the Tauri GUI, web server, cloud + MLX inference, the Parish
   Designer, rundale-bench, and demo mode.
2. **ADR number collision** — three files shared the number `018`
   (`npc-intelligence-dimensions`, `engine-config-extraction`,
   `web-testing-server`); two were absent from both ADR indexes.
3. **The linear phase model no longer matched reality** — Phase 8 (Tauri GUI)
   was marked "Planned" while the whole app ran on it, and there were two
   different "Phase 9"s. Shipped work spanning many subsystems in parallel did
   not fit a single phase pointer.
4. **`design/` and `plans/` had blurred purposes** — `design/` mixed durable
   subsystem reference with brainstorm/RFC idea dumps and even a couple of
   implementation plans; `plans/` mixed historical phase plans with active
   feature plans. Only a minority of design docs carried any status marker, and
   plans used five different status words.

## Decision

1. **Purpose-based folders.**
   - `design/` holds only durable subsystem reference (how a shipped or extant
     system works).
   - `design/ideas/` holds brainstorms, RFCs, and speculative proposals.
   - `plans/` holds active implementation plans.
   - `plans/archive/` holds completed or historical plans, including the linear
     phase plans.
2. **A status header on every design and plan doc**, as the first blockquote
   after the H1:
   `> Status: <Status> · Updated: <date> · [Docs Index](<rel>)`
   - Design vocabulary: `Implemented`, `Partial`, `Proposed`, `Brainstorm`,
     `Superseded`.
   - Plan vocabulary: `Complete`, `In progress`, `Planned`, `Proposed`,
     `Abandoned`.
   - ADRs keep their existing `## Status` field.
3. **The roadmap is a feature-status matrix**, not a linear phase pointer. Each
   subsystem/feature row carries a status, its primary design doc, and related
   ADRs. The historical phases are preserved as a provenance table linking the
   archived plans.
4. **`docs/index.md` is the exhaustive hub** — every file under `design/`,
   `design/ideas/`, `plans/`, `plans/archive/`, and `adr/` is linked, grouped by
   purpose and annotated with status.
5. **ADR numbers are unique.** The two misnumbered `018` ADRs were renumbered to
   022 and 023.

## Consequences

### Positive

- The design-vs-plans distinction is now structural (folder = purpose), so it is
  obvious where a new document belongs.
- The index and roadmap reflect what actually shipped; agents and contributors
  can trust them as the authoritative map.
- Status headers make completeness visible at a glance without opening each doc.

### Negative

- Moving files broke many relative links, which had to be repaired in one pass.
- The status headers and the exhaustive index require discipline to keep current
  as docs are added.

## Alternatives Considered

1. **Index-only cleanup** — fix the hub and ADR indexes but leave files in place.
   Rejected: leaves `design/` mixing reference and brainstorms, so the user's
   core "how is this split?" problem persists.
2. **Tag-in-place** — add status headers and rebuild the index but keep the
   existing flat folders. Rejected for the same reason: lower churn, but the
   purpose ambiguity remains.
3. **Keep the linear phase model** — correct statuses within the Phase 1–9
   framing. Rejected: parallel shipped work (web, MLX, bench, designer) does not
   fit a linear story, and the framing was itself the source of drift.

## Related

- [ADR-012](012-documentation-hierarchy.md) — the v1 hierarchy this ADR amends
- [docs/index.md](../index.md) — the rebuilt hub
- [docs/requirements/roadmap.md](../requirements/roadmap.md) — the feature-status matrix
