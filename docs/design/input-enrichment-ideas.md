# Input Line Enrichment Ideas

> Parent: [Player Input](player-input.md) | [GUI Design](gui-design.md) | [Docs Index](../index.md)

Brainstorming ideas for making the text entry line richer and more interactive, drawing from chat apps (Slack, Discord, Telegram, iMessage, Twitch), social platforms (Twitter/X), and games with chat interfaces (Minecraft, MMOs, MUDs).

**Status**: Ideation — none of these are committed. Pick and choose.

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

## 6. Quick Reactions / Emoji Responses (Slack, Discord, iMessage, Telegram)

**Inspiration**: Slack's emoji reactions on messages, Discord reactions, iMessage tapbacks.

Let the player react to NPC dialogue with a quick gesture instead of typing a full response:

- Click a small reaction bar below the NPC's message: nod, laugh, frown, shrug, applaud
- Or type shortcodes: `:nod:`, `:laugh:`, `:suspicious:`
- The NPC receives the reaction as context: "The player nodded in response"
- Faster than typing "I nod" — reduces friction for roleplay

Keep reactions period-appropriate: no modern emoji, use gestures and expressions from 1820s Ireland.

**Effort**: Medium — frontend reaction UI + backend prompt injection.

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

Wild idea — but Parish's streaming NPC responses would look great on a Twitch stream.

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

## Priority Ranking

| Idea | Effort | Impact | Recommendation |
|------|--------|--------|----------------|
| `/slash` command autocomplete | Low | High | **Build next** — reuses @mention infra |
| Input history (Up/Down) | Low | High | **Build next** — table stakes UX |
| `*action*` emotes | Low | Medium | **Build soon** — enhances RP |
| Multi-line input | Low | Medium | Build soon |
| Typing indicator | Low-Med | Medium | Build soon — makes NPCs feel alive |
| Location quick-travel chips | Low | Medium | Build soon |
| Whisper syntax | Medium | Medium | Build later — needs context scoping |
| Quick reactions | Medium | Medium | Build later |
| Reply-to context | Medium | Medium | Build later |
| Inline rich preview | Medium | Low-Med | Nice to have |
| Tone indicator | Medium | Low-Med | Nice to have |
| Tab-complete nouns | Medium | Medium | Build later |
| Voice range modifiers | Med-High | High | Build later — great emergent gameplay |
| Contextual suggestions | High | High | Build later — needs LLM or rules |
| Streamer mode | Very High | Niche | Someday/maybe |
