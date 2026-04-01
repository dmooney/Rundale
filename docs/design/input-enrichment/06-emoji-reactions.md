# Design: Bidirectional Emoji Reactions

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #6

## Overview

Three reaction flows: (1) player reacts to NPC messages with emoji, (2) NPCs react to player messages with emoji, (3) NPCs react to each other's messages. Reactions are lightweight nonverbal signals that enrich conversations without requiring full dialogue responses.

## Reaction Palette

Period-appropriate gestures mapped to emoji. The UI shows emoji but NPC context receives natural language:

| Emoji | NPC sees | Keyboard shortcut |
|-------|----------|-------------------|
| 😊 | "smiled warmly" | 1 |
| 😠 | "looked angry" | 2 |
| 😢 | "looked sorrowful" | 3 |
| 😳 | "looked startled" | 4 |
| 🤔 | "looked thoughtful" | 5 |
| 😏 | "smirked knowingly" | 6 |
| 👀 | "raised an eyebrow" | 7 |
| 🤫 | "made a hushing gesture" | 8 |
| 😂 | "laughed heartily" | 9 |
| 🙄 | "rolled their eyes" | 0 |
| 🍺 | "raised a glass" | - |
| ✝️ | "crossed themselves" | = |

This palette is defined as shared data, used by both frontend and backend.

### Frontend Palette Definition

```typescript
// ui/src/lib/reactions.ts (new file)

export interface ReactionDef {
  emoji: string;
  description: string;  // what the NPC sees
  key: string;           // keyboard shortcut (for reaction picker)
}

export const REACTION_PALETTE: ReactionDef[] = [
  { emoji: '😊', description: 'smiled warmly', key: '1' },
  { emoji: '😠', description: 'looked angry', key: '2' },
  { emoji: '😢', description: 'looked sorrowful', key: '3' },
  { emoji: '😳', description: 'looked startled', key: '4' },
  { emoji: '🤔', description: 'looked thoughtful', key: '5' },
  { emoji: '😏', description: 'smirked knowingly', key: '6' },
  { emoji: '👀', description: 'raised an eyebrow', key: '7' },
  { emoji: '🤫', description: 'made a hushing gesture', key: '8' },
  { emoji: '😂', description: 'laughed heartily', key: '9' },
  { emoji: '🙄', description: 'rolled their eyes', key: '0' },
  { emoji: '🍺', description: 'raised a glass', key: '-' },
  { emoji: '✝️', description: 'crossed themselves', key: '=' },
];
```

### Backend Palette Definition

```rust
// crates/parish-core/src/npc/reactions.rs (new file)

/// A reaction emoji with its natural-language description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionDef {
    pub emoji: String,
    pub description: String,
}

/// The canonical reaction palette.
pub const REACTION_PALETTE: &[(&str, &str)] = &[
    ("😊", "smiled warmly"),
    ("😠", "looked angry"),
    ("😢", "looked sorrowful"),
    ("😳", "looked startled"),
    ("🤔", "looked thoughtful"),
    ("😏", "smirked knowingly"),
    ("👀", "raised an eyebrow"),
    ("🤫", "made a hushing gesture"),
    ("😂", "laughed heartily"),
    ("🙄", "rolled their eyes"),
    ("🍺", "raised a glass"),
    ("✝️", "crossed themselves"),
];

/// Look up the natural-language description for an emoji.
pub fn reaction_description(emoji: &str) -> Option<&'static str> {
    REACTION_PALETTE.iter()
        .find(|(e, _)| *e == emoji)
        .map(|(_, desc)| *desc)
}
```

## Flow 1: Player Reacts to NPC Messages

### Frontend — Reaction Picker

When the player hovers over an NPC message in the chat log, a small reaction bar appears below the bubble.

#### ChatPanel.svelte Changes

Add hover state and reaction picker per NPC message:

