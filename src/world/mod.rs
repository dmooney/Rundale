//! World state and location graph.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. Geography is static; only people and events
//! within it are dynamic.

pub mod description;
pub mod encounter;
pub mod graph;
pub mod movement;
pub mod palette;
pub mod time;

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::GameClock;

use crate::error::ParishError;
use graph::{LocationData, WorldGraph};

/// Classifies a location for ambient sound purposes.
///
/// Each location in the world graph has a `LocationKind` that determines
/// which ambient sounds play there and how sounds propagate from it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// A public house — fiddle music, crowd murmur, hearth sounds.
    Pub,
    /// A church — bell tolls, hymns, stone echo silence.
    Church,
    /// A working farm — animals, roosters, dogs.
    Farm,
    /// A crossroads — wind, summer dance music.
    Crossroads,
    /// Lakeshore or river — water lapping, reeds, waterfowl.
    Waterside,
    /// Bogland — wind, curlew calls, eerie silence.
    Bog,
    /// A village cluster — domestic sounds, children, roosters.
    Village,
    /// A shop — indoor, bell, conversation.
    Shop,
    /// A school — children reciting, silence.
    School,
    /// A sports field — hurling, cheering.
    SportField,
    /// A fairy fort — hawthorn wind, uncanny silence.
    FairyFort,
    /// A lime kiln — fire crackle, wind.
    LimeKiln,
    /// A post or letter office — indoor, quiet.
    PostOffice,
    /// A road or path — wind, footsteps, default fallback.
    #[default]
    Road,
}

impl fmt::Display for LocationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocationKind::Pub => write!(f, "Pub"),
            LocationKind::Church => write!(f, "Church"),
            LocationKind::Farm => write!(f, "Farm"),
            LocationKind::Crossroads => write!(f, "Crossroads"),
            LocationKind::Waterside => write!(f, "Waterside"),
            LocationKind::Bog => write!(f, "Bog"),
            LocationKind::Village => write!(f, "Village"),
            LocationKind::Shop => write!(f, "Shop"),
            LocationKind::School => write!(f, "School"),
            LocationKind::SportField => write!(f, "Sport Field"),
            LocationKind::FairyFort => write!(f, "Fairy Fort"),
            LocationKind::LimeKiln => write!(f, "Lime Kiln"),
            LocationKind::PostOffice => write!(f, "Post Office"),
            LocationKind::Road => write!(f, "Road"),
        }
    }
}

/// Current weather conditions in the game world.
///
/// Affects color palette tinting (desaturation, brightness, color temperature)
/// and location description templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weather {
    /// Clear skies — no palette modification.
    Clear,
    /// Overcast — slightly darker and desaturated.
    Overcast,
    /// Rain — darker with a blue-gray tint.
    Rain,
    /// Fog — washed out, low contrast.
    Fog,
    /// Storm — much darker and heavily desaturated.
    Storm,
}

impl fmt::Display for Weather {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Weather::Clear => write!(f, "Clear"),
            Weather::Overcast => write!(f, "Overcast"),
            Weather::Rain => write!(f, "Rain"),
            Weather::Fog => write!(f, "Fog"),
            Weather::Storm => write!(f, "Storm"),
        }
    }
}

impl std::str::FromStr for Weather {
    type Err = crate::error::ParishError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Clear" => Ok(Weather::Clear),
            "Overcast" => Ok(Weather::Overcast),
            "Rain" => Ok(Weather::Rain),
            "Fog" => Ok(Weather::Fog),
            "Storm" => Ok(Weather::Storm),
            _ => Err(crate::error::ParishError::Config(format!(
                "unknown weather: {s}"
            ))),
        }
    }
}

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
    /// Current weather conditions affecting palette and descriptions.
    pub weather: Weather,
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
            weather: Weather::Clear,
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
            weather: Weather::Clear,
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
        assert_eq!(world.weather, Weather::Clear);
        assert!(world.text_log.is_empty());
        assert_eq!(world.locations.len(), 1);
    }

    #[test]
    fn test_weather_display() {
        assert_eq!(Weather::Clear.to_string(), "Clear");
        assert_eq!(Weather::Overcast.to_string(), "Overcast");
        assert_eq!(Weather::Rain.to_string(), "Rain");
        assert_eq!(Weather::Fog.to_string(), "Fog");
        assert_eq!(Weather::Storm.to_string(), "Storm");
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

    #[test]
    fn test_location_kind_default() {
        assert_eq!(LocationKind::default(), LocationKind::Road);
    }

    #[test]
    fn test_location_kind_display() {
        assert_eq!(LocationKind::Pub.to_string(), "Pub");
        assert_eq!(LocationKind::Church.to_string(), "Church");
        assert_eq!(LocationKind::FairyFort.to_string(), "Fairy Fort");
        assert_eq!(LocationKind::LimeKiln.to_string(), "Lime Kiln");
        assert_eq!(LocationKind::PostOffice.to_string(), "Post Office");
        assert_eq!(LocationKind::SportField.to_string(), "Sport Field");
        assert_eq!(LocationKind::Road.to_string(), "Road");
    }

    #[test]
    fn test_location_kind_serde_roundtrip() {
        let kinds = vec![
            LocationKind::Pub,
            LocationKind::Church,
            LocationKind::Farm,
            LocationKind::FairyFort,
            LocationKind::LimeKiln,
            LocationKind::PostOffice,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: LocationKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, parsed);
        }
    }

    #[test]
    fn test_location_kind_snake_case_deserialization() {
        let parsed: LocationKind = serde_json::from_str("\"fairy_fort\"").unwrap();
        assert_eq!(parsed, LocationKind::FairyFort);
        let parsed: LocationKind = serde_json::from_str("\"post_office\"").unwrap();
        assert_eq!(parsed, LocationKind::PostOffice);
        let parsed: LocationKind = serde_json::from_str("\"sport_field\"").unwrap();
        assert_eq!(parsed, LocationKind::SportField);
    }

    #[test]
    fn test_location_kind_in_parish_file() {
        let path = Path::new("data/parish.json");
        if path.exists() {
            let world = WorldState::from_parish_file(path, LocationId(2)).unwrap();
            let data = world.graph.get(LocationId(2)).unwrap();
            assert_eq!(data.location_kind, LocationKind::Pub);
            let church = world.graph.get(LocationId(3)).unwrap();
            assert_eq!(church.location_kind, LocationKind::Church);
            let village = world.graph.get(LocationId(15)).unwrap();
            assert_eq!(village.location_kind, LocationKind::Village);
        }
    }
}
