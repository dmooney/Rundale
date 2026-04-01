# Design: Whisper / Private Message Syntax

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #5

## Overview

When multiple NPCs are present at a location, let the player whisper to one NPC so others don't "hear" the message. The whispered content is excluded from other NPCs' conversation context. This creates gameplay possibilities around secrets, conspiracies, and confiding.

## User-Facing Behavior

1. Player types: `@Padraig (whisper) I saw Father Callahan at the fairy fort`
2. Chat log shows: `(whispered to Padraig): I saw Father Callahan at the fairy fort` — with a visual whisper indicator
3. Padraig's NPC responds, aware this was whispered privately
4. Other NPCs present (e.g., Siobhan) do NOT incorporate this exchange into their context
5. Alternative syntax: `>Padraig the land agent is cheating you`

## Syntax Design

Two trigger syntaxes, both detected in the frontend before submission:

### Syntax A: `@Name (whisper) message`

Natural extension of the existing `@mention` system. The `(whisper)` modifier follows the mention:

```
@Padraig (whisper) I saw Father Callahan at the fairy fort
@Siobhan (whisper) Don't tell anyone, but I found gold
```

### Syntax B: `>Name message`

Shorter, MUD-style syntax:

```
>Padraig the land agent is cheating you
>Siobhan I have a secret to tell you
```

Both syntaxes are detected and normalized to the same backend representation.

## Frontend Changes

### InputField.svelte

#### Whisper Detection

After extracting plain text via `getPlainText()`, detect whisper syntax before submission:

```typescript
interface WhisperInfo {
  target: string;    // NPC name
  message: string;   // The whispered content
}

function detectWhisper(text: string): WhisperInfo | null {
  // Syntax A: @Name (whisper) message
  const whisperMentionRe = /^@(\S+)\s*\(whisper\)\s*(.+)$/i;
  let match = text.match(whisperMentionRe);
  if (match) {
    return { target: match[1], message: match[2] };
  }

  // Syntax B: >Name message
  const gtRe = /^>(\S+)\s+(.+)$/;
  match = text.match(gtRe);
  if (match) {
    return { target: match[1], message: match[2] };
  }

  return null;
}
```

#### Modified Submit

When a whisper is detected, send a modified payload to the backend:

```typescript
async function handleSubmit(e: Event) {
  // ... existing validation ...
  const trimmed = getPlainText().trim();
  if (!trimmed || $streamingActive) return;

  pushHistory(trimmed);
  historyIndex = -1;
  clearEditor();

  const whisper = detectWhisper(trimmed);
  if (whisper) {
    await submitInput(trimmed, { whisper: true, target: whisper.target });
  } else {
    await submitInput(trimmed);
  }
}
```

### Chat Panel Rendering

Whispered messages get a distinct visual treatment:

```svelte
{#if entry.whisper}
  <div class="bubble-row player">
    <div class="bubble-wrapper">
      <span class="label whisper-label">
        whispered to {entry.whisper_target}
      </span>
      <div class="bubble player whisper">
        {entry.content}
      </div>
    </div>
  </div>
{/if}
```

```css
.bubble.whisper {
  opacity: 0.85;
  font-style: italic;
  border: 1px dashed var(--color-accent);
  background: transparent;
}

.whisper-label {
  font-style: italic;
  font-size: 0.75rem;
  color: var(--color-muted);
}
```

NPC whispered responses also get the dashed border treatment, making the entire whispered exchange visually distinct from normal conversation.

### IPC Changes — `ui/src/lib/ipc.ts`

Extend `submitInput` to accept optional whisper metadata:

```typescript
export async function submitInput(
  text: string,
  opts?: { whisper?: boolean; target?: string }
) {
  const payload: SubmitInputRequest = { text };
  if (opts?.whisper && opts.target) {
    payload.whisper = true;
    payload.whisper_target = opts.target;
  }
  await invoke('submit_input', payload);
}
```

### Type Changes — `ui/src/lib/types.ts`

```typescript
interface SubmitInputRequest {
  text: string;
  whisper?: boolean;
  whisper_target?: string;
}

interface TextLogEntry {
  source: string;
  content: string;
  streaming?: boolean;
  whisper?: boolean;          // NEW
  whisper_target?: string;    // NEW
}

interface TextLogPayload {
  source: string;
  content: string;
  whisper?: boolean;          // NEW
  whisper_target?: string;    // NEW
}
```