```svelte
<script>
  import { REACTION_PALETTE } from '$lib/reactions';
  import { reactToMessage } from '$lib/ipc';

  let hoveredMessageIdx = $state(-1);
</script>

{#each $textLog as entry, idx}
  {#if entry.source !== 'player' && entry.source !== 'system'}
    <!-- NPC message bubble -->
    <div
      class="bubble-row npc"
      onmouseenter={() => hoveredMessageIdx = idx}
      onmouseleave={() => hoveredMessageIdx = -1}
    >
      <div class="bubble-wrapper">
        <span class="label">{entry.source}</span>
        <div class="bubble npc">{entry.content}</div>

        <!-- Reaction picker (on hover) -->
        {#if hoveredMessageIdx === idx}
          <div class="reaction-picker" role="toolbar" aria-label="React to message">
            {#each REACTION_PALETTE as reaction}
              <button
                class="reaction-btn"
                title={reaction.description}
                onclick={() => reactToMessage(idx, reaction.emoji)}
              >
                {reaction.emoji}
              </button>
            {/each}
          </div>
        {/if}

        <!-- Existing reactions -->
        {#if entry.reactions && entry.reactions.length > 0}
          <div class="reaction-bar">
            {#each entry.reactions as r}
              <span class="reaction-badge" title={r.source}>
                {r.emoji}
                {#if r.source !== 'player'}
                  <span class="reaction-source">{r.source}</span>
                {/if}
              </span>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
{/each}
```

#### CSS

```css
.reaction-picker {
  display: flex;
  gap: 0.15rem;
  padding: 0.2rem 0.25rem;
  background: var(--color-panel-bg);
  border: 1px solid var(--color-border);
  border-radius: 12px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  width: fit-content;
  margin-top: 0.2rem;
}

.reaction-btn {
  background: none;
  border: none;
  padding: 0.15rem 0.2rem;
  font-size: 0.85rem;
  cursor: pointer;
  border-radius: 4px;
  line-height: 1;
  transition: transform 0.1s, background 0.1s;
}

.reaction-btn:hover {
  transform: scale(1.3);
  background: var(--color-input-bg);
}

.reaction-bar {
  display: flex;
  gap: 0.25rem;
  margin-top: 0.2rem;
  flex-wrap: wrap;
}

.reaction-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.15rem;
  font-size: 0.75rem;
  background: var(--color-input-bg);
  border: 1px solid var(--color-border);
  border-radius: 10px;
  padding: 0.1rem 0.35rem;
}

.reaction-source {
  font-size: 0.65rem;
  color: var(--color-muted);
}
```

### Type Changes

```typescript
// ui/src/lib/types.ts

interface Reaction {
  emoji: string;
  source: string;  // "player" or NPC name
}

interface TextLogEntry {
  source: string;
  content: string;
  streaming?: boolean;
  reactions?: Reaction[];   // NEW
}
```

### IPC — New Command

```typescript
// ui/src/lib/ipc.ts

export async function reactToMessage(messageIndex: number, emoji: string) {
  await invoke('react_to_message', { message_index: messageIndex, emoji });
}
```

### Store Update

When the player reacts, optimistically add the reaction to the local store immediately (no round-trip wait):

```typescript
// In the reactToMessage wrapper or a store action:
textLog.update(log => {
  const entry = log[messageIndex];
  if (entry) {
    const reactions = entry.reactions ?? [];
    // Replace existing player reaction (one per message) or add
    const existing = reactions.findIndex(r => r.source === 'player');
    if (existing >= 0) {
      reactions[existing] = { emoji, source: 'player' };
    } else {
      reactions.push({ emoji, source: 'player' });
    }
    entry.reactions = reactions;
  }
  return [...log];
});
```

## Flow 2: NPCs React to Player Messages

When the player says something, present NPCs can attach emoji reactions to the player's message — without generating a full dialogue response.

### Backend — Reaction Generation

Two approaches, chosen per implementation phase:

#### Phase 1: Rule-Based Reactions

