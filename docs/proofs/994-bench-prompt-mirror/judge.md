Judge for issue #994 — bench dialogue system prompt mirrors runtime tier-1

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Criterion 1 — helper reads template + appends language directive

Met. `parish/scripts/local-eval/eval_lib.py` (lines 462-589 in the diff) defines `build_dialogue_system_prompt()`, which reads `mods/rundale/prompts/tier1_system.txt` via `_RUNDALE_TIER1_TEMPLATE.read_text(...)`, fills the persona slots (`name`, `age`, `occupation`, `personality`, `mood`, `improv_section`, `intel_guidance`, `tone_guidance`), and concatenates the rendered body with `_language_directive(player_language, native_language)` defaulting to `en-IE` / `ga-IE`. `_language_directive` is a line-for-line Python mirror of `parish_npc::language_directive` (parish/crates/parish-npc/src/lib.rs:379-429), and when `native` is `ga-IE` it appends `_GA_IE_PHRASE_GUIDE`, which I confirmed is byte-identical to the runtime `GA_IE_PHRASE_GUIDE` constant (lib.rs:346-360) — both render to a 739-char string. My independent invocation of `build_dialogue_system_prompt()` produced a 3894-char prompt, matching transcript.txt line 2.

## Criterion 2 — four runtime tier-1 improvements present

Met. Re-rendering the prompt and probing for substrings confirmed all four improvements plus the supporting guards listed in the criterion: `"Acts of Union of 1800"` and `"no electricity, no railways, no photography"` (1820 fact preamble), `"Top o' the mornin'"` and `"begorrah"` (cultural-guidelines stage-Irish guard), `"STAY IN CHARACTER as Brigid O'Brien"` (persona binding), `"Dia dhuit"` / `"seanchaí"` (GA_IE phrase whitelist), plus `"Do NOT emit Cyrillic"` (Latin-script guard), `"Speak in en-IE"` (locale), and `"SPRINKLE only"` (sprinkle clause). The first three flow from the template at mods/rundale/prompts/tier1_system.txt, which contains the exact sentences in lines 3, 5, and 10; the GA_IE whitelist and Latin guard flow from the language directive appended by the helper. Evidence rows match transcript.txt lines 4-10.

## Criterion 3 — all five callers patched

Met. A `grep` for the old hardcoded persona seed `"kind but direct, with a deep knowledge of local plants"` across all five caller files returned zero matches — the string now lives only inside `eval_lib._BRIGID_PERSONALITY`. Each caller imports `build_dialogue_system_prompt` from `eval_lib` and assigns its return value to `DIALOGUE_SYS` (or `SYSTEM` in `gen_dlg.py`): cache_dialogue_replies.py:38+48, rundale_bench.py:34+91, bench_perf.py:39+50, gen_samples.py:33+223, gen_dlg.py:24+32. Transcript.txt lines 13-17 record the same five sites all resolving to the same 3894-char string, confirming a single shared helper.

## Criterion 4 — clean imports + non-empty resolution

Met. My independent `importlib.util` load of all five scripts completed without `SyntaxError`, `ImportError`, or `KeyError`. Each script exposed its expected attribute (`DIALOGUE_SYS` for four, `SYSTEM` for `gen_dlg.py`) and each value was a non-empty `str` of length 3894. The transcript shows the same `len=3894 sample="You are Brigid O'Brien, a 42-year-old midwife in"` result for all five.

## Independent re-checks

- `git diff --stat HEAD` confirmed the PR touches the six expected files (eval_lib.py +126 lines, plus the five callers); no unrelated changes.
- `git diff HEAD -- parish/scripts/local-eval/eval_lib.py` inspected the full new helper — it reads `tier1_system.txt` via `pathlib.Path(__file__).resolve().parents[3] / "mods" / "rundale" / "prompts" / "tier1_system.txt"`, which I verified resolves to the real on-disk file (1897 bytes, modified May 17).
- `python3 -c "...; print(build_dialogue_system_prompt())" | wc -c` returned 3935 UTF-8 bytes; computing `len(p)` separately returned 3894 characters and `len(p.encode("utf-8"))` returned 3934, so 3934 prompt bytes + 1 trailing newline from `print()` = 3935. The 3894-char figure in the bundle is consistent. The judge-instruction note "around 3895" was a character-count expectation that matches the str-length 3894 plus newline; the byte-count discrepancy is explained by the prompt containing non-ASCII characters (`á`, `é`, `í`, `ó`, `ú`, etc.) in the GA_IE phrase whitelist.
- Re-rendered prompt and probed for all 10 substrings listed in evidence.md — every probe returned `True`.
- Diffed `_GA_IE_PHRASE_GUIDE` (eval_lib.py:478-494) against the runtime `GA_IE_PHRASE_GUIDE` (parish/crates/parish-npc/src/lib.rs:346-360) after collapsing Rust line-continuation backslashes: equal, both 739 chars.
- `grep -rn "kind but direct, with a deep knowledge of local plants"` across the five caller files returned 0 hits, confirming the old hardcoded persona is gone from all sites.
- Loaded all five caller modules via `importlib.util.spec_from_file_location` + `exec_module`; each loaded without error and exposed its expected `DIALOGUE_SYS` / `SYSTEM` attribute as a non-empty string of length 3894.
