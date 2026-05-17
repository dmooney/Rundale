# Acceptance criteria — issue #994

Eval-pipeline change. No runtime behaviour change.

## Goal

The rundale-bench dialogue system prompt must track the runtime tier-1
prompt so prompt-quality work (#990 and successors) shows up in the
leaderboard.

## Observable criteria

1. `parish/scripts/local-eval/eval_lib.py` exports
   `build_dialogue_system_prompt()` that reads
   `mods/rundale/prompts/tier1_system.txt`, substitutes the bench's
   Brigid O'Brien persona slots, and appends the same language
   directive (`en-IE` / `ga-IE`, including the `GA_IE_PHRASE_GUIDE`)
   that `parish_npc::language_directive` emits at runtime.

2. The rendered prompt contains the four runtime tier-1 improvements
   that the old hardcoded `DIALOGUE_SYS` was missing:
   - **1820 fact preamble** ("Acts of Union of 1800", "no electricity,
     no railways, no photography").
   - **Cultural guidelines** forbidding stage-Irish dialect
     ("Top o' the mornin'", "begorrah").
   - **Persona binding** ("STAY IN CHARACTER as Brigid O'Brien").
   - **GA_IE phrase whitelist** (curated Irish phrases) and Latin-script
     guard, derived from the runtime `language_directive`.

3. All five sites that previously hardcoded a Brigid `DIALOGUE_SYS`
   string now call the shared helper:
   - `parish/testing/rundale-bench/cache_dialogue_replies.py`
   - `parish/testing/rundale-bench/rundale_bench.py`
   - `parish/testing/rundale-bench/bench_perf.py`
   - `parish/scripts/local-eval/gen_samples.py`
   - `parish/scripts/local-eval/gen_dlg.py`

4. All five scripts import without `SyntaxError`, `ImportError`, or
   `KeyError`, and resolve `DIALOGUE_SYS` (or `SYSTEM` in `gen_dlg.py`)
   to a non-empty string at module-load time.

## Out of scope

- Re-caching grok-4.3 + re-running the judge to measure the dialogue-mean
  lift (issue point #3). Requires real API spend and belongs in a
  follow-up so the prompt change can be reviewed independently.

- Reconciling the divergence between the runtime tier-1 builder in
  `parish_npc::build_tier1_system_prompt` and the mod-shipped mirror in
  `mods/rundale/prompts/tier1_system.txt`. The issue treats the mod
  template as the source of truth; this PR honours that. A future task
  should either generate the template from the runtime builder, or
  rewrite the runtime builder to consume the template.