Fast, zero-latency reactions based on keyword matching and NPC mood:

```rust
// crates/parish-core/src/npc/reactions.rs

/// Generate a quick reaction from an NPC based on keywords and mood.
///
/// Returns None if the NPC wouldn't react to this input.
pub fn generate_rule_reaction(
    npc: &Npc,
    player_input: &str,
    other_speaker: Option<&str>,
) -> Option<String> {
    let input_lower = player_input.to_lowercase();

    // Keyword → likely reaction mapping
    let keyword_reactions: &[(&[&str], &str)] = &[
        (&["death", "died", "killed", "murder"], "😢"),
        (&["fairy", "fairies", "púca", "banshee"], "✝️"),
        (&["drink", "whiskey", "poitín", "ale"], "🍺"),
        (&["joke", "funny", "laugh", "haha"], "😂"),
        (&["secret", "don't tell", "between us"], "🤫"),
        (&["rent", "evict", "landlord", "agent"], "😠"),
        (&["gold", "treasure", "fortune", "money"], "👀"),
    ];

    for (keywords, emoji) in keyword_reactions {
        if keywords.iter().any(|kw| input_lower.contains(kw)) {
            // 60% chance to react (not every NPC reacts every time)
            if rand::random::<f64>() < 0.6 {
                return Some(emoji.to_string());
            }
        }
    }

    None
}
```

#### Phase 2: LLM-Generated Reactions (Future)

Add a tiny inference request alongside Tier 1 dialogue:

```rust
/// Prompt for generating NPC reactions (appended to Tier 1 response format).
///
/// The reaction field is added to the JSON metadata:
/// { "action": "...", "mood": "...", "reaction_emoji": "😊" }
```

This piggybacks on the existing inference call — the NPC's structured JSON response already includes `action` and `mood`; adding `reaction_emoji` is a one-field extension.

### Backend — Broadcasting Reactions

New event type for NPC reactions:

```rust
// crates/parish-core/src/ipc/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcReactionPayload {
    pub message_index: usize,  // which textLog entry to attach to
    pub emoji: String,
    pub source: String,        // NPC name
}
```

Emitted via EventBus:

```rust
state.event_bus.emit("npc-reaction", &NpcReactionPayload {
    message_index: player_msg_idx,
    emoji: reaction_emoji,
    source: npc.name.clone(),
});
```

### Frontend — Receiving NPC Reactions

New event listener in `+page.svelte`:

```typescript
onEvent('npc-reaction', (payload: NpcReactionPayload) => {
  textLog.update(log => {
    const entry = log[payload.message_index];
    if (entry) {
      const reactions = entry.reactions ?? [];
      reactions.push({ emoji: payload.emoji, source: payload.source });
      entry.reactions = reactions;
    }
    return [...log];
  });
});
```

## Flow 3: NPC-to-NPC Reactions

Generated during Tier 2 background simulation ticks. When NPCs are at the same location, they can react to each other's recent statements.

### Integration with Tier 2 Ticks

Extend the Tier 2 prompt to include reaction generation:

```rust
// In build_tier2_prompt(), add to the JSON output format:
// "reactions": [{"source": "Siobhan", "to_statement_by": "Padraig", "emoji": "😟"}]
```

The Tier 2 response parser extracts reactions and emits `npc-reaction` events for any that correspond to messages visible in the current chat log.

This is the lowest-priority flow and can be deferred to a later phase.

## Backend — Player Reaction Context

When the player reacts to an NPC message, the reaction is stored and injected into the NPC's next conversation context.

### Storage

