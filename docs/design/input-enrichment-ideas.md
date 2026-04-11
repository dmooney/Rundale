# Input Line Enrichment Ideas

> Parent: [Player Input](player-input.md) | [GUI Design](gui-design.md) | [Docs Index](../index.md)

Brainstorming ideas for making the text entry line richer and more interactive, drawing from chat apps (Slack, Discord, Telegram, iMessage, Twitch), social platforms (Twitter/X), and games with chat interfaces (Minecraft, MMOs, MUDs).

**Status**: Wave 1 shipped. See priority table at the bottom for per-feature status.

---

## 1. `/slash` Command Autocomplete (Slack, Discord, Minecraft)

**Inspiration**: Slack's `/` menu, Minecraft's `/gamemode`, Discord's slash command picker.

When the player types `/`, show an autocomplete dropdown of all available system commands — identical UX to the `@mention` picker. Each entry shows the command name and a brief description.

- Filter as you type: `/sp` narrows to `/speed`, `/save`
- Show argument hints inline: `/fork <name>`, `/load <name>`
- Could replace the need to memorize commands or check `/help`
- Debug commands only appear when `--features debug` is active

**Effort**: Low — reuses the `@mention` dropdown infrastructure almost entirely.

---

## 2. Emote / Action Prefix with `*asterisks*` (MUDs, RP servers, Discord)

**Inspiration**: MUD emote commands (`/me waves`), Discord RP convention (`*sighs heavily*`), tabletop RP.

Let the player wrap text in `*asterisks*` to indicate physical actions rather than speech. The chat log renders these in italics and the LLM system prompt tells the NPC to respond to the action, not dialogue.

- `*tips hat to Padraig*` → displayed as: _You tip your hat to Padraig._
- `*examines the old map on the wall*` → NPC might narrate what you see
- `@Siobhan *slides a coin across the bar*` → combine with @mention

The backend would detect `*...*` wrapping and set `PlayerIntent.intent = Interact` or pass an `action_mode: true` flag in the NPC prompt context.

**Effort**: Low — mostly prompt engineering + CSS italic styling.

---

## 3. Input History with Arrow Keys (Terminal, Minecraft, every CLI ever)

**Inspiration**: Bash/zsh history, Minecraft chat history, MUD command recall.

Press Up/Down arrow to cycle through previous inputs. Essential for:

- Re-issuing a movement command: "go to the pub" → just press Up
- Editing a typo: press Up, fix, submit
- Replaying a question to a different NPC: Up, change @mention, submit

Store last N inputs (e.g., 50) in the Svelte store. Persist across sessions via localStorage.

**Effort**: Low — purely frontend, no backend changes.

---

## 4. Typing Indicator / NPC "Thinking" Animation (iMessage, Slack, Telegram)

**Inspiration**: iMessage's `...` bubble, Slack's "X is typing...", Telegram's "recording audio..."

When an NPC is generating a response (inference in progress), show a subtle indicator in the chat panel or next to their name in the sidebar:

- Three animated dots (`...`) in the chat area where their response will appear
- NPC's name in the sidebar gets a subtle pulse or "thinking" label
- Different animation for different cognitive states: Tier 1 NPCs "think carefully", quick responses could show brief hesitation

Currently we show "Waiting..." in the input placeholder. This is functional but cold — the typing indicator makes NPCs feel alive.

**Effort**: Low-Medium — frontend animation work, ties into existing `streamingActive` state.

---

## 5. Whisper / Private Message Syntax (Twitch, MMOs, MUDs)

**Inspiration**: Twitch `/w username`, WoW `/whisper`, MUD `tell`.

When multiple NPCs are present, let the player whisper to one so others don't "hear":

- `@Padraig (whisper) I saw Father Callahan at the fairy fort last night`
- Or use a dedicated prefix: `>Padraig the land agent is cheating you`

The NPC system prompt would note this was whispered privately. Other NPCs present don't incorporate it into their context. Creates gameplay possibilities — sharing secrets, conspiracies, confiding.

**Effort**: Medium — needs backend changes to exclude whispered content from other NPCs' context windows.

---

