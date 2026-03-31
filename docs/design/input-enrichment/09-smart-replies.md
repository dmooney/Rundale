# Design: Contextual Action Suggestions / Smart Replies

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #9

## Overview

Show 2-3 contextual quick-reply chips above the input field based on the current game situation. These reduce blank-page paralysis for new players and surface contextually appropriate actions. Suggestions update after each player action or NPC response.

Examples:
- At the pub with Padraig: `[Order a drink]` `[Ask about the news]` `[Tell a story]`
- At the church: `[Pray]` `[Speak to the priest]` `[Examine the headstones]`
- After an NPC says something surprising: `[Tell me more]` `[I don't believe you]` `[Change the subject]`

## Architecture Decision: Hybrid Rule-Based + LLM

Two generation strategies, used together:

| Strategy | When | Latency | Cost |
|----------|------|---------|------|
| **Rule-based** | Always available; immediate | 0ms | Free |
| **LLM-generated** | After NPC response; async | 500-2000ms | 1 inference call |

Rule-based suggestions appear instantly. LLM suggestions arrive asynchronously and replace or augment the rule-based ones once ready.

## Rule-Based Suggestion Engine

### Location-Type Rules

Suggestions based on location properties from `LocationData` (defined in `crates/parish-core/src/world/graph.rs`):

```rust
// crates/parish-core/src/npc/suggestions.rs (new file)

/// Generate rule-based action suggestions for the current context.
pub fn generate_suggestions(
    location: &LocationData,
    npcs_here: &[&Npc],
    last_npc_response: Option<&str>,
) -> Vec<String> {
    let mut suggestions = Vec::new();

    // Location-based suggestions
    let name_lower = location.name.to_lowercase();
    if name_lower.contains("pub") || name_lower.contains("tavern") {
        suggestions.push("Order a drink".to_string());
        suggestions.push("Ask about local news".to_string());
    } else if name_lower.contains("church") {
        suggestions.push("Pray".to_string());
        suggestions.push("Examine the headstones".to_string());
    } else if name_lower.contains("farm") {
        suggestions.push("Ask about the harvest".to_string());
        suggestions.push("Offer to help".to_string());
    } else if name_lower.contains("fort") || name_lower.contains("fairy") {
        suggestions.push("Look for fairy rings".to_string());
        suggestions.push("Leave an offering".to_string());
    }

    // NPC-based suggestions
    if let Some(npc) = npcs_here.first() {
        if !npc.introduced {
            suggestions.insert(0, format!("Introduce yourself to {}", npc.name));
        } else {
            suggestions.push(format!("Ask {} how they are", npc.name));
        }
    }

    // Conversation continuation suggestions
    if last_npc_response.is_some() {
        suggestions.push("Tell me more".to_string());
        suggestions.push("Change the subject".to_string());
    }

    // Universal fallbacks
    if suggestions.is_empty() {
        suggestions.push("Look around".to_string());
        suggestions.push("Check the time".to_string());
    }

    // Cap at 3
    suggestions.truncate(3);
    suggestions
}
```

### Mod-Driven Location Suggestions (Future Enhancement)

The location-type rules above are hardcoded. For better mod support, location suggestions could be defined in `world.json`:

```json
{
  "id": "pub",
  "name": "Darcy's Pub",
  "suggestions": ["Order a drink", "Ask about local news", "Start a song"]
}
```

This would be loaded into `LocationData` and used instead of keyword matching. Deferred to a later phase.

## LLM-Generated Suggestions

### Prompt Design

A lightweight prompt that generates 3 contextual suggestions based on the current game state. Uses the cheapest available model (Tier 3 / small model).

```rust
const SUGGESTION_SYSTEM_PROMPT: &str = r#"
You are a game suggestion engine for an 1820s Irish village text adventure.
Given the current context, suggest exactly 3 short actions the player could take.

Rules:
- Each suggestion is 2-6 words
- Suggestions should be diverse (don't suggest 3 similar things)
- At least one suggestion should involve an NPC if one is present
- Suggestions should feel natural for the setting and situation
- Never suggest anachronistic actions

Output: A JSON array of exactly 3 strings.
Example: ["Order a pint of ale", "Ask about the harvest", "Examine the old map"]
"#;
```

### Context Prompt

```rust
fn build_suggestion_context(
    location: &LocationData,
    npcs: &[&Npc],
    last_exchange: Option<(&str, &str)>,  // (player_input, npc_response)
    time_of_day: &str,
) -> String {
    let mut ctx = format!(
        "Location: {} ({})\nTime: {}\n",
        location.name,
        if location.indoor { "indoors" } else { "outdoors" },
        time_of_day,
    );

    if !npcs.is_empty() {
        ctx.push_str("NPCs present: ");
        let names: Vec<String> = npcs.iter().map(|n|
            format!("{} ({})", n.name, n.occupation)
        ).collect();
        ctx.push_str(&names.join(", "));
        ctx.push('\n');
    }

    if let Some((player, npc)) = last_exchange {
        ctx.push_str(&format!(
            "Last exchange:\n  Player: \"{}\"\n  NPC: \"{}\"\n",
            player,
            // Truncate long responses
            &npc.chars().take(200).collect::<String>(),
        ));
    }

    ctx
}
```

