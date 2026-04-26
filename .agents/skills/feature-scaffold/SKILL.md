---
name: feature-scaffold
description: Decompose a new gameplay feature depth-first before writing code — a design note, a failing fixture, a plan, and a /prove script. Apply at the start of any non-trivial feature so review stays cheap and the harness keeps up. Pass the feature name as an argument, e.g. /feature-scaffold market-day.
disable-model-invocation: false
argument-hint: <kebab-case-feature-name>
---

Scaffold the four artifacts a new gameplay feature needs **before** the implementation lands. This codifies the depth-first decomposition pattern from OpenAI's harness-engineering post: break a goal into design → fixture → plan → proof, and let the human review each artifact independently before any code is written.

## Steps

Use the kebab-case argument as `$NAME` (e.g. `market-day`, `harvest-festival`, `letter-delivery`).

1. **Design note** — `docs/design/$NAME.md`
   - Restate the feature in one paragraph. What does the player experience?
   - List the affected subsystems by crate (`parish-world`, `parish-npc`, `parish-inference`, `parish-persistence`, `parish-config`, etc.).
   - Sketch the data model changes (new fields on `Npc` / `World`, new event variants, new mod files under `mods/rundale/`).
   - Specify the **observable signal** in the script harness: what JSON line(s) prove the feature is live. Reference the relevant `ActionResult` variants from `crates/parish-cli/src/testing.rs`.
   - List feature-flag name (`config.flags.is_enabled("$NAME")`) per the non-negotiable rule in `AGENTS.md` §6.

2. **Failing fixture** — `testing/fixtures/play_$NAME.txt`
   - One command per line, comments with `#`. Use `/wait`, `/tick`, `/status`, `/time`, `/npcs`, `/map`, `look` to make the feature observable.
   - The fixture should currently demonstrate **the absence** of the feature — e.g., `/wait 480` over a festival day, then `/status`, and the lack of festival data is what changes once implemented.
   - Pattern after `testing/fixtures/play_weather.txt` and `testing/fixtures/banshee_playtest.txt`.

3. **Implementation plan** — `docs/plans/$NAME.md`
   - Ordered list of code-level steps: which files change, in which order, why.
   - One commit per step. Conventional-commit prefix per `AGENTS.md` §"Commit and PR expectations".
   - Note tests that must be added or updated. If gameplay-visible, schedule a `/prove $NAME` and (optionally) `/rubric` snapshot run.

4. **Stop here.** Do not start implementing. Surface the three files and ask the user to review. The point of scaffolding is to make the cost of redirection low — a wrong design caught now costs three short markdown files; caught after coding costs a feature.

## After review

Once the user signs off:

- Implement the plan one commit at a time.
- After the implementation, run `/prove $NAME` (reads the fixture's JSON output critically) and consider adding `play_$NAME` to `BASELINED_FIXTURES` in `crates/parish-cli/tests/eval_baselines.rs` if the output is deterministic — that locks the feature against future regression.
- Update `docs/requirements/roadmap.md` to mark the corresponding item `[x]`.
- Run `just harness-audit` — confirm the feature no longer appears as a coverage gap.

## Why this exists

OpenAI's harness-engineering post observes that the team "worked depth-first: breaking down larger goals into smaller building blocks (design, code, review, test, etc.), prompting the agent to construct those blocks." The skill makes that pattern the default for Rundale gameplay features, and the artifacts double as the agent's authoritative map of the change.

Companion to `/prove` (proves a finished feature works), `/rubric` (snapshot + structural sensors), and `/play` (autonomous play-test).