## 6. Bidirectional Emoji Reactions (Slack, Discord, iMessage, Telegram)

**Inspiration**: Slack's emoji reactions on messages, Discord reactions, iMessage tapbacks — but **both directions**.

### Player reacts to NPC messages

Hover over (or tap) any NPC message in the chat log to reveal a reaction picker. Click to attach a reaction emoji beneath the message:

```
Padraig Darcy:
  "Ah, the land agent was here again. Raised the rent on
   Murphy's place by a full shilling."
   😠 👀 😢
```

- The reaction is injected into the NPC's conversation context on the next exchange: "The player reacted with anger to your comment about the rent increase"
- NPCs adjust their behavior accordingly — a laugh might encourage gossip, anger might make them cautious, a suspicious look might make them clam up
- Multiple reactions accumulate: the NPC sees the pattern of your nonverbal responses over time

### NPCs react to player messages

When the player says something, present NPCs can spontaneously attach reactions to the player's message — **without generating a full dialogue response**:

```
You:
  "I heard Father Callahan was seen at the fairy fort."
   😳 (Padraig)   🤫 (Siobhan)
```

- Generated cheaply: the LLM returns a reaction emoji as structured output alongside (or instead of) a full response
- Multiple NPCs can react simultaneously — even NPCs who aren't the conversation target
- Creates a sense of a living room: say something provocative and watch the reactions ripple
- Could use a tiny/fast model or even rule-based keyword matching for common reactions

### NPC-to-NPC reactions

When an NPC speaks, other NPCs present can react too:

```
Padraig Darcy:
  "The harvest will be poor this year, mark my words."
   😟 (Siobhan)   🙄 (Niamh)
```

- Generated during Tier 2 background ticks — no extra latency for the player
- Reveals NPC relationships and opinions nonverbally
- Siobhan the farmer worries about the harvest; Niamh rolls her eyes at her father's doom-saying

### Reaction Palette

Period-appropriate gestures mapped to emoji. The UI shows emoji but the NPC context receives the natural language description:

| Emoji | NPC sees | When to use |
|-------|----------|-------------|
| 😊 | "smiled warmly" | Approval, friendliness |
| 😠 | "looked angry" | Disagreement, offense |
| 😢 | "looked sorrowful" | Sympathy, sadness |
| 😳 | "looked startled" | Surprise, shock |
| 🤔 | "looked thoughtful" | Pondering, interest |
| 😏 | "smirked knowingly" | Skepticism, irony |
| 👀 | "raised an eyebrow" | Curiosity, suspicion |
| 🤫 | "made a hushing gesture" | Secrecy, warning |
| 😂 | "laughed heartily" | Amusement |
| 🙄 | "rolled their eyes" | Dismissal, impatience |
| 🍺 | "raised a glass" | Toast, camaraderie |
| ✝️ | "crossed themselves" | Piety, superstition, shock |

### Implementation sketch

```
ChatPanel.svelte:
  - Each message gets a hover → reaction picker (row of emoji buttons)
  - Reactions stored in textLog entries: reactions: [{emoji, source}]
  - Rendered below message text, small and inline

Backend (commands.rs):
  - New IPC command: react_to_message(message_id, emoji)
  - Stores reaction in NPC conversation context for next exchange
  - New IPC event: npc_reaction — emitted when NPC reacts

NPC prompt injection:
  - "Recent nonverbal reactions from the player: smiled at your joke,
     looked angry when you mentioned the rent"
  - "React to the player's message with a single emoji from this set: [...]"

Tier 2 ticks:
  - When NPCs are in the same room, generate reactions to each other's
    statements as part of the group simulation prompt
```

**Effort**: Medium-High — reaction UI + backend context tracking + NPC reaction generation.

**Why this is worth the effort**: Reactions are the fastest form of player expression. They let you participate in a conversation without composing a sentence. And NPC reactions make a room full of characters feel alive — you can *see* Siobhan's worry and Niamh's eye-roll without either of them saying a word.

---

## 7. Location Quick-Travel Buttons (Minecraft coordinates, MMO fast travel)

**Inspiration**: Clicking coordinates in Minecraft chat, MMO teleport lists, map markers.