## Backend Changes

### Request Type — `crates/parish-server/src/routes.rs`

```rust
#[derive(Deserialize)]
struct SubmitInputRequest {
    text: String,
    #[serde(default)]
    whisper: bool,
    whisper_target: Option<String>,
}
```

### IPC Payload — `crates/parish-core/src/ipc/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLogPayload {
    pub source: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whisper: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_target: Option<String>,
}
```

### Route Handler — `crates/parish-server/src/routes.rs`

Modify `submit_input` handler to pass whisper info through:

```rust
async fn submit_input(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitInputRequest>,
) -> StatusCode {
    let text = req.text.trim().to_string();
    if text.is_empty() { return StatusCode::OK; }

    // Emit player message with whisper metadata
    state.event_bus.emit("text-log", &TextLogPayload {
        source: "player".to_string(),
        content: format!("> {}", text),
        whisper: req.whisper,
        whisper_target: req.whisper_target.clone(),
    });

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => handle_system_command(cmd, &state).await,
        InputResult::GameInput(_) => {
            if req.whisper {
                handle_whisper_conversation(
                    text,
                    req.whisper_target.unwrap_or_default(),
                    &state,
                ).await;
            } else {
                handle_game_input(text, &state).await;
            }
        }
    }

    StatusCode::OK
}
```

### Whisper Conversation Handler

New function in `routes.rs`:

```rust
/// Handle a whispered message to a specific NPC.
///
/// The key difference from normal conversation: the whispered content
/// is NOT added to other NPCs' context windows.
async fn handle_whisper_conversation(
    raw: String,
    target_name: String,
    state: &Arc<AppState>,
) {
    let world = state.world.lock().unwrap();
    let mut npc_manager = state.npc_manager.lock().unwrap();

    // Find the target NPC by name at the current location
    let npcs_here = npc_manager.npcs_at(world.player_location);
    let target_npc = npcs_here.iter().find(|npc|
        npc.name.eq_ignore_ascii_case(&target_name)
    );

    let Some(npc) = target_npc else {
        state.event_bus.emit("text-log", &TextLogPayload {
            source: "system".into(),
            content: format!("{} is not here.", target_name),
            whisper: false,
            whisper_target: None,
        });
        return;
    };

    let npc_id = npc.id;
    let npc_name = npc.name.clone();

    // Build context with whisper annotation
    let whisper_context = format!(
        "(The player whispers privately to you, so no one else can hear): \"{}\"",
        raw
    );

    // Use the same enhanced prompt/context system, but with whisper framing
    let system_prompt = ticks::build_enhanced_system_prompt(npc, false);
    let other_npcs: Vec<&Npc> = npcs_here.iter()
        .filter(|n| n.id != npc_id)
        .collect();
    let context = ticks::build_enhanced_context(
        npc, &world, &whisper_context, &other_npcs,
    );

    // Mark as introduced
    npc_manager.mark_introduced(npc_id);

    drop(npc_manager);
    drop(world);

    // Submit inference (same as normal conversation)
    // ... (identical streaming logic) ...

    // KEY: When recording this interaction in memory, tag it as whispered
    // so Tier 2 background ticks don't leak it to other NPCs
    // Memory entry: "A traveller whispered to me: [content]. I responded: [response]"

    // Emit NPC response with whisper flag
    state.event_bus.emit("text-log", &TextLogPayload {
        source: npc_name,
        content: response_dialogue,
        whisper: true,
        whisper_target: Some(target_name),
    });
}
```

### Memory Tagging — `crates/parish-core/src/npc/memory.rs`

Extend `MemoryEntry` to support a `private` flag:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub participants: Vec<String>,
    pub location: LocationId,
    #[serde(default)]
    pub private: bool,  // NEW: whispered interactions
}
```

When building context for Tier 2 background ticks (`build_tier2_prompt`), **exclude private memories**:

```rust
/// Returns context string for the last N non-private memories.
pub fn context_string_public(&self, n: usize) -> String {
    self.entries.iter()
        .rev()
        .filter(|e| !e.private)
        .take(n)
        .map(|e| format!("[{}] {}", e.timestamp.format("%H:%M"), e.content))
        .collect::<Vec<_>>()
        .join("\n")
}
```

For Tier 1 (direct player interaction), private memories ARE included — the NPC remembers what was whispered to them.

### Context Exclusion

The core privacy mechanism: when building Tier 2 simulation context or when another NPC overhears:

