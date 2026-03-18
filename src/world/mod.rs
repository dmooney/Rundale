//! World state and location graph.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. Geography is static; only people and events
//! within it are dynamic.

pub mod time;

use std::collections::HashMap;
use time::GameClock;

/// Unique identifier for a location in the world graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
/// Holds the game clock, player position, all locations, weather,
/// and the scrollback text log displayed in the TUI.
pub struct WorldState {
    /// The game clock mapping real time to game time.
    pub clock: GameClock,
    /// The player's current location.
    pub player_location: LocationId,
    /// All locations in the world, keyed by id.
    pub locations: HashMap<LocationId, Location>,
    /// Current weather description (e.g. "Clear", "Overcast").
    pub weather: String,
    /// Scrollback text log displayed in the main TUI panel.
    pub text_log: Vec<String>,
}

impl WorldState {
    /// Creates a new world state with a single test location ("The Crossroads").
    ///
    /// The game clock starts at 8:00 AM on March 20, 2026 (spring morning).
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

        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 3, 20, 8, 0, 0).unwrap());

        Self {
            clock,
            player_location: crossroads_id,
            locations,
            weather: "Clear".to_string(),
            text_log: Vec::new(),
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
}
