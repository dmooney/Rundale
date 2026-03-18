# ADR-004: Git-Like Branching Save System

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Parish aims for an invisible save system where the player never thinks about saving. They quit whenever they want, load whenever they want, and the world is exactly where they left it. Beyond basic persistence, the design calls for the ability to fork and explore alternate timelines: "What if I had told Mary the truth instead?"

This requires a save system that supports:

- Automatic, continuous persistence with no player intervention
- Named save branches that can be created, switched between, and listed
- Independent clocks per branch (time does not pass in unplayed branches)
- Clean switching between branches without data corruption
- A mental model that is intuitive enough for players to use without confusion

## Decision

Adopt a **git-like branching model** for the save system, mapping persistence concepts to familiar version control analogies:

| Save System Concept | Git Analogy |
|---------------------|-------------|
| Journal (real-time mutation log) | Working directory |
| Snapshot (periodic compaction) | Commit |
| Branch (named save) | Branch |
| Fork (create alternate timeline) | `git checkout -b` |
| Load (switch to a different timeline) | `git checkout` |

**Behavior:**

- **Autosave on quit**: When the player quits, the current journal is flushed and a final snapshot is taken. No explicit save action needed.
- **Fork**: Creates a snapshot of the current state and starts a new named branch. The original branch is preserved at its current point.
- **Load**: Switches to a different branch's snapshot and journal. The current branch is auto-saved first.
- **Independent clocks**: Each branch maintains its own in-game time. No time passes in branches that are not being played.
- **Background persistence**: A dedicated thread handles snapshot compaction on a background CPU core.

**Player-facing commands:**

- `/save` -- Manual snapshot to current branch
- `/fork <name>` -- Snapshot and create new named branch
- `/load <name>` -- Load a branch head
- `/branches` -- List all branches with timestamps and context
- `/log` -- Show history of current branch

## Consequences

**Positive:**

- Transparent to the player: quitting and resuming "just works"
- Timeline exploration enables experimentation without fear of losing progress
- The git analogy provides a well-understood mental model for branching
- No save management burden on the player for normal play
- Each branch is self-contained with its own clock, avoiding temporal paradoxes

**Negative:**

- Storage grows with each branch (each fork creates a full snapshot copy)
- Branch-aware queries add complexity to the persistence layer
- Branch switching must be atomic: a crash during switch could leave the system in an inconsistent state
- Players unfamiliar with version control may find branching confusing
- Need garbage collection or branch deletion to manage long-term storage growth

## Alternatives Considered

- **Single save slot**: Simplest implementation but precludes timeline branching entirely. No way to explore "what if" scenarios without losing progress.
- **Manual save points**: Traditional approach where the player explicitly saves. Breaks the immersion goal of invisible persistence. Players forget to save and lose progress.
- **Multiple manual save files**: Gives the player branching power but places the management burden on them. Naming, organizing, and remembering which save is which becomes tedious.
- **Automatic checkpoints only**: Periodic snapshots without branching. Simple but loses the timeline exploration feature.

## Related

- [docs/design/persistence.md](../design/persistence.md)
- [ADR-003: SQLite WAL Persistence](003-sqlite-wal-persistence.md)
