# TD-019 + TD-020: UI Component Splits (Batch B)

## What was changed

### TD-019: InputField dropdown extraction (P1, Complexity)

Extracted three dropdown systems from `InputField.svelte` (1321 lines) into separate components:

| Component | Lines | Purpose |
|-----------|-------|---------|
| `MentionDropdown.svelte` | 76 | NPC @mention autocomplete list |
| `SlashDropdown.svelte` | 74 | `/command` autocomplete list |
| `ModelDropdown.svelte` | 74 | `/model ...` catalog autocomplete list |

Each component owns its own scoped styles (`.mention-dropdown`, `.mention-item`, etc.) and receives data + callbacks via `$props()`. InputField.svelte dropped from 1321 to 1233 lines (-88).

No behavioral changes — the conditional rendering (`{#if dropdownMode === 'mention' && ...}`) and keyboard/input detection logic remain in InputField.

### TD-020: DebugPanel tab extraction (P2, Complexity)

Extracted each of the 8 tab bodies from `DebugPanel.svelte` (1083 lines) into separate components:

| Component | Lines | Tab |
|-----------|-------|-----|
| `DebugOverviewTab.svelte` | 129 | Overview (clock, location, tiers, event bus, gossip, auth) |
| `DebugNpcsTab.svelte` | 319 | NPC list + detail with schedule/memories/relationships |
| `DebugWorldTab.svelte` | 62 | Locations, worn paths, text log |
| `DebugWeatherTab.svelte` | 22 | Weather engine state |
| `DebugGossipTab.svelte` | 41 | Gossip items with distortion |
| `DebugConversationsTab.svelte` | 34 | Conversation exchange log |
| `DebugEventsTab.svelte` | 39 | Game events + debug events |
| `DebugInferenceTab.svelte` | 338 | Provider info, presets, call log + detail view |

Helper functions moved into their consuming tab:
- `PRESET_PROVIDERS` + `applyPreset()` + `npcLabelFromEntry()` → DebugInferenceTab
- `strengthBar()` → DebugNpcsTab

DebugPanel.svelte dropped from 1083 to 211 lines (-872). The shell still owns the panel layout, tab bar, dock toggle, and passes store-derived props to each tab component.

## Files changed (13 total)

**New files (11):**
- `src/components/MentionDropdown.svelte`
- `src/components/SlashDropdown.svelte`
- `src/components/ModelDropdown.svelte`
- `src/components/DebugOverviewTab.svelte`
- `src/components/DebugNpcsTab.svelte`
- `src/components/DebugWorldTab.svelte`
- `src/components/DebugWeatherTab.svelte`
- `src/components/DebugGossipTab.svelte`
- `src/components/DebugConversationsTab.svelte`
- `src/components/DebugEventsTab.svelte`
- `src/components/DebugInferenceTab.svelte`

**Modified files (2):**
- `src/components/InputField.svelte` (1321 → 1233, -88)
- `src/components/DebugPanel.svelte` (1083 → 211, -872)

**Total delta:** +1188 new lines in sub-components, -960 lines in parent files = +228 net (new CSS/style duplication in standalone components)

## Commands run

```sh
npx vitest run    # 396/396 passed
npx svelte-check  # 0 new errors (only pre-existing issues)
```