```rust
// crates/parish-core/src/npc/reactions.rs

/// Recent player reactions toward this NPC.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReactionLog {
    /// Last N reactions from the player, with context.
    entries: Vec<ReactionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionEntry {
    pub emoji: String,
    pub description: String,     // "looked angry"
    pub context: String,         // what the NPC said that was reacted to (truncated)
    pub timestamp: DateTime<Utc>,
}

impl ReactionLog {
    const MAX_ENTRIES: usize = 10;

    pub fn add(&mut self, emoji: &str, context: &str, timestamp: DateTime<Utc>) {
        if let Some(desc) = reaction_description(emoji) {
            self.entries.push(ReactionEntry {
                emoji: emoji.to_string(),
                description: desc.to_string(),
                context: context.chars().take(80).collect(),
                timestamp,
            });
            if self.entries.len() > Self::MAX_ENTRIES {
                self.entries.remove(0);
            }
        }
    }

    /// Format recent reactions for NPC prompt context.
    pub fn context_string(&self, n: usize) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = self.entries.iter().rev().take(n).map(|e| {
            format!("- The player {} when you said \"{}\"", e.description, e.context)
        }).collect();
        format!("Recent nonverbal reactions from the player:\n{}", lines.join("\n"))
    }
}
```

### Prompt Injection

In `build_enhanced_context()`, append the reaction context:

```rust
// After memory context, add reaction context:
let reaction_ctx = npc.reaction_log.context_string(5);
if !reaction_ctx.is_empty() {
    context.push_str("\n\n");
    context.push_str(&reaction_ctx);
}
```

This gives the NPC awareness of the player's nonverbal feedback over time.

## Route Handler — Player React

```rust
// crates/parish-server/src/routes.rs

#[derive(Deserialize)]
struct ReactRequest {
    message_index: usize,
    emoji: String,
}

async fn react_to_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReactRequest>,
) -> StatusCode {
    // Look up the message to find which NPC said it
    // (message_index maps to the textLog on the frontend;
    //  we need to track a parallel log on the backend or
    //  include the NPC name in the request)

    // Store the reaction in the NPC's reaction_log
    let mut npc_manager = state.npc_manager.lock().unwrap();
    // Find NPC by the message source...
    // npc.reaction_log.add(&req.emoji, &message_content, Utc::now());

    StatusCode::OK
}
```

**Implementation note:** The backend needs to maintain a message log parallel to the frontend's `textLog` to map `message_index` → NPC name + content. Alternatively, the frontend sends `{ npc_name, message_content, emoji }` instead of an index. The latter is simpler and avoids synchronization issues.

Revised request:

```rust
#[derive(Deserialize)]
struct ReactRequest {
    npc_name: String,
    message_snippet: String,  // first ~80 chars of the message being reacted to
    emoji: String,
}
```

## Message Index Tracking

Rather than requiring index synchronization between frontend and backend, use a **message ID** approach:

### Frontend

Assign each `TextLogEntry` a unique ID on creation:

```typescript
interface TextLogEntry {
  id: string;             // NEW: unique ID (e.g., crypto.randomUUID())
  source: string;
  content: string;
  streaming?: boolean;
  reactions?: Reaction[];
}
```

### Backend

Generate the ID server-side and include it in `TextLogPayload`:

```rust
pub struct TextLogPayload {
    pub id: String,           // NEW: UUID
    pub source: String,
    pub content: String,
}
```

Now reactions reference `message_id` instead of an index — robust against log truncation, reordering, etc.

## Data Flow — Complete Example

```
1. NPC says something:
   Backend emits: text-log { id: "abc-123", source: "Padraig", content: "The rent was raised..." }
   Frontend: textLog gets new entry with id "abc-123"

2. Player hovers over Padraig's message:
   Frontend: reaction picker appears below bubble

3. Player clicks 😠:
   Frontend: optimistically adds { emoji: "😠", source: "player" } to entry.reactions
   Frontend: calls reactToMessage("abc-123", "😠")
   Backend: stores reaction in Padraig's reaction_log: "player looked angry at 'The rent was raised...'"

4. Player says "That's outrageous!":
   Backend: handle_npc_conversation() runs
   Backend: Padraig's context includes: "Recent nonverbal reactions: The player looked angry when you said 'The rent was raised...'"
   Backend: Padraig responds, aware of the player's anger

5. NPCs react to player's message:
   Backend: generate_rule_reaction() for each NPC at location
   Backend: Siobhan gets 😢 reaction (keyword: "rent")
   Backend emits: npc-reaction { message_id: "def-456", emoji: "😢", source: "Siobhan" }
   Frontend: adds { emoji: "😢", source: "Siobhan" } to the player's message reactions
```

