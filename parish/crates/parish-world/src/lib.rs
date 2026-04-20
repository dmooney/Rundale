//! World state and location graph for the Parish game engine.

pub mod description;
pub mod encounter;
pub mod geo;
pub mod graph;
pub mod liminal;
pub mod movement;
pub mod transport;
pub mod weather;

/// Re-export time types from parish-types for cross-crate convenience.
pub mod time {
    pub use parish_types::time::*;
}

/// Re-export event types from parish-types for cross-crate convenience.
pub mod events {
    pub use parish_types::events::*;
}

pub use parish_types::{Location, LocationId, Weather};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use parish_types::{ConversationLog, EventBus, GameClock, GossipNetwork, ParishError};

use graph::{LocationData, WorldGraph};
use weather::WeatherEngine;

/// Maximum number of entries kept in the backend text log, matching the
/// frontend cap (`MAX_TEXT_LOG_SIZE` in `apps/ui/src/stores/game.ts`).
const MAX_TEXT_LOG: usize = 500;

/// Central game state container.
///
/// Holds the game clock, player position, the world graph, weather,
/// and the scrollback text log displayed in the UI.
pub struct WorldState {
    /// The game clock mapping real time to game time.
    pub clock: GameClock,
    /// The player's current location.
    pub player_location: LocationId,
    /// All locations in the world, keyed by id (legacy, used by NPC context).
    pub locations: HashMap<LocationId, Location>,
    /// The world graph with full location data and connections.
    pub graph: WorldGraph,
    /// Current weather conditions affecting palette and descriptions.
    pub weather: Weather,
    /// Dynamic weather state machine that transitions over time.
    pub weather_engine: WeatherEngine,
    /// Scrollback text log displayed in the main text panel.
    pub text_log: Vec<String>,
    /// Cross-tier event bus for publishing and subscribing to game events.
    pub event_bus: EventBus,
    /// Set of location IDs the player has visited (for fog-of-war map).
    pub visited_locations: HashSet<LocationId>,
    /// Edge traversal counts for "worn path" footprints on the map.
    ///
    /// Keys are canonically ordered `(min_id, max_id)` pairs. The count
    /// increments each time the player walks along that edge.
    pub edge_traversals: HashMap<(LocationId, LocationId), u32>,
    /// Gossip propagation network tracking information spread among NPCs.
    pub gossip_network: GossipNetwork,
    /// Recent conversation exchanges for scene awareness and NPC memory.
    pub conversation_log: ConversationLog,
    /// The player's name, learned from dialogue (e.g. "My name is Ciaran").
    /// `None` until the player introduces themselves.
    pub player_name: Option<String>,
    /// Monotonically increasing counter incremented once per background tick.
    ///
    /// Used by `handle_game_input` to detect TOCTOU races: the generation is
    /// captured before the world lock is released for LLM inference, then
    /// compared after the lock is re-acquired.  A mismatch means the world
    /// changed (NPCs moved, clock advanced, weather shifted) while the
    /// intent was being parsed.  See issue #283.
    pub tick_generation: u64,
}

impl WorldState {
    /// Creates a new world state with a single test location ("The Crossroads").
    ///
    /// The game clock starts at 8:00 AM on March 20, 1820 (spring morning).
    pub fn new() -> Self {
        use chrono::{TimeZone, Utc};

        let crossroads_id = LocationId(1);
        let crossroads = Location {
            id: crossroads_id,
            name: "The Crossroads".to_string(),
            description: "A quiet crossroads where four narrow roads meet. \
                A weathered stone wall lines the eastern side, half-hidden \
                by brambles. To the north, smoke rises from a cluster of \
                cottages. The air smells of turf and wet grass."
                .to_string(),
            indoor: false,
            public: true,
            lat: 53.618,
            lon: -8.095,
        };

        let mut locations = HashMap::new();
        locations.insert(crossroads_id, crossroads);

        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());
        let weather_engine = WeatherEngine::new(Weather::Clear, clock.now());

