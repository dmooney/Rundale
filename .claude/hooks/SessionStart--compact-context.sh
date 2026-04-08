#!/usr/bin/env bash
set -euo pipefail

# SessionStart hook (matcher: compact): re-inject project context after compaction

cat <<'CONTEXT'
=== Parish Project Context (re-injected after compaction) ===

PROJECT: Parish -- An Irish Living World Text Adventure (Rust, Cargo workspace)

WORKSPACE MEMBERS:
- Root crate (src/): CLI entry point, headless mode, test harness
- crates/parish-core/: Pure game logic library
- crates/parish-tauri/: Tauri 2 desktop backend
- ui/: Svelte 5 + TypeScript frontend

QUALITY GATES (must pass before every commit):
  cargo fmt --check
  cargo clippy -- -D warnings
  cargo test
Use /check skill to run all three, /verify for full pre-push checklist.

GAME TEST HARNESS:
  cargo run -- --script testing/fixtures/test_walkthrough.txt
Outputs JSON. Always verify changes with the harness, not just unit tests.

GIT CONVENTIONS:
- Conventional commits: feat:, fix:, refactor:, docs:, test:
- Docs must be updated with every commit (README.md, CLAUDE.md, docs/)
- Test coverage must stay above 90%
- Never push without running /verify

CRITICAL FILES (do not edit directly):
- Cargo.lock (managed by cargo)
- mods/kilteevan-1820/world.json, mods/kilteevan-1820/npcs.json (world data, use geo-tool)

KEY PATHS:
- docs/index.md -- documentation hub
- docs/design/overview.md -- architecture
- testing/fixtures/ -- 20 test script files

=== End re-injected context ===
CONTEXT

exit 0