## Phased Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| **Phase 1** | Player → NPC reactions (hover picker, context injection) | Medium |
| **Phase 2** | NPC → Player reactions (rule-based, keyword matching) | Low |
| **Phase 3** | NPC → NPC reactions (Tier 2 integration) | Medium |
| **Phase 4** | LLM-generated NPC reactions (structured output extension) | Low |

Phase 1 is the most impactful — it gives the player a new input modality. Phases 2-4 add life to the room.

## Edge Cases

| Case | Behavior |
|------|----------|
| React to own message | Not supported — picker only appears on NPC messages |
| React to system message | Not supported — picker only for NPC bubbles |
| Multiple reactions from player | One reaction per message; re-clicking replaces |
| React during streaming | Picker doesn't appear on the streaming message (incomplete) |
| NPC reacts to old message | Index/ID valid as long as message is in textLog; silently ignored if not found |
| Reaction to whispered message | Allowed (only target NPC's context affected) |
| Many NPCs react simultaneously | All reactions render in the reaction bar; may wrap to multiple lines |
| NPC reaction generation latency | Rule-based is instant; LLM-based emitted after brief delay |
| Save/load with reactions | ReactionLog on NPCs is serialized; textLog reactions are ephemeral (session-only) |

## Testing

### Backend (cargo test)

1. **ReactionLog::add**: Adds entry, caps at MAX_ENTRIES
2. **ReactionLog::context_string**: Formats correctly, respects n limit
3. **reaction_description**: Maps emoji to description, returns None for unknown
4. **generate_rule_reaction**: Returns expected emoji for keyword matches; returns None sometimes (probability)
5. **Serialization**: ReactionLog round-trips through serde_json

### Frontend (Vitest)

1. **Reaction picker**: Appears on NPC message hover, hidden on mouseleave
2. **Click reaction**: Adds to entry.reactions, calls IPC
3. **Reaction bar**: Renders badges with emoji and source name
4. **NPC reaction event**: Adds reaction to correct message by ID
5. **Replace reaction**: Second click replaces first player reaction

### E2E (Playwright)

1. Hover over NPC message → reaction picker visible
2. Click emoji → badge appears below message
3. NPC reaction event → badge appears on player message

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/lib/reactions.ts` | **New** — reaction palette definitions |
| `ui/src/lib/types.ts` | Add `Reaction`, `id` to `TextLogEntry`, reaction fields |
| `ui/src/lib/ipc.ts` | Add `reactToMessage()` command |
| `ui/src/components/ChatPanel.svelte` | Reaction picker on hover, reaction bar rendering |
| `ui/src/stores/game.ts` | Handle `npc-reaction` events |
| `ui/src/routes/+page.svelte` | Wire up `npc-reaction` event listener |
| `crates/parish-core/src/npc/reactions.rs` | **New** — ReactionDef, ReactionLog, reaction_description, generate_rule_reaction |
| `crates/parish-core/src/npc/mod.rs` | Add `reaction_log: ReactionLog` to `Npc` struct |
| `crates/parish-core/src/npc/ticks.rs` | Inject reaction context into enhanced context |
| `crates/parish-core/src/ipc/types.rs` | Add `NpcReactionPayload`, `id` to `TextLogPayload` |
| `crates/parish-server/src/routes.rs` | Add `react_to_message` handler, NPC reaction generation in conversation flow |

## Effort Estimate

**Medium-High** — Phase 1 (player reactions) is medium effort. The full bidirectional system with NPC reactions, rule-based generation, and context injection is a larger feature but can be shipped incrementally.