Show clickable location chips above or beside the input field for adjacent locations:

- `[Darcy's Pub]` `[The Church]` `[Murphy's Farm]`
- Clicking one is equivalent to typing "go to Darcy's Pub"
- Updates dynamically based on current location's exits
- Could also appear inline when NPCs mention locations: "You should visit **[the fairy fort]**"

The MapPanel already handles click-to-travel. This brings that affordance closer to the text flow for players who don't use the map.

**Effort**: Low — frontend chip components + existing movement IPC.

---

## 8. Multi-line Input / Shift+Enter (Slack, Discord, Telegram)

**Inspiration**: Slack's Shift+Enter for newlines, Discord multi-line input, Telegram's expandable input.

Allow Shift+Enter to insert a newline for longer messages. The input field expands vertically (up to ~4 lines) then scrolls. Enter alone still submits.

Useful for:
- Longer roleplay actions: describing a complex gesture
- Dictating a letter or message in-game
- Composing a prayer or song verse (period-appropriate)

**Effort**: Low — swap `<input>` for `<textarea>`, handle Shift+Enter vs Enter.

---

## 9. Contextual Action Suggestions / Smart Replies (Google Messages, iMessage, Twitch predictions)

**Inspiration**: Google Messages' smart replies, Gmail suggested responses, Twitch prediction prompts.

Show 2-3 contextual quick-reply chips above the input based on the current situation:

- At the pub with Padraig: `[Order a drink]` `[Ask about the news]` `[Tell a story]`
- At the church: `[Pray]` `[Speak to the priest]` `[Examine the headstones]`
- After an NPC says something surprising: `[Tell me more]` `[I don't believe you]` `[Change the subject]`

Could be generated by a lightweight LLM call (Tier 3 model) or hand-crafted per location type. Reduces blank-page paralysis for new players.

**Effort**: High — needs a suggestion generation system (LLM or rule-based), context-aware.

---

## 10. Streamer / Audience Mode (Twitch Chat, Twitch Plays)

**Inspiration**: Twitch Plays Pokemon, Twitch chat voting, audience participation games.

A spectator mode where multiple people can suggest inputs via a WebSocket:

- Suggestions appear as a vote tally above the input
- The "player" picks one, or the most-voted action auto-submits after a timer
- NPCs could react to the chaos: "Why do ye keep changing yer mind?"

Wild idea — but Rundale's streaming NPC responses would look great on a Twitch stream.

**Effort**: Very High — needs WebSocket server, voting UI, audience client.

---

## 11. Inline Rich Text Preview (Slack, Discord, Twitter)

**Inspiration**: Slack's message formatting preview, Discord markdown, Twitter's URL cards.

As the player types, show a live preview of how their input will be interpreted:

- `@Padraig` renders as a highlighted mention chip (blue, like Slack)
- `*action*` renders in italics
- `/command` renders with a command icon
- Location names auto-link: typing "the pub" highlights it as a recognized location

The input field becomes a mini rich-text editor while staying plain-text underneath.

**Effort**: Medium — needs a contenteditable div or overlay rendering layer.

---

## 12. Input Tone Indicator (original — no direct app analog)

**Inspiration**: Email tone detectors, Grammarly sentiment, but adapted for 1820s RP.

A subtle indicator beside the input showing how the NPC will likely perceive the player's tone:

- Friendly / Neutral / Hostile / Suspicious / Formal / Playful
- Updates as you type, based on keyword analysis
- Helps players calibrate — "will Padraig take offense at this?"
- Ties into the anachronism detection system (already built)

Could be a small colored dot or word that shifts: `tone: friendly` → `tone: confrontational`

**Effort**: Medium — needs a lightweight tone classifier (could reuse anachronism detection patterns).

---

## 13. Recent Conversation Context Chip (Telegram reply-to, Discord reply threads)

**Inspiration**: Telegram's reply-to-message, Discord's reply feature, Slack threads.

Let the player reference a specific previous NPC statement by clicking "reply" on it in the chat log. This pins a small context chip above the input:

