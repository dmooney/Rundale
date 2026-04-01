# Design: `/slash` Command Autocomplete

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #1

## Overview

When the player types `/` in the input field, show an autocomplete dropdown listing all available system commands. The dropdown filters as the player continues typing, displays argument hints, and allows keyboard/mouse selection. This reuses the existing `@mention` dropdown infrastructure in `InputField.svelte`.

## User-Facing Behavior

1. Player types `/` — dropdown appears above input with all commands
2. Player continues typing `/sp` — list filters to `/speed`, `/save`, `/spinner`
3. Player presses ArrowUp/ArrowDown to navigate, Enter or Tab to select
4. Selected command text replaces the `/...` in the input field
5. Commands with arguments show a hint: `/fork <name>`, `/load <name>`
6. Escape dismisses the dropdown
7. Debug commands (`/debug`) only appear when debug features are active

## Command Registry

A static array of command descriptors drives both the dropdown and help text. Defined in the frontend since this is a purely cosmetic feature — the backend already parses commands independently.

```typescript
// ui/src/lib/commands.ts (new file)

export interface CommandDescriptor {
  name: string;           // e.g. "/save"
  description: string;    // e.g. "Manual snapshot to current branch"
  args?: string;          // e.g. "<name>" — shown as hint
  debug?: boolean;        // only show when debug mode is active
}

export const COMMANDS: CommandDescriptor[] = [
  { name: '/pause',    description: 'Freeze all simulation ticks' },
  { name: '/resume',   description: 'Unfreeze simulation' },
  { name: '/quit',     description: 'Save and exit' },
  { name: '/save',     description: 'Manual snapshot to current branch' },
  { name: '/fork',     description: 'Create a new named save branch', args: '<name>' },
  { name: '/load',     description: 'Load a named save branch', args: '<name>' },
  { name: '/branches', description: 'List all save branches' },
  { name: '/log',      description: 'History of current branch' },
  { name: '/status',   description: 'Show current game status' },
  { name: '/help',     description: 'Show help text' },
  { name: '/irish',    description: 'Toggle Irish pronunciation sidebar' },
  { name: '/improv',   description: 'Toggle improv craft mode for NPC dialogue' },
  { name: '/provider', description: 'Show or set LLM provider', args: '[name]' },
  { name: '/model',    description: 'Show or set model name', args: '[name]' },
  { name: '/key',      description: 'Show or set API key', args: '[key]' },
  { name: '/cloud',    description: 'Show cloud provider info', args: '[provider key]' },
  { name: '/speed',    description: 'Set game speed', args: '<slow|normal|fast|ludicrous>' },
  { name: '/about',    description: 'Show credits' },
  { name: '/debug',    description: 'Debug commands', args: '[subcommand]', debug: true },
  { name: '/spinner',  description: 'Show loading spinner', args: '<seconds>', debug: true },
];
```

### Per-Category Commands

The per-category dot-notation commands (`/provider.dialogue`, `/model.simulation`, etc.) are generated dynamically from a base set crossed with three categories:

```typescript
export const CATEGORIES = ['dialogue', 'simulation', 'intent'] as const;

export function allCommands(debugActive: boolean): CommandDescriptor[] {
  const base = debugActive ? COMMANDS : COMMANDS.filter(c => !c.debug);
  const categoryCommands: CommandDescriptor[] = [];
  for (const cat of CATEGORIES) {
    categoryCommands.push(
      { name: `/provider.${cat}`, description: `Show/set provider for ${cat}`, args: '[name]' },
      { name: `/model.${cat}`,    description: `Show/set model for ${cat}`, args: '[name]' },
      { name: `/key.${cat}`,      description: `Show/set API key for ${cat}`, args: '[key]' },
    );
  }
  return [...base, ...categoryCommands];
}
```

## Frontend Changes

### InputField.svelte

Add a second trigger detection path alongside the existing `@mention` system:

```
State additions:
  showSlash: boolean       — controls slash dropdown visibility
  slashQuery: string       — text after the `/` for filtering
  slashSelectedIndex: number — keyboard navigation index
```

#### Trigger Detection

Add `findSlashTrigger()` alongside existing `findMentionTrigger()`:

```typescript
function findSlashTrigger(): { query: string } | null {
  const text = getPlainText();
  // Only trigger if `/` is at the very start of the input
  if (!text.startsWith('/')) return null;
  const query = text.slice(1); // everything after `/`
  // Don't trigger if there's a space (command is complete, typing args)
  if (query.includes(' ')) return null;
  return { query };
}
```

Modify `detectMention()` → `detectDropdowns()`:

```typescript
function detectDropdowns() {
  // Check slash first (takes priority when typing starts with /)
  const slashTrigger = findSlashTrigger();
  if (slashTrigger !== null) {
    slashQuery = slashTrigger.query;
    showSlash = true;
    showMentions = false;
    slashSelectedIndex = 0;
    return;
  }
  showSlash = false;

  // Existing @mention detection
  const trigger = findMentionTrigger();
  if (trigger !== null && $npcsHere.length > 0) {
    mentionQuery = trigger.query;
    showMentions = true;
    selectedIndex = 0;
  } else {
    showMentions = false;
  }
}
```

#### Keyboard Navigation

Extend `handleKeydown()` to handle slash dropdown when `showSlash` is true — same pattern as mention dropdown (ArrowUp/ArrowDown/Tab/Enter/Escape).

#### Selection Behavior

When the player selects a command:

1. Replace the entire editor content with the command name (e.g., `/fork`)
2. If the command has `args`, append a space so the player can start typing the argument
3. Place cursor at end
4. Close the dropdown

```typescript
function selectCommand(cmd: CommandDescriptor) {
  if (!editorEl) return;
  const text = cmd.args ? `${cmd.name} ` : cmd.name;
  editorEl.textContent = text;
  // Place cursor at end
  const range = document.createRange();
  const sel = window.getSelection();
  range.selectNodeContents(editorEl);
  range.collapse(false);
  sel?.removeAllRanges();
  sel?.addRange(range);
  showSlash = false;
  editorEl.focus();
}
```

#### Derived Filtered List

```typescript
const filteredCommands = $derived(() => {
  const cmds = allCommands(debugActive);
  if (slashQuery === '') return cmds;
  const q = slashQuery.toLowerCase();
  return cmds.filter(c => c.name.toLowerCase().startsWith('/' + q));
});
```

### Dropdown UI

Render the slash dropdown in the same position as the mention dropdown, using the same visual pattern:

```svelte
{#if showSlash && filteredCommands.length > 0}
  <ul class="slash-dropdown" role="listbox" aria-label="Commands">
    {#each filteredCommands as cmd, i}
      <li
        role="option"
        aria-selected={i === slashSelectedIndex}
        class="slash-item"
        class:selected={i === slashSelectedIndex}
        onmousedown={(e) => { e.preventDefault(); selectCommand(cmd); }}
        onmouseenter={() => (slashSelectedIndex = i)}
      >
        <span class="slash-name">{cmd.name}</span>
        {#if cmd.args}
          <span class="slash-args">{cmd.args}</span>
        {/if}
        <span class="slash-desc">{cmd.description}</span>
      </li>
    {/each}
  </ul>
{/if}
```

### CSS

Reuse `.mention-dropdown` positioning but with command-specific styling:

```css
.slash-dropdown {
  /* Same positioning as .mention-dropdown */
  position: absolute;
  bottom: 100%;
  left: 0.75rem;
  right: 0.75rem;
  /* ... same base styles ... */
}

.slash-item {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  padding: 0.4rem 0.75rem;
  cursor: pointer;
  font-size: 0.9rem;
}

.slash-name {
  font-weight: 600;
  font-family: monospace;
  color: var(--color-accent);
}

.slash-args {
  font-size: 0.8rem;
  font-family: monospace;
  opacity: 0.6;
}

.slash-desc {
  font-size: 0.78rem;
  color: var(--color-muted);
  margin-left: auto;
}
```

## Backend Changes

**None.** The command registry is purely frontend. The backend already parses commands in `crates/parish-core/src/input/mod.rs:parse_system_command()`. The autocomplete is cosmetic — it helps the player type valid commands but doesn't change how they're processed.

### Future: Server-Driven Registry

If commands become mod-driven or change at runtime, add a `GET /api/commands` endpoint returning `Vec<CommandDescriptor>`. For now, a static frontend list is simpler and avoids an extra IPC round-trip.

## Data Flow

```
Player types "/"
    → detectDropdowns() detects slash trigger
    → showSlash = true, slashQuery = ""
    → Dropdown renders with all commands

Player types "/sp"
    → slashQuery = "sp"
    → filteredCommands = ["/speed", "/spinner", "/save"]

Player presses ArrowDown twice, then Enter
    → selectCommand({ name: "/spinner", args: "<seconds>" })
    → Editor content becomes "/spinner "
    → Cursor at end, ready to type seconds argument

Player types "5" and hits Enter
    → handleSubmit sends "/spinner 5"
    → Backend classify_input → SystemCommand(Spinner(5))
```

## Edge Cases

| Case | Behavior |
|------|----------|
| `/` followed immediately by space | Close dropdown (space = query has spaces) |
| `/` in middle of text | No trigger — slash must be at position 0 |
| `/` with @mention chip before it | `getPlainText()` starts with `@Name/...` — no trigger |
| Empty filter result | Dropdown hidden (no matches) |
| Debug commands when debug inactive | Filtered out by `allCommands(false)` |
| Very long command list | Dropdown has `max-height: 12rem; overflow-y: auto` (same as mentions) |
| Player pastes `/save` | `handleInput` → `detectDropdowns()` fires, shows filtered list |

## Testing

### Frontend (Vitest)

1. **Trigger detection**: Type `/` → dropdown appears; type `a` at start → no dropdown
2. **Filtering**: Type `/sp` → only commands starting with `/sp` shown
3. **Keyboard navigation**: ArrowDown increments `slashSelectedIndex`
4. **Selection**: Tab/Enter inserts command text, closes dropdown
5. **Escape**: Closes dropdown without changing input
6. **Args hint**: `/fork` selection inserts `/fork ` (trailing space)
7. **No conflict with @mention**: `@Padraig /foo` — no slash dropdown (not at pos 0)

### Manual

- Visual check: dropdown appears above input, not overlapping chat
- All commands present, descriptions readable
- Debug commands hidden by default, shown with debug feature

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/lib/commands.ts` | **New** — command registry |
| `ui/src/components/InputField.svelte` | Add slash trigger detection, dropdown, keyboard handling |
| `ui/src/components/InputField.test.ts` | Add slash autocomplete tests |

## Effort Estimate

**Low** — 90% of the infrastructure (dropdown, keyboard nav, positioning) already exists in the @mention system. The main new work is the command registry and a second trigger path.