1. **Tier 1 (target NPC)**: Full context, including whispered content
2. **Tier 2 (background NPCs)**: Private memories excluded via `context_string_public()`
3. **Other NPCs at location**: Do NOT get the whispered exchange in their context at all

This is enforced in `build_enhanced_context()`:

```rust
// In handle_whisper_conversation:
// Only the target NPC gets the conversation context.
// Other NPCs' context_string_public() already filters private memories.
```

## Data Flow

```
Player types: "@Padraig (whisper) I saw Father Callahan at the fairy fort"

Frontend:
  → detectWhisper() → { target: "Padraig", message: "I saw Father Callahan..." }
  → submitInput(text, { whisper: true, target: "Padraig" })

Backend:
  → submit_input receives whisper=true, whisper_target="Padraig"
  → Emits player text-log with whisper flag
  → handle_whisper_conversation("I saw Father Callahan...", "Padraig", state)
    → Finds Padraig at current location
    → Builds context: "(The player whispers privately to you...)"
    → Sends to LLM — Padraig responds
    → Records memory with private=true
    → Emits NPC response text-log with whisper=true

Frontend receives text-log events:
  → Player message rendered with dashed border, "whispered to Padraig" label
  → NPC response rendered with same whisper styling

Later, Tier 2 tick runs:
  → Siobhan (also present) builds context
  → context_string_public() skips the private memory
  → Siobhan has no knowledge of the whispered exchange
```

## Edge Cases

| Case | Behavior |
|------|----------|
| Whisper to NPC not present | System message: "Padraig is not here." |
| Whisper to unknown name | Fuzzy match against NPCs at location; fail → "No one by that name is here." |
| Whisper with no message | Treat as empty input; ignored |
| `>` at start of non-whisper text | Only triggers if followed by a name + space + message |
| `>` in middle of text | Not detected as whisper (must be at position 0) |
| Only one NPC present | Whisper still works but is functionally identical to normal speech |
| Whisper + action: `@Padraig (whisper) *slides note*` | Combine whisper and action enrichment — both flags set |
| NPC responds to whisper loudly | The NPC's response is also marked whisper; prompt instructs them to whisper back |
| Save/load with private memories | `private` field serialized via serde; survives persistence |

## Testing

### Backend (cargo test)

1. **Whisper detection**: Parse `@Name (whisper) msg` and `>Name msg` correctly
2. **Memory privacy**: `context_string_public()` excludes `private: true` entries
3. **Memory inclusion**: `context_string()` (full) includes private entries
4. **NPC lookup**: Whisper to present NPC succeeds; absent NPC returns error message
5. **TextLogPayload serialization**: `whisper` and `whisper_target` fields serialize/skip correctly

### Frontend (Vitest)

1. **detectWhisper**: Both syntaxes parsed correctly
2. **detectWhisper**: Returns null for non-whisper input
3. **Chat rendering**: Whisper messages get `.whisper` class and dashed border
4. **Label**: "whispered to Padraig" shown above whisper bubbles

### Integration

1. Whisper to NPC A while NPC B is present → B doesn't reference the whispered content later
2. Whisper creates private memory → save → load → memory still private

## Files to Modify

| File | Change |
|------|--------|
| `ui/src/components/InputField.svelte` | Add `detectWhisper()`, modify submit to pass whisper metadata |
| `ui/src/components/ChatPanel.svelte` | Whisper-specific rendering (dashed border, italic, label) |
| `ui/src/lib/types.ts` | Add `whisper` and `whisper_target` to `TextLogEntry`, `TextLogPayload`, `SubmitInputRequest` |
| `ui/src/lib/ipc.ts` | Extend `submitInput()` to accept whisper options |
| `crates/parish-server/src/routes.rs` | Add `handle_whisper_conversation()`, modify `submit_input` handler |
| `crates/parish-core/src/ipc/types.rs` | Add whisper fields to `TextLogPayload` |
| `crates/parish-core/src/npc/memory.rs` | Add `private` field to `MemoryEntry`, add `context_string_public()` |
| `crates/parish-core/src/npc/ticks.rs` | Use `context_string_public()` for Tier 2 context building |

## Effort Estimate

**Medium** — the frontend syntax detection is straightforward, but the backend needs careful context scoping to ensure whispered content doesn't leak to other NPCs through memory or Tier 2 simulation. The memory `private` flag and context exclusion are the critical correctness requirements.
