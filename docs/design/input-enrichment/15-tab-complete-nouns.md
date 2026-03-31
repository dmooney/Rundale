# Design: Tab-Complete for Known Nouns

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #15

## Overview

Press Tab to cycle through completable tokens based on game context: location names, NPC names, known objects, and system commands. Unlike `@mention` (which only triggers on `@`) and `/slash` (which only triggers on `/`), tab-completion works on any partial word the player is typing.

## User-Facing Behavior

1. Player types `pub` and presses Tab → text completes to `Darcy's Pub`
2. Player types `Padr` and presses Tab → text completes to `Padraig`
3. Player types `cross` and presses Tab → text completes to `The Crossroads`
4. Pressing Tab again cycles to the next match if there are multiple
5. Pressing any other key accepts the current completion and resumes normal typing
6. If no match is found, Tab does nothing (no beep, no visual change)

## Known Nouns Registry

A unified registry of completable tokens, assembled from multiple game data sources.

### Frontend Store — `ui/src/stores/nouns.ts`

```typescript
import { derived } from 'svelte/store';
import { mapData, npcsHere, worldState } from './game';

export interface KnownNoun {
  text: string;       // the completable string
  category: 'location' | 'npc' | 'command';
  priority: number;   // lower = higher priority (for sort order)
}

/**
 * All known nouns derived from current game state.
 * Updates automatically when mapData, npcsHere, or worldState change.
 */
export const knownNouns = derived(
  [mapData, npcsHere],
  ([$mapData, $npcsHere]) => {
    const nouns: KnownNoun[] = [];

    // Locations (all known, not just adjacent)
    if ($mapData) {
      for (const loc of $mapData.locations) {
        nouns.push({
          text: loc.name,
          category: 'location',
          priority: loc.adjacent ? 0 : 2,  // adjacent locations rank higher
        });
      }
    }

    // NPCs at current location
    for (const npc of $npcsHere) {
      nouns.push({
        text: npc.name,
        category: 'npc',
        priority: 1,
      });
    }

    // Sort by priority, then alphabetically
    nouns.sort((a, b) => a.priority - b.priority || a.text.localeCompare(b.text));
    return nouns;
  }
);
```

### Future Expansion: Objects and Inventory

When the game adds interactable objects or inventory, those nouns join the registry:

```typescript
// Future:
// { text: "stone cross", category: "object", priority: 1 }
// { text: "old map", category: "object", priority: 1 }
```

For now, locations and NPC names cover the most common completion needs.

## Tab-Completion Algorithm

### Matching Logic

When Tab is pressed, find the word being typed (the "prefix") and match it against known nouns:

```typescript
interface CompletionState {
  active: boolean;           // currently cycling completions
  prefix: string;            // the original text before first Tab
  matches: KnownNoun[];      // all matching nouns
  currentIndex: number;      // which match is currently shown
  prefixStart: number;       // character offset where the prefix starts in the full text
}

function findMatches(prefix: string, nouns: KnownNoun[]): KnownNoun[] {
  if (prefix.length === 0) return [];
  const lower = prefix.toLowerCase();

  return nouns.filter(noun => {
    const nounLower = noun.text.toLowerCase();
    // Match if prefix appears at start of any word in the noun
    // e.g., "pub" matches "Darcy's Pub", "cross" matches "The Crossroads"
    return nounLower.startsWith(lower) ||
           nounLower.split(/[\s']+/).some(word => word.startsWith(lower));
  });
}
```

### Word Extraction

Extract the word being typed from the cursor position backward:

```typescript
function extractPrefix(): { prefix: string; start: number } | null {
  const text = getPlainText();
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;

  // Get cursor position in plain text
  const range = sel.getRangeAt(0);
  const node = range.startContainer;
  if (node.nodeType !== Node.TEXT_NODE) return null;

  const fullText = node.textContent ?? '';
  const cursorPos = range.startOffset;

  // Walk backward from cursor to find word start
  let start = cursorPos;
  while (start > 0 && fullText[start - 1] !== ' ' && fullText[start - 1] !== '\n') {
    start--;
  }

  const prefix = fullText.slice(start, cursorPos);
  if (prefix.length === 0) return null;

  return { prefix, start };
}
```

## Frontend Changes

### InputField.svelte

Add completion state and Tab handling:

```typescript
import { get } from 'svelte/store';
import { knownNouns } from '../stores/nouns';

let completion = $state<CompletionState>({
  active: false,
  prefix: '',
  matches: [],
  currentIndex: 0,
  prefixStart: 0,
});

function resetCompletion() {
  completion = {
    active: false,
    prefix: '',
    matches: [],
    currentIndex: 0,
    prefixStart: 0,
  };
}
```

#### Modified `handleKeydown()`

Tab handling integrated into the existing keydown handler:

```typescript
// In handleKeydown(), after mention/slash dropdown handling:

if (e.key === 'Tab') {
  // Don't interfere with mention dropdown Tab selection
  if (showMentions && filteredNpcs.length > 0) {
    // existing mention select behavior
    return;
  }

  e.preventDefault();

  if (completion.active) {
    // Already cycling — advance to next match
    completion.currentIndex =
      (completion.currentIndex + 1) % completion.matches.length;
    applyCompletion();
    return;
  }

  // Start new completion
  const extracted = extractPrefix();
  if (!extracted) return;

  const nouns = get(knownNouns);
  const matches = findMatches(extracted.prefix, nouns);
  if (matches.length === 0) return;

  completion = {
    active: true,
    prefix: extracted.prefix,
    matches,
    currentIndex: 0,
    prefixStart: extracted.start,
  };
  applyCompletion();
  return;
}

// Any other key press while completing → accept completion and reset
if (completion.active && e.key !== 'Shift') {
  resetCompletion();
  // Don't prevent default — let the key be typed normally
}
```

#### Apply Completion

Replace the prefix text with the selected match:

```typescript
function applyCompletion() {
  if (!editorEl || !completion.active) return;

  const match = completion.matches[completion.currentIndex];
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return;

  const range = sel.getRangeAt(0);
  const node = range.startContainer;
  if (node.nodeType !== Node.TEXT_NODE) return;

  const text = node.textContent ?? '';
  const before = text.slice(0, completion.prefixStart);
  const after = text.slice(completion.prefixStart + completion.prefix.length);

  // On subsequent Tab presses, we need to replace the previously completed text
  // Track the current completion length to know what to replace
  const currentCompletionLen = completion.currentIndex === 0 && !completion.active
    ? completion.prefix.length
    : (completion.matches[(completion.currentIndex - 1 + completion.matches.length) % completion.matches.length]?.text.length ?? completion.prefix.length);

  // Replace text in the node
  const newText = before + match.text + after;
  node.textContent = newText;

  // Place cursor after the completed text
  const newCursorPos = completion.prefixStart + match.text.length;
  const newRange = document.createRange();
  newRange.setStart(node, Math.min(newCursorPos, newText.length));
  newRange.collapse(true);
  sel.removeAllRanges();
  sel.addRange(newRange);
}
```

**Simpler approach** — track the "replaced region" length:

```typescript
let replacedLength = $state(0);  // length of the currently inserted completion text

function applyCompletion() {
  if (!editorEl || !completion.active) return;

  const match = completion.matches[completion.currentIndex];
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return;

  const range = sel.getRangeAt(0);
  const node = range.startContainer;
  if (node.nodeType !== Node.TEXT_NODE) return;

  const text = node.textContent ?? '';

  // The region to replace: from prefixStart, length = replacedLength (or prefix length on first Tab)
  const replaceLen = replacedLength > 0 ? replacedLength : completion.prefix.length;
  const before = text.slice(0, completion.prefixStart);
  const after = text.slice(completion.prefixStart + replaceLen);

  node.textContent = before + match.text + after;
  replacedLength = match.text.length;

  // Cursor after completion
  const cursorPos = completion.prefixStart + match.text.length;
  const newRange = document.createRange();
  newRange.setStart(node, Math.min(cursorPos, node.textContent!.length));
  newRange.collapse(true);
  sel.removeAllRanges();
  sel.addRange(newRange);
}
```

#### Visual Hint (Optional)

Show the completion inline with a dimmed suffix, similar to IDE ghost text:

```
Player types: "go to cross|"  (cursor at |)
After Tab:    "go to The Crossroads|"
```

No ghost text needed — the completion replaces the prefix directly. But a subtle flash or brief highlight on the completed portion could help the player notice the change.

### Input Handler Reset

When the player types (not Tab), reset completion:

