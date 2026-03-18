# Player Input & Command System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Natural Language Input

The primary interaction is undecorated natural language text. The player just types and the game figures out intent via LLM parsing.

Examples:

- "Go to the pub"
- "Tell Mary I saw her husband at the crossroads"
- "Look around"
- "Pick up the stone"

## System Commands

System commands use `/` prefix for now (placeholder — may change to a prefix-free autocomplete system later).

**Target UX (future)**: No prefix at all. The system detects exact/fuzzy matches against a small fixed command set and shows an inline confirmation prompt: "Quit the game? y/n". If the player says no, the input passes through to the game world. False positives are harmless because of the confirmation step.

### Command List

| Command        | Description                                                             |
|----------------|-------------------------------------------------------------------------|
| `/pause`       | Freeze all simulation ticks, TUI stays up                              |
| `/resume`      | Unfreeze simulation                                                     |
| `/quit`        | Persist current state, clean shutdown                                   |
| `/save`        | Manual snapshot to current branch                                       |
| `/fork <name>` | Snapshot current state, create new named branch, continue on new branch |
| `/load <name>` | Load a branch head, resume from that point                              |
| `/branches`    | List all branches with timestamps and brief context                     |
| `/log`         | Show history of current branch (git log style)                          |
| `/status`      | Current branch name, in-game date, play time, NPC count by tier         |
| `/help`        | Show help reference                                                     |
| `/map`         | (Future) Simple ASCII parish layout                                     |

## Related

- [Inference Pipeline](inference-pipeline.md) — Player input parsed via LLM intent detection
- [Persistence](persistence.md) — /save, /fork, /load, /branches, /log commands
- [ADR 006: Natural Language Input](../adr/006-natural-language-input.md)

## Source Modules

- [`src/input/`](../../src/input/) — Player input parsing, command detection
- [`src/inference/`](../../src/inference/) — LLM-based intent parsing
