# Design: Location Quick-Travel Buttons

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #7

## Overview

Show clickable location chips above the input field for all adjacent locations. Clicking a chip is equivalent to typing "go to [location]". The chips update dynamically when the player moves. This brings the map's click-to-travel affordance into the text flow for players who prefer not to use the map.

## User-Facing Behavior

1. Player arrives at the Crossroads
2. Above the input field, location chips appear: `[Darcy's Pub]` `[The Church]` `[Murphy's Farm]` `[The Fairy Fort]`
3. Player clicks `[Darcy's Pub]`
4. Equivalent to submitting "go to Darcy's Pub" — movement resolves, location description appears
5. Chips update to show Darcy's Pub's exits
6. During streaming (NPC responding), chips are disabled (same as input field)

## Frontend Changes

### New Component — `ui/src/components/QuickTravel.svelte`

A small chip bar rendered between ChatPanel and InputField:

```svelte
<script lang="ts">
  import { mapData, streamingActive } from '../stores/game';
  import { submitInput } from '$lib/ipc';

  const adjacentLocations = $derived(
    ($mapData?.locations ?? [])
      .filter(loc => loc.adjacent && loc.id !== $mapData?.player_location)
      .sort((a, b) => a.name.localeCompare(b.name))
  );

  async function travel(locationName: string) {
    if ($streamingActive) return;
    await submitInput(`go to ${locationName}`);
  }
</script>

{#if adjacentLocations.length > 0}
  <div class="quick-travel" role="navigation" aria-label="Quick travel">
    {#each adjacentLocations as loc}
      <button
        class="travel-chip"
        disabled={$streamingActive}
        onclick={() => travel(loc.name)}
        title="Travel to {loc.name}"
      >
        {loc.name}
      </button>
    {/each}
  </div>
{/if}
```

#### CSS

```css
.quick-travel {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
  padding: 0.35rem 0.75rem;
  background: var(--color-panel-bg);
  border-top: 1px solid var(--color-border);
}

.travel-chip {
  display: inline-flex;
  align-items: center;
  padding: 0.2rem 0.6rem;
  font-size: 0.78rem;
  font-family: inherit;
  color: var(--color-accent);
  background: transparent;
  border: 1px solid var(--color-accent);
  border-radius: 12px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
  white-space: nowrap;
}

.travel-chip:hover:not(:disabled) {
  background: var(--color-accent);
  color: var(--color-bg);
}

.travel-chip:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
```

### Layout Integration — `ui/src/routes/+page.svelte`

Insert `QuickTravel` between `ChatPanel` and `InputField` in the chat column:

```svelte
<div class="chat-col">
  <ChatPanel />
  <QuickTravel />
  <InputField />
</div>
```

The component sits directly above the input, visually connected to it. The `border-top` on both QuickTravel and InputField's `.input-form` creates a clean stacked appearance.

### Existing Data Source

The `mapData` store already contains everything needed:

```typescript
// From ui/src/lib/types.ts
interface MapLocation {
  id: string;
  name: string;
  lat: number;
  lon: number;
  adjacent: boolean;  // ← this flag is exactly what we need
}

interface MapData {
  locations: MapLocation[];
  edges: [string, string][];
  player_location: string;
}
```

The `adjacent` flag is computed server-side in `build_map_data()` (`crates/parish-core/src/ipc/handlers.rs:72-76`) — locations connected to the player's current position are marked `adjacent: true`. No backend changes needed.

### Store Reactivity

The `mapData` store is updated whenever a `world-update` event fires (after movement, time changes, etc.). The QuickTravel component derives from `mapData`, so chips auto-update after movement.

Current flow in `+page.svelte`:

```typescript
onEvent('world-update', async () => {
  worldState.set(await getWorldSnapshot());
  mapData.set(await getMap());           // ← triggers QuickTravel re-derive
  npcsHere.set(await getNpcsHere());
  // ...
});
```

## Backend Changes

**None.** All data is already available in the `MapData` response. The `adjacent` flag on `MapLocation` was designed for exactly this kind of feature.

