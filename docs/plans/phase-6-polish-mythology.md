# Plan: Phase 6 — Polish & Mythology Hooks

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)

## Goal

Complete the system command UI (/help, /map, /status, /log, /branches), refine the night-time atmosphere, and install structural hooks for future Irish mythology content without implementing supernatural events yet.

## Prerequisites

- Phase 5 complete: all four cognitive tiers, weather, seasons, persistence with branches
- TUI rendering pipeline stable (Phase 1)
- World graph with parish locations and metadata (Phase 2)

## Tasks

1. **Implement `/help` command in `src/input/mod.rs` + `src/tui/mod.rs`**
   - `fn render_help() -> Vec<String>` — returns formatted help text listing all commands with descriptions
   - Render in main text panel when `/help` is entered
   - Include: /pause, /resume, /quit, /save, /fork, /load, /branches, /log, /status, /help, /map
   - Each entry: command, arguments, one-line description

2. **Implement `/map` command in `src/tui/map.rs`** (new file)
   - `fn render_ascii_map(graph: &WorldGraph, player_location: LocationId, npcs: &NpcManager) -> Vec<String>`
   - Simple text-based map: list each location with connections shown as `--` lines
   - Mark player location with `[*]`, locations with NPCs present with `(N)` where N is count
   - Example output:
     ```
     [*] The Crossroads -- Darcy's Pub (3)
          |-- St. Brigid's Church
          |-- Murphy's Farm (1)
     ```
   - Show only locations within 3 edges of player (avoid overwhelming output)

3. **Implement `/status` command**
   - `fn render_status(world: &WorldState, npcs: &NpcManager, branch_name: &str, play_start: Instant) -> Vec<String>`
   - Display: branch name, in-game date (e.g., "14 March, Year 2"), time of day, season, weather
   - Real play time: hours:minutes since session started
   - NPC counts by tier: "Tier 1: 2 | Tier 2: 5 | Tier 3: 12 | Tier 4: 31"
   - Current location name

4. **Implement `/log` command UI**
   - Load snapshot history from `Database::branch_log()`
   - Render as scrollable list in main text panel:
     ```
     Branch: main
     [3] 14 Mar Year 2, afternoon — autosave
     [2] 14 Mar Year 2, morning — autosave
     [1] 13 Mar Year 2, evening — manual save
     ```
   - Each entry shows snapshot id, game time, and whether it was auto or manual

5. **Implement `/branches` command UI**
   - Load branch list from `Database::list_branches()`
   - Render as table:
     ```
     Branch          Created              Last Played
     * main          2026-03-18 14:00     14 Mar Year 2
       experiment    2026-03-18 15:30     13 Mar Year 2
     ```
   - Active branch marked with `*`

6. **Add `mythological_significance` to `Location`**
   - Add field: `mythological_significance: Option<MythSignificance>` to `Location` struct
   - `MythSignificance` struct: `kind: MythKind`, `description: String`, `active_seasons: Vec<Season>`
   - `MythKind` enum: `FairyFort`, `HolyWell`, `Crossroads`, `Bog`, `AncientRuin`, `StandingStone`
   - Derive `Serialize, Deserialize` for persistence and data files

7. **Implement festival event hooks in `src/world/time.rs`**
   - `fn check_festival_transition(prev: &DateTime<Utc>, now: &DateTime<Utc>) -> Option<Festival>` — detects when clock crosses a festival boundary between ticks
   - Publish `FestivalEvent { festival: Festival, year: i32 }` to EventBus when transition detected
   - NPCs can subscribe: inject festival awareness into context prompts
   - Placeholder handler: log festival occurrence, add text to TUI ("The first day of Bealtaine. Summer is here.")

8. **Implement night-time palette differentiation in `src/tui/mod.rs`**
   - Extend `palette_for_time` with smoother gradients: 24 distinct hour-based palettes interpolated via linear RGB lerp
   - `fn lerp_color(a: Color, b: Color, t: f32) -> Color` — smooth transition between time-of-day anchors
   - Night palette: cooler blues, lower contrast text; Midnight: near-black with dim grey text
   - Weather modifier: `fn apply_weather_modifier(palette: &ColorPalette, weather: &WeatherState) -> ColorPalette` — desaturate for overcast, blue-shift for rain, heavy desaturation for fog

9. **Implement atmospheric text tone for night**
   - Modify `render_description` (Phase 2) to include tone hints based on time
   - Night descriptions use different vocabulary: "quiet", "shadow", "still", "distant", "flickering"
   - Add `fn atmosphere_prefix(tod: &TimeOfDay, weather: &WeatherState) -> Option<String>` — returns an opening atmospheric sentence for location descriptions at night/dusk/dawn

10. **Add NPC belief/superstition fields**
    - Add to `Npc` struct: `beliefs: Vec<Belief>`
    - `Belief` struct: `content: String`, `conviction: f32` (0.0-1.0), `source: BeliefSource`
    - `BeliefSource` enum: `Tradition`, `PersonalExperience`, `Gossip`, `Church`
    - Include beliefs in Tier 1 context construction: "You believe: {beliefs}"
    - Initial data: assign 1-3 beliefs per NPC in `data/npcs.json` (e.g., Tommy believes in fairies, Fr. Declan warns against the fairy fort, Aoife is skeptical)

11. **Mark mythological locations in parish data**
    - Update `data/parish.json`: add `mythological_significance` to The Fairy Fort (FairyFort), The Crossroads (Crossroads), Lough Shore (HolyWell nearby)
    - Description templates for these locations include subtle atmospheric hooks at night: "The hawthorn around the fort seems to lean inward in the dark."

12. **Write tests**
    - `test_help_text_completeness`: assert help output mentions every Command enum variant
    - `test_map_rendering`: create small graph, render map, assert player marker and NPC counts appear
    - `test_status_display`: verify all fields present (branch, date, time, weather, NPC counts)
    - `test_festival_transition_detection`: advance clock across Nov 1, assert Samhain detected
    - `test_color_lerp`: verify lerp(black, white, 0.5) = mid-grey
    - `test_night_atmosphere`: assert atmosphere_prefix returns Some for Night/Midnight, None for Midday

## Design References

- [TUI Design](../design/tui-design.md)
- [Time System](../design/time-system.md)
- [Weather System](../design/weather-system.md)

## Key Decisions

- [ADR-001: Graph-Based World](../adr/001-graph-based-world.md)
- [ADR-007: Time Scale 20min Day](../adr/007-time-scale-20min-day.md)

## Acceptance Criteria

- `/help` displays all commands with descriptions
- `/map` renders an ASCII representation of nearby locations with player marker
- `/status` shows branch, game date/time, weather, play time, NPC tier counts
- `/log` and `/branches` display formatted persistence history
- Night-time TUI palette is distinctly cooler/darker than daytime with smooth transitions
- Fairy Fort, Holy Well, and Crossroads have `mythological_significance` set in data
- Festival transitions fire events at correct dates and display text in TUI
- At least 3 NPCs have beliefs that appear in their dialogue context
- `cargo test` passes all new tests

## Open Issues

- ASCII map layout algorithm: simple list vs. spatial positioning (would need approximate coordinates per location)
- How many palette anchor points are needed for smooth 24-hour transitions (24 vs. 7 named times)
- Mythology content scope: this phase installs hooks only; actual supernatural events are out of scope
