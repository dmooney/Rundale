//! Sound propagation through the world graph.
//!
//! Uses BFS over the `WorldGraph` to determine which sounds the player
//! can hear and at what volume, based on graph distance (traversal
//! minutes), weather dampening, and indoor attenuation.

use std::collections::{HashMap, VecDeque};

use crate::world::graph::WorldGraph;
use crate::world::time::{Season, TimeOfDay};
use crate::world::{LocationId, Weather};

use super::catalog::{Propagation, SoundCatalog, SoundEntry};

/// A sound that is audible to the player after propagation.
#[derive(Debug, Clone)]
pub struct AudibleSound<'a> {
    /// Reference to the catalog entry.
    pub entry: &'a SoundEntry,
    /// Distance in traversal minutes from the source to the player.
    pub distance_minutes: u16,
    /// Final computed volume after attenuation, weather, and indoor effects.
    pub volume: f32,
}

/// Computes all sounds audible at the player's location.
///
/// Performs BFS from the player's location through the world graph,
/// collecting matching sounds at each visited location with volume
/// attenuation based on distance, weather, and indoor status.
pub fn audible_sounds<'a>(
    player_location: LocationId,
    graph: &WorldGraph,
    catalog: &'a SoundCatalog,
    time: TimeOfDay,
    season: Season,
    weather: Weather,
    indoor: bool,
) -> Vec<AudibleSound<'a>> {
    let mut candidates = Vec::new();
    let weather_damp = weather_dampening(weather);

    // 1. Local sounds from the player's location
    if let Some(loc_data) = graph.get(player_location) {
        let kind = loc_data.location_kind;
        for entry in catalog.matching_entries(kind, time, season, weather) {
            let vol = entry.base_volume * weather_damp * if indoor { 0.4 } else { 1.0 };
            if vol > 0.001 {
                candidates.push(AudibleSound {
                    entry,
                    distance_minutes: 0,
                    volume: vol,
                });
            }
        }
    }

    // 2. Propagated sounds via BFS
    let mut visited: HashMap<LocationId, u16> = HashMap::new();
    visited.insert(player_location, 0);
    let mut frontier: VecDeque<(LocationId, u16)> = VecDeque::new();
    frontier.push_back((player_location, 0));

    // Maximum propagation distance any sound can travel
    let max_any_propagation = catalog
        .entries()
        .iter()
        .map(|e| e.propagation.max_distance())
        .max()
        .unwrap_or(0);

    while let Some((loc_id, dist)) = frontier.pop_front() {
        for (neighbor_id, conn) in graph.neighbors(loc_id) {
            let new_dist = dist + conn.traversal_minutes;

            // Skip if we've already found a shorter path
            if let Some(&existing) = visited.get(&neighbor_id)
                && existing <= new_dist
            {
                continue;
            }

            // Don't explore beyond the maximum propagation distance
            if new_dist > max_any_propagation {
                continue;
            }

            visited.insert(neighbor_id, new_dist);

            if let Some(neighbor_data) = graph.get(neighbor_id) {
                let kind = neighbor_data.location_kind;
                for entry in catalog.matching_entries(kind, time, season, weather) {
                    if entry.propagation.reaches(new_dist) {
                        let att = attenuation(new_dist, &entry.propagation);
                        let vol =
                            entry.base_volume * att * weather_damp * if indoor { 0.4 } else { 1.0 };
                        if vol > 0.001 {
                            candidates.push(AudibleSound {
                                entry,
                                distance_minutes: new_dist,
                                volume: vol,
                            });
                        }
                    }
                }

                // Continue BFS if any sound could still propagate further
                if new_dist < max_any_propagation {
                    frontier.push_back((neighbor_id, new_dist));
                }
            }
        }
    }

    // 3. Weather overlay sounds (global, independent of location)
    for entry in catalog.matching_weather_entries(weather) {
        let vol = entry.base_volume * if indoor { 0.4 } else { 1.0 };
        if vol > 0.001 {
            candidates.push(AudibleSound {
                entry,
                distance_minutes: 0,
                volume: vol,
            });
        }
    }

    candidates
}

