# Repository Guidelines — Rundale on the Parish Engine

`AGENTS.md` is the source of truth for repo guidelines. `CLAUDE.md` is a symlink to it, so any edit here is automatically visible to Claude Code as well.

**Before anything else, skim [LEARNINGS.md](LEARNINGS.md)** — brief
bullet list of gotchas and surprising defaults that aren't obvious from
reading code or git history. Append a bullet whenever you discover
something a future agent would benefit from knowing. The
`Stop--learnings-reminder` hook nudges you to revisit it on non-trivial
sessions.

Start with the detailed agent docs in [docs/agent/README.md](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, and Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout and module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — Tokio, SQLite, Ollama, mode parity pitfalls
- [git-workflow.md](docs/agent/git-workflow.md) — commits, tests, and PR standards
- [agent-check.md](docs/agent/agent-check.md) — proof evidence and judge verdict gate
- [skills.md](docs/agent/skills.md) — `/check`, `/parish-engine`, `/backlog`, etc.
- [harness.md](docs/agent/harness.md) — one-page map of every sensor, skill, and gate (start here when something fails)
- [codebase-map.md](docs/agent/codebase-map.md) — top-level directory index with per-area `CLAUDE.md` pointers

**Rundale** is the game (Irish living world, 1820). **Parish** is the engine (Rust workspace + frontends).

## Current project state (quick map)

- Rust workspace: **24 crates** under `parish/crates/` — see [docs/agent/architecture.md](docs/agent/architecture.md) for the full table.
  - Binaries: `parish-engine` (headless entry point — `--headless` / `--script FILE` / Tauri-launch; despite the name it is a thin binary, the engine is `parish-core` + leaf crates), `parish-server` (Axum HTTP/WS server, library + binary), `parish-tauri` (desktop), `parish-client` (binary `parish`, thin HTTP client — see [Ways to run Parish](README.md#ways-to-run-parish)), `parish-mcp`, `parish-scenario`, `parish-geo-tool`, `parish-npc-tool`.
  - Composition: `parish-core` re-exports the leaf crates under stable namespaces.
  - Leaf logic crates: `parish-chronicle`, `parish-config`, `parish-diagnostics`, `parish-editor`, `parish-inference`, `parish-input`, `parish-mod`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-providers`, `parish-setup`, `parish-types`, `parish-world`.
  - These crates make up the **Parish** game engine.
- Frontend: `parish/apps/ui/` (Svelte 5 + TypeScript)
- Rundale game content: `mods/rundale/`
- Test fixtures: `parish/testing/fixtures/`
- Deploy artifacts: `deploy/`
- Documentation hub: `docs/index.md`

## Non-negotiable engineering rules

Rules marked **(enforced)** are checked mechanically by `cargo test` / CI — see `parish/crates/parish-core/tests/architecture_fitness.rs`. The rest are still convention.

1. **Module ownership (enforced):** Shared logic belongs in a leaf crate; `parish-core` composes them. Never duplicate leaf-crate logic in `parish/crates/parish-engine/src/`. Orphaned source files (on disk but not declared as `mod`) are rejected. Crate map: [docs/agent/architecture.md](docs/agent/architecture.md).
2. **Mode parity (partially enforced):** Tauri, headless CLI, and web server must share behavior. The fitness test forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` / `wry` / `tao`. Wiring parity (every IPC handler called from every entry point) is still convention.
3. **Tests with behavior changes:** Add/adjust tests for every behavior change.
4. **Gameplay proof:** For gameplay features, run `/parish-engine prove <feature>` — unit tests alone are not sufficient.
5. **No unexplained `#[allow]`:** Only with explicit justification.
6. **Feature flags for new engine/gameplay features:** Gate with `config.flags.is_enabled("feature-name")`, default-on, document in the PR.
7. **Keep README.md up to date:** feature list, repository structure, credits. Run `just notices` when dependencies change.
8. **Five Whys before patching:** Diagnose bugs, regressions, and unexpected behavior with the `/five-whys` skill (or the method) to reach root cause first.
9. **Resolve runtime paths from explicit config, not the cwd:** Resolve saves/mods/data dirs once at startup and store on `AppState` / `GlobalState`; never `current_dir()`, parent-walks, or marker-file searches from request handlers (#771). Resolver APIs, platform roots, and env overrides: [docs/agent/gotchas.md](docs/agent/gotchas.md).
10. **Truthful test automation:** Anything named or scheduled as a test must execute the
    production behavior it claims to cover, contain a machine-checkable oracle, and
    propagate failures to its caller. Exploratory proof scripts belong outside regression
    test directories. Every confirmed escaped bug gets a regression test at the lowest
    production seam that would have caught it. Scheduled automation must be green,
    explicitly paused with a tracking issue, or removed.
11. **Scaling guardrails:** Any PR touching `AppState`, session persistence, real-time push, inference calls, identity lookups, mod loading, or request-ID tracing must be reviewed against the seam checklist in [docs/agent/scaling-rules.md](docs/agent/scaling-rules.md).
12. **Cross-runtime orchestration belongs in `parish-core`:** Game-loop, IPC, and session handlers shared by the server, Tauri, and CLI entry points — including their constants, payload structs, and helpers — are defined once in a backend-agnostic crate, parameterized via traits (e.g. `EventEmitter`); entry-point crates are thin wiring. Never copy an orchestration body, constant, or payload struct into a second entry-point crate — the divergence is invisible at review time and silently produces security drift (#687, #696).
13. **[REMOVED]**
14. **Validate artifact content, not just the envelope:** A handler returning a produced artifact (screenshot, export, render, generated file) must verify real content before reporting success — a blank/degenerate result is an `Err`, never "nonzero bytes = success". Pattern: `reject_blank_capture` in `parish/crates/parish-tauri/src/commands/screenshot.rs` (#1301).
15. **Dialogue prompts must ground the model in the actual world:** Every NPC system prompt includes a `PEOPLE YOU KNOW` and a `PLACES IN THIS PARISH` list with instructions to decline to confirm anyone or anywhere not on them. Enforcement: `build_enhanced_system_prompt_with_config` in `parish-npc/src/ticks/prompt.rs` (`location_names` must be `Some(...)` in production); test: `parish-core/tests/dialogue_prompt_anchor.rs`; flag `npc-dialogue-grounding`, default-on (#1394).
16. **Cap external API payloads before sending:** Validate payload size client-side against the provider's documented limit (e.g. GitHub issue body ≤ 65536 chars) and truncate to ≤ 90% of it with a `[truncated N chars]` marker — never rely on the provider's 4xx. A test asserting `payload.len() <= budget` is required for every such code path (#1375).
17. **Survey existing tooling before building any:** Check whether the repo already provides it — the `parish-harness` binary (built-in web server/dashboard), `justfile` recipes, `parish/scripts/**`, `.claude/skills/` — and run or extend the existing tool in place. Never hand-roll a throwaway duplicate.
18. **Report only verification you actually ran:** Any claim that a test passed or a process ran must be backed by literal command output from the current session. State skips and failures explicitly — never estimate, extrapolate, or fabricate a result.
19. **Production scope means end-to-end, not minimum arguable plumbing:** Rundale is a production-quality game, engine, and toolset. When an issue asks for a production pipeline/workflow/tool, implement the full stated workflow and proof path from source-of-truth inputs through generated/reviewed outputs and runtime integration. Do not downgrade scope to cached artifacts, manual intermediates, placeholders, or "assembly only" unless the issue explicitly says that is acceptable. If provider access, credentials, or product constraints block the true end-to-end workflow, mark the work incomplete/blocked instead of calling it done.
20. **Character-art identity must be distinct across the cast:** Encode stable structured facial geometry separately from age, affect, hair/headwear topology, wardrobe, and props. Hair/headwear must expose machine-comparable front, rear, covering, and overall-silhouette families; reject missing, duplicate, or near-duplicate facial vectors and repeated hairstyle topology within relevant cohorts before generation. A pair matching itself is not enough: shared full-face style references must not become an identity prior for unrelated characters, and approval must compare each result against the full cast.
21. **Persist billable external artifacts before validating them:** When an external API returns a non-deterministic generated artifact, write the raw bytes, content hash, provider request ID, and source provenance before content validation. Store each attempt immutably: a rejection must retain that raw artifact and link it from a failure receipt, and a retry must never overwrite an earlier paid response.
22. **Validate generated art against the asset-specific visual contract:** File format, dimensions, and nonblank pixels are necessary but insufficient. Keep portrait, marker, scene, and UI-art prompts/references separate; encode machine-checkable composition and style signals where practical (for example bounds, fill, ink density, or palette), retain human review for semantic judgment, and test that a representative wrong-style artifact is rejected.
23. **Character markers are character-only cutouts:** Make marker identity readable from face, hair/headwear, clothing, body shape, and stance. Reject held or carried objects, extra people, furniture, architecture, vegetation, scenery fragments, ground planes, and shadows unless an issue explicitly opts into contextual markers; worn clothing and headwear remain valid identity cues.
24. **Commit generated files as transactions:** Finish every fallible preparation and handled-failure cleanup before the final source-snapshot comparison, then perform the same-filesystem rename immediately. Inject source mutation at the last cleanup seam and exercise competing snapshots across processes.
25. **Launch portable Node tools without a shell:** Cross-platform Node automation must use `shell: false` and invoke JavaScript CLI entry points through `process.execPath`, never platform wrappers such as `.cmd`; execute the default path in tests with spaces in filesystem paths.
26. **Keep shared-target artifacts worktree-coherent:** When a build script embeds worktree-local files, parallel test tooling that shares a Cargo target must key the build to those inputs and preserve and validate the resulting executable before releasing its coordination lock. Never launch the shared final binary after Cargo releases its own lock; use the Playwright managed-server helper as the reference (#1717).
27. **Make managed-test lifecycles crash-safe on every platform:** Locks, candidates, and copied executables must have bounded startup recovery and mechanically tested platform policy. A fresh active-use lease is a capability: its heartbeat owner must fence and stop the child if ownership is lost, while a pruner must preserve lazily read artifacts and fail closed on malformed state. Retirement must atomically replace a stale lease with a tombstone, preserve its artifacts for another full grace, and reclaim only in a later pass. Test the real process-manager paths: POSIX group signals must leave the launcher alive to stop/wait the child and release ownership; Windows `taskkill /T /F` may skip hooks, so bounded expiry is the required fallback (#1717).
28. **Preserve merged end-to-end contracts when extending or extracting a shared spec:** Retain every already-merged public-behavior assertion across API, IPC, gameplay, and UI surfaces, and add new coverage alongside it; bounded behavior must cover the cap boundary and overflow. A focused test for the new slice does not prove earlier consumer contracts still exist.
29. **Audit player-facing interaction models, not only controls:** Before declaring a material visual UI change complete, run [the illustrated-notebook UX audit](.github/prompts/rundale-illustrated-notebook-ux-audit.prompt.md) against the live desktop and mobile surface. A label must lead to its ordinary semantic destination; persistent-object navigation (such as notebook tabs) renders in that object, visibly distinct controls have visibly distinct outcomes, and any deliberate overlay preserves the object’s visual and task continuity. Record the interaction model, screenshots/recording, findings, and semantic Playwright coverage. Functional tests that only assert that an overlay opened are insufficient.
30. **Ground background simulation in canonical state (enforced):** LLM simulation prompts must carry each participant's exact current location and authored current activity. Every async result must retain immutable fingerprints plus a monotonic participant-lineage revision from the canonical snapshot used to generate it, then revalidate those anchors at the one public shared apply seam; stale, restored-branch, ABA, missing-anchor, and contradictory-known-location results must have zero side effects (#1785).
31. **Preserve live play signals and isolate replacement contexts:** The first viewport must visibly retain player input, narration, streamed NPC dialogue, location changes, and state-derived notebook content. Successful new-game/branch/reconnect replacement clears prior prompt, presentation, dedup, and retained-event state while preserving a lifetime-monotonic cursor; failure preserves the old context (#1774, #1778, #1782, #1783).
32. **Player-visible gameplay status comes from authoritative state:** Consequential player actions must mutate canonical engine state, publish semantic events, survive save/load and journal recovery, and flow through shared IPC in every runtime. Local UI drafts and production placeholders must not masquerade as progression (#1781).
33. **Validate semantic model output at the canonical apply seam:** Prompt contracts and deterministic guards must share canonical constants. Schema-valid model fields must not change identity, relationships, hints, recommendations, or other gameplay/UI metadata until cross-checked against authored world data and the final delivered dialogue (#1776, #1779, #1786, #1788–#1790).
34. **Commit save/session identity before publishing a runtime:** Acquire a candidate save lock before any SQLite open, persist a complete candidate and atomic active-identity marker, then perform only infallible live-state publication. Cold restore/create must single-flight and recheck under one lifecycle gate; lock, marker, registry, or recovery failure must not fall back to another ledger, publish a session, or start workers.
35. **Long-running HTTP harnesses must prove session and interaction continuity:** Reuse one authoritative server session across more requests than the configured admission cap, including local HTTP when production cookies are `Secure`, and reset/reacquire dialogue after terminal interactions so every counted sample actually reaches inference. A harness that silently creates sessions or counts post-farewell no-op commands invalidates soak/reliability evidence.
36. **Promote local-inference presets only from passing production evidence:** A recommended model/backend/sampling/hardware profile requires a content-addressed promotion receipt from the frozen production-prompt holdout that passes dialogue and multiturn quality, hard-failure, parser-soak, guard-intervention, p95 latency/throughput, and memory-headroom gates. Development-split scores, preliminary leaderboard rows, hand-entered summaries, and average quality alone must never change a shipped recommendation.
37. **Treat model termination as part of the response contract:** Streaming provider clients must reject every non-success finish reason (for example `length` / `MAX_TOKENS`) instead of parsing or displaying the partial body. Mandatory-reasoning profiles must budget and measure reasoning-token headroom separately from the player-visible response; a low effort label is not a token ceiling.
38. **Separate qualification policy by serving topology:** Local promotion latency gates must not be reused for routed cloud models. Cloud screening hard-gates structural validity, guard intervention, evidence completeness, and request reliability; latency and throughput rank qualified profiles unless a separately measured product SLO explicitly makes one a release gate.
39. **Give judges the complete evidence needed by their rubric:** Any quality axis that depends on system-prompt facts—character identity, mood contract, known people, known places, or period rules—must receive those exact production facts in the judge bundle. Persist every paid judge attempt before validation, journal retries immutably, and trip a batch-wide circuit breaker on authentication, billing, quota, or rate-limit failures.
40. **Cloud dialogue qualification requires independent judge families:** Never let a model family vote on itself. A promotable cloud profile needs at least two eligible judge families, uses the policy-defined consensus statistic, and routes split pass/fail votes or excessive score spread to an explicit adjudication state. Same-family judgments may remain visible as diagnostic evidence but cannot count toward promotion.

## Standard commands

```sh
just build         # cargo build (default member parish-engine)
just run-client    # cargo run -p parish-client (thin HTTP client → running server)
just run           # cargo tauri dev
just run-headless
just check         # fmt + clippy + tests
just agent-check   # proof evidence + judge verdict gate
just verify        # check + harness walkthrough

just ui-test       # frontend unit tests
just ui-e2e        # Playwright end-to-end tests
just screenshots   # regenerate docs/screenshots/*.png
```

## Driving Parish via MCP (`parish-mcp`)

`.mcp.json` at the repo root registers `parish-mcp` as a project-level MCP
server. When you start a Claude Code session here, the tools below are
available as `mcp__parish__*`:

| Tool                       | Effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parish_world_snapshot`    | Read clock, player location, weather, recent log.                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_map`               | Read the location graph plus the player's position.                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `parish_npcs_here`         | List NPCs co-located with the player.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `parish_engine_state`      | Read the canonical deterministic engine state for QA validation — `active_scene`, `clock`, `weather`, `player`, `npcs`, `grapevine`. Assert the UI against this after each interaction to detect UI-vs-engine drift (#1331).                                                                                                                                                                                                                                                                             |
| `parish_save_state`        | Read save-file / branch metadata.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_turn`              | Read recent canonical exchanges, the newest 20 unseen retained world events in chronological order, and current scene state. Pass its lifetime-monotonic `event_cursor` back as `since`; overflow drops the oldest unseen events and advances the cursor to the coherent current total.                                                                                                                                                                                                              |
| `parish_submit_input`      | Send player input — movement, action, dialogue, system commands. Optional `addressed_to` array scopes dialogue.                                                                                                                                                                                                                                                                                                                                                                                          |
| `parish_new_game`          | Start a fresh game on a new save branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_save_game`         | Save the current branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_load_branch`       | Load a branch by integer id.                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_status`      | Reads first-run setup state: `{implemented, complete, provider, model, base_url, has_api_key, has_env_key}`.                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_byok`        | Submits a BYOK provider config (`provider`, `api_key`, optional `base_url`/`model`). Persists to keychain + `parish.toml`, rebuilds the inference worker, emits `setup-done`.                                                                                                                                                                                                                                                                                                                            |
| `parish_latest_screenshot` | Read metadata for the most recent player-triggered screenshot (`path`, `taken_at`, `size_bytes`). Capture is initiated by pressing F2 in the live desktop window.                                                                                                                                                                                                                                                                                                                                        |
| `parish_file_bug`          | File a bug report (`title`, optional `description`/`context`). Bundles a live screenshot + recent logs + game state into a GitHub issue on the configured repo (`dmooney/rundale` by default) and returns the issue URL. Auto-appends a "black box" diagnostic payload — raw LLM prompt/response history, the `get_engine_state` snapshot, and the last raw user intent (#1331). In dry-run / no-token mode writes the composed report to disk (`created:false`, `bundle_path` set). For auto-QA agents. |
| `tauri_invoke`             | Generic escape hatch — call any backend command (e.g. `editor_*`, `get_debug_snapshot`) by name.                                                                                                                                                                                                                                                                                                                                                                                                         |

The MCP server is a _bridge_: it speaks HTTP to a running Parish backend on
`127.0.0.1:3030`. **Before using any `mcp__parish__*` tool**, ensure a
backend is up:

```sh
# Headless web server (works in any sandbox; recommended in CI / on the web):
bash parish/scripts/parish-mcp-backend.sh start    # spawn + wait for /api/health
bash parish/scripts/parish-mcp-backend.sh status   # report pid + health
bash parish/scripts/parish-mcp-backend.sh stop     # graceful shutdown

# Desktop, when a display is available — drives the live window:
cargo run -p parish-tauri -- --mcp-port 3030
```

If a tool call returns an MCP `isError: true` with `transport error: ...`,
the backend isn't running — call `parish-mcp-backend.sh start` first.

Two distinct things get called "MCP bridge" (#1366 §6) — be precise:
the `parish-mcp` **crate** is the stdio MCP server that exposes the
`mcp__parish__*` tools and forwards them over HTTP, while
`parish-tauri/src/mcp_bridge.rs` is the **embedded HTTP listener** inside the
desktop app that answers those forwarded calls when Tauri is the backend
(the web server answers them natively). For deeper context on both see
[parish/crates/parish-mcp/README.md](parish/crates/parish-mcp/README.md)
and [parish/crates/parish-tauri/src/mcp_bridge.rs](parish/crates/parish-tauri/src/mcp_bridge.rs).

## Driving Parish via the `parish` CLI client

The `parish` binary (`parish-client` crate) is a thin synchronous HTTP client for a
**running** Parish server. Calls `POST /api/command` and returns the full response in one
round-trip — no WebSocket, no polling.

```sh
# Start the server first:
bash parish/scripts/parish-mcp-backend.sh start     # port 3030
# or: just web 3001                                  # port 3001

# Drive it:
parish [--server http://localhost:3001] "look"       # single-shot
parish --script testing/fixtures/test_walkthrough.txt  # batch
parish                                               # interactive REPL
parish --json "go to the church" | jq .kind         # raw JSON

# PARISH_SERVER env var sets the default URL:
PARISH_SERVER=http://localhost:3001 parish "look"
```

Use `parish --script` for proof transcripts requiring real NPC inference.
Use `just run-headless --script` for deterministic, fast harness-level testing.

## Commit and PR expectations

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- One logical change per commit.
- PRs should explain behavior changes, link issues, list commands run, and include screenshots / updated Playwright baselines for visible UI changes.
