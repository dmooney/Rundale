//! NPC data file loading.
//!
//! Loads NPC definitions from a JSON file and hydrates them into
//! fully initialized [`Npc`] instances with bidirectional relationships.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::ParishError;
use crate::npc::memory::ShortTermMemory;
use crate::npc::types::{
    DailySchedule, Intelligence, NpcState, Relationship, RelationshipKind, ScheduleEntry,
};
use crate::npc::{Npc, NpcId};
use crate::world::LocationId;

/// Top-level JSON structure for the NPC data file.
#[derive(Debug, Deserialize)]
struct NpcFile {
    npcs: Vec<NpcFileEntry>,
}

/// A single NPC entry in the data file.
#[derive(Debug, Deserialize)]
struct NpcFileEntry {
    id: u32,
    name: String,
    #[serde(default)]
    brief_description: String,
    age: u8,
    occupation: String,
    personality: String,
    #[serde(default)]
    intelligence: Option<IntelligenceFileEntry>,
    home: u32,
    workplace: Option<u32>,
    mood: String,
    schedule: Vec<ScheduleFileEntry>,
    relationships: Vec<RelationshipFileEntry>,
    #[serde(default)]
    knowledge: Vec<String>,
}

/// Intelligence ratings in the data file.
///
/// Maps directly to [`Intelligence`] dimensions. All fields default to 3
/// (average) if omitted in JSON.
#[derive(Debug, Deserialize)]
struct IntelligenceFileEntry {
    #[serde(default = "default_intelligence_value")]
    verbal: u8,
    #[serde(default = "default_intelligence_value")]
    analytical: u8,
    #[serde(default = "default_intelligence_value")]
    emotional: u8,
    #[serde(default = "default_intelligence_value")]
    practical: u8,
    #[serde(default = "default_intelligence_value")]
    wisdom: u8,
    #[serde(default = "default_intelligence_value")]
    creative: u8,
}

/// Default intelligence dimension value (average).
fn default_intelligence_value() -> u8 {
    3
}

impl From<IntelligenceFileEntry> for Intelligence {
    fn from(e: IntelligenceFileEntry) -> Self {
        Intelligence::new(
            e.verbal,
            e.analytical,
            e.emotional,
            e.practical,
            e.wisdom,
            e.creative,
        )
    }
}

/// A schedule entry in the data file.
#[derive(Debug, Deserialize)]
struct ScheduleFileEntry {
    start_hour: u8,
    end_hour: u8,
    location: u32,
    activity: String,
}

/// A relationship entry in the data file.
#[derive(Debug, Deserialize)]
struct RelationshipFileEntry {
    target_id: u32,
    kind: RelationshipKind,
    strength: f64,
}

/// Loads NPCs from a JSON data file.
///
/// Parses the file, creates `Npc` instances with schedules and knowledge,
/// then hydrates bidirectional relationships (if A relates to B, B also
/// gets a reciprocal relationship entry if one doesn't already exist).
pub fn load_npcs_from_file(path: &Path) -> Result<Vec<Npc>, ParishError> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        ParishError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read NPC file {}: {}", path.display(), e),
        ))
    })?;
    load_npcs_from_str(&contents)
}

