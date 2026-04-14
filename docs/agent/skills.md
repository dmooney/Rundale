# Claude Code Skills

Custom slash commands defined in `.skills/` (with compatibility symlinks from `.claude/skills/` and `.codex/skills/`):

| Skill | Description |
|---|---|
| `/check` | Run fmt + clippy + tests (quality gate) |
| `/game-test [script]` | Run GameTestHarness to verify game behavior |
| `/verify` | Full pre-push checklist (quality gate + harness) |
| `/screenshot` | Regenerate GUI screenshots via Playwright (headless Chromium) |
| `/fix-issue` | End-to-end GitHub issue workflow |
| `/chrome-test` | Live Chrome browser testing session via Claude-in-Chrome MCP |
| `/play [scenario]` | Play-test the game via script harness |
| `/prove <feature>` | Prove a gameplay feature works at runtime (required after implementing features) |
