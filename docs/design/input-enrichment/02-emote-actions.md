# Design: Emote / Action Prefix with `*asterisks*`

> Parent: [Input Enrichment Ideas](../input-enrichment-ideas.md) | Idea #2

## Overview

Let the player wrap text in `*asterisks*` to indicate physical actions rather than dialogue. The chat log renders these in italics and the backend sets an `action_mode` flag in the NPC prompt context, telling the NPC to respond to the physical action rather than treating it as speech.

## User-Facing Behavior

1. Player types: `*tips hat to Padraig*` and hits Enter
2. Chat log displays: _You tip your hat to Padraig._ (italicized, no quote marks)
3. The NPC responds to the physical gesture, not as if spoken to
4. Combined forms work: `@Padraig *slides a coin across the bar* Any news today?`
   - The action part is flagged as physical, the remaining text as dialogue
5. Partial asterisks are treated as plain text (no unmatched `*`)

## Frontend Changes

### ChatPanel.svelte

Detect `*...*` segments in message content and render them in italics.

#### Rendering Logic

Add a helper function to parse message content into segments:

```typescript
interface MessageSegment {
  text: string;
  italic: boolean;
}

function parseSegments(content: string): MessageSegment[] {
  const segments: MessageSegment[] = [];
  const regex = /\*([^*]+)\*/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(content)) !== null) {
    // Text before the asterisk match
    if (match.index > lastIndex) {
      segments.push({ text: content.slice(lastIndex, match.index), italic: false });
    }
    // The matched action text (without asterisks)
    segments.push({ text: match[1], italic: true });
    lastIndex = match.index + match[0].length;
  }

  // Remaining text after last match
  if (lastIndex < content.length) {
    segments.push({ text: content.slice(lastIndex), italic: false });
  }

  return segments.length > 0 ? segments : [{ text: content, italic: false }];
}
```

#### Bubble Template

Replace the plain `{entry.content}` rendering with segment-aware rendering:

```svelte
<!-- Inside the bubble -->
{#each parseSegments(entry.content) as seg}
  {#if seg.italic}
    <em class="action-text">{seg.text}</em>
  {:else}
    {seg.text}
  {/if}
{/each}
```

#### CSS

```css
.action-text {
  font-style: italic;
  color: var(--color-muted);
}
```

The muted color distinguishes actions from speech visually, even without the asterisks.

### InputField.svelte — Live Preview (Optional Enhancement)

As the player types `*...*`, the asterisk-enclosed text could render in italics within the contenteditable div using an input event hook. This is a nice-to-have but not essential for the first implementation — the chat log rendering is the priority.

## Backend Changes

### Input Classification — `crates/parish-core/src/input/mod.rs`

Add action detection to the input parsing pipeline. This runs after `classify_input()` returns `GameInput` and before intent parsing.

#### New Struct

```rust
/// Parsed player input with action segments extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichedInput {
    /// The raw input text as submitted.
    pub raw: String,
    /// Dialogue text (non-action portions).
    pub dialogue: Option<String>,
    /// Action descriptions extracted from *asterisks*.
    pub actions: Vec<String>,
    /// The @mention target, if any.
    pub mention: Option<String>,
}
```

#### Extraction Function

```rust
/// Extracts `*action*` segments from player input.
///
/// Returns the text with actions separated from dialogue.
/// Example: `@Padraig *slides coin* Any news?`
///   → actions: ["slides coin"], dialogue: "Any news?", mention: "Padraig"
pub fn extract_actions(raw: &str) -> EnrichedInput {
    let mut actions = Vec::new();
    let mut dialogue_parts = Vec::new();
    let mut remaining = raw.to_string();

    // Extract *action* segments
    let re = regex::Regex::new(r"\*([^*]+)\*").unwrap();
    let mut last_end = 0;
    for cap in re.captures_iter(raw) {
        let full_match = cap.get(0).unwrap();
        let action_text = cap.get(1).unwrap().as_str().trim();
        if !action_text.is_empty() {
            actions.push(action_text.to_string());
        }
        // Collect non-action text before this match
        let before = raw[last_end..full_match.start()].trim();
        if !before.is_empty() {
            dialogue_parts.push(before.to_string());
        }
        last_end = full_match.end();
    }
    // Trailing text after last action
    let trailing = raw[last_end..].trim();
    if !trailing.is_empty() {
        dialogue_parts.push(trailing.to_string());
    }

    // Extract mention from the dialogue portion
    let dialogue_text = dialogue_parts.join(" ");
    let (mention, final_dialogue) = match extract_mention(&dialogue_text) {
        Some(m) => (Some(m.name), if m.remainder.is_empty() { None } else { Some(m.remainder) }),
        None => (None, if dialogue_text.is_empty() { None } else { Some(dialogue_text) }),
    };

    EnrichedInput {
        raw: raw.to_string(),
        dialogue: final_dialogue,
        actions,
        mention,
    }
}
```

### NPC Prompt Context — `crates/parish-core/src/npc/ticks.rs`

Modify `build_enhanced_context()` to accept `EnrichedInput` instead of a raw string. The context prompt changes based on whether the input contains actions:

#### Context Template Modification

Current context (in `tier1_context.txt`):
```
The player {player_action}
```

