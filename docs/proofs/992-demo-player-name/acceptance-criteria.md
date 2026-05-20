# Acceptance Criteria: 992-demo-player-name

## Task

The demo auto-player invents a player name on demand when an NPC asks for it. In
the post-fix run of PR #990, Qwen2.5-14B answered "Ye can call me Br Q." — a
malformed single-letter artifact. The auto-player must instead always introduce
itself with a stable, plausible 1820 east-Roscommon name, and use that same
name on every subsequent introduction (no drift across turns).

The fix lives in `mods/rundale/demo-prompt.txt` (the `extra_prompt` injected
into the auto-player system prompt by `get_llm_player_action`). Pinning the
name there keeps the change in mod content, requires no Rust edit, and the
prompt already flows through `parish-tauri::commands::get_llm_player_action`.

## Criteria

- Demo prompt content pins one specific name (e.g. "Aiden Carney") with an
  explicit "if asked your name, give this exact name" rule — observable via:
  `grep -i 'name' mods/rundale/demo-prompt.txt` shows the pinned name and rule.
- The pinned name is plausible for 1820 east Roscommon (recognisable Irish
  given + Roscommon surname) — observable via: visual inspection in
  `evidence.md` against a historical sanity check.
- In a live demo transcript, when an NPC prompts the auto-player to introduce
  itself, the auto-player's response contains the pinned name verbatim and
  contains no single-letter "name" tokens (no "Br Q"-style artifact) —
  observable via: `just demo 0 12` transcript, search for the pinned name in
  player lines after any NPC "your name" / "who are ye" prompt.
- Across the same transcript, the auto-player does not introduce itself with
  any alternate name (no drift) — observable via: every introduction line in
  the transcript contains the same pinned name.

## Verification script

Demo mode runs only inside the Tauri desktop GUI (no CLI script-harness path
reaches `get_llm_player_action`), so the CLI fixture pattern does not apply
to this task. Verification is a live demo run.

Run: `just demo 0 12 2>&1 | tee docs/proofs/992-demo-player-name/transcript.txt`

Expected signals in transcript:
- At least one NPC line asking the player's name (e.g. "what do they call ye",
  "your name", "who are ye"). If none appears in 12 turns, re-run with a
  larger turn budget or seed an explicit prompt via an NPC encounter.
- Every auto-player response that introduces itself contains the pinned name
  string verbatim.
- No single-letter or single-syllable malformed name tokens in player lines
  (`grep -E '\b(Br|[A-Z]) [A-Z]\b'` returns no auto-player matches).
