//! NPC data file loading.
//!
//! Loads NPC definitions from a JSON file and hydrates them into
//! fully initialized [`Npc`] instances with bidirectional relationships.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::ParishError;
use crate::npc::memory::{LongTermMemory, ShortTermMemory};
use crate::npc::reactions::ReactionLog;
use crate::npc::types::{
    Intelligence, NpcState, Relationship, RelationshipKind, ScheduleEntry, ScheduleVariant,
    SeasonalSchedule,
};
use crate::npc::{Npc, NpcId};
use crate::world::LocationId;
use crate::world::time::{DayType, Season};

/// Top-level JSON structure for the NPC data file.
#[derive(Debug, Deserialize)]
struct NpcFile {
    npcs: Vec<NpcFileEntry>,
}

/// A single NPC entry in the data file.
///
/// Supports two schedule formats for backward compatibility:
/// - Legacy: `"schedule": [...]` — flat array of entries, treated as the default variant.
/// - New: `"seasonal_schedule": [...]` — array of variants with optional season/day_type.
///
/// If both are present, `seasonal_schedule` takes priority.
#[derive(Debug, Deserialize)]
struct NpcFileEntry {
    id: u32,
    name: String,
    #[serde(default)]
    brief_description: Option<String>,
    age: u8,
    occupation: String,
    personality: String,
    #[serde(default)]
    intelligence: Option<IntelligenceFileEntry>,
    home: u32,
    workplace: Option<u32>,
    mood: String,
    /// Legacy flat schedule (backward compat).
    #[serde(default)]
    schedule: Option<Vec<ScheduleFileEntry>>,
    /// Season-aware schedule with variants.
    #[serde(default)]
    seasonal_schedule: Option<Vec<ScheduleVariantFileEntry>>,
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
    #[serde(default)]
    cuaird: bool,
}

/// A schedule variant in the seasonal_schedule data file format.
#[derive(Debug, Deserialize)]
struct ScheduleVariantFileEntry {
    #[serde(default)]
    season: Option<Season>,
    #[serde(default)]
    day_type: Option<DayType>,
    entries: Vec<ScheduleFileEntry>,
}

/// A relationship entry in the data file.
#[derive(Debug, Deserialize)]
struct RelationshipFileEntry {
    target_id: u32,
    kind: RelationshipKind,
    strength: f64,
}

/// Converts raw file schedule entries into [`ScheduleEntry`] values.
fn convert_entries(entries: &[ScheduleFileEntry]) -> Vec<ScheduleEntry> {
    entries
        .iter()
        .map(|s| ScheduleEntry {
            start_hour: s.start_hour,
            end_hour: s.end_hour,
            location: LocationId(s.location),
            activity: s.activity.clone(),
            cuaird: s.cuaird,
        })
        .collect()
}

