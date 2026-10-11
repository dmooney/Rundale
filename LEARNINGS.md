# LEARNINGS — gotchas for future maintainers

Brief notes for traps that span several areas. A trap that belongs to one
directory goes in that directory's `AGENTS.md`, which loads when you work
there. Search this file for the subsystem you are changing rather than
reading it end to end. Keep items to 2-3 lines.

## Determinism and clocks

- **Exact no-time-advance assertions must freeze `GameClock` first.** Its default
  36× wall-clock mapping crosses a whole game second after about 28 ms, so a
  loaded parallel test can otherwise turn scheduler delay into a false mutation.
- **Game randomness must replay.** Roll with `dice::seed("<purpose>", &[game_minutes, ids...])` (`DiceRoll::seeded`, `seeded_n`), never the thread RNG, and never build player-visible or prompt lists by iterating a `HashMap`/`HashSet`: `NpcManager` and `Npc::relationships` are `BTreeMap` for id order. `tests/script_replay.rs` runs every fixture twice and fails on any byte difference. The script harness clock is detached from wall time (`GameClock::detach_from_wall_clock`); the live server clock is not.
- **`WorldGraph::location_ids()` comes from a `HashMap`, so its order changes per process.** Sort it before it reaches prompts or payloads; the dialogue prompt's `CURRENT AUTHORED FACTS` landmark lines varied between runs until #2041, which made request-body differentials flaky.
- **`NpcManager::npcs_at` iterates a `HashMap`.** Sort by id before its names reach journaled text (the scene presence line) or a test's expected string, or the order changes per process.

## Chronicle logs across runtimes

- **Subscriber must rebind on branch switch.** Server (`limerick-server/src/session.rs`) + Tauri (`limerick-tauri/src/setup.rs`) + CLI (`App::rebind_log_managers_if_branch_changed`) each compare `current_branch_id` per event and rebuild the manager on mismatch. Otherwise post-`/load`/`/fork` events land under the old branch's `logs/branch-<old>/`. Same applies to `LocationLogManager` (#1011 #1034).

## Player input and addressing

- **Absent-NPC feedback crosses three paths.** `extract_npc_mentions` supplies names, shared `handle_npc_conversation` must report absent targets before generic empty-location idle, and script-mode `GameTestHarness` has its own empty-room shortcut. Cover all three when changing NPC addressing.
- **A new player slash command has three discoverability seams.** Keep the Rust parser, shared help table, UI registry, and docs drift oracle aligned. Catalog growth also invalidates ambiguous autocomplete tests: use collision-resistant prefixes and run the `InputField` suite.
- **Explicit dialogue recipients must outrank semantic name mentions.** Presence questions intentionally scan the full NPC roster, so feeding every mentioned name into the recipient vector makes a question's subject emit false absence narration or an unsolicited reply; chip and `talk to … about …` recipients are authoritative.
- **An explicit `@NPC` token must resolve identity against the full authored roster before applying the presence filter.** If mention extraction starts from only co-located NPCs, the absent identity disappears and ambient dialogue can silently retarget a bystander; preserve the name, classify it as absent at the shared target seam, and return local feedback before inference.
- **Addressed dialogue still makes an intent call first.** `handle_game_input` classifies free text before routing even with `addressed_to` chips, so a scripted host must answer an `Intent` call before the `Dialogue` call. Unmet NPCs are labelled by brief description, not name, in dialogue events.
- **Only `talk to X` / `speak to X` name an explicit recipient in free text.** `ask X about Y` routes through natural mentions (present people only), so an absent or ambiguous `X` there is not an explicit address. Write addressee tests with `talk to`, a chip, `@Name`, or a leading vocative (`Widow, ...`).

## Dialogue apply seam and guards