### Inference Integration

Suggestions are generated **asynchronously after each NPC response completes**. This avoids adding latency to the main conversation flow.

```rust
// In handle_npc_conversation(), after stream-end:

// Fire-and-forget suggestion generation
let suggestion_state = state.clone();
tokio::spawn(async move {
    let suggestions = generate_llm_suggestions(
        &suggestion_state,
        &location,
        &npcs_here,
        Some((&player_input, &npc_dialogue)),
    ).await;

    if let Some(suggestions) = suggestions {
        suggestion_state.event_bus.emit("suggestions", &SuggestionsPayload {
            suggestions,
        });
    }
});
```

The `generate_llm_suggestions` function:

```rust
async fn generate_llm_suggestions(
    state: &Arc<AppState>,
    location: &LocationData,
    npcs: &[&Npc],
    last_exchange: Option<(&str, &str)>,
) -> Option<Vec<String>> {
    let client = state.client.lock().unwrap().clone()?;
    let config = state.config.lock().unwrap();
    let model = config.model_name.clone();
    drop(config);

    let context = build_suggestion_context(
        location, npcs, last_exchange,
        &state.world.lock().unwrap().clock.time_of_day().to_string(),
    );

    let result = client.generate_json::<Vec<String>>(
        &model,
        &context,
        Some(SUGGESTION_SYSTEM_PROMPT),
    ).await;

    match result {
        Ok(suggestions) if suggestions.len() >= 2 => {
            Some(suggestions.into_iter().take(3).collect())
        }
        _ => None,  // Fall back to rule-based (already shown)
    }
}
```

### Timeout and Fallback

- LLM suggestion request has a 3-second timeout
- If it fails or times out, rule-based suggestions remain shown
- If the player acts before suggestions arrive, the stale suggestions are discarded

## Frontend Changes

### New Component — `ui/src/components/Suggestions.svelte`

```svelte
<script lang="ts">
  import { suggestions, streamingActive } from '../stores/game';
  import { submitInput } from '$lib/ipc';

  async function useSuggestion(text: string) {
    if ($streamingActive) return;
    await submitInput(text);
  }
</script>

{#if $suggestions.length > 0 && !$streamingActive}
  <div class="suggestions" role="toolbar" aria-label="Suggested actions">
    {#each $suggestions as suggestion}
      <button
        class="suggestion-chip"
        onclick={() => useSuggestion(suggestion)}
      >
        {suggestion}
      </button>
    {/each}
  </div>
{/if}
```

#### CSS

```css
.suggestions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
  padding: 0.3rem 0.75rem;
  background: var(--color-panel-bg);
  border-top: 1px solid var(--color-border);
}

.suggestion-chip {
  display: inline-flex;
  align-items: center;
  padding: 0.2rem 0.6rem;
  font-size: 0.78rem;
  font-family: inherit;
  color: var(--color-fg);
  background: var(--color-input-bg);
  border: 1px solid var(--color-border);
  border-radius: 12px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
  white-space: nowrap;
}

.suggestion-chip:hover {
  border-color: var(--color-accent);
  background: var(--color-panel-bg);
}
```

### Layout — `ui/src/routes/+page.svelte`

```svelte
<div class="chat-col">
  <ChatPanel />
  <QuickTravel />     <!-- idea #7 -->
  <Suggestions />     <!-- this feature -->
  <InputField />
</div>
```

If both QuickTravel and Suggestions are visible, they stack. QuickTravel shows location exits (accent-colored, outlined), Suggestions shows actions (neutral-colored, filled background). The visual distinction is clear.

### New Store

```typescript
// In ui/src/stores/game.ts

export const suggestions = writable<string[]>([]);
```

### New Event Type

```typescript
// ui/src/lib/types.ts

interface SuggestionsPayload {
  suggestions: string[];
}
```

### Event Listener — `+page.svelte`

```typescript
onEvent('suggestions', (payload: SuggestionsPayload) => {
  suggestions.set(payload.suggestions);
});

// Clear suggestions when player submits input
// (prevents stale suggestions from persisting)
onEvent('text-log', (payload: TextLogPayload) => {
  if (payload.source === 'player') {
    suggestions.set([]);
  }
});
```

### IPC Types — Backend

```rust
// crates/parish-core/src/ipc/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionsPayload {
    pub suggestions: Vec<String>,
}
```

## Suggestion Lifecycle

