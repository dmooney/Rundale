# Automated Chrome Test Plan

> Uses browser MCP tools for browser automation testing.
> Complement to the existing Playwright E2E tests (headless, mocked IPC).
> These tests run against a **live axum server** with real game state.

## Prerequisites

- Axum web server running: `cargo run -- --web 3001`
- `.env` configured with a valid LLM provider (or Ollama running locally)
- Chrome open with a browser automation extension connected

## Test Suites

### 1. Page Load & Initial State

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1.1 | Page loads without errors | Navigate to `http://127.0.0.1:3001`, read console | No errors in console |
| 1.2 | Status bar populated | Read page elements | Location name, time, weather, season all non-empty |
| 1.3 | Map renders | Check map panel for SVG content | SVG with location nodes visible, not "Loading map..." |
| 1.4 | NPCs sidebar populated | Read NPCs Here section | At least one NPC listed (at starting location) |
| 1.5 | Initial description shown | Read chat panel | Location description text present |
| 1.6 | Input field ready | Read interactive elements | Textbox with placeholder "What do you do?" and Send button |
| 1.7 | Debug button present | Find DBG button | Button with "Toggle debug panel" label exists |

### 2. Navigation & Movement

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 2.1 | Travel to adjacent location | Type "go to the crossroads", submit | Status bar shows "The Crossroads", travel narration in chat |
| 2.2 | Map updates on travel | After 2.1, check map | Player dot moved to new position |
| 2.3 | NPCs update on travel | After 2.1, read NPCs sidebar | NPC list reflects new location |
| 2.4 | Invalid location | Type "go to narnia", submit | "You haven't the faintest notion" message with exit list |
| 2.5 | Already-here detection | Type current location name, submit | "Sure, you're already standing right here." |
| 2.6 | Multi-hop travel | Travel A→B→C→A | All transitions work, player returns to start |
| 2.7 | Exit list accuracy | Compare exits in description to map edges | All listed exits correspond to adjacent map nodes |

### 3. NPC Conversation (requires live LLM)

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 3.1 | Talk to NPC | Type free text at location with NPCs, submit | NPC name label appears, followed by response text |
| 3.2 | Streaming tokens | Watch chat during NPC response | Text appears incrementally (not all at once) |
| 3.3 | NPC sidebar updates | After first conversation | NPC shows full name, role, mood |
| 3.4 | Irish word hints | After NPC response | Focail panel shows at least one Irish word with pronunciation |
| 3.5 | No NPC idle message | Talk at empty location | One of: "The wind stirs...", "Only the sound...", "A dog barks...", "The clouds shift..." |
| 3.6 | Input disabled during streaming | Submit input, immediately check input field | Input field disabled while response streams |

### 4. System Commands

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 4.1 | `/help` | Submit "/help" | Command list displayed (help, pause, resume, speed, status) |
| 4.2 | `/status` | Submit "/status" | "Location: {name} \| {time} \| {season}" |
| 4.3 | `/pause` | Submit "/pause" | "The clocks of the parish stand still." |
| 4.4 | `/resume` | Submit "/resume" after pause | "Time stirs again in the parish." |
| 4.5 | `/speed fast` | Submit "/speed fast" | "The parish quickens its step." + clock advances faster |
| 4.6 | `/speed normal` | Submit "/speed normal" | Speed resets message |
| 4.7 | `/speed invalid` | Submit "/speed banana" | Error message about unknown speed |
| 4.8 | Empty submit | Press Enter with empty input | Nothing happens, no error |

### 5. Debug Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 5.1 | Toggle open | Click DBG button | Debug panel appears below game area |
| 5.2 | Toggle close | Click X button | Debug panel hides |
| 5.3 | F12 shortcut | Press F12 key | Debug panel toggles |
| 5.4 | Overview tab | Open debug, click Overview | Clock time, location, tier summary shown |
| 5.5 | NPCs tab | Click NPCs tab | All NPCs listed with tier, mood, location |
| 5.6 | NPC in transit | Check NPCs tab after time passes | At least one NPC shows "In Transit" status |
| 5.7 | Inference tab | Click Inference tab | Provider name and model shown |

### 6. Theme & Time

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 6.1 | Time advances | Wait or use `/speed fast`, check status bar | Time label changes (Morning→Late Morning→Midday→etc.) |
| 6.2 | Theme palette updates | Observe CSS variables over time | Background/text colors shift with time of day |
| 6.3 | Clock in debug matches status bar | Compare debug Overview clock to status bar | Consistent time shown |

### 7. Error Handling & Edge Cases

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 7.1 | Special characters in input | Type `<script>alert('xss')</script>` | Text rendered as-is, no script execution |
| 7.2 | Very long input | Type 500+ character string | Server handles it, no crash |
| 7.3 | Rapid repeated submits | Submit 5 commands quickly | All processed in order, no duplication |
| 7.4 | WebSocket reconnect | Kill and restart server, wait 2s | Page reconnects, continues working |
| 7.5 | Console clean | Check console after full test run | No errors or uncaught exceptions |

## Automation Notes

When automating with browser MCP tools:

- Inspect or create a tab at session start to get tab IDs
- Use `navigate` to load the page
- Use `read_page` with `filter: "interactive"` to find input/button refs
- Use `form_input` to set text in the input field
- Use `computer` with `action: "key"` and `text: "Return"` for Enter submission
- Use `computer` with `action: "wait"` (max 10s) for LLM response time
- Use `computer` with `action: "screenshot"` for visual verification
- Use `read_console_messages` with `pattern: "error|Error"` for error checks
- Use `read_page` to verify text content appeared in the DOM
- Ref IDs change after navigation — always re-read after page transitions
