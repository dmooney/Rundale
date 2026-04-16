//! NPC data file loading.
//!
//! Loads NPC definitions from a JSON file and hydrates them into
//! fully initialized [`Npc`] instances with bidirectional relationships.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::memory::{LongTermMemory, ShortTermMemory};
use crate::reactions::ReactionLog;
use crate::types::{
    Intelligence, NpcState, Relationship, RelationshipKind, ScheduleEntry, ScheduleVariant,
    SeasonalSchedule,
};
use crate::{Npc, NpcId};
use parish_types::LocationId;
use parish_types::ParishError;
use parish_world::time::{DayType, Season};

/// Top-level JSON structure for the NPC data file.
///
/// Exposed publicly so the Parish Designer editor can round-trip
/// `npcs.json` without duplicating the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcFile {
    pub npcs: Vec<NpcFileEntry>,
}

/// A single NPC entry in the data file.
///
/// Supports two schedule formats for backward compatibility:
/// - Legacy: `"schedule": [...]` — flat array of entries, treated as the default variant.
/// - New: `"seasonal_schedule": [...]` — array of variants with optional season/day_type.
///
/// If both are present, `seasonal_schedule` takes priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcFileEntry {
    pub id: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief_description: Option<String>,
    pub age: u8,
    pub occupation: String,
    pub personality: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intelligence: Option<IntelligenceFileEntry>,
    pub home: u32,
    pub workplace: Option<u32>,
    pub mood: String,
    /// Legacy flat schedule (backward compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Vec<ScheduleFileEntry>>,
    /// Season-aware schedule with variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasonal_schedule: Option<Vec<ScheduleVariantFileEntry>>,
    pub relationships: Vec<RelationshipFileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge: Vec<String>,
}

/// Intelligence ratings in the data file.
///
/// Maps directly to [`Intelligence`] dimensions. All fields default to 3
/// (average) if omitted in JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceFileEntry {
    #[serde(default = "default_intelligence_value")]
    pub verbal: u8,
    #[serde(default = "default_intelligence_value")]
    pub analytical: u8,
    #[serde(default = "default_intelligence_value")]
    pub emotional: u8,
    #[serde(default = "default_intelligence_value")]
    pub practical: u8,
    #[serde(default = "default_intelligence_value")]
    pub wisdom: u8,
    #[serde(default = "default_intelligence_value")]
    pub creative: u8,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleFileEntry {
    pub start_hour: u8,
    pub end_hour: u8,
    pub location: u32,
    pub activity: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cuaird: bool,
}

/// A schedule variant in the seasonal_schedule data file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleVariantFileEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<Season>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_type: Option<DayType>,
    pub entries: Vec<ScheduleFileEntry>,
}

/// A relationship entry in the data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipFileEntry {
    pub target_id: u32,
    pub kind: RelationshipKind,
    pub strength: f64,
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
        use parish_world::time::{DayType, Season};
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

        use parish_world::time::{DayType, Season};
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
        use parish_world::time::{DayType, Season};
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
    fn test_npc_file_schema_round_trip() {
        // Round-trip the real mods/rundale/npcs.json through the public
        // NpcFile schema. The re-serialized JSON must deserialize back to a
        // structurally identical NpcFile. This is the single most important
        // schema test: it catches drift between the editor and the game
        // loader, which would silently corrupt source files on save.
        let path = Path::new("../../mods/rundale/npcs.json");
        if !path.exists() {
            return;
        }
        let raw = std::fs::read_to_string(path).unwrap();
        let original: NpcFile = serde_json::from_str(&raw).unwrap();
        let re_serialized = serde_json::to_string_pretty(&original).unwrap();
        let roundtripped: NpcFile = serde_json::from_str(&re_serialized).unwrap();
        assert_eq!(
            original.npcs.len(),
            roundtripped.npcs.len(),
            "NPC count must match after round-trip"
        );
        for (a, b) in original.npcs.iter().zip(roundtripped.npcs.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.brief_description, b.brief_description);
            assert_eq!(a.age, b.age);
            assert_eq!(a.occupation, b.occupation);
            assert_eq!(a.home, b.home);
            assert_eq!(a.workplace, b.workplace);
            assert_eq!(a.mood, b.mood);
            assert_eq!(a.relationships.len(), b.relationships.len());
            assert_eq!(a.knowledge, b.knowledge);
        }
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
