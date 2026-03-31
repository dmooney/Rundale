---
name: chrome-test
description: Run a live Chrome browser testing session against the Parish web server using Claude-in-Chrome MCP tools. Builds frontend, starts server, navigates Chrome, and runs through the test plan.
---

Run an interactive Chrome browser test session for Parish. Follow the test plan
in `docs/plans/chrome-test-plan.md`.

## Setup

1. **Build frontend**: `cd ui && npm run build`
2. **Check if server is running**: `curl -s -o /dev/null -w "%{http_code}" http://localhost:3001/`
3. **Start server if needed**: `cargo run -- --web 3001` (run in background, wait for 200 on health check)
4. **Connect Chrome**: Call `mcp__claude-in-chrome__tabs_context_mcp` with `createIfEmpty: true`. If "No Chrome extension connected" error, tell the user to enable the extension and retry.
5. **Create/navigate tab**: Create a new tab or use an existing one. Navigate to `http://127.0.0.1:3001`.

## Test Execution

Run through these test categories from the test plan. Take screenshots at key
points. Track pass/fail for each test.

### Required Tests (always run)
- **Page Load**: Verify status bar, map, NPCs sidebar, chat panel, input field all render
- **Navigation**: Travel to at least 2 locations, verify map/status/NPCs update
- **Edge Cases**: Invalid location, already-here, empty submit
- **System Commands**: `/help`, `/status`, `/pause`, `/resume`
- **Console Check**: Read browser console for errors at start and end

### Optional Tests (run if LLM provider configured in .env)
- **NPC Conversation**: Talk to an NPC, verify streaming response
- **Irish Words**: Verify Focail panel populates after NPC conversation
- **Idle Message**: Talk at empty location, verify atmospheric message

### Optional Tests (run if explicitly requested)
- **Debug Panel**: Toggle open, check all tabs
- **Speed Commands**: `/speed fast`, `/speed normal`
- **Theme Updates**: Observe palette changes over time

## Reporting

After testing:

1. **Summary**: Print a pass/fail table of all tests run
2. **Bugs**: List any bugs found with reproduction steps
3. **Console**: Report any browser console errors
4. **Server logs**: Check server output for errors/warnings

If bugs are found, ask the user if they want GitHub issues filed.

Write the session results to `docs/reviews/chrome-testing-session.md` (append a
new dated section if the file already exists).
