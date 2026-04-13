# Rundale — Claude Code Guide

Detailed guides live in [docs/agent/](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout & module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — tokio, rusqlite, ollama, mode parity
- [git-workflow.md](docs/agent/git-workflow.md) — commits, standards, /prove
- [skills.md](docs/agent/skills.md) — `/check`, `/verify`, `/prove`, `/play`, ...

## Non-negotiable rules

1. **Module ownership.** Shared game logic lives only in `crates/parish-core/`. The `parish-cli` crate re-exports it. Never duplicate modules under `crates/parish-cli/src/`.
2. **Mode parity.** Tauri, headless CLI, and the web server must all share behavior — implement in `parish-core` and wire from every entry point.
3. **Tests on every change.** Coverage must stay above 90% (`cargo tarpaulin`). Run `/check` before any commit and `/verify` before any push.
4. **Prove gameplay features.** After implementing any gameplay change, run `/prove <feature>` — unit tests passing is not enough.
5. **No `#[allow]`** without a justifying comment.
6. **Feature flags for new engine features.** Every new gameplay or engine feature must be gated behind a feature flag (enabled by default). Add the flag check via `config.flags.is_enabled("my-feature-name")` and document the flag name in the PR. Use `/flag disable <name>` to turn it off if issues arise in production.

## Quick start

```sh
just build       # cargo build (default member = parish-cli)
just run         # cargo tauri dev (desktop)
just run-headless
just check       # fmt + clippy + tests
just verify      # check + harness walkthrough
```

See [docs/index.md](docs/index.md) for the documentation hub.

## Coding behavior

Guidelines to reduce common LLM coding mistakes (via [Karpathy](https://github.com/forrestchang/andrej-karpathy-skills)). **Tradeoff:** these bias toward caution over speed — use judgment on trivial tasks.

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.
