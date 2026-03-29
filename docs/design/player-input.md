# Player Input & Command System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [006](../adr/006-natural-language-input.md)

## Natural Language Input

The primary interaction is undecorated natural language text. The player just types and the game figures out intent via LLM parsing.

Examples:

- "Go to the pub"
- "Tell Mary I saw her husband at the crossroads"
- "Look around"
- "Pick up the stone"

## NPC @Mention Targeting

Players can direct dialogue to a specific NPC using `@mention` syntax at the start of their input, similar to chat apps like Slack or Discord.

Examples:

- `@Padraig how's business?` — talks to Padraig Darcy
- `@Siobhan tell me about the harvest` — talks to Siobhan Murphy
- `hello everyone` — talks to the first NPC present (default behavior)

### Autocomplete

When the player types `@` in the input field (Tauri GUI), an autocomplete dropdown appears listing all NPCs at the current location. The dropdown:

- Filters as the player types (e.g., `@Pa` narrows to "Padraig Darcy")
- Supports keyboard navigation (Arrow keys, Tab/Enter to select, Escape to dismiss)
- Shows NPC name and occupation (if introduced)
- Inserts `@FirstName` into the input on selection

### Name Matching

The backend matches `@mentions` case-insensitively against NPC display names:

- **Exact match**: `@Padraig Darcy` matches the full name
- **First-name match**: `@Padraig` matches by first name
- **Brief description match**: Un-introduced NPCs can be targeted by their brief description
- **Fallback**: If no match is found, the first NPC at the location is used

### Source

- `parish_core::input::extract_mention()` — Extracts `@name` from input
- `NpcManager::find_by_name()` — Case-insensitive NPC lookup at a location
- `InputField.svelte` — Autocomplete dropdown UI

## System Commands

System commands use `/` prefix for now (placeholder — may change to a prefix-free autocomplete system later).

**Target UX (future)**: No prefix at all. The system detects exact/fuzzy matches against a small fixed command set and shows an inline confirmation prompt: "Quit the game? y/n". If the player says no, the input passes through to the game world. False positives are harmless because of the confirmation step.

### Command List

| Command        | Description                                                             |
|----------------|-------------------------------------------------------------------------|
| `/pause`       | Freeze all simulation ticks, TUI stays up                              |
| `/resume`      | Unfreeze simulation                                                     |
| `/speed [preset]` | Show or set game speed (`slow`/`normal`/`fast`/`fastest`/`ludicrous`) |
| `/quit`        | Persist current state, clean shutdown                                   |
| `/save`        | Manual snapshot to current branch                                       |
| `/fork <name>` | Snapshot current state, create new named branch, continue on new branch |
| `/load <name>` | Load a branch head, resume from that point                              |
| `/branches`    | List all branches with timestamps and brief context                     |
| `/log`         | Show history of current branch (git log style)                          |
| `/status`      | Current branch name, in-game date, play time, NPC count by tier         |
| `/help`        | Show help reference                                                     |
| `/map`         | (Future) Simple ASCII parish layout                                     |

## Debug Commands

> Requires `--features debug`. See [Debug System](debug-system.md) for full details.

Debug commands use the same `/` prefix as system commands. All are feature-gated and compile out of release builds.

| Command | Description |
|---------|-------------|
| `/debug npcs` | List all NPCs with location, mood, activity, tier |
| `/debug npc <name\|id>` | Full state dump for a single NPC |
| `/debug inference` | Queue status, throughput, recent request previews |
| `/debug tiers` | NPC tier assignments and last tick times |
| `/debug world` | Game clock, weather, season, player location |
| `/debug tasks` | Background task health and error counts |
| `/debug log [subsystem]` | Recent tracing log entries by subsystem |
| `/debug perf` | Frame time, inference latency percentiles |
| `/debug panel` | Toggle live debug panel overlay |

## Related

- [Inference Pipeline](inference-pipeline.md) — Player input parsed via LLM intent detection
- [Persistence](persistence.md) — /save, /fork, /load, /branches, /log commands
- [ADR 006: Natural Language Input](../adr/006-natural-language-input.md)

## Source Modules

- [`src/input/`](../../src/input/) — Player input parsing, command detection
- [`src/inference/`](../../src/inference/) — LLM-based intent parsing