/// Loads NPCs from a JSON string.
///
/// Useful for testing without requiring a file on disk.
pub fn load_npcs_from_str(json: &str) -> Result<Vec<Npc>, ParishError> {
    let file: NpcFile = serde_json::from_str(json).map_err(ParishError::Serialization)?;

    // First pass: create all NPCs with their direct relationships
    let mut npcs: Vec<Npc> = file
        .npcs
        .iter()
        .map(|entry| {
            let schedule = DailySchedule {
                entries: entry
                    .schedule
                    .iter()
                    .map(|s| ScheduleEntry {
                        start_hour: s.start_hour,
                        end_hour: s.end_hour,
                        location: LocationId(s.location),
                        activity: s.activity.clone(),
                    })
                    .collect(),
            };

            let relationships: HashMap<NpcId, Relationship> = entry
                .relationships
                .iter()
                .map(|r| (NpcId(r.target_id), Relationship::new(r.kind, r.strength)))
                .collect();

            // Use provided brief_description or fall back to "a {occupation}"
            let brief_description = if entry.brief_description.is_empty() {
                format!("a {}", entry.occupation.to_lowercase())
            } else {
                entry.brief_description.clone()
            };

            Npc {
                id: NpcId(entry.id),
                name: entry.name.clone(),
                brief_description,
                age: entry.age,
                occupation: entry.occupation.clone(),
                personality: entry.personality.clone(),
                intelligence: entry
                    .intelligence
                    .as_ref()
                    .map(|i| {
                        Intelligence::new(
                            i.verbal,
                            i.analytical,
                            i.emotional,
                            i.practical,
                            i.wisdom,
                            i.creative,
                        )
                    })
                    .unwrap_or_default(),
                location: LocationId(entry.home),
                mood: entry.mood.clone(),
                home: Some(LocationId(entry.home)),
                workplace: entry.workplace.map(LocationId),
                schedule: Some(schedule),
                relationships,
                memory: ShortTermMemory::new(),
                knowledge: entry.knowledge.clone(),
                state: NpcState::default(),
                deflated_summary: None,
            }
        })
        .collect();

    // Second pass: ensure bidirectional relationships
    // Collect relationship additions needed
    let mut additions: Vec<(NpcId, NpcId, RelationshipKind, f64)> = Vec::new();
    for npc in &npcs {
        for (target_id, rel) in &npc.relationships {
            additions.push((npc.id, *target_id, rel.kind, rel.strength));
        }
    }

    // Apply reciprocal relationships where missing
    for (from_id, to_id, kind, strength) in additions {
        if let Some(target_npc) = npcs.iter_mut().find(|n| n.id == to_id) {
            target_npc
                .relationships
                .entry(from_id)
                .or_insert_with(|| Relationship::new(kind, strength));
        }
    }

    Ok(npcs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_npcs_from_file() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        assert_eq!(npcs.len(), 8, "expected 8 NPCs in data file");
    }

    #[test]
    fn test_npc_identities() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();

        let names: Vec<&str> = npcs.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"Padraig Darcy"));
        assert!(names.contains(&"Siobhan Murphy"));
        assert!(names.contains(&"Fr. Declan Tierney"));
        assert!(names.contains(&"Roisin Connolly"));
        assert!(names.contains(&"Tommy O'Brien"));
        assert!(names.contains(&"Aoife Brennan"));
        assert!(names.contains(&"Mick Flanagan"));
        assert!(names.contains(&"Niamh Darcy"));
    }

    #[test]
    fn test_all_npcs_have_home() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        for npc in &npcs {
            assert!(
                npc.home.is_some(),
                "{} should have a home location",
                npc.name
            );
        }
    }

    #[test]
    fn test_all_npcs_have_schedules() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        for npc in &npcs {
            assert!(
                npc.schedule.is_some(),
                "{} should have a schedule",
                npc.name
            );
            let schedule = npc.schedule.as_ref().unwrap();
            assert!(
                !schedule.entries.is_empty(),
                "{} schedule should not be empty",
                npc.name
            );
        }
    }

    #[test]
    fn test_bidirectional_relationships() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        let npc_map: HashMap<NpcId, &Npc> = npcs.iter().map(|n| (n.id, n)).collect();

        for npc in &npcs {
            for (target_id, _rel) in &npc.relationships {
                let target = npc_map.get(target_id).unwrap_or_else(|| {
                    panic!(
                        "{} has relationship with NPC {} but that NPC doesn't exist",
                        npc.name, target_id.0
                    )
                });
                assert!(
                    target.relationships.contains_key(&npc.id),
                    "{} relates to {} but {} has no reciprocal relationship",
                    npc.name,
                    target.name,
                    target.name
                );
            }
        }
    }

    #[test]
    fn test_each_npc_has_relationships() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        for npc in &npcs {
            assert!(
                npc.relationships.len() >= 3,
                "{} should have at least 3 relationships, has {}",
                npc.name,
                npc.relationships.len()
            );
        }
    }

    #[test]
    fn test_npcs_have_knowledge() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        for npc in &npcs {
            assert!(
                !npc.knowledge.is_empty(),
                "{} should have knowledge entries",
                npc.name
            );
        }
    }

    #[test]
    fn test_load_minimal_json() {
        let json = r#"{
            "npcs": [{
                "id": 99,
                "name": "Test NPC",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": null,
                "mood": "calm",
                "schedule": [
                    {"start_hour": 0, "end_hour": 23, "location": 1, "activity": "resting"}
                ],
                "relationships": []
            }]
        }"#;
        let npcs = load_npcs_from_str(json).unwrap();
        assert_eq!(npcs.len(), 1);
        assert_eq!(npcs[0].name, "Test NPC");
        assert_eq!(npcs[0].home, Some(LocationId(1)));
        assert!(npcs[0].workplace.is_none());
        assert!(npcs[0].relationships.is_empty());
    }

    #[test]
    fn test_npc_starts_at_home() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let npcs = load_npcs_from_file(path).unwrap();
        for npc in &npcs {
            assert_eq!(
                npc.location,
                npc.home.unwrap(),
                "{} should start at home location",
                npc.name
            );
        }
    }
}
