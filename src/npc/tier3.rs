//! Tier 3 batch inference for distant NPCs.
//!
//! Processes groups of up to 8 NPCs per batch with a single LLM call
//! every game-day (1440 game-minutes).

use serde::{Deserialize, Serialize};

use crate::npc::{Npc, NpcId};

/// Maximum NPCs per Tier 3 batch.
pub const TIER3_BATCH_SIZE: usize = 8;

/// Game-minutes between Tier 3 ticks (1 game day).
pub const TIER3_TICK_GAME_MINUTES: i64 = 1440;

/// Update for a single NPC from Tier 3 batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3Update {
    /// Which NPC this update applies to.
    pub npc_id: NpcId,
    /// New mood to set, if any.
    #[serde(default)]
    pub new_mood: Option<String>,
    /// Summary of what happened to this NPC.
    #[serde(default)]
    pub summary: String,
    /// Relationship changes produced by batch processing.
    #[serde(default)]
    pub relationship_changes: Vec<Tier3RelChange>,
}

/// Relationship change from Tier 3 processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3RelChange {
    /// The NPC whose relationship is affected.
    pub target_id: NpcId,
    /// Change in relationship strength.
    pub delta: f64,
}

/// Batch response from Tier 3 LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3BatchResponse {
    /// Per-NPC updates from the batch.
    #[serde(default)]
    pub updates: Vec<Tier3Update>,
}

/// Builds a batch prompt listing all NPCs with name, occupation, mood, and location.
///
/// The prompt asks the LLM to produce a JSON `Tier3BatchResponse` summarizing
/// what each NPC has been doing over the past game-day.
pub fn build_tier3_prompt(npcs: &[&Npc]) -> String {
    let mut prompt = String::from(
        "You are simulating background activity for NPCs in a rural Irish parish in 1820.\n\
         For each NPC listed below, produce a brief summary of what they did today, \
         an optional mood change, and any relationship changes.\n\n\
         NPCs:\n",
    );

    for npc in npcs {
        prompt.push_str(&format!(
            "- ID {}: {} ({}), mood: \"{}\", location: {}\n",
            npc.id.0, npc.name, npc.occupation, npc.mood, npc.location.0
        ));
    }

    prompt.push_str(
        "\nRespond with JSON in this exact format:\n\
         {\"updates\": [{\"npc_id\": <id>, \"new_mood\": \"<mood or null>\", \
         \"summary\": \"<what they did>\", \
         \"relationship_changes\": [{\"target_id\": <id>, \"delta\": <float>}]}]}\n",
    );

    prompt
}

/// Parses a JSON string into a `Tier3BatchResponse`.
///
/// Returns `None` if the JSON is malformed or cannot be deserialized.
pub fn parse_tier3_response(json: &str) -> Option<Tier3BatchResponse> {
    serde_json::from_str(json).ok()
}

/// Applies mood changes from Tier 3 batch updates to NPC state.
///
/// For each update that contains a `new_mood`, finds the matching NPC
/// in the slice and updates its mood.
pub fn apply_tier3_updates(npcs: &mut [Npc], updates: &[Tier3Update]) {
    for update in updates {
        if let Some(npc) = npcs.iter_mut().find(|n| n.id == update.npc_id)
            && let Some(ref mood) = update.new_mood
        {
            npc.mood.clone_from(mood);
        }
    }
}

