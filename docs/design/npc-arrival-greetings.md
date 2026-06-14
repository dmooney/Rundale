# Design: npc-arrival-greetings flag

## Player experience

Today, walking into any populated location triggers a burst of spontaneous NPC
greetings — "Good day to ye, stranger", welcomes, self-introductions, nods. In a
crowded spot (The Crossroads cycles Martin / Roisin / Sean / Mick / Niamh through
on schedule) this reads as NPCs "going buckwild" the moment you arrive. The new
default-off `npc-arrival-greetings` flag silences these spontaneous greetings:
NPCs no longer greet you unprompted on arrival. They still respond — and still
introduce themselves by name — the moment you actually speak to them. Players who
want the old lively-arrival behavior opt in with `/flag enable
npc-arrival-greetings`.

## Affected subsystems

- **`parish-core`** (`game_session.rs`): `apply_arrival_reactions` — the single
  shared generation+logging point. This is the gate site (rule #12: one
  backend-agnostic implementation, all entry points inherit it).
- **`parish-config`**: no schema change — flags are dynamic string keys via
  `FeatureFlags::is_enabled`. Nothing to add.
- **`parish-npc`** (`reactions/arrival_reactions`): unchanged — the generator is
  simply not invoked when the flag is off.
- Entry points (`parish-tauri`, `parish-server`, `parish-engine`): unchanged —
  they call the shared path; the gate is upstream of their streaming code
  (`movement.rs` `stream_reaction_texts` / `stream_arrival_reactions`), which all
  receive an empty reaction list and emit nothing.

## Data-model changes

None. No new struct fields, no new event variants, no save-schema impact, no new
`mods/rundale/` files. Pure behavioral gate on an existing dynamic flag.

## Observable signal

In the harness JSON, an arrival at a populated location currently appends NPC
greeting lines to the log / streams reaction turns. After the gate: at default
(off) those lines are absent; after `/flag enable npc-arrival-greetings` they
return. The `play_npc-arrival-greetings.txt` fixture makes this visible by moving
between populated locations in both flag states.

## Feature flag

`config.flags.is_enabled("npc-arrival-greetings")` — checked in
`apply_arrival_reactions`. Default **off** (unknown flag → `false` → muted), per
the explicit product decision for this task (deviates from the AGENTS.md §6
default-on convention deliberately: the whole point is for spontaneous greetings
to be off by default; opt in to restore). Documented in the PR.

## Introduction preservation (key correctness note)

`apply_arrival_reactions` calls `mark_introduced` for NPCs whose greeting
introduces them. Gating the whole function off therefore skips
introduce-on-arrival. This is **not** a regression: the dialogue path already
calls `mark_introduced(speaker_id)` (`parish-core/src/ipc/handlers.rs:814`) when
an NPC first speaks to the player. So with greetings muted, NPCs still become
named on first conversation — they just aren't auto-named by walking past them.
AC4 verifies this explicitly.
