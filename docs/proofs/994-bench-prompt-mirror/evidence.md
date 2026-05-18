# Evidence — issue #994 bench prompt mirror

Evidence type: gameplay transcript

The verification target is the eval pipeline, not a runtime feature.
This bundle captures the rendered system prompt + caller import-checks
as proof that the bench now mirrors the runtime tier-1 grounding. No
live game process was exercised because no runtime path changed (per
agent-check, `parish/scripts/**` is exempt and `parish/testing/**` is
proof-relevant but not runtime-shipping).

Transcript: [transcript.txt](transcript.txt)

## Criterion → transcript mapping

### Criterion 1 — helper reads template + appends language directive

`eval_lib.build_dialogue_system_prompt()` renders a 3894-char prompt.
The hardcoded `DIALOGUE_SYS` it replaces was 280 chars.

```
== sanity-check: render the new bench dialogue system prompt ==
prompt length: 3894 chars
```

### Criterion 2 — four runtime tier-1 improvements present

```
  [x] 1820 fact preamble              -> True
  [x] cultural guidelines             -> True
  [x] persona binding                 -> True
  [x] GA_IE phrase whitelist          -> True
  [x] Latin-script guard              -> True
  [x] en-IE locale                    -> True
  [x] ga-IE sprinkle clause           -> True
```

Substring probes:
- `'Acts of Union of 1800'` matches the runtime's 1820 fact preamble.
- `"Top o' the mornin'"` matches the cultural-guidelines stage-Irish
  guard.
- `"STAY IN CHARACTER as Brigid O'Brien"` matches the persona binding
  with the bench's persona name substituted.
- `'Dia dhuit'` matches the GA_IE_PHRASE_GUIDE that
  `parish_npc::language_directive` appends when the native language is
  `ga-IE`.
- `'Do NOT emit Cyrillic'` matches the runtime's Latin-script guard.
- `'Speak in en-IE'` and `'SPRINKLE only'` confirm the mod's
  `player_language` / `native_language` pair from
  `mods/rundale/mod.toml` drives the directive.

### Criterion 3 — all five callers patched

The transcript loads each script and reads its `DIALOGUE_SYS` /
`SYSTEM` attribute:

```
  parish/testing/rundale-bench/cache_dialogue_replies.py: DIALOGUE_SYS len=3894 ...
  parish/testing/rundale-bench/rundale_bench.py:          DIALOGUE_SYS len=3894 ...
  parish/testing/rundale-bench/bench_perf.py:             DIALOGUE_SYS len=3894 ...
  parish/scripts/local-eval/gen_samples.py:               DIALOGUE_SYS len=3894 ...
  parish/scripts/local-eval/gen_dlg.py:                   SYSTEM       len=3894 ...
```

All five resolve to the same canonical 3894-char string; the five
hardcoded prompts are gone.

### Criterion 4 — clean imports + non-empty resolution

Every script loaded via `importlib` without raising, and each
exported a non-empty `DIALOGUE_SYS` (or `SYSTEM` in `gen_dlg.py`).
The transcript's per-script `len=3894 sample="..."` line shows both
properties at once.