With action awareness:
```
// If actions + dialogue:
The player *slides a coin across the bar* and says: "Any news today?"

// If actions only (no dialogue):
The player *tips hat to you*. (This is a physical action, not speech.
Respond to the gesture naturally.)

// If dialogue only (no actions — existing behavior):
The player says: "Any news today?"
```

#### Implementation in `build_enhanced_context()`

```rust
fn format_player_input(enriched: &EnrichedInput) -> String {
    let action_str = enriched.actions.iter()
        .map(|a| format!("*{}*", a))
        .collect::<Vec<_>>()
        .join(" and ");

    match (&enriched.dialogue, enriched.actions.is_empty()) {
        (Some(dialogue), true) => {
            // Pure dialogue (existing behavior)
            format!("The player says: \"{}\"", dialogue)
        }
        (None, false) => {
            // Pure action
            format!(
                "The player {}. (This is a physical action, not speech. \
                 Respond to the gesture naturally.)",
                action_str
            )
        }
        (Some(dialogue), false) => {
            // Mixed action + dialogue
            format!(
                "The player {} and says: \"{}\"",
                action_str, dialogue
            )
        }
        (None, true) => {
            // Shouldn't happen (empty input), fall back
            "The player does nothing in particular.".to_string()
        }
    }
}
```

### Intent Classification

When actions are present, the local intent parser should classify the input as `Interact` rather than `Talk`:

```rust
// In parse_intent_local(), before movement/look checks:
let enriched = extract_actions(raw_input);
if !enriched.actions.is_empty() && enriched.dialogue.is_none() {
    // Pure action — classify as Interact, skip movement checks
    return Some(PlayerIntent {
        intent: IntentKind::Interact,
        target: enriched.mention.clone(),
        dialogue: None,
        raw: raw_input.to_string(),
    });
}
```

## Data Flow

```
Player types: "*tips hat* Good morning @Padraig"

Frontend:
  → getPlainText() returns "*tips hat* Good morning @Padraig"
  → submitInput("*tips hat* Good morning @Padraig")

Backend (routes.rs):
  → classify_input() → GameInput
  → handle_game_input()
    → extract_actions("*tips hat* Good morning @Padraig")
      → EnrichedInput {
           actions: ["tips hat"],
           dialogue: Some("Good morning"),
           mention: Some("Padraig"),
         }
    → handle_npc_conversation() with enriched input
      → format_player_input() produces:
        "The player *tips hat* and says: \"Good morning\""
      → This becomes {player_action} in the context prompt

NPC sees in context:
  "The player *tips hat* and says: \"Good morning\""
  → Responds to both the gesture and the greeting

Chat log renders:
  Player bubble: *tips hat* Good morning @Padraig
  (where "tips hat" is in italics)
```

## Edge Cases

| Case | Behavior |
|------|----------|
| Unmatched `*` | Treated as literal text, no extraction |
| Empty `**` | Ignored (empty action string filtered out) |
| Nested `*foo *bar* baz*` | Outer match: `foo *bar` — regex is non-greedy |
| `*action*` in NPC response | Also rendered italic in ChatPanel (consistent) |
| Multiple actions `*waves* *smiles*` | Both extracted as separate action items |
| Action + movement `*runs* go to the pub` | Movement keyword detected → `IntentKind::Move` takes priority |
| Very long action text | No limit enforced — truncation happens at the LLM token level |

## Testing

### Backend (cargo test)

1. **extract_actions** with pure action: `*waves*` → actions: ["waves"], dialogue: None
2. **extract_actions** with mixed: `*nods* hello` → actions: ["nods"], dialogue: Some("hello")
3. **extract_actions** with multiple: `*waves* *smiles*` → actions: ["waves", "smiles"]
4. **extract_actions** with mention: `@Padraig *tips hat*` → mention: Some("Padraig"), actions: ["tips hat"]
5. **extract_actions** with no actions: `hello there` → actions: [], dialogue: Some("hello there")
6. **extract_actions** with unmatched asterisk: `5 * 3 = 15` → actions: [], dialogue: Some("5 * 3 = 15")
7. **format_player_input** for each case: pure dialogue, pure action, mixed, empty

### Frontend (Vitest)

1. **parseSegments**: `"*waves* hello"` → [{text: "waves", italic: true}, {text: " hello", italic: false}]
2. **parseSegments**: `"hello"` → [{text: "hello", italic: false}]
3. **parseSegments**: `"*a* and *b*"` → three segments, two italic
4. **Rendering**: Check that `<em>` tags appear for italic segments in the chat panel

## Files to Modify

| File | Change |
|------|--------|
| `crates/parish-core/src/input/mod.rs` | Add `EnrichedInput` struct, `extract_actions()` function |
| `crates/parish-server/src/routes.rs` | Use `extract_actions()` in `handle_game_input()`, pass to conversation handler |
| `crates/parish-core/src/npc/ticks.rs` | Modify `build_enhanced_context()` to accept `EnrichedInput`, add `format_player_input()` |
| `ui/src/components/ChatPanel.svelte` | Add `parseSegments()`, render italic action text |

## Effort Estimate

**Low** — frontend is a simple regex + conditional `<em>` tag. Backend is a new extraction function and a prompt template adjustment. No new IPC commands or events needed.