/// Computes volume attenuation based on distance and propagation type.
///
/// Returns a multiplier in `[0.0, 1.0]`.
pub fn attenuation(distance_minutes: u16, propagation: &Propagation) -> f32 {
    match propagation {
        Propagation::Local => {
            if distance_minutes == 0 {
                1.0
            } else {
                0.0
            }
        }
        Propagation::Near => {
            if distance_minutes == 0 {
                1.0
            } else {
                0.5 // Neighbors hear at half volume
            }
        }
        Propagation::Medium => {
            // Linear falloff over ~15 minutes
            (1.0 - (distance_minutes as f32 / 15.0)).max(0.0)
        }
        Propagation::Far(max) => {
            // Logarithmic falloff — bells are loud
            let ratio = distance_minutes as f32 / *max as f32;
            (1.0 - ratio.sqrt()).max(0.0)
        }
    }
}

/// Returns a volume multiplier based on weather conditions.
///
/// Weather affects how far sound carries and how clearly it's heard.
pub fn weather_dampening(weather: Weather) -> f32 {
    match weather {
        Weather::Clear => 1.0,
        Weather::Overcast => 0.95,
        Weather::Rain => 0.6,
        Weather::Fog => 0.5,
        Weather::Storm => 0.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::LocationKind;
    use crate::world::graph::WorldGraph;

    /// Builds a small test graph:
    ///
    /// ```text
    /// [1:Crossroads] --3min-- [2:Pub] --5min-- [3:Church]
    ///       |                                       |
    ///       8min                                   6min
    ///       |                                       |
    ///   [4:Farm]                              [5:Bog Road]
    /// ```
    fn test_graph() -> WorldGraph {
        let json = r#"{
            "locations": [
                {
                    "id": 1, "name": "The Crossroads",
                    "description_template": "test", "indoor": false, "public": true,
                    "location_kind": "crossroads",
                    "connections": [
                        {"target": 2, "traversal_minutes": 3, "path_description": "lane"},
                        {"target": 4, "traversal_minutes": 8, "path_description": "road"}
                    ]
                },
                {
                    "id": 2, "name": "Darcy's Pub",
                    "description_template": "test", "indoor": true, "public": true,
                    "location_kind": "pub",
                    "connections": [
                        {"target": 1, "traversal_minutes": 3, "path_description": "lane"},
                        {"target": 3, "traversal_minutes": 5, "path_description": "road"}
                    ]
                },
                {
                    "id": 3, "name": "St. Brigid's Church",
                    "description_template": "test", "indoor": false, "public": true,
                    "location_kind": "church",
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "road"},
                        {"target": 5, "traversal_minutes": 6, "path_description": "track"}
                    ]
                },
                {
                    "id": 4, "name": "Murphy's Farm",
                    "description_template": "test", "indoor": false, "public": false,
                    "location_kind": "farm",
                    "connections": [
                        {"target": 1, "traversal_minutes": 8, "path_description": "road"}
                    ]
                },
                {
                    "id": 5, "name": "The Bog Road",
                    "description_template": "test", "indoor": false, "public": true,
                    "location_kind": "bog",
                    "connections": [
                        {"target": 3, "traversal_minutes": 6, "path_description": "track"}
                    ]
                }
            ]
        }"#;
        WorldGraph::load_from_str(json).unwrap()
    }

    #[test]
    fn test_attenuation_local() {
        assert_eq!(attenuation(0, &Propagation::Local), 1.0);
        assert_eq!(attenuation(1, &Propagation::Local), 0.0);
    }

    #[test]
    fn test_attenuation_near() {
        assert_eq!(attenuation(0, &Propagation::Near), 1.0);
        assert_eq!(attenuation(3, &Propagation::Near), 0.5);
        assert_eq!(attenuation(10, &Propagation::Near), 0.5);
    }

    #[test]
    fn test_attenuation_medium() {
        assert_eq!(attenuation(0, &Propagation::Medium), 1.0);
        let at_7 = attenuation(7, &Propagation::Medium);
        assert!(at_7 > 0.5 && at_7 < 0.6);
        assert_eq!(attenuation(15, &Propagation::Medium), 0.0);
        assert_eq!(attenuation(20, &Propagation::Medium), 0.0);
    }

    #[test]
    fn test_attenuation_far() {
        assert_eq!(attenuation(0, &Propagation::Far(60)), 1.0);
        let at_15 = attenuation(15, &Propagation::Far(60));
        assert!(at_15 > 0.4 && at_15 < 0.6);
        assert_eq!(attenuation(60, &Propagation::Far(60)), 0.0);
    }

    #[test]
    fn test_weather_dampening_values() {
        assert_eq!(weather_dampening(Weather::Clear), 1.0);
        assert_eq!(weather_dampening(Weather::Overcast), 0.95);
        assert_eq!(weather_dampening(Weather::Rain), 0.6);
        assert_eq!(weather_dampening(Weather::Fog), 0.5);
        assert_eq!(weather_dampening(Weather::Storm), 0.3);
    }

    #[test]
    fn test_audible_sounds_at_crossroads() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();
        let sounds = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Dawn,
            Season::Spring,
            Weather::Clear,
            false,
        );

        // Should hear crossroads wind (local)
        assert!(sounds.iter().any(|s| s.entry.path.contains("crossroads")));

        // Should hear farm rooster from 8 min away (Near propagation)
        assert!(
            sounds
                .iter()
                .any(|s| s.entry.path.contains("rooster")
                    && s.entry.source_kind == LocationKind::Farm),
            "should hear farm rooster propagated from Murphy's Farm"
        );
    }

    #[test]
    fn test_church_bells_propagate_far() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();

        // From the bog road (id 5), church (id 3) is 6 min away
        let sounds = audible_sounds(
            LocationId(5),
            &graph,
            &catalog,
            TimeOfDay::Dawn,
            Season::Spring,
            Weather::Clear,
            false,
        );

        assert!(
            sounds.iter().any(|s| s.entry.path.contains("bell_angelus")),
            "should hear church bells from the bog road"
        );
    }

    #[test]
    fn test_pub_music_propagates_to_neighbor() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();

        // From the crossroads (id 1), pub (id 2) is 3 min away
        let sounds = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Night,
            Season::Summer,
            Weather::Clear,
            false,
        );

        // Fiddle reel has Near propagation, should reach 3 min
        assert!(
            sounds.iter().any(|s| s.entry.path.contains("fiddle")),
            "should hear pub fiddle from the crossroads"
        );
    }

    #[test]
    fn test_indoor_dampening() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();

        let outdoor = audible_sounds(
            LocationId(2),
            &graph,
            &catalog,
            TimeOfDay::Night,
            Season::Summer,
            Weather::Clear,
            false,
        );

        let indoor = audible_sounds(
            LocationId(2),
            &graph,
            &catalog,
            TimeOfDay::Night,
            Season::Summer,
            Weather::Clear,
            true,
        );

        // Indoor sounds should be quieter
        let outdoor_vol: f32 = outdoor.iter().map(|s| s.volume).sum();
        let indoor_vol: f32 = indoor.iter().map(|s| s.volume).sum();
        assert!(
            indoor_vol < outdoor_vol,
            "indoor volume ({indoor_vol}) should be less than outdoor ({outdoor_vol})"
        );
    }

    #[test]
    fn test_weather_reduces_volume() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();

        let clear = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Clear,
            false,
        );
        let storm = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Storm,
            false,
        );

        // Compare same entries — storm should have lower volumes
        // (weather overlays add new entries, so compare non-overlay entries)
        let clear_base: f32 = clear
            .iter()
            .filter(|s| !s.entry.is_weather_overlay)
            .map(|s| s.volume)
            .sum();
        let storm_base: f32 = storm
            .iter()
            .filter(|s| !s.entry.is_weather_overlay)
            .map(|s| s.volume)
            .sum();
        if clear_base > 0.0 {
            assert!(
                storm_base < clear_base,
                "storm base volume ({storm_base}) should be less than clear ({clear_base})"
            );
        }
    }

    #[test]
    fn test_weather_overlays_added_in_storm() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();
        let sounds = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Night,
            Season::Summer,
            Weather::Storm,
            false,
        );

        assert!(
            sounds.iter().any(|s| s.entry.is_weather_overlay),
            "storm should include weather overlay sounds"
        );
    }

    #[test]
    fn test_no_weather_overlays_in_clear() {
        let graph = test_graph();
        let catalog = SoundCatalog::new();
        let sounds = audible_sounds(
            LocationId(1),
            &graph,
            &catalog,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Clear,
            false,
        );

        assert!(
            !sounds.iter().any(|s| s.entry.is_weather_overlay),
            "clear weather should have no weather overlay sounds"
        );
    }
}
