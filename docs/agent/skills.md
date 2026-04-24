# Agent Skills

Custom slash commands defined in `.agents/skills/` (with compatibility symlinks from `.claude/skills/` and `.skills/`):

| Skill | Description |
|---|---|
| `/check` | Run fmt + clippy + tests (quality gate) |
| `/game-test [script]` | Run GameTestHarness to verify game behavior |
| `/verify` | Full pre-push checklist (quality gate + harness) |
| `/screenshot` | Regenerate GUI screenshots via Playwright (headless Chromium) |
| `/fix-issue` | End-to-end GitHub issue workflow |
| `/chrome-test` | Live browser testing session via browser MCP tools |
| `/play [scenario]` | Play-test the game via script harness |
| `/prove <feature>` | Prove a gameplay feature works at runtime (required after implementing features) |
