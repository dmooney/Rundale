# Agent Skills

Custom slash commands defined in `.agents/skills/`, with `.claude/skills/` as a compatibility symlink:

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
| `/rubric` | Run snapshot-baseline + structural-rubric eval sensors over gameplay fixtures (sister to `/prove`) |
| `/feature-scaffold <name>` | Depth-first decomposition: design note + failing fixture + plan, before any code |
| `/triage-backlog` | Apply theme + priority labels to open issues lacking them. Vocabulary in [`triage-vocabulary.md`](triage-vocabulary.md). Paired with the `triage-audit` weekly workflow. |
| `/crate-audit` | Audit workspace crate layout (naming, manifests, big-file splits, extraction candidates, README freshness). Produces a phased pure-relocation refactor PR. |
| `/techdebt [path]` | Continuous debt loop: process `TODO.md`, dispatch fix agents, discover new debt when empty, and repeat until no actionable debt remains in scope. |
| `/demo [turns] [pause]` | Run the LLM auto-player in the live Tauri app to verify Tauri IPC changes, surface streaming/frontend bugs, and generate NPC dialogue samples. Use after changes that the script harness cannot exercise. |
