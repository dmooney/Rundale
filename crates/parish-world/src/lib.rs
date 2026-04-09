//! World state and location graph for the Parish game engine.

pub mod description;
pub mod encounter;
pub mod geo;
pub mod graph;
pub mod movement;
pub mod palette;
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

    /// Appends a line to the text log.
    pub fn log(&mut self, text: String) {
        self.text_log.push(text);
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}
