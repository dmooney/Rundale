# Regression Coverage Audit — April 2026

## Executive summary

Parish has a **strong gameplay-correctness harness** (snapshot baselines,
structural rubrics, 74-test fixture suite, architecture-fitness gates)
and a **strong inference-protocol harness** (mocked-HTTP suites for
Ollama and OpenAI, exhaustive `resolve_config` precedence tests). Where
coverage is thin or absent, the gaps cluster in five places:

1. **Tauri / GUI runtime surface** — `parish-tauri` ships with zero
   tests; 11 of 17 Svelte components have no unit test (`SavePicker`,
   `DebugPanel`, the entire mod editor pane).
2. **Mod-content validation** — none of the 11 mod artefact loaders has
   a malformed-input test; prompt-template `{placeholder}` interpolation
   is unverified; the default mod's expected counts (22 locations, 23
   NPCs, 4 festivals) are not pinned by a smoke test.
3. **Cross-mode wiring parity** — enforced at the dependency level but
   not the wiring level. Per `CLAUDE.md` rule 2 this is acknowledged
   convention, but it is the open frontier.
4. **Provider breadth** — Anthropic has 54 in-source `#[test]` markers
   yet no mocked-HTTP integration test; 9 of the 13 advertised providers
   ride on the OpenAI-compatible path with no provider-specific smoke.
5. **Silent feature regressions on shipped code** — encounters never
   fire in-game, autosave isn't time-tested, festivals have no
   end-to-end fixture, the per-category `reaction` slash variants and
   the frontend slash-command registry (1/32) are barely covered.

Per-area reports live under [`docs/audits/regression/`](regression/).

> **Label note:** the user requested the `test-needed` label on each
> issue. As of audit time this label does **not** exist on
> `dmooney/Rundale`. Per the agreed policy, the issues below were filed
> with priority + theme labels only. Recommend creating
> `test-needed` and back-applying it.

## Per-area scorecard

| # | Area | Coverage tier | Top gaps (severity) | Report |
|---|------|---------------|---------------------|--------|
| 1 | Game World | **Good** (graph/BFS, time, weather state machine all well-tested) | Encounters never fire in-game (**P0**); no festival fixture (**P0**); fuzzy fixture not baselined (**P1**) | [01-game-world.md](regression/01-game-world.md) |
| 2 | NPC System | **Partial** (anachronism gold standard; LOD dispatch has unit but not integration) | Tier promotion/demotion no integration test (**P0**); Tier 4 no end-to-end fixture (**P0**); Intelligence Profile prompt mapping untested (**P1**) | [02-npc-system.md](regression/02-npc-system.md) |
| 3 | Player Input | **Partial** (~83% of 41 slash-command tokens referenced; ~22 are unit-only — 10 base commands + 12 per-category variants) | Per-category `reaction` (`/model.reaction` etc.) untested (**P0**); frontend slash-command registry only 1/32 (**P0**); `IntentKind::Interact` has no local-parse test (**P0**) | [03-player-input.md](regression/03-player-input.md) |
| 4 | Persistence | **Partial** (snapshot/roundtrip excellent; UI + autosave thin) | Autosave 45s timer no test (**P0**); `SavePicker.svelte` no test (**P0**); branch DAG layout untested (**P0**) | [04-persistence.md](regression/04-persistence.md) |
| 5 | LLM / Inference | **Partial** (Ollama + OpenAI gold standard; rest thin) | Anthropic has zero mocked-HTTP test (**P0**); 9 of 13 providers untested at wire shape (**P0**) | [05-llm-inference.md](regression/05-llm-inference.md) |
| 6 | GUI | **Thin** (~35% of components have unit tests) | `SavePicker.svelte` no test (**P0**); `DebugPanel.svelte` 5 tabs no test (**P0**); 6 of 7 editor components no test (**P0**) | [06-gui.md](regression/06-gui.md) |
| 7 | Mod System | **Thin** (`world.json` strong; everything else light) | No malformed-mod tests for any of 11 artefacts (**P0**); prompt template `{placeholder}` interpolation untested (**P0**); no round-trip mod-load count assertion (**P0**) | [07-mod-system.md](regression/07-mod-system.md) |
| 8 | Runtime Modes | **Partial** (web server good; Tauri none; wiring parity none) | Wiring parity has zero enforcement (**P0**); `parish-tauri` has zero test files (**P0**) | [08-runtime-modes.md](regression/08-runtime-modes.md) |
| 9 | Developer Tools | **Partial** (90 in-source unit tests; no integration) | No Overpass/Nominatim HTTP mock (**P1**); coordinate-resolver fallback chain untested (**P1**) | [09-dev-tools.md](regression/09-dev-tools.md) |

