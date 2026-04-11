# ADR-012: Hierarchical Documentation Organization

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted

## Date

2026-03-22

## Context

Rundale documentation grew organically across multiple development sessions. By the end of Phase 3, several problems had emerged:

1. **Status drift**: Phase completion status was tracked in multiple places (README.md, docs/index.md, roadmap.md, individual phase plans) and had fallen out of sync. Phase 3 was complete but still shown as "Planned" in some documents.
2. **Module tree drift**: The source module tree in CLAUDE.md and the architecture overview hadn't been updated as new modules were added in Phases 2-3 (npc submodules, debug.rs, geo_tool, gui submodules).
3. **Flat navigation**: While docs/index.md existed as a hub, it wasn't clear which document to read first for a given concern. Agents and developers had to read multiple documents to find the right entry point.
4. **Missing cross-references**: The research/ directory wasn't linked from the index. Known issues referenced plans that were already complete.
5. **Archival content mixed with active**: The original DESIGN.md and completed "maybe bad ideas" sat alongside active documentation without clear separation.

These problems are especially costly for AI agents working with context limits — every unnecessary document read wastes context window space.

## Decision

Organize documentation in a strict hierarchy optimized for progressive disclosure:

### Layer 1: Entry Points (minimal context needed)
- **README.md** — Project overview, quick start, documentation tree diagram
- **CLAUDE.md** — Agent quick reference: build commands, standards, module tree, gotchas

### Layer 2: Navigation Hub
- **docs/index.md** — Single source of truth for phase status, links to all documents organized by category, with "Key Design Docs" column linking phases to relevant design docs

### Layer 3: Status & Planning
- **docs/requirements/roadmap.md** — Authoritative per-item status (checkboxes)
- **docs/plans/*.md** — Detailed implementation plans per phase (each has its own Status header)
- **docs/plans/open-questions.md** — Design decisions (all resolved)

### Layer 4: Design & Architecture
- **docs/design/overview.md** — Architecture overview with complete module tree, links to all subsystem docs
- **docs/design/*.md** — 14 subsystem design documents

### Layer 5: Decisions & Reference
- **docs/adr/README.md** — ADR index with template
- **docs/adr/*.md** — Individual architecture decision records
- **docs/research/*.md** — Historical research informing design

### Layer 6: Development Operations
- **docs/journal.md** — Cross-session development notes
- **docs/known-issues.md** — Active bugs (with severity and current state)
- **docs/maybe-bad-ideas.md** — Ideas under consideration (shipped items separated)

### Status reconciliation rules
1. **roadmap.md** is the authoritative source for per-item completion status
2. **docs/index.md** phase table must match roadmap.md summary status
3. **README.md** shows only the current-phase summary sentence
4. Each **phase plan** has a Status header that must match roadmap.md
5. When a phase is completed, all four locations must be updated in the same commit

### Navigation rules
1. Every document (except README.md) has a breadcrumb back to docs/index.md
2. docs/index.md links every phase to its relevant design docs (not just the plan)
3. Design docs link to related ADRs and vice versa
4. CLAUDE.md module tree must match the architecture overview module tree

## Consequences

### Positive
- Agents can find information in 1-2 document reads instead of scanning multiple files
- Status is consistent across all documents (single source of truth in roadmap.md)
- Module trees accurately reflect the actual source code
- Progressive disclosure means agents only load detailed docs when needed

### Negative
- Maintaining cross-references adds overhead to every documentation update
- The reconciliation rules require discipline — status must be updated in multiple places per commit
- New contributors must learn the hierarchy (mitigated by the tree diagram in README.md)

## Alternatives Considered

1. **Single monolithic document** — The original DESIGN.md approach. Rejected: too large for context-limited agents, hard to keep current.
2. **Wiki-style flat pages** — All docs at one level with tags/search. Rejected: no natural reading order, agents can't search efficiently.
3. **Auto-generated from code** — Generate docs from doc comments. Rejected: design rationale and status tracking can't be derived from code.

## Related

- [docs/index.md](../index.md) — The documentation hub this ADR describes
- [DESIGN.md](../../DESIGN.md) — The original monolithic document this hierarchy replaces