```
1. Player arrives at new location:
   → handle_look() completes
   → Rule-based suggestions generated immediately
   → Emits "suggestions" event: ["Look around", "Introduce yourself to Padraig", "Order a drink"]
   → Chips appear above input

2. Player clicks [Introduce yourself to Padraig]:
   → submitInput("Introduce yourself to Padraig")
   → suggestions.set([]) — chips clear immediately
   → NPC conversation begins, streaming starts

3. NPC response completes (stream-end):
   → Rule-based suggestions generated: ["Tell me more", "Ask about the news", "Order a drink"]
   → Emits "suggestions" event → chips appear
   → LLM suggestion request fires asynchronously

4. LLM suggestions arrive (800ms later):
   → ["Ask about Father Callahan", "Buy Padraig a drink", "Tell him about your journey"]
   → Emits "suggestions" event → chips update (replace rule-based)

5. Player types their own message and submits:
   → suggestions.set([]) — chips clear
   → New cycle begins
```

## Backend Route Changes

### After Look

```rust
// In handle_look(), after emitting location description:
let world = state.world.lock().unwrap();
let npc_manager = state.npc_manager.lock().unwrap();
let location = world.current_location_data();
let npcs: Vec<&Npc> = npc_manager.npcs_at(world.player_location)
    .iter().collect();

let suggestions = generate_suggestions(&location, &npcs, None);
state.event_bus.emit("suggestions", &SuggestionsPayload { suggestions });
```

### After NPC Conversation

```rust
// In handle_npc_conversation(), after stream-end:

// Immediate rule-based suggestions
let rule_suggestions = generate_suggestions(&location, &npcs, Some(&npc_dialogue));
state.event_bus.emit("suggestions", &SuggestionsPayload {
    suggestions: rule_suggestions,
});

// Async LLM suggestions (fire-and-forget)
tokio::spawn(async move { /* ... generate_llm_suggestions ... */ });
```

## Edge Cases

| Case | Behavior |
|------|----------|
| No NPCs, generic location | Fallback suggestions: "Look around", "Check the time" |
| LLM unavailable (no provider) | Rule-based suggestions only; no LLM attempt |
| LLM times out (>3s) | Rule-based suggestions remain shown |
| Player submits before LLM suggestions arrive | `suggestions.set([])` clears; LLM result is emitted but immediately relevant |
| Multiple rapid movements | Each `handle_look()` emits new suggestions; last one wins |
| Suggestion text is a system command | Treated as game input; `classify_input()` won't match `/` prefix |
| Very long suggestion text (from LLM) | Chip wraps; `white-space: nowrap` keeps individual chips on one line |
| Clicking suggestion during streaming | Button is hidden (`!$streamingActive` guard) |
| QuickTravel + Suggestions both visible | They stack vertically; visually distinct (different border/bg styles) |

## Testing

### Backend (cargo test)

1. **generate_suggestions at pub**: Returns drink-related suggestion
2. **generate_suggestions with unmet NPC**: "Introduce yourself" appears first
3. **generate_suggestions with conversation**: "Tell me more" appears
4. **generate_suggestions fallback**: Empty location → "Look around" + "Check the time"
5. **Truncation**: Always returns max 3 suggestions
6. **LLM prompt**: Verify context string includes location, NPCs, last exchange

### Frontend (Vitest)

1. **Rendering**: When `$suggestions` has items → chips visible
2. **Click handler**: Clicking chip calls `submitInput` with suggestion text
3. **Hidden during streaming**: `$streamingActive` true → component hidden
4. **Cleared on player input**: text-log with source "player" → suggestions empty

### E2E (Playwright)

1. Navigate to pub → suggestion chips appear
2. Click a suggestion → input submitted, chips clear, NPC responds
3. After NPC response → new suggestions appear

## Files to Modify

| File | Change |
|------|--------|
| `crates/parish-core/src/npc/suggestions.rs` | **New** — rule-based engine + LLM prompt/context |
| `crates/parish-core/src/npc/mod.rs` | Re-export suggestions module |
| `crates/parish-core/src/ipc/types.rs` | Add `SuggestionsPayload` |
| `crates/parish-server/src/routes.rs` | Emit suggestions after `handle_look()` and `handle_npc_conversation()` |
| `ui/src/components/Suggestions.svelte` | **New** — suggestion chip bar |
| `ui/src/stores/game.ts` | Add `suggestions` writable store |
| `ui/src/lib/types.ts` | Add `SuggestionsPayload` |
| `ui/src/routes/+page.svelte` | Wire up `suggestions` event listener, place `<Suggestions />` in layout |

## Effort Estimate

**High** — the rule-based engine is straightforward (low effort), but the LLM suggestion generation adds inference integration, timeout handling, and async lifecycle management. The full feature with both strategies is high effort; shipping rule-based only first is medium effort.

### Recommended Phasing

| Phase | Scope | Effort |
|-------|-------|--------|
| **Phase 1** | Rule-based suggestions only | Medium |
| **Phase 2** | LLM-generated suggestions (async, fire-and-forget) | Medium |
| **Phase 3** | Mod-driven location suggestions (world.json) | Low |