        Self {
            clock,
            player_location: crossroads_id,
            locations,
            graph: WorldGraph::new(),
            weather: Weather::Clear,
            weather_engine,
            text_log: Vec::new(),
            event_bus: EventBus::new(),
            visited_locations: HashSet::from([crossroads_id]),
            edge_traversals: HashMap::new(),
            gossip_network: GossipNetwork::new(),
            conversation_log: ConversationLog::new(),
            player_name: None,
            tick_generation: 0,
        }
    }

    /// Creates a world state from a parish data file.
    ///
    /// Loads the world graph from JSON and sets the player at the
    /// specified starting location. Also populates the legacy `locations`
    /// map for backward compatibility with NPC context building.
    pub fn from_parish_file(path: &Path, start_location: LocationId) -> Result<Self, ParishError> {
        use chrono::{TimeZone, Utc};

        let graph = WorldGraph::load_from_file(path)?;

        let mut locations = HashMap::new();
        for loc_id in graph.location_ids() {
            if let Some(data) = graph.get(loc_id) {
                locations.insert(
                    loc_id,
                    Location {
                        id: loc_id,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                        lat: data.lat,
                        lon: data.lon,
                    },
                );
            }
        }

        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());
        let weather_engine = WeatherEngine::new(Weather::Clear, clock.now());

        Ok(Self {
            clock,
            player_location: start_location,
            locations,
            graph,
            weather: Weather::Clear,
            weather_engine,
            text_log: Vec::new(),
            event_bus: EventBus::new(),
            visited_locations: HashSet::from([start_location]),
            edge_traversals: HashMap::new(),
            gossip_network: GossipNetwork::new(),
            conversation_log: ConversationLog::new(),
            player_name: None,
            tick_generation: 0,
        })
    }

    /// Creates a world state from mod parameters.
    ///
    /// Equivalent to `from_parish_file` but also sets the start date from an
    /// RFC 3339 string. Used by `parish-core`'s mod loader so that `parish-world`
    /// does not need to depend on `GameMod` directly.
    pub fn from_mod_params(
        world_path: &Path,
        start_location: LocationId,
        start_date_rfc3339: &str,
    ) -> Result<Self, ParishError> {
        let graph = WorldGraph::load_from_file(world_path)?;

        let mut locations = HashMap::new();
        for loc_id in graph.location_ids() {
            if let Some(data) = graph.get(loc_id) {
                locations.insert(
                    loc_id,
                    Location {
                        id: loc_id,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                        lat: data.lat,
                        lon: data.lon,
                    },
                );
            }
        }

        let start_dt = chrono::DateTime::parse_from_rfc3339(start_date_rfc3339)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|e| {
                tracing::warn!(
                    start_date = start_date_rfc3339,
                    error = %e,
                    "Failed to parse mod start_date, falling back to current time"
                );
                chrono::Utc::now()
            });

        let clock = GameClock::new(start_dt);
        let weather_engine = WeatherEngine::new(Weather::Clear, clock.now());

        Ok(Self {
            clock,
            player_location: start_location,
            locations,
            graph,
            weather: Weather::Clear,
            weather_engine,
            text_log: Vec::new(),
            event_bus: EventBus::new(),
            visited_locations: HashSet::from([start_location]),
            edge_traversals: HashMap::new(),
            gossip_network: GossipNetwork::new(),
            conversation_log: ConversationLog::new(),
            player_name: None,
            tick_generation: 0,
        })
    }

    /// Marks a location as visited for the fog-of-war map.
    pub fn mark_visited(&mut self, id: LocationId) {
        self.visited_locations.insert(id);
    }

    /// Records a traversal along a path of locations, incrementing edge counts.
    ///
    /// Edges are stored in canonical order (smaller ID first) so that
    /// A→B and B→A are the same edge.
    pub fn record_path_traversal(&mut self, path: &[LocationId]) {
        for window in path.windows(2) {
            let (a, b) = if window[0] < window[1] {
                (window[0], window[1])
            } else {
                (window[1], window[0])
            };
            *self.edge_traversals.entry((a, b)).or_insert(0) += 1;
        }
    }

    /// Returns a reference to the player's current location.
    ///
    /// # Panics
    ///
    /// Panics if the player's location id is not in the locations map.
    pub fn current_location(&self) -> &Location {
        self.locations
            .get(&self.player_location)
            .expect("player location must exist in world")
    }

    /// Returns the current location's data from the world graph, if loaded.
    pub fn current_location_data(&self) -> Option<&LocationData> {
        self.graph.get(self.player_location)
    }

    /// Appends a line to the text log, evicting the oldest entries when the
    /// log exceeds [`MAX_TEXT_LOG`].
    pub fn log(&mut self, text: String) {
        self.text_log.push(text);
        if self.text_log.len() > MAX_TEXT_LOG {
            let excess = self.text_log.len() - MAX_TEXT_LOG;
            self.text_log.drain(..excess);
        }
    }

    /// Increments the tick generation counter.
    ///
    /// Called once per background tick cycle.  Wraps on overflow (a game
    /// session is not expected to run for 2^64 ticks).
    pub fn increment_tick_generation(&mut self) {
        self.tick_generation = self.tick_generation.wrapping_add(1);
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_crossroads() {
        let world = WorldState::new();
        assert_eq!(world.player_location, LocationId(1));
        assert_eq!(world.current_location().name, "The Crossroads");
    }

    #[test]
    fn new_initial_collections_are_fresh() {
        let world = WorldState::new();
        assert!(world.text_log.is_empty());
        assert!(world.edge_traversals.is_empty());
        // Starting location is pre-marked as visited.
        assert!(world.visited_locations.contains(&LocationId(1)));
        assert_eq!(world.visited_locations.len(), 1);
        assert!(world.player_name.is_none());
    }

    #[test]
    fn new_default_weather_is_clear() {
        let world = WorldState::new();
        assert_eq!(world.weather, Weather::Clear);
    }

    #[test]
    fn default_matches_new() {
        let a = WorldState::default();
        let b = WorldState::new();
        assert_eq!(a.player_location, b.player_location);
        assert_eq!(a.weather, b.weather);
        assert_eq!(a.text_log.len(), b.text_log.len());
    }

    #[test]
    fn log_appends_to_text_log() {
        let mut world = WorldState::new();
        world.log("hello".to_string());
        world.log("world".to_string());
        assert_eq!(world.text_log, vec!["hello", "world"]);
    }

    #[test]
    fn mark_visited_adds_location() {
        let mut world = WorldState::new();
        world.mark_visited(LocationId(42));
        assert!(world.visited_locations.contains(&LocationId(42)));
    }

    #[test]
    fn mark_visited_is_idempotent() {
        let mut world = WorldState::new();
        world.mark_visited(LocationId(5));
        world.mark_visited(LocationId(5));
        assert_eq!(
            world
                .visited_locations
                .iter()
                .filter(|&&id| id == LocationId(5))
                .count(),
            1
        );
    }

    #[test]
    fn record_path_traversal_canonicalises_edge_order() {
        let mut world = WorldState::new();
        // Walk 2 → 1 then 1 → 2 — both should land on the same canonical edge.
        world.record_path_traversal(&[LocationId(2), LocationId(1)]);
        world.record_path_traversal(&[LocationId(1), LocationId(2)]);
        assert_eq!(world.edge_traversals.len(), 1);
        assert_eq!(
            world.edge_traversals.get(&(LocationId(1), LocationId(2))),
            Some(&2)
        );
        // The reversed key should never appear.
        assert!(
            !world
                .edge_traversals
                .contains_key(&(LocationId(2), LocationId(1)))
        );
    }

    #[test]
    fn record_path_traversal_handles_multi_hop_paths() {
        let mut world = WorldState::new();
        // Path A→B→C should register two edges.
        world.record_path_traversal(&[LocationId(1), LocationId(2), LocationId(3)]);
        assert_eq!(
            world.edge_traversals.get(&(LocationId(1), LocationId(2))),
            Some(&1)
        );
        assert_eq!(
            world.edge_traversals.get(&(LocationId(2), LocationId(3))),
            Some(&1)
        );
    }

    #[test]
    fn record_path_traversal_ignores_empty_and_single() {
        let mut world = WorldState::new();
        world.record_path_traversal(&[]);
        world.record_path_traversal(&[LocationId(1)]);
        assert!(world.edge_traversals.is_empty());
    }

    #[test]
    fn current_location_data_none_for_empty_graph() {
        // new() sets up a legacy `locations` map but an empty `graph`.
        let world = WorldState::new();
        assert!(world.current_location_data().is_none());
    }

    #[test]
    #[should_panic(expected = "player location must exist")]
    fn current_location_panics_when_player_location_missing() {
        let mut world = WorldState::new();
        world.player_location = LocationId(999);
        let _ = world.current_location();
    }
}