- **Interrogative work turns have two distinct contracts.** “Is there work for me?” may admit a direct task assignment, while “who should I ask for work?” requires a grounded occupational referral; do not exempt both from dialogue obligations merely because they end in `?`.
- **A guard-activation rate is meaningful only with exact reasons and calibrated detectors.** A generic `display_pipeline` counter hid which canonical transform ran, while a narrow mood phrase list rewrote already-sharp local-model dialogue. Emit each apply-seam reason and keep real-output false positives as regression fixtures.
- **Atmosphere is supplemental intent metadata, not a mutually exclusive action.** Keep the player's main `Talk`/`Move`/`Interact` classification intact, validate model-proposed topics against the raw utterance, and render the cue at the shared game-loop seam so it cannot swallow an addressed NPC turn.
- **`dialogue-corrected` is too late to be a safety boundary.** A raw provider stream has already reached the player before that replacement event; drain candidate tokens internally and publish only the canonical apply result so rejected text and metadata have no transient UI or persistent effects.
- **Overlapping canonical guards need order-independent quality telemetry.** Determine whether a configured final display constraint is material against the complete accepted candidate before earlier semantic guards shorten it, but still apply the constraint only once after validation; otherwise coverage and optimization can expose telemetry that varies with guard order.
- **Dialogue grounding needs typed claims, not longer allow-list prose.** Preserve unresolved player-introduced people/places in a bounded location-scoped context, resolve pronouns only when one typed referent remains, and validate festival/role/workplace/location claims against the pre-inference authored snapshot before any effect.
- **Task assignment admission and atomic staging share `is_task_request_input`.** Add explicit acceptance/follow-up phrases there, not only at the dialogue apply seam; compound narrated work must also route locally to `Interact`, and matching must score affirmative clause-leading verbs rather than just the sentence's first verb.
- **Declarative multi-facet dialogue requests need typed completeness, not a question-mark heuristic.** Derive explicit obligations before inference, render them in player order, and recheck the final post-transform line at the canonical apply seam; a partial candidate must be replaced whole with an authored noncommittal answer that covers every facet.

## Inference routing across runtimes

