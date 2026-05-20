Evidence type: live gameplay transcript

# Evidence — location-context-logs

Live run of `parish --script
parish/testing/fixtures/play_location-context-logs.txt` with
`PARISH_USER_DATA_DIR=/tmp/parish-loclog-proof` to capture the
generated files in isolation from the real user data dir.

## Run command

```sh
PARISH_USER_DATA_DIR=/tmp/parish-loclog-proof \
  cargo run -p parish -- \
  --script parish/testing/fixtures/play_location-context-logs.txt
```

## Mapping criteria → observed output

### C1: one file per location

```
$ ls /tmp/parish-loclog-proof/logs/branch-1/loc-*.md | wc -l
22
$ grep -c '"id":' mods/rundale/world.json
22
```

The world has 22 locations; 22 `loc-NNN-slug.md` files were generated
on session start by `LocationLogManager::write_all_profiles`.
Verified via `find` listing — file slugs include
`loc-001-the-crossroads.md`, `loc-002-darcy-s-pub.md`,
`loc-015-kilteevan-village.md`, etc.

### C2: PROFILE section content

See `sample-loc-001-the-crossroads.txt` — captured verbatim from the
live run. Profile contains:

- H1: `# The Crossroads — Location Log`
- Flags line: `*Outdoor · Public*`
- `## Description` with the description template verbatim, including
  `{weather}` / `{time}` placeholders.
- `## Geography`: coordinates `53.6362°N, 8.1153°W`, `Kind: Manual`,
  `Also known as: crossroads`, `Source: manually placed from modern
  Kilteevan crossroads coord`.
- `## Mythological Significance`: *Crossroads hold power in Irish
  folklore — a place between places, where the veil is thin.*
- `## Connections`: 7 entries with path descriptions.

`sample-loc-015-kilteevan-village.txt` exercises additional fields —
hazard tag (`The Holy Well — a mossy path ... *(⚠ Flood)*`), residents
section (Brigid Ni Fhatharta, Sean Ruadh Kelly, Peig Hannigan).
`sample-loc-002-darcy-s-pub.txt` shows indoor=true rendering as
`Indoor · Public` and the residents section listing
`Padraig Darcy (Publican)`.

### C3: PlayerMoved → "Player arrived" with from-location

Script ran `go to The Crossroads` from Kilteevan Village, then `go to
Darcy's Pub`. Two PlayerMoved events fired. Resulting journal entries:

`sample-loc-001-the-crossroads.txt`:

```
### Monday 20 March 1820, 08:14 — Player arrived
*Arrived from Kilteevan Village*
```

`sample-loc-002-darcy-s-pub.txt`:

```
### Monday 20 March 1820, 08:20 — Player arrived
*Arrived from The Crossroads*
```

Two distinct destination files received their own arrival heading;
the `*Arrived from <prev>*` body cites the correct origin in both
cases.

### C4: NpcArrived → "<name> arrived" heading

The walk into Darcy's Pub triggered tier promotion for the two NPCs
co-located there. Both entered the LocationLogManager's view via
`NpcArrived` and were recorded in `sample-loc-002-darcy-s-pub.txt`:

```
### Monday 20 March 1820, 08:20 — Niamh Darcy arrived
### Monday 20 March 1820, 08:20 — Padraig Darcy arrived
```

The arrival timestamp lines up with the player's arrival, confirming
the per-location writer received the same event stream as the
character-log writer.

### C5: branch-scoped directory

Files live under `/tmp/parish-loclog-proof/logs/branch-1/`. The
`branch-1` segment comes from `app.active_branch_id`. Switching to
another branch would produce a sibling `branch-N/` folder with its
own profile set.

### C6: flag-off no-op (covered by unit test)

`disabled_manager_is_noop` in `location_log.rs::tests` verifies that
constructing `LocationLogManager::new_at_dir(path, false)` followed
by `write_all_profiles` and `process_event` produces no files on
disk. Test runs as part of the standard `cargo test -p parish-core`
suite; passed in the workspace test run cited below.

## Backing test runs

- `cargo test --workspace`: 2858 passed, 15 ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: no issues.
- `cargo fmt --check`: clean.

## Acceptance criteria summary

- C1 ✓ 22 files generated, matching world location count.
- C2 ✓ Profile section rendered with all required fields.
- C3 ✓ PlayerMoved entries on both destination files.
- C4 ✓ NpcArrived entries on Darcy's Pub log.
- C5 ✓ Branch-scoped path (`logs/branch-1/`).
- C6 ✓ Flag-off no-op covered by `disabled_manager_is_noop`.

Acceptance criteria: met
