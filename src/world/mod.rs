//! World state and location graph.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. Geography is static; only people and events
//! within it are dynamic.

pub mod description;
pub mod encounter;
pub mod graph;
pub mod movement;
pub mod time;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::GameClock;

use crate::error::ParishError;
use graph::{LocationData, WorldGraph};

/// Unique identifier for a location in the world graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocationId(pub u32);

/// A named location in the game world.
///
/// Locations are nodes in the world graph. Each has a textual
/// description, and flags indicating whether it is indoors and/or
/// public.
#[derive(Debug, Clone)]
pub struct Location {
    /// Unique identifier.
    pub id: LocationId,
    /// Human-readable name (e.g. "The Crossroads").
    pub name: String,
    /// Prose description shown when the player arrives.
    pub description: String,
    /// Whether this location is indoors.
    pub indoor: bool,
    /// Whether this location is publicly accessible.
    pub public: bool,
}

/// Central game state container.
///
/// Holds the game clock, player position, the world graph, weather,
/// and the scrollback text log displayed in the TUI.
pub struct WorldState {
    /// The game clock mapping real time to game time.
    pub clock: GameClock,
    /// The player's current location.
    pub player_location: LocationId,
    /// All locations in the world, keyed by id (legacy, used by NPC context).
    pub locations: HashMap<LocationId, Location>,
    /// The world graph with full location data and connections.
    pub graph: WorldGraph,
    /// Current weather description (e.g. "Clear", "Overcast").
    pub weather: String,
    /// Scrollback text log displayed in the main TUI panel.
    pub text_log: Vec<String>,
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
        };

        let mut locations = HashMap::new();
        locations.insert(crossroads_id, crossroads);

        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());

        Self {
            clock,
            player_location: crossroads_id,
            locations,
            graph: WorldGraph::new(),
            weather: "Clear".to_string(),
            text_log: Vec::new(),
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

        // Build legacy locations map from graph data
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
                    },
                );
            }
        }

        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());

        Ok(Self {
            clock,
            player_location: start_location,
            locations,
            graph,
            weather: "Clear".to_string(),
            text_log: Vec::new(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_state_new() {
        let world = WorldState::new();
        assert_eq!(world.player_location, LocationId(1));
        assert_eq!(world.weather, "Clear");
        assert!(world.text_log.is_empty());
        assert_eq!(world.locations.len(), 1);
    }

    #[test]
    fn test_current_location() {
        let world = WorldState::new();
        let loc = world.current_location();
        assert_eq!(loc.name, "The Crossroads");
        assert!(!loc.indoor);
        assert!(loc.public);
    }

    #[test]
    fn test_log() {
        let mut world = WorldState::new();
        world.log("Hello, world.".to_string());
        assert_eq!(world.text_log.len(), 1);
        assert_eq!(world.text_log[0], "Hello, world.");
    }

    #[test]
    fn test_default() {
        let world = WorldState::default();
        assert_eq!(world.player_location, LocationId(1));
    }

    #[test]
    fn test_location_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(LocationId(1));
        set.insert(LocationId(2));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&LocationId(1)));
    }

    #[test]
    fn test_from_parish_file() {
        let path = Path::new("data/parish.json");
        if path.exists() {
            let world = WorldState::from_parish_file(path, LocationId(15)).unwrap();
            assert_eq!(world.player_location, LocationId(15));
            assert!(world.locations.len() >= 12);
            assert!(world.graph.location_count() >= 12);
            assert_eq!(world.current_location().name, "Kilteevan Village");
        }
    }

    #[test]
    fn test_current_location_data() {
        let path = Path::new("data/parish.json");
        if path.exists() {
            let world = WorldState::from_parish_file(path, LocationId(15)).unwrap();
            let data = world.current_location_data().unwrap();
            assert_eq!(data.name, "Kilteevan Village");
            assert!(data.description_template.contains("{time}"));
        }
    }
}