- `Replying to Padraig: "The land agent came through yesterday..."` [x]
- The backend includes this specific quote in the NPC prompt context
- Avoids the "what were we talking about?" problem in long conversations
- Click [x] to dismiss

**Effort**: Medium — frontend reply UI + backend context injection.

---

## 14. Voice / Shout Range Modifier (MMOs, MUDs, Minecraft)

**Inspiration**: WoW `/say` vs `/yell` vs `/whisper`, MUD room-scope messaging, Minecraft chat range mods.

Let the player control the "volume" of their speech with prefixes or a toggle:

| Prefix | Range | Effect |
|--------|-------|--------|
| *(normal)* | Room | Only NPCs present hear you |
| `!` or CAPS | Shout | NPCs in adjacent locations may hear/react |
| `>` | Whisper | Only targeted NPC hears |
| `(thought)` | Internal | No NPC hears — narrated as inner monologue |

A shout at the crossroads might draw NPCs from the pub. A whisper ensures privacy. Inner monologue lets the player think "on screen" for roleplay flavor.

**Effort**: Medium-High — backend needs to propagate input to NPCs at adjacent locations.

---

## 15. Tab-Complete for Known Nouns (MUDs, Zork, CLI)

**Inspiration**: Classic MUD tab completion, bash tab completion, IDE autocomplete.

Press Tab to cycle through completable tokens based on what's in the current context:

- Location names: `pub` → `Darcy's Pub`
- NPC names (without @): `Padr` → `Padraig`
- Known objects: `sto` → `stone cross`
- System commands: `/s` → `/save`

Different from @mention — this is general-purpose completion for any recognized game noun.

**Effort**: Medium — needs a "known nouns" registry aggregating locations, NPCs, and objects.

---

## 16. Push-to-Talk Voice Input (Claude Code, Discord, Xbox Game Chat)

**Inspiration**: Claude Code's spacebar-hold voice mode, Discord push-to-talk, Xbox party chat, Siri hold-to-speak.

Hold spacebar (when the input field is empty) to speak instead of type. Release to transcribe and insert the text into the input field — then the player can review/edit before hitting Enter.

### UX flow

1. Player holds spacebar (input field must be empty or focused and empty to avoid capturing typing)
2. Microphone activates — a visual waveform/pulse indicator appears in/above the input field
3. Player speaks: "Go to the pub and talk to Padraig about the harvest"
4. Player releases spacebar
5. Transcribed text appears in the input field: `go to the pub and talk to Padraig about the harvest`
6. Player can edit, add @mentions, or just hit Enter to submit

### Implementation options

| Approach | Platforms | Latency | Offline | Dependencies |
|----------|-----------|---------|---------|-------------|
| **Web Speech API** | Windows + macOS | Low | No* | None — built into WebView |
| **Whisper.cpp sidecar** | All (incl. Linux) | Medium | Yes | ~75MB model, `cpal` for mic capture |
| **Whisper via Ollama** | All | Medium | Yes | Ollama (already required) |

\* Web Speech API on some platforms sends audio to cloud services for recognition.

**Recommended phased approach:**

1. **Phase 1**: Web Speech API — works immediately on Windows (WebView2/Chromium) and macOS (WKWebView). Zero new dependencies. Feature-detect and hide the button on unsupported platforms (Linux/WebKitGTK).
2. **Phase 2**: Whisper.cpp as a Tauri sidecar process for full offline, cross-platform support. Ship the `tiny` or `base` model (~75MB). Captures audio via Rust `cpal` crate, pipes to whisper, returns text via IPC.

### Platform support matrix

| Platform | WebView | Web Speech API | Whisper sidecar |
|----------|---------|----------------|-----------------|
| Windows | WebView2 (Chromium) | Works | Works |
| macOS | WKWebView | Works (uses Siri STT) | Works |
| Linux | WebKitGTK | Unreliable | Works (recommended path) |

### Frontend implementation sketch