/// Splits a slice of NPC ids into batches of the given size.
///
/// The last batch may contain fewer than `batch_size` elements.
pub fn split_into_batches(npc_ids: &[NpcId], batch_size: usize) -> Vec<Vec<NpcId>> {
    if batch_size == 0 {
        return vec![];
    }
    npc_ids.chunks(batch_size).map(|c| c.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::memory::{LongTermMemory, ShortTermMemory};
    use crate::npc::types::NpcState;
    use crate::world::LocationId;
    use std::collections::HashMap;

    fn make_npc(id: u32, name: &str, occupation: &str, mood: &str, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            brief_description: format!("a {}", occupation.to_lowercase()),
            age: 35,
            occupation: occupation.to_string(),
            personality: "test".to_string(),
            location: LocationId(location),
            mood: mood.to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
        }
    }

    #[test]
    fn test_build_tier3_prompt_contains_npc_info() {
        let npc1 = make_npc(1, "Padraig", "Publican", "content", 2);
        let npc2 = make_npc(2, "Siobhan", "Teacher", "cheerful", 5);
        let npcs: Vec<&Npc> = vec![&npc1, &npc2];

        let prompt = build_tier3_prompt(&npcs);
        assert!(prompt.contains("Padraig"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("content"));
        assert!(prompt.contains("Siobhan"));
        assert!(prompt.contains("Teacher"));
        assert!(prompt.contains("cheerful"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_build_tier3_prompt_empty() {
        let npcs: Vec<&Npc> = vec![];
        let prompt = build_tier3_prompt(&npcs);
        assert!(prompt.contains("NPCs:"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_parse_tier3_response_valid() {
        let json = r#"{"updates": [
            {"npc_id": 1, "new_mood": "happy", "summary": "Worked the bar",
             "relationship_changes": [{"target_id": 2, "delta": 0.1}]}
        ]}"#;
        let resp = parse_tier3_response(json).unwrap();
        assert_eq!(resp.updates.len(), 1);
        assert_eq!(resp.updates[0].npc_id, NpcId(1));
        assert_eq!(resp.updates[0].new_mood, Some("happy".to_string()));
        assert_eq!(resp.updates[0].summary, "Worked the bar");
        assert_eq!(resp.updates[0].relationship_changes.len(), 1);
        assert!((resp.updates[0].relationship_changes[0].delta - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_tier3_response_invalid() {
        assert!(parse_tier3_response("not json").is_none());
        assert!(parse_tier3_response("").is_none());
    }

    #[test]
    fn test_parse_tier3_response_minimal() {
        let json = r#"{"updates": []}"#;
        let resp = parse_tier3_response(json).unwrap();
        assert!(resp.updates.is_empty());
    }

    #[test]
    fn test_apply_tier3_updates_mood_change() {
        let mut npcs = vec![
            make_npc(1, "Padraig", "Publican", "calm", 2),
            make_npc(2, "Siobhan", "Teacher", "neutral", 5),
        ];
        let updates = vec![
            Tier3Update {
                npc_id: NpcId(1),
                new_mood: Some("happy".to_string()),
                summary: "Good day".to_string(),
                relationship_changes: vec![],
            },
            Tier3Update {
                npc_id: NpcId(2),
                new_mood: None,
                summary: "Quiet day".to_string(),
                relationship_changes: vec![],
            },
        ];

        apply_tier3_updates(&mut npcs, &updates);
        assert_eq!(npcs[0].mood, "happy");
        assert_eq!(npcs[1].mood, "neutral"); // unchanged
    }

    #[test]
    fn test_apply_tier3_updates_unknown_npc() {
        let mut npcs = vec![make_npc(1, "Padraig", "Publican", "calm", 2)];
        let updates = vec![Tier3Update {
            npc_id: NpcId(99),
            new_mood: Some("angry".to_string()),
            summary: "Bad day".to_string(),
            relationship_changes: vec![],
        }];
        // Should not panic
        apply_tier3_updates(&mut npcs, &updates);
        assert_eq!(npcs[0].mood, "calm");
    }

    #[test]
    fn test_split_into_batches_exact() {
        let ids: Vec<NpcId> = (0..8).map(NpcId).collect();
        let batches = split_into_batches(&ids, 8);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 8);
    }

    #[test]
    fn test_split_into_batches_remainder() {
        let ids: Vec<NpcId> = (0..10).map(NpcId).collect();
        let batches = split_into_batches(&ids, 8);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 8);
        assert_eq!(batches[1].len(), 2);
    }

    #[test]
    fn test_split_into_batches_empty() {
        let ids: Vec<NpcId> = vec![];
        let batches = split_into_batches(&ids, 8);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_split_into_batches_zero_size() {
        let ids: Vec<NpcId> = (0..5).map(NpcId).collect();
        let batches = split_into_batches(&ids, 0);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_tier3_constants() {
        assert_eq!(TIER3_BATCH_SIZE, 8);
        assert_eq!(TIER3_TICK_GAME_MINUTES, 1440);
    }
}