Coverage tiers: **Good** (multi-layer with rubrics) · **Partial**
(unit/integration but missing UI/E2E or rubric) · **Thin** (one layer
or worse).

## Cross-cutting findings

- **The leaf crates are healthier than the binaries.** `parish-world`,
  `parish-npc`, `parish-input`, `parish-config`, `parish-persistence`,
  `parish-inference` all have 100+ in-source tests and at least one
  integration test file. `parish-tauri` has zero. `parish-cli` is
  intermediate. Where bugs land, they tend to land at the
  binary/wiring layer that the harness is thinnest at.
- **The harness has world-class snapshot + rubric infrastructure**
  (`crates/parish-cli/tests/eval_baselines.rs:35-207`) that **only 3 of
  31 fixtures opt into**. Several P1 gaps below are simply
  "promote a working fixture into `BASELINED_FIXTURES`."
- **`just harness-audit`** (`scripts/harness-audit.sh`) already
  surfaces fixture-vs-feature gaps. This audit is complementary —
  scoped to features.md sub-features, including the UI surface,
  mod loaders, and providers that the harness-audit script doesn't
  enumerate.
- **A "test the prompt" gap appears in three reports.** NPC mood,
  intelligence profile, anachronism category, and prompt-template
  interpolation all flow into LLM prompts but no test asserts what
  string actually reaches the LLM. One `parish-inference` test that
  renders a full prompt and asserts content fragments would close
  multiple silent regressions at once.

## Prioritized backlog

The 15 P0/P1 issues below are filed individually on `dmooney/Rundale`.
P2 gaps and any P0/P1 overflow are tracked in a single rolled-up
issue. Each issue cites its area report.

### Filed individually (15)

- [ ] **[P0]** Game World — Travel encounters never fire in-game (`check_encounter` has zero callers)
- [ ] **[P0]** Game World — No festival fixture or end-to-end test for the four Irish festivals
- [ ] **[P0]** NPC — Tier promotion/demotion (proximity-driven LOD) has no integration test
- [ ] **[P0]** NPC — Tier 4 CPU rules engine has no end-to-end fixture
- [ ] **[P0]** Player Input — Per-category `reaction` (`/model.reaction`, `/provider.reaction`, `/key.reaction`) untested
- [ ] **[P0]** Player Input — Frontend slash-command registry covers only 1 of 32 commands
- [ ] **[P0]** Persistence — Autosave (45s timer) has no test; player session loss is silent
- [ ] **[P0]** Persistence — `SavePicker.svelte` and branch-DAG layout untested
- [ ] **[P0]** LLM — Anthropic provider has zero mocked-HTTP test
- [ ] **[P0]** LLM — 9 of 13 providers ride OpenAI-compatible path with no provider-specific smoke
- [ ] **[P0]** GUI — `DebugPanel.svelte` (5 tabs) has no test
- [ ] **[P0]** Mod System — No malformed-input tests for any of 11 mod artefacts
- [ ] **[P0]** Mod System — Prompt template `{placeholder}` interpolation untested
- [ ] **[P0]** Runtime Modes — Wiring parity has zero enforcement; IPC drift between web/Tauri ships silently
- [ ] **[P0]** Runtime Modes — `parish-tauri` has zero test files

### Tracking issue (rolled up)

- All P1 gaps named in the per-area reports
- All P2 gaps named in the per-area reports
- Items pulled forward to per-area issues are marked there

## Verification

- `just check` — confirms `check-doc-paths.sh` is happy with every
  backtick path cited in this audit. (Run as part of the PR.)
- Each report's claims cite file:line for spot-checking.
- No source code is modified — this PR adds documentation only.
