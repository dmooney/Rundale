# Acceptance Criteria: character-context-logs

## Task

Add per-character markdown log files on disk that record each NPC's and the
player's full history. Each file opens with a rewritten **profile** section
(vital stats, intelligence, backstory, relationships, schedule) and is
followed by an append-only **journal** of timestamped diary entries derived
from `GameEvent`s on the event bus.

The profile is rewritten on every session start (between
`<!-- PROFILE_START -->` / `<!-- PROFILE_END -->` markers); the journal is
preserved untouched across sessions and grows over time. Two new event
shapes feed the journal: `DialogueOccurred` is extended with full
`player_said` / `npc_said` text (existing summary field retained), and a
new `PlayerMoved { from, to }` event is published on every successful
player movement.

Logs live under
`<resolve_user_data_dir(app_name)>/logs/<branch-id>/`:
- `player.md`
- `npc-<NNN>-<slug>.md` per NPC

Feature flag: `character-logs`, default on.

## Criteria

- **C1 — Log directory exists per branch.** After running the verification
  script the directory `<user-data-dir>/Rundale/logs/<branch-id>/`
  contains at least `player.md` and one `npc-*.md` file.
  Observable via: `ls $(parish_persistence::paths::resolve_user_data_dir("Rundale"))/logs/<branch>/`
  in `transcript.txt`.

- **C2 — NPC profile section is rewritten at session start.** Each
  `npc-*.md` opens with `<!-- PROFILE_START -->` and contains the NPC's
  name, age, occupation, intelligence stats, relationships, and schedule
  before `<!-- PROFILE_END -->`. Observable via: `cat
  npc-001-padraig-darcy.md` head in `transcript.txt` showing the markers
  and the expected fields.

- **C3 — Profile rewrite preserves existing Journal entries.** Running the
  script twice does not erase journal entries written in the first run.
  Observable via: the first-run journal lines appear in the second-run
  `cat` of the same file in `transcript.txt`.

- **C4 — `PlayerMoved` events append to `player.md` Journal.** After the
  fixture moves the player between two locations, `player.md` contains a
  journal entry of the form `### <date>, <time> — Arrived at <to>` (one
  entry per arrival). Observable via: `grep "Arrived at" player.md` in
  `transcript.txt`.

- **C5 — `DialogueOccurred` writes both player and NPC lines to the NPC's
  Journal.** After the fixture speaks to an NPC, that NPC's log file
  contains a journal entry with `**You:** <player text>` on one line and
  `**<NPC name>:** <npc reply>` on the next. Observable via: `grep -A 2
  "\*\*You:\*\*" npc-001-padraig-darcy.md` in `transcript.txt`.

- **C6 — Journal entry headings carry game-time timestamps.** Every
  journal entry's heading is `### <weekday> <day> <month> <year>,
  <HH:MM>`, matching the game's in-fiction clock — not wall-clock UTC.
  Observable via: at least one journal heading in `transcript.txt`
  contains "1820" and an `HH:MM` matching the game clock advanced by
  `/wait`.

- **C7 — Feature flag gates the writer.** Re-running the script with
  `character-logs` disabled produces no new files and no new journal
  entries. Observable via: a second invocation with the flag turned off
  shows the log file mtime unchanged in `transcript.txt`.

## Verification script

Run:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-cli -- \
    --script parish/testing/fixtures/play_character-context-logs.txt
```

Then capture the on-disk evidence (the harness JSON alone does not prove
file output; this is the live-proof tier per CLAUDE.md rule #10):

```sh
LOG_DIR="$HOME/Library/Application Support/Rundale/logs"
# Linux: LOG_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/Rundale/logs"
BRANCH=$(ls -t "$LOG_DIR" | head -n1)
ls "$LOG_DIR/$BRANCH/"
head -60 "$LOG_DIR/$BRANCH/npc-001-padraig-darcy.md"
grep -n "Arrived at" "$LOG_DIR/$BRANCH/player.md"
grep -n -A 2 '\*\*You:\*\*' "$LOG_DIR/$BRANCH"/npc-*.md
```

All of the above (script run + post-run file inspection) goes into
`docs/proofs/character-context-logs/transcript.txt`.

Expected signals in `transcript.txt`:
- `player.md` and `npc-001-*.md` listed under the branch dir (**C1**).
- `<!-- PROFILE_START -->` / `<!-- PROFILE_END -->` markers and the fields
  named in C2 inside an NPC file (**C2**).
- After the second run, journal lines from the first run are still
  present (**C3**).
- `### …, HH:MM — Arrived at …` lines in `player.md` (**C4**, **C6**).
- `**You:** …` followed by `**<NPC>:** …` in an NPC file (**C5**).
- Second-run-with-flag-off `stat` mtime equals first-run mtime (**C7**).
