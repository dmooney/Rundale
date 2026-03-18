# Plan: Phase 1 — Core Loop

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)

## Goal

Get a single-location, single-NPC game loop running end-to-end: player types natural language, Ollama parses intent, NPC responds via LLM, response renders in a true-color TUI with a working day/night cycle.

## Prerequisites

- Rust project scaffolded (done: Cargo.toml, module stubs, `ParishError`, tokio/tracing init)
- Ollama running on `localhost:11434` with Qwen3 14B loaded (for manual testing)

## Tasks

1. **Implement `GameClock` in `src/world/time.rs`**
   - `GameClock` struct: fields `start_real: Instant`, `start_game: DateTime<Utc>`, `paused: bool`, `speed_factor: f64` (default 72.0 for 20min = 1 day)
   - Method `now(&self) -> DateTime<Utc>` — maps wall clock to game time
   - Method `advance(&mut self, game_minutes: i64)` — shift `start_game` forward (used during traversal)
   - `TimeOfDay` enum: `Dawn`, `Morning`, `Midday`, `Afternoon`, `Dusk`, `Night`, `Midnight`
   - Method `time_of_day(&self) -> TimeOfDay` — derive from current game hour
   - `Season` enum: `Spring`, `Summer`, `Autumn`, `Winter` with method `from_date(date: NaiveDate) -> Season`
   - `Festival` enum: `Imbolc`, `Bealtaine`, `Lughnasa`, `Samhain` with `fn check_festival(date: NaiveDate) -> Option<Festival>`

2. **Implement basic `Location` and `WorldState` in `src/world/mod.rs`**
   - `LocationId` newtype: `pub struct LocationId(pub u32)`
   - `Location` struct: `id: LocationId`, `name: String`, `description: String`, `indoor: bool`, `public: bool`
   - `WorldState` struct: `clock: GameClock`, `player_location: LocationId`, `locations: HashMap<LocationId, Location>`, `weather: String`, `text_log: Vec<String>`
   - Constructor `WorldState::new()` — creates a single test location ("The Crossroads")

3. **Implement TUI terminal init/restore in `src/tui/mod.rs`**
   - `fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>>` — enable raw mode, enter alternate screen, install panic hook that restores terminal
   - `fn restore_terminal(terminal: &mut Terminal<...>) -> Result<()>` — disable raw mode, leave alternate screen
   - Panic hook: `std::panic::set_hook` wrapping default hook with terminal restore

4. **Implement main render loop in `src/tui/mod.rs`**
   - `App` struct: holds `WorldState`, `input_buffer: String`, `should_quit: bool`
   - `fn draw(frame: &mut Frame, app: &App)` — layout with `Layout::vertical` split into top bar (1 line), main panel (fill), input line (1 line)
   - Top bar: `Paragraph` showing `"{location} | {time_of_day} | {weather} | {season}"`
   - Main panel: `Paragraph` rendering `app.world.text_log` with word wrap
   - Input line: `Paragraph` showing `"> {input_buffer}"`
   - Event loop: `crossterm::event::poll` with 100ms timeout, handle `KeyEvent` for typing, Enter, Esc

5. **Implement day/night color palette in `src/tui/mod.rs`**
   - `fn palette_for_time(tod: &TimeOfDay) -> ColorPalette` where `ColorPalette` has `bg: Color`, `fg: Color`, `accent: Color`
   - RGB values: Dawn(255,220,180), Morning(255,245,220), Midday(255,255,240), Afternoon(240,220,170), Dusk(60,70,110), Night(20,25,40), Midnight(10,12,20)
   - Apply `bg` to all block backgrounds, `fg` to text, `accent` to top bar

6. **Implement `Command` enum and parser in `src/input/mod.rs`**
   - `Command` enum: `Pause`, `Resume`, `Quit`, `Save`, `Fork(String)`, `Load(String)`, `Branches`, `Log`, `Status`, `Help`
   - `fn parse_system_command(input: &str) -> Option<Command>` — match on `/pause`, `/quit`, etc.
   - `PlayerIntent` struct: `intent: IntentKind`, `target: Option<String>`, `dialogue: Option<String>`, `raw: String`
   - `IntentKind` enum: `Move`, `Talk`, `Look`, `Interact`, `Examine`, `Unknown`
   - `InputResult` enum: `SystemCommand(Command)`, `GameInput(String)` — returned by `fn classify_input(raw: &str) -> InputResult`