- **NPC `dialogue` is served by the BASE provider/model slot, NOT per-category client resolution.** Only `intent` / `reaction` / `simulation` route through `GameConfig::resolve_category_client` (which reads `category_provider/model/base_url`). The dialogue path in `limerick-core/game_loop/npc_turn.rs` uses `config.model_name` + the single base worker built by `rebuild_inference_worker` (which only takes base provider/url/key). Consequence: setting `/provider.dialogue` / `/model.dialogue` updates the debug-snapshot `categories[dialogue]` entry but the actual dialogue inference STILL uses the base model — the snapshot lies. To BYOK the dialogue model you must change the BASE slot: `/provider`, `/url`, `/model`, `/key` (no `.dialogue` suffix). The harness `apply_byok` (#1365) special-cases `category=="dialogue"` → base commands, all others → dotted. Verify a rebind actually took effect by reading `inference.call_log[].model` after a real turn, never just the `categories` snapshot. The web player-dialogue path (`handle_npc_conversation`) uses this base queue too, so a benchmark must set the base slot to the dialogue candidate and route other categories away explicitly. Also: base `/url` (`SetBaseUrl`) and `/url.<cat>` (`SetCategoryBaseUrl`) commands were added in #1365 — and base `SetModel` / `SetCategoryModel` now emit `RebuildInference` (they previously did NOT, so a model-only change silently didn't rebind the worker).
- **Provider-default promotions need an exact four-category assertion.** Tauri/server materialize provider presets while CLI paths may inherit the base model, so merely checking every preset slot is nonempty permits runtime drift. The BYOK wizard must also persist a model only when it differs from the recommended preset; otherwise future default changes are pinned to yesterday's value.

## Proving gameplay

- **The offline `simulator` inference backend does NOT write to the debug-snapshot `inference.call_log`** (only cloud/HTTP backends do). In a keyless sandbox (no `ANTHROPIC_API_KEY` etc.) dialogue falls back to the simulator, so you cannot capture a Tier-1 `prompt_text` via `GET /api/debug-snapshot` even though NPCs visibly reply. To prove dialogue-prompt _content_ (e.g. that location/context reaches the model) without a model, assert against the prompt-builder's output directly from the real world graph — e.g. `WorldState::from_world_file("mods/rundale/world.json", id)` + `build_tier1_context` in an integration test (see `limerick-npc/tests/tier1_context_real_world.rs`).

## World content

- **`mythological_significance` is player-facing source text, not private author metadata.** The offline `listen` command renders it verbatim, including strings produced by `limerick-geo-tool`; keep both curated world entries and generator templates in restrained, period-appropriate in-world prose.

## Toolchain, Cargo, and dependencies

- **The shared Cargo target can reuse a test binary with another worktree's compile-time `CARGO_MANIFEST_DIR`.** A `limerick-mcp` test may then fail while reading a dead path such as `/private/tmp/<other-worktree>/limerick/crates/limerick-tauri/src/mcp_bridge.rs`, even though the file exists here. Rebuild only that stale package with `cd limerick && cargo clean -p limerick-mcp`; for the provider-registry tests, run the gate with an absolute source tree override (`LIMERICK_MODS_DIR="$PWD/mods" just check` from the repo root) so it loads the shipped cloud-provider mods deterministically.
- **Cargo's lock-only security patches can become compile-time breakage when a new target dependency activates them.** Before adding a Linux-reachable desktop crate, run the audit workflow's inverse-tree checks for patched packages; `rfd` defaults once made the intentionally unreachable `wayland-scanner` pin compile.
- **A Homebrew Cargo before the rustup proxy on PATH bypasses the pinned toolchain.** Verify `rustc --version` and run gates through the rustup proxy; LLVM coverage tools must match the compiler.
- **Two trees on the shared cargo target can link each other's crates.** Building a second worktree's `limerick-tauri` after the first reused the other tree's `limerick-core` fingerprint; when the older sources still compile against the newer crate, the mismatch is silent. Before each side's build, touch every file changed between the trees, as `prove_diff.py` does, and confirm the binary with a string only that side contains.

## Gates and formatting

- **Non-Rust lint/format is now gated in CI + `just lint-all`** (alongside the Rust gates). Per language: TS/Svelte = ESLint flat config (`limerick/apps/ui/eslint.config.js`) + Prettier (`.prettierrc`); docs/data = root Prettier (`prettier.config.js`, scoped to `**/*.{md,json,jsonc,yml,yaml}`) + markdownlint (`.markdownlint-cli2.jsonc`); Python = ruff + mypy + pytest + yamllint (`pyproject.toml`, `requirements-dev.txt`, local venv `.venv-dev`); shell = shellcheck + shfmt (`.shellcheckrc`, flags `-i 4 -ci -bn`); TOML = taplo (`taplo.toml`). Gotchas: (1) the UI uses **npm + Node 24 via fnm** — Node 26 + `engine-strict=true` makes `npm install` abort, so `fnm use 24` before any `npm`/`npx`. (2) `.prettierignore` is **gitignore syntax — no inline comments** (a `path  # note` line is treated as a literal pattern and silently fails to ignore). (3) taplo must run with `array_auto_collapse=false`, `array_auto_expand=false`, `align_comments=false`, `indent_string="    "`, and exclude globs need `**/target/**` (not `target/**`) or it mangles the hand-curated mod arrays and scans build artifacts. (4) yamllint must `disable` the `braces` rule — GitHub Actions `${{ }}` reads as flow-mapping braces. (5) the auto-fmt Claude hook (`.claude/hooks/PostToolUse--auto-fmt.sh`) now formats by extension (prettier/ruff/shfmt/taplo). (6) running pytest/mypy/ruff creates `.pytest_cache`/`.mypy_cache`/`.ruff_cache` — all gitignored AND in the prettier/markdownlint ignores.
- **`witness-scan` must ignore binary files explicitly.** PNG bytes can accidentally match placeholder regexes and report only "binary file matches"; guard each path with `grep -Iq . "$f" || continue` before running ripgrep.
- **Root `just check` does not run `just lint-shell`, and Windows repository text uses CRLF.** Run the shell gate after editing `.sh` files, and make repository-text sensors accept both `\n` and `\r\n` rather than assuming the checkout's line endings.
- **Generated JSON drift checks must compare parsed values when Prettier owns checked-in formatting.** `serde_json::to_string_pretty` and Prettier disagree on compact arrays, so byte equality makes the generator and repository formatting gates mutually exclusive.

## Graphify

- **Graphify `cluster-only` can collapse distinct merged semantic nodes that share a basename and label when only one has `source_location`.** After `merge-graphs`, verify the exact node count; for a false ghost-dedup, cluster the node-link graph directly rather than forcing the smaller rebuild.
- **Graphify 0.9.16 `merge-graphs` drops top-level hyperedges.** Restore them with repo-qualified IDs and members, then validate every member against the merged node set; discard already-dangling source hyperedges.
- **Graphify AST extraction must run from an importable script on spawn-based platforms.** Calling `graphify.extract.extract` from stdin makes multiprocessing workers look for a nonexistent `<stdin>` module and silently degrades to zero-node files; use a real file with an `if __name__ == "__main__"` guard.
- **Graphify ignore globstars do not cover files directly under the named directory.** Pair `area/**/*.png` with `area/*.png` (and likewise for other extensions), then verify the detected file list; otherwise top-level artifacts silently remain in a selective graph scope.
- **Generated `graphify-out/` trees need gate-level exclusions.** Exclude them before broad runtime prefixes such as `mods/*` in `agent-check`, and separately exclude them from repo-wide Prettier and markdownlint; otherwise graph-only PRs spuriously demand an evidence page or lint/reformat generator-owned artifacts.

## Shell

- **`awk -v var=value` interprets backslash escapes in `value`.** `\n` becomes a real newline mid-string. To insert a multi-character literal containing `\n` (e.g. a `printf` format), write the trap to a temp file and use `getline < tf` inside `BEGIN`, not `-v`.
- **Hooks under `set -euo pipefail` need `|| true` at the END of every command-substitution pipeline.** `grep` no-match exits 1 → pipefail propagates → silent exit. Visible only as "Stop hook error: Failed with non-blocking status code: No stderr output".
- **zsh reserves lowercase `path` as its executable-search array.** A
  `while read path` loop rewrites command lookup and makes tools appear missing;
  use a task-specific name such as `candidate_path` in cleanup scripts.

## Git and GitHub

- **`gh pr view --json baseRepository` was removed** ("Unknown JSON field: baseRepository"). Derive `<owner>/<repo>` from `gh pr view --json url --jq .url` + `sed` instead.
- **`gh pr edit` cannot change a PR's head branch.** If a sub-agent pushes a redo to `branch-v2` instead of the original branch, the PR keeps tracking the old SHA. Recovery: `git push origin +refs/remotes/origin/branch-v2:refs/heads/original-branch` to force-rewrite the PR's branch, then delete `branch-v2`.
- **A worktree's branch may be locked to a _different_ worktree.** If `git checkout <branch>` fails with `'<branch>' is already used by worktree at <path>`, the orchestrator pre-created that branch in another agent worktree; that sibling may have an _untracked_ fixture but **no commits / nothing pushed**. Don't fight the lock — create a distinct local branch (e.g. `<branch>-impl`) in your own worktree and open the PR from it. (Hit during #1379.)
- **GitHub merge queues are unavailable on user-owned repositories.** Use strict required checks plus required conversation resolution as the fallback; the exact unblock trigger is transfer to an eligible organization or future GitHub support.
- **A GitHub milestone can hold issues outside its own theme.** "Mobile Phase 1 - Portable turn API" also carries proof-process issues (#2054-2057) filed mid-milestone; `gh issue list --milestone "..."` mixes them with the turn-API work, so a nonzero open count doesn't mean the phase's own feature work is unfinished. Check each issue's title/scope, not just the milestone's open-issue count.

## Agent process

- **Background `Bash run_in_background` reports the exit code of the WHOLE compound command.** `just check > log 2>&1; echo "EXIT=$?" >> log` makes the harness report the trailing `echo`'s exit (0), masking a real `just check` failure. Read the actual tool exit from a standalone invocation, or `grep` the log for `Recipe .* failed` / `error:` — don't trust the harness "exit code 0" on a chained command.
- **Do not add a `CLAUDE.md` anywhere on the path.** Claude Code (v2.1.277+) reads `AGENTS.md` natively and loads a subdirectory's `AGENTS.md` when it reads a file there, but only while no `CLAUDE.md` or `CLAUDE.local.md` sits in the working directory or above it; one such file switches `AGENTS.md` loading off. The `CLAUDE.md -> AGENTS.md` symlinks were removed on 2026-09-28 by owner decision. Claude Code ignores instructions under `.agents/`, so skills there reach it only through the `.claude/skills` symlink.
- **A fresh visual branch is not a fresh implementation.** For “start fresh,” inventory and exclude rejected code, assets, and proof; retain only explicit user-approved exceptions, and capture a new comparison against the named concept.
- **Never hand off with a known verification error just because it predates the branch.** If an in-scope gate exposes an existing failure, repair it before publishing or keep the work explicitly blocked; “already on main” is context, not an acceptable final state.