```svelte
<!-- In InputField.svelte -->
<script>
  let isRecording = $state(false);

  function handleKeydown(e: KeyboardEvent) {
    // Hold space to record (only when input is empty)
    if (e.key === ' ' && text === '' && !isRecording) {
      e.preventDefault();
      startRecording();
    }
  }

  function handleKeyup(e: KeyboardEvent) {
    if (e.key === ' ' && isRecording) {
      stopRecording(); // triggers transcription → fills text
    }
  }

  function startRecording() {
    isRecording = true;
    const recognition = new webkitSpeechRecognition();
    recognition.lang = 'en-IE'; // Irish English!
    recognition.interimResults = true;
    recognition.onresult = (e) => {
      text = e.results[0][0].transcript;
    };
    recognition.start();
  }
</script>

{#if isRecording}
  <div class="recording-indicator">Listening...</div>
{/if}
```

### Considerations

- **Language**: Set recognition locale to `en-IE` (Irish English) for better handling of place names and Irish-English speech patterns
- **Privacy**: Web Speech API may send audio to cloud services; document this. Whisper.cpp is fully local.
- **Irish words**: Neither Web Speech API nor Whisper will handle Irish Gaelic words well. Player can always edit the transcription before submitting.
- **Keybinding conflict**: Only activate spacebar-hold when input is empty. When the player has typed text, spacebar inserts a normal space.
- **Tauri permissions**: Need to add microphone capability to `src-tauri/capabilities/default.json`
- **Visual feedback**: Show a waveform or pulsing dot during recording so the player knows the mic is active

**Effort**: Low (Web Speech API path) to Medium (Whisper sidecar path).

**Why this matters**: Voice input is faster than typing for natural language. Rundale is a conversation game — speaking to NPCs instead of typing to them is a natural fit. And with push-to-talk (not always-on), it stays intentional.

---

## Priority Ranking

| Idea | Effort | Impact | Status |
|------|--------|--------|--------|
| `/slash` command autocomplete | Low | High | **Shipped (Wave 1)** — unified dropdown with @mention |
| Input history (Up/Down) | Low | High | **Shipped (Wave 1)** — localStorage, 50 entries |
| Push-to-talk voice input | Low | High | Build next — Web Speech API phase first |
| `*action*` emotes | Low | Medium | **Shipped (Wave 1)** — italic rendering + backend action context |
| Multi-line input | Low | Medium | **Shipped (Wave 1)** — Shift+Enter for newline |
| Typing indicator | Low-Med | Medium | Build soon — makes NPCs feel alive |
| Location quick-travel chips | Low | Medium | **Shipped (Wave 1)** — adjacent location pills above input |
| Bidirectional emoji reactions | Med-High | High | Build soon — makes rooms feel alive |
| Whisper syntax | Medium | Medium | Build later — needs context scoping |
| Reply-to context | Medium | Medium | Build later |
| Inline rich preview | Medium | Low-Med | Nice to have |
| Tone indicator | Medium | Low-Med | Nice to have |
| Tab-complete nouns | Medium | Medium | **Shipped (Wave 2)** — derived noun store + Tab cycling |
| Voice range modifiers | Med-High | High | Build later — great emergent gameplay |
| Contextual suggestions | High | High | Build later — needs LLM or rules |
| Streamer mode | Very High | Niche | Someday/maybe |

## Wave 1 Implementation Notes

Shipped in a single commit. Key design decisions:

- **Unified dropdown**: The `@mention` and `/slash` dropdowns share a single `dropdownMode` state (`'mention' | 'slash' | null`), reusing all markup and CSS. Command list lives in `ui/src/lib/slash-commands.ts`.
- **History vs. dropdown**: ArrowUp/Down only triggers history when `dropdownMode === null` and the cursor is on the first/last line (compatible with multi-line editing).
- **Emote passthrough**: `*action*` text is sent as raw text to the backend (no IPC changes). `build_tier1_context` in `npc/mod.rs` detects the `*...*` wrapping and substitutes action-mode phrasing for the NPC prompt.
- **Multi-line**: The contenteditable div already had `white-space: pre-wrap` and `max-height: 6em`. Only change was Shift+Enter key handling and `getPlainText()` handling `<br>` / `<div>` nodes.
- **Travel chips**: Derived from the existing `mapData` store's `adjacent` flag. Hidden during streaming.