7. **Implement `OllamaClient` in `src/inference/client.rs`**
   - `OllamaClient` struct: `client: reqwest::Client`, `base_url: String`
   - `OllamaClient::new(base_url: &str) -> Self` — build reqwest client with 30s timeout
   - `async fn generate(&self, model: &str, prompt: &str, system: Option<&str>) -> Result<String>` — POST to `/api/generate`, parse `response` field from JSON body
   - `async fn generate_json<T: DeserializeOwned>(&self, model: &str, prompt: &str, system: Option<&str>) -> Result<T>` — same but deserialize response as structured JSON

8. **Implement inference queue in `src/inference/mod.rs`**
   - `InferenceRequest` struct: `id: u64`, `model: String`, `prompt: String`, `system: Option<String>`, `response_tx: oneshot::Sender<InferenceResponse>`
   - `InferenceResponse` struct: `id: u64`, `text: String`, `error: Option<String>`
   - `InferenceQueue` struct: wraps `mpsc::Sender<InferenceRequest>`
   - `fn spawn_inference_worker(client: OllamaClient, rx: mpsc::Receiver<InferenceRequest>) -> JoinHandle<()>` — loop pulling from rx, calling client.generate, sending response back via oneshot

9. **Implement basic `Npc` struct in `src/npc/mod.rs`**
   - `NpcId` newtype: `pub struct NpcId(pub u32)`
   - `Npc` struct: `id: NpcId`, `name: String`, `age: u8`, `occupation: String`, `personality: String`, `location: LocationId`, `mood: String`
   - `NpcAction` struct (structured output): `action: String`, `target: Option<String>`, `dialogue: Option<String>`, `mood: String`, `internal_thought: Option<String>` — derive `Deserialize`

10. **Implement Tier 1 NPC context construction in `src/npc/mod.rs`**
    - `fn build_tier1_system_prompt(npc: &Npc) -> String` — combines personality, occupation, mood into a system prompt
    - `fn build_tier1_context(npc: &Npc, world: &WorldState, player_input: &str) -> String` — includes location, time, weather, player action

11. **Implement player intent parsing in `src/input/mod.rs`**
    - `async fn parse_intent(queue: &InferenceQueue, raw_input: &str, world: &WorldState) -> Result<PlayerIntent>` — sends input to Ollama with a system prompt instructing structured JSON output, deserializes into `PlayerIntent`

12. **Wire the game loop in `src/main.rs`**
    - Initialize `WorldState`, `OllamaClient`, spawn inference worker
    - Initialize terminal, create `App`
    - Loop: draw frame, poll input, on Enter: classify input, if `SystemCommand(Quit)` break, if `GameInput` send to intent parser, build NPC context, send inference request, render NPC response to text log
    - On exit: restore terminal

13. **Write tests for `GameClock` in `src/world/time.rs`**
    - `test_time_of_day_transitions`: construct clock at specific hours, assert correct `TimeOfDay`
    - `test_season_from_date`: assert Jan->Winter, Mar->Spring, Jun->Summer, Sep->Autumn
    - `test_festival_detection`: assert Feb 1->Imbolc, May 1->Bealtaine, Aug 1->Lughnasa, Nov 1->Samhain

14. **Write test for `OllamaClient` in `src/inference/client.rs`**
    - `test_generate_request_format`: use a mock HTTP server (or `#[ignore]` test against live Ollama) to verify request body shape and response parsing
    - `test_command_parsing`: assert `/quit` -> `Command::Quit`, `/fork main` -> `Command::Fork("main")`, `"go to pub"` -> `GameInput`

## Design References

- [Architecture Overview](../design/overview.md)
- [TUI Design](../design/tui-design.md)
- [Time System](../design/time-system.md)
- [Player Input](../design/player-input.md)

## Key Decisions

- [ADR-005: Ollama Local Inference](../adr/005-ollama-local-inference.md)
- [ADR-006: Natural Language Input](../adr/006-natural-language-input.md)
- [ADR-007: Time Scale 20min Day](../adr/007-time-scale-20min-day.md)

## Acceptance Criteria

- `cargo build` succeeds with no warnings
- `cargo test` passes all unit tests for GameClock, command parsing
- Running `cargo run` opens a TUI showing a location, time-of-day, and an input prompt
- Typing text and pressing Enter sends it through the inference pipeline (if Ollama is running) and displays an NPC response
- Day/night color palette visibly shifts as game time advances
- `/quit` cleanly restores the terminal and exits

## Open Issues

- Exact Ollama model names to use (e.g., `qwen3:14b` vs `qwen3:14b-q4_K_M`)
- Whether to stream Ollama responses token-by-token or wait for full completion
- Input cursor positioning and text editing (backspace works; arrow keys deferred)
