# Design: Input History with Arrow Keys

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #3

## Overview

Press Up/Down arrow keys to cycle through previously submitted inputs, like a terminal shell history. The history is stored in a Svelte store and persisted across sessions via `localStorage`. This is a purely frontend feature with no backend changes.

## User-Facing Behavior

1. Player submits several inputs during a session
2. With the input field focused and empty (or at any point), pressing Up recalls the most recent input
3. Pressing Up again moves further back in history
4. Pressing Down moves forward toward more recent inputs
5. Pressing Down past the newest entry restores whatever the player was typing before navigating
6. History persists across page reloads / sessions via `localStorage`
7. Maximum 50 entries stored; oldest entries are evicted

## Frontend Changes

### New Store — `ui/src/stores/history.ts`

```typescript
import { writable, get } from 'svelte/store';

const STORAGE_KEY = 'parish-input-history';
const MAX_HISTORY = 50;

function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch {
    // Corrupted data — start fresh
  }
  return [];
}

function saveHistory(entries: string[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    // Storage full or unavailable — silently fail
  }
}

/** The ordered list of previous inputs (oldest first). */
export const inputHistory = writable<string[]>(loadHistory());

/** Add a new entry to history. Deduplicates consecutive identical entries. */
export function pushHistory(text: string) {
  inputHistory.update(entries => {
    // Don't add duplicate of the most recent entry
    if (entries.length > 0 && entries[entries.length - 1] === text) {
      return entries;
    }
    const updated = [...entries, text];
    // Evict oldest if over limit
    if (updated.length > MAX_HISTORY) {
      updated.splice(0, updated.length - MAX_HISTORY);
    }
    saveHistory(updated);
    return updated;
  });
}

/** Clear all history (for testing or user preference). */
export function clearHistory() {
  inputHistory.set([]);
  saveHistory([]);
}
```

### InputField.svelte — History Navigation

Add state for tracking the navigation cursor:

```typescript
import { inputHistory, pushHistory } from '../stores/history';

// -1 means "not navigating" (showing current draft)
let historyIndex = $state(-1);
// Saves whatever the player was typing before pressing Up
let draftText = $state('');
```

#### Modified `handleKeydown()`

Add Up/Down handling when the mention and slash dropdowns are **not** active:

```typescript
// After existing dropdown handling, before Enter handling:

if (e.key === 'ArrowUp' && !showMentions && !showSlash) {
  const history = get(inputHistory);
  if (history.length === 0) return;

  e.preventDefault();

  if (historyIndex === -1) {
    // Starting to navigate — save current draft
    draftText = getPlainText();
    historyIndex = history.length - 1;
  } else if (historyIndex > 0) {
    historyIndex--;
  }

  setEditorText(history[historyIndex]);
  return;
}

if (e.key === 'ArrowDown' && !showMentions && !showSlash) {
  if (historyIndex === -1) return; // Not navigating

  e.preventDefault();

  const history = get(inputHistory);
  if (historyIndex < history.length - 1) {
    historyIndex++;
    setEditorText(history[historyIndex]);
  } else {
    // Past the end — restore draft
    historyIndex = -1;
    setEditorText(draftText);
  }
  return;
}
```

#### Helper: `setEditorText()`

Replace the entire contenteditable content with plain text and place cursor at end:

```typescript
function setEditorText(text: string) {
  if (!editorEl) return;
  // Clear all content (chips, text nodes, etc.)
  editorEl.textContent = text;
  // Place cursor at end
  const range = document.createRange();
  const sel = window.getSelection();
  if (editorEl.childNodes.length > 0) {
    range.selectNodeContents(editorEl);
    range.collapse(false); // collapse to end
  }
  sel?.removeAllRanges();
  sel?.addRange(range);
}
```

**Note:** Recalled history entries are inserted as plain text. If the original input contained `@mention` chips, the recalled version shows `@Name` as text (not a chip). This is intentional — recreating chips from plain text would require re-parsing and could fail if the NPC is no longer present.

#### Modified `handleSubmit()`

Push to history on successful submit and reset navigation state:

```typescript
async function handleSubmit(e: Event) {
  e.preventDefault();
  if (showMentions && filteredNpcs.length > 0) {
    selectNpc(filteredNpcs[selectedIndex].name);
    return;
  }
  const trimmed = getPlainText().trim();
  if (!trimmed || $streamingActive) return;

  pushHistory(trimmed);   // ← NEW
  historyIndex = -1;      // ← NEW: reset navigation
  draftText = '';          // ← NEW

  clearEditor();
  showMentions = false;
  await submitInput(trimmed);
}
```

#### Reset on Manual Typing

When the player starts typing after navigating history, reset the navigation state:

```typescript
function handleInput() {
  // If user types while navigating history, they've diverged — reset
  if (historyIndex !== -1) {
    historyIndex = -1;
    // Don't reset draftText — they're now editing a recalled entry
  }
  detectDropdowns();
}
```

## ContentEditable Cursor Considerations

The `ArrowUp` key in a contenteditable div normally moves the cursor up a line in multi-line content. Since the input field supports multi-line content (max-height: 6em with overflow), we need to be careful:

- **Single-line content**: ArrowUp should always navigate history
- **Multi-line content**: ArrowUp should only navigate history when the cursor is on the first line

To handle this, check cursor position before intercepting:

```typescript
function isCursorOnFirstLine(): boolean {
  if (!editorEl) return true;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return true;

  const range = sel.getRangeAt(0);
  // Create a range from start of editor to cursor
  const preRange = document.createRange();
  preRange.setStart(editorEl, 0);
  preRange.setEnd(range.startContainer, range.startOffset);

  // If the text before cursor contains no newlines, we're on line 1
  const textBefore = preRange.toString();
  return !textBefore.includes('\n');
}

function isCursorOnLastLine(): boolean {
  if (!editorEl) return true;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return true;

  const range = sel.getRangeAt(0);
  // Create a range from cursor to end of editor
  const postRange = document.createRange();
  postRange.setStart(range.endContainer, range.endOffset);
  postRange.setEnd(editorEl, editorEl.childNodes.length);

  const textAfter = postRange.toString();
  return !textAfter.includes('\n');
}
```

Updated key handler:

```typescript
if (e.key === 'ArrowUp' && !showMentions && !showSlash && isCursorOnFirstLine()) {
  // ... history navigation
}

if (e.key === 'ArrowDown' && !showMentions && !showSlash && isCursorOnLastLine()) {
  // ... history navigation
}
```

## Backend Changes

**None.** This is a purely frontend feature.

## Data Flow

```
Session starts:
  → loadHistory() reads localStorage → ["go to pub", "hello @Padraig"]

Player types "look around", hits Enter:
  → pushHistory("look around")
  → localStorage now: ["go to pub", "hello @Padraig", "look around"]
  → historyIndex = -1 (reset)

Player presses ArrowUp (input is empty):
  → draftText = "" (save empty draft)
  → historyIndex = 2 → editor shows "look around"

Player presses ArrowUp again:
  → historyIndex = 1 → editor shows "hello @Padraig"

Player presses ArrowDown:
  → historyIndex = 2 → editor shows "look around"

Player presses ArrowDown again:
  → historyIndex = -1 → editor shows "" (restored draft)

Player types "tell me", then presses ArrowUp:
  → draftText = "tell me" (save draft)
  → historyIndex = 2 → editor shows "look around"

Player presses ArrowDown past end:
  → historyIndex = -1 → editor shows "tell me" (draft restored)
```

## Edge Cases

| Case | Behavior |
|------|----------|
| Empty history, press Up | Nothing happens |
| At oldest entry, press Up again | Stay at index 0 (don't wrap) |
| At newest entry, press Down | Restore draft, historyIndex = -1 |
| Submit duplicate of last entry | Not added (consecutive dedup) |
| Submit same text as 3 entries ago | Added (only dedup consecutive) |
| History exceeds 50 entries | Oldest entries evicted |
| localStorage unavailable (iframe, quota) | History works in-session but doesn't persist; no errors |
| Multi-line input in history | Recalled correctly; ArrowUp only triggers on first line |
| @mention chip in original input | Recalled as plain `@Name` text (no chip recreation) |
| Player edits recalled entry | historyIndex resets; editing doesn't modify history |
| History navigation during streaming | Input is disabled during streaming; no conflict |

## Testing

### Frontend (Vitest)

1. **pushHistory**: Adds entries, deduplicates consecutive, caps at 50
2. **clearHistory**: Empties store and localStorage
3. **loadHistory**: Returns parsed array from localStorage; returns [] on corruption
4. **ArrowUp from empty input**: Sets editor to most recent entry
5. **ArrowUp/ArrowDown cycling**: Correctly traverses history
6. **Draft preservation**: Draft saved on first Up, restored on Down past newest
7. **Submit resets navigation**: After submit, historyIndex = -1
8. **No conflict with mention dropdown**: ArrowUp during mention dropdown → navigates dropdown, not history

### Manual

- Type several inputs, press Up/Down to cycle
- Reload page, press Up — history persists
- Type half a message, Up to browse, Down to restore draft

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/stores/history.ts` | **New** — history store with localStorage persistence |
| `ui/src/components/InputField.svelte` | Add history navigation (Up/Down), push on submit, draft preservation |
| `ui/src/components/InputField.test.ts` | Add history navigation tests |

## Effort Estimate

**Low** — purely frontend, no backend changes. The main complexity is cursor-line detection for multi-line content and draft preservation, both straightforward.