The `submitInput("go to Darcy's Pub")` call uses the existing movement pipeline — `classify_input()` → `GameInput` → `parse_intent_local()` → `IntentKind::Move` → `handle_movement()`.

## Data Flow

```
Player arrives at Crossroads:
  Backend emits: world-update event
  Frontend: fetches getMap() → MapData {
    player_location: "crossroads",
    locations: [
      { id: "crossroads", name: "The Crossroads", adjacent: false },  // current
      { id: "pub", name: "Darcy's Pub", adjacent: true },
      { id: "church", name: "The Church", adjacent: true },
      { id: "farm", name: "Murphy's Farm", adjacent: true },
      { id: "fort", name: "The Fairy Fort", adjacent: true },
      { id: "village", name: "Kilteevan Village", adjacent: false },  // not adjacent
    ]
  }
  QuickTravel derives: [Darcy's Pub, Murphy's Farm, The Church, The Fairy Fort]
  Chips render in alphabetical order

Player clicks [Darcy's Pub]:
  → submitInput("go to Darcy's Pub")
  → Backend: parse_intent_local → Move("Darcy's Pub")
  → handle_movement: resolves to "pub" location
  → world-update event fires
  → MapData refreshes with new adjacency
  → QuickTravel chips update to pub's exits
```

## Inline Location Links (Optional Enhancement)

When NPCs mention location names in dialogue, auto-link them as clickable chips within the chat bubble:

```
Padraig: "You should visit [the fairy fort] before sunset."
```

This requires:
1. A known-locations list available to the frontend
2. A text-scanning pass in `ChatPanel` that detects location names in NPC dialogue
3. Replacing matched text with clickable `<button>` elements

This is a nice-to-have enhancement. The primary feature (chips above input) works without it.

### Implementation sketch for inline links:

```typescript
// In ChatPanel.svelte

import { mapData } from '../stores/game';

function linkifyLocations(text: string): string {
  const locations = $mapData?.locations ?? [];
  let result = text;
  for (const loc of locations) {
    // Case-insensitive match, word-boundary aware
    const pattern = new RegExp(`\\b(${escapeRegex(loc.name)})\\b`, 'gi');
    result = result.replace(pattern,
      `<button class="inline-location" data-loc="${loc.name}">$1</button>`
    );
  }
  return result;
}
```

**Deferred** — this requires innerHTML rendering with event delegation, which adds complexity. Implement as a follow-up.

## Edge Cases

| Case | Behavior |
|------|----------|
| No adjacent locations | QuickTravel component renders nothing (hidden via `{#if}`) |
| Many adjacent locations (6+) | Chips wrap to multiple lines; `flex-wrap: wrap` handles this |
| Long location name | Chip stays on one line (`white-space: nowrap`); wraps to next row if needed |
| Click during streaming | Button is `disabled`; click does nothing |
| Rapid double-click | First click submits; second is ignored (input disabled during movement processing) |
| Location name with special chars | `submitInput()` sends plain text; backend fuzzy matching handles it |
| Map not loaded yet | `adjacentLocations` derived from `$mapData ?? []` — empty array, no chips shown |
| Player at isolated location | No adjacent locations → no chips shown |

## Testing

### Frontend (Vitest)

1. **Chip rendering**: With mapData containing adjacent locations → correct number of chips
2. **No chips**: With mapData having no adjacent locations → component not rendered
3. **Click handler**: Clicking chip calls `submitInput("go to LocationName")`
4. **Disabled state**: When `$streamingActive` is true → buttons disabled
5. **Alphabetical order**: Chips sorted by name
6. **Excludes current location**: Player's current location not shown as a chip

### E2E (Playwright)

1. Navigate to a location with multiple exits → chips visible above input
2. Click a chip → player moves, chips update to new exits
3. During NPC streaming → chips are grayed out / unclickable

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/components/QuickTravel.svelte` | **New** — chip bar component |
| `ui/src/routes/+page.svelte` | Import and place `<QuickTravel />` between ChatPanel and InputField |

## Effort Estimate

**Low** — all data already exists in the `mapData` store. The component is ~40 lines of Svelte + CSS. No backend changes. No new IPC commands or events.