```typescript
function handleInput() {
  if (completion.active) {
    resetCompletion();
  }
  detectDropdowns();
}
```

## Backend Changes

**None.** All noun data is already available in `mapData` (locations) and `npcsHere` (NPCs) stores. The completion is purely a frontend text manipulation feature.

### Future: Backend Noun Registry

If we add objects, inventory, or topic nouns, the backend could provide a dedicated endpoint:

```
GET /api/known-nouns → { locations: [...], npcs: [...], objects: [...] }
```

For now, deriving from existing stores is sufficient and avoids a new IPC round-trip.

## Data Flow

```
Player types: "go to cr"

Player presses Tab:
  → extractPrefix() → { prefix: "cr", start: 6 }
  → knownNouns contains: [
      { text: "The Crossroads", category: "location", priority: 0 },
      { text: "The Church", category: "location", priority: 0 },
      ...
    ]
  → findMatches("cr") → [
      { text: "The Crossroads", ... },  // "crossroads" starts with "cr"
    ]
    (Note: "The Church" doesn't match because "church" starts with "ch", not "cr")
  → completion.active = true, currentIndex = 0
  → applyCompletion() → editor: "go to The Crossroads"

Player presses Tab again (if multiple matches):
  → currentIndex = 1 → next match applied

Player presses Space (or any non-Tab key):
  → resetCompletion()
  → Normal typing resumes
```

## Interaction with Other Features

| Feature | Interaction |
|---------|-------------|
| @mention dropdown | Tab selects mention when dropdown is open (existing behavior, takes priority) |
| /slash dropdown | Tab selects command when dropdown is open (from idea #1, takes priority) |
| Input history (Up/Down) | No conflict — Tab and arrows are independent |
| Emote `*asterisks*` | Tab can complete nouns inside asterisks: `*waves at Padr` → Tab → `*waves at Padraig` |

Priority order when Tab is pressed:
1. Mention dropdown open → select mention (existing)
2. Slash dropdown open → select command (idea #1)
3. Neither dropdown open → noun tab-completion (this feature)

## Edge Cases

| Case | Behavior |
|------|----------|
| No matches for prefix | Tab does nothing |
| Single match | Tab completes immediately; pressing Tab again cycles back to same match |
| Prefix is already a complete match | Tab still activates (might have longer matches) |
| Empty input + Tab | No prefix extracted → Tab does nothing |
| Tab in middle of word | Prefix is text from last space to cursor; completion replaces that segment |
| NPC leaves location while completing | `knownNouns` updates reactively; completion state uses stale matches (harmless — user just presses Tab again) |
| Completion text contains spaces | Works correctly — "The Crossroads" replaces "cross" |
| Completion text contains apostrophes | Works correctly — "Darcy's Pub" replaces "dar" |
| Tab after @mention chip | Prefix starts after chip's trailing nbsp; works normally |

## Testing

### Frontend (Vitest)

1. **findMatches**: "pub" matches "Darcy's Pub"; "padr" matches "Padraig"
2. **findMatches**: "xyz" returns empty array
3. **findMatches**: "the" matches multiple locations starting with "The"
4. **extractPrefix**: Cursor at end of "go to cr" → prefix "cr", start 6
5. **extractPrefix**: Cursor in middle of text → correct prefix extracted
6. **Tab press**: With matching prefix → editor text updated
7. **Tab cycling**: Multiple matches → each Tab advances to next
8. **Non-Tab key resets**: After completing, typing resets completion state
9. **Priority**: Mention dropdown open → Tab selects mention, not noun completion

### Derived Store Tests

1. **knownNouns**: With mapData + npcsHere → correct noun list
2. **Priority sorting**: Adjacent locations before non-adjacent
3. **Reactivity**: Updating mapData → knownNouns updates

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/stores/nouns.ts` | **New** — derived store for known nouns |
| `ui/src/components/InputField.svelte` | Add Tab-completion logic (extractPrefix, findMatches, applyCompletion, handleKeydown changes) |
| `ui/src/components/InputField.test.ts` | Add tab-completion tests |

## Effort Estimate

**Medium** — the matching and cycling logic is moderate complexity. The main challenge is correctly manipulating the contenteditable DOM to replace text at the right position, especially when completions contain spaces or apostrophes. No backend changes needed.