/// Builds a [`SeasonalSchedule`] from an NPC file entry.
///
/// Supports two formats:
/// - `seasonal_schedule`: array of variants with optional season/day_type (preferred).
/// - `schedule`: legacy flat array, wrapped as a single default variant.
fn build_seasonal_schedule(entry: &NpcFileEntry) -> SeasonalSchedule {
    if let Some(variants) = &entry.seasonal_schedule {
        SeasonalSchedule {
            variants: variants
                .iter()
                .map(|v| ScheduleVariant {
                    season: v.season,
                    day_type: v.day_type,
                    entries: convert_entries(&v.entries),
                })
                .collect(),
        }
    } else if let Some(flat) = &entry.schedule {
        // Legacy format: wrap as a single default variant (None, None)
        SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: convert_entries(flat),
            }],
        }
    } else {
        SeasonalSchedule {
            variants: Vec::new(),
        }
    }
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
            let schedule = build_seasonal_schedule(entry);

            let relationships: HashMap<NpcId, Relationship> = entry
                .relationships
                .iter()
                .map(|r| (NpcId(r.target_id), Relationship::new(r.kind, r.strength)))
                .collect();

            let brief_description = entry
                .brief_description
                .clone()
                .unwrap_or_else(|| format!("a {}", entry.occupation.to_lowercase()));

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
                long_term_memory: LongTermMemory::new(),
                knowledge: entry.knowledge.clone(),
                state: NpcState::default(),
                deflated_summary: None,
                reaction_log: ReactionLog::default(),
                last_activity: None,
                is_ill: false,
            }
        })
        .collect();

    // Validate referential integrity: all relationship targets must exist
    let valid_ids: std::collections::HashSet<NpcId> = npcs.iter().map(|n| n.id).collect();
    for npc in &npcs {
        for target_id in npc.relationships.keys() {
            if !valid_ids.contains(target_id) {
                return Err(ParishError::Setup(format!(
                    "{} has relationship with NPC {} but that NPC doesn't exist",
                    npc.name, target_id.0
                )));
            }
        }
    }

    // Second pass: ensure bidirectional relationships
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
        assert_eq!(npcs.len(), 23, "expected 23 NPCs in data file");
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
                !schedule.variants.is_empty(),
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
            for target_id in npc.relationships.keys() {
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

    #[test]
    fn test_legacy_schedule_loaded_as_default_variant() {
        let json = r#"{
            "npcs": [{
                "id": 99,
                "name": "Legacy NPC",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": null,
                "mood": "calm",
                "schedule": [
                    {"start_hour": 0, "end_hour": 11, "location": 1, "activity": "sleeping"},
                    {"start_hour": 12, "end_hour": 23, "location": 2, "activity": "working"}
                ],
                "relationships": []
            }]
        }"#;
        let npcs = load_npcs_from_str(json).unwrap();
        let sched = npcs[0].schedule.as_ref().unwrap();
        // Legacy format should produce a single default variant
        assert_eq!(sched.variants.len(), 1);
        assert!(sched.variants[0].season.is_none());
        assert!(sched.variants[0].day_type.is_none());
        assert_eq!(sched.variants[0].entries.len(), 2);
        // Should resolve for any season/day combination
        use crate::world::time::{DayType, Season};
        let loc = sched.location_at(15, Season::Winter, DayType::Sunday);
        assert_eq!(loc, Some(LocationId(2)));
    }

    #[test]
    fn test_seasonal_schedule_loaded_with_variants() {
        let json = r#"{
            "npcs": [{
                "id": 99,
                "name": "Seasonal NPC",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": null,
                "mood": "calm",
                "seasonal_schedule": [
                    {
                        "entries": [
                            {"start_hour": 0, "end_hour": 23, "location": 1, "activity": "default routine"}
                        ]
                    },
                    {
                        "season": "winter",
                        "entries": [
                            {"start_hour": 0, "end_hour": 23, "location": 2, "activity": "winter routine"}
                        ]
                    },
                    {
                        "day_type": "sunday",
                        "entries": [
                            {"start_hour": 0, "end_hour": 23, "location": 3, "activity": "sunday mass"}
                        ]
                    }
                ],
                "relationships": []
            }]
        }"#;
        let npcs = load_npcs_from_str(json).unwrap();
        let sched = npcs[0].schedule.as_ref().unwrap();
        assert_eq!(sched.variants.len(), 3);

        use crate::world::time::{DayType, Season};
        // Summer weekday -> default (location 1)
        assert_eq!(
            sched.location_at(12, Season::Summer, DayType::Weekday),
            Some(LocationId(1))
        );
        // Winter weekday -> winter variant (location 2)
        assert_eq!(
            sched.location_at(12, Season::Winter, DayType::Weekday),
            Some(LocationId(2))
        );
        // Summer sunday -> sunday variant (location 3)
        assert_eq!(
            sched.location_at(12, Season::Summer, DayType::Sunday),
            Some(LocationId(3))
        );
    }

    #[test]
    fn test_cuaird_flag_loaded() {
        let json = r#"{
            "npcs": [{
                "id": 99,
                "name": "Cuaird NPC",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": null,
                "mood": "calm",
                "seasonal_schedule": [
                    {
                        "entries": [
                            {"start_hour": 0, "end_hour": 18, "location": 1, "activity": "working"},
                            {"start_hour": 19, "end_hour": 23, "location": 2, "activity": "visiting neighbours", "cuaird": true}
                        ]
                    }
                ],
                "relationships": []
            }]
        }"#;
        let npcs = load_npcs_from_str(json).unwrap();
        let sched = npcs[0].schedule.as_ref().unwrap();
        use crate::world::time::{DayType, Season};
        let entry = sched
            .entry_at(20, Season::Summer, DayType::Weekday)
            .unwrap();
        assert!(entry.cuaird);
        assert_eq!(entry.activity, "visiting neighbours");

        let entry = sched
            .entry_at(10, Season::Summer, DayType::Weekday)
            .unwrap();
        assert!(!entry.cuaird);
    }

    #[test]
    fn test_invalid_relationship_target_returns_error() {
        let json = r#"{
            "npcs": [{
                "id": 1,
                "name": "Alice",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": null,
                "mood": "calm",
                "schedule": [
                    {"start_hour": 0, "end_hour": 23, "location": 1, "activity": "resting"}
                ],
                "relationships": [
                    {"target_id": 999, "kind": "Friend", "strength": 0.5}
                ]
            }]
        }"#;
        let result = load_npcs_from_str(json);
        assert!(
            result.is_err(),
            "should error on invalid relationship target"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("999"),
            "error should mention the invalid NPC id: {err_msg}"
        );
    }
}
