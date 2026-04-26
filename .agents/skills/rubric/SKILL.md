---
name: rubric
description: Run the gameplay eval rubrics and snapshot baselines for the script harness. Sister to /prove — reproducible regression sensors instead of "Claude reads the JSON". Use after editing gameplay code, before opening a PR, or whenever you want a fast structural check on fixture output.
disable-model-invocation: false
---

Run the eval rubrics and snapshot baselines that live alongside the script-harness fixtures. This is the **inferential-sensor** half of the harness: deterministic, machine-checked, no human reading required.

## What this checks

1. **Snapshot baselines.** Each fixture in `BASELINED_FIXTURES` (in `crates/parish-cli/tests/eval_baselines.rs`) is run through `run_script_captured`, serialized to JSON, and diffed against `testing/evals/baselines/<fixture>.json`. Any drift fails the test with a "live | baseline" diff window.
2. **Structural rubrics.** Every baselined fixture is also asserted to satisfy:
   - `rubric_anachronisms_are_empty` — no NpcResponse may surface anachronistic terms.
   - `rubric_movement_minutes_are_positive` — no Moved with `minutes == 0` (catches a frozen game clock).
   - `rubric_look_descriptions_are_non_empty` — no Looked with empty description (catches silent renderer failure).

## Steps

1. Run the suite: `cargo test -p parish --test eval_baselines`
2. **If a baseline test fails** — read the "live | baseline" diff window in the panic message. Two cases:
   - The drift is **unintentional** (a regression from your change). Fix the code, rerun.
   - The drift is **intentional** (you deliberately changed the gameplay). Confirm the new output is correct, then run `just baselines` to regenerate, and review the diff in `git diff testing/evals/baselines/`.
3. **If a rubric test fails** — the panic message names the fixture, the step, and the canonical fix (e.g. "travel time must be positive — check world graph edge weights and parish_world::movement::resolve_movement"). Fix the code, rerun.

## When to use

- After editing anything in `parish-world`, `parish-npc`, `parish-cli/src/testing.rs`, or `mods/rundale/`.
- Before opening or updating a PR with gameplay changes.
- As a faster, lower-noise alternative to `/prove` when the change is structural and you don't need to read the JSON yourself.

## Adding a new fixture to the baseline set

1. Confirm the fixture's structured output is **deterministic** — run `cargo run -- --script testing/fixtures/<fixture>.txt` twice and `diff` the JSON. Differences in `new_log_lines` are fine (not part of `ScriptResult`); differences elsewhere are not.
2. Add the fixture's stem to `BASELINED_FIXTURES` and a matching `#[test] fn baseline_<fixture>()` in `crates/parish-cli/tests/eval_baselines.rs`.
3. Run `just baselines` to capture the initial JSON.
4. Commit the new test entry alongside its baseline.

## Why this exists

OpenAI's harness-engineering post calls out the *Behaviour* harness as the hardest of the three regulation categories — historically tested by hand-reading test output. Capture-on-green snapshot baselines give you a cheap, deterministic regression sensor without an LLM judge: gameplay output is captured once, diffed forever. The rubric tests catch whole categories of regression (frozen clock, empty descriptions, anachronism leaks) that prose-reading would miss.

Companion to `/prove` (reads JSON and reasons about it), `/play` (autonomous play-test), and `/check` / `/verify` (full quality gates).
