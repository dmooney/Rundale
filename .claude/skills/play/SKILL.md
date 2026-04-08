---
name: play
description: Play-test the Parish game using the script harness. Sends commands, reads JSON output, and evaluates the gameplay experience.
disable-model-invocation: false
argument-hint: [scenario or script-file]
---

Play-test the Parish game via the `--script` mode, which outputs structured JSON per command.

## Steps

1. **Build first**: Run `cargo build` to ensure the project compiles.

2. **Determine what to test**:
   - If `$ARGUMENTS` is a `.txt` file path, use it directly as the script file.
   - If `$ARGUMENTS` is a scenario description (e.g., "explore all locations", "talk to every NPC", "test time passage"), generate an appropriate test script file at `testing/fixtures/play_session.txt`.
   - If no arguments, generate a comprehensive exploration script that:
     - Checks `/status`, `/time`, `/map`, `/npcs`
     - Visits every reachable location via movement commands
     - Uses `/wait` to advance time and observe NPC schedule changes
     - Uses `/tick` to test manual schedule advancement
     - Tests `/save`, `/fork`, `/branches`, `/log` for persistence
     - Tests `/speed fast` and `/speed normal`

3. **Run the script**: `cargo run -- --script <script-file>`

4. **Analyze the JSON output** line by line. For each line, check:
   - **Movement**: `"result": "moved"` entries have valid `to` locations and reasonable `minutes`
   - **System commands**: Responses are non-empty and contain expected data
   - **Map**: Contains locations and connections
   - **NPCs**: Shows NPC details when present at a location
   - **Time**: Shows hour, minute, season, weather
   - **Wait**: Time advances correctly (compare before/after)
   - **Errors**: No panics, no empty responses, no unexpected `"result": "unknown_input"`

5. **Report findings**: Provide a play-test summary including:
   - Locations visited and whether descriptions were generated
   - NPCs encountered and at which locations
   - Time/season progression observed
   - Any anomalies, bugs, or missing features
   - Overall assessment of the gameplay experience

## Tips for Script Generation

Available commands in scripts:
- Movement: `go to <location>`, `walk to <location>`
- Look: `look`, `look around`
- System: `/status`, `/time`, `/map`, `/npcs`, `/wait [N]`, `/tick`
- Persistence: `/save`, `/fork <name>`, `/load <name>`, `/branches`, `/log`
- Speed: `/speed fast`, `/speed normal`, `/speed slow`
- Control: `/pause`, `/resume`, `/new`
- Debug: `/debug`, `/debug npcs`, `/debug clock`, `/debug here`, `/debug schedule`

Location names can be found by running `/map` first or checking `mods/kilteevan-1820/world.json`.
