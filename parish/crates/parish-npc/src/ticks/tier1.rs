//! Tier 1 tick — per-player-interaction response processing.
//!
//! Applies LLM responses from Tier 1 inference: updates NPC mood, records
//! interaction memories, and propagates witness memories to bystanders.

use chrono::{DateTime, Utc};

use crate::memory::{MemoryEntry, try_promote};
use crate::{Npc, NpcId, NpcStreamResponse};
use parish_config::NpcConfig;
use parish_world::LocationId;

use super::truncate::truncate_for_memory;

/// Processes a Tier 1 NPC response using the given config, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
pub fn apply_tier1_response_with_config(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: DateTime<Utc>,
    config: &NpcConfig,
    player_name: Option<&str>,
) -> Vec<String> {
    let mut events = Vec::new();

    // Update mood from metadata
    if let Some(ref meta) = response.metadata
        && !meta.mood.is_empty()
        && meta.mood != npc.mood
    {
        events.push(format!("{} mood: {} -> {}", npc.name, npc.mood, meta.mood));
        npc.mood = meta.mood.clone();
    }

    // Record memory of the interaction, using player's name if known
    let speaker_label = player_name.unwrap_or("A newcomer");
    let content = format!(
        "{} said: '{}'. Responded: {}",
        speaker_label,
        player_input,
        truncate_for_memory(&response.dialogue, config.memory_truncation_dialogue)
    );
    events.push(format!(
        "{} remembers: {}",
        npc.name,
        truncate_for_memory(&content, config.memory_truncation_event_log)
    ));
    let mem_entry = MemoryEntry {
        timestamp: game_time,
        content,
        participants: vec![NpcId(0), npc.id], // NpcId(0) = player
        location: npc.location,
        kind: Some(crate::memory::MemoryKind::SpokeWithPlayer),
    };
    if let Some(evicted) = npc.memory.add(mem_entry) {
        let npc_name = npc.name.clone();
        let loc_name = String::new(); // location name not available here
        try_promote(&mut npc.long_term_memory, &evicted, &[npc_name], &loc_name);
    }

    events
}

/// Processes a Tier 1 NPC response, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
#[cfg(test)]
pub(crate) fn apply_tier1_response(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: DateTime<Utc>,
) -> Vec<String> {
    apply_tier1_response_with_config(
        npc,
        response,
        player_input,
        game_time,
        &NpcConfig::default(),
        None,
    )
}

/// Records witness memories for NPCs who overheard a player-NPC conversation.
///
/// When the player speaks to one NPC, other NPCs at the same location
/// witness the exchange and store it in their short-term memory. This
/// gives bystander NPCs awareness of what's been said around them.
pub fn record_witness_memories(
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    speaker_id: NpcId,
    speaker_name: &str,
    player_input: &str,
    npc_dialogue: &str,
    game_time: DateTime<chrono::Utc>,
    location: LocationId,
) -> Vec<String> {
    let mut debug_events = Vec::new();

    let content = format!(
        "Overheard: a newcomer said '{}' and {} replied '{}'",
        player_input, speaker_name, npc_dialogue,
    );

    // Collect witness IDs first to avoid borrow issues
    let witness_ids: Vec<NpcId> = npcs
        .values()
        .filter(|npc| npc.location == location && npc.id != speaker_id)
        .filter(|npc| matches!(npc.state, crate::types::NpcState::Present))
        .map(|npc| npc.id)
        .collect();

    for witness_id in witness_ids {
        let mem_entry = MemoryEntry {
            timestamp: game_time,
            content: content.clone(),
            participants: vec![NpcId(0), speaker_id, witness_id],
            location,
            kind: Some(crate::memory::MemoryKind::OverheardConversation),
        };

        if let Some(witness) = npcs.get_mut(&witness_id) {
            debug_events.push(format!(
                "{} overheard: {}",
                witness.name,
                truncate_for_memory(&content, 80),
            ));

            if let Some(evicted) = witness.memory.add(mem_entry) {
                let witness_name = witness.name.clone();
                try_promote(&mut witness.long_term_memory, &evicted, &[witness_name], "");
            }
        }
    }

    debug_events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_test_npc;
    use chrono::TimeZone;
    use std::collections::HashMap;

    #[test]
    fn test_witness_memory_created_for_bystander() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, 1);
        let witness = make_test_npc(2, 1);
        npcs.insert(NpcId(1), {
            let mut n = speaker;
            n.name = "Padraig".to_string();
            n
        });
        npcs.insert(NpcId(2), {
            let mut n = witness;
            n.name = "Niamh".to_string();
            n
        });

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Tell me about the weather",
            "Ah, it's grand today",
            game_time,
            LocationId(1),
        );

        assert_eq!(events.len(), 1);
        assert!(events[0].contains("Niamh overheard"));

        // Witness should have the memory
        let witness = npcs.get(&NpcId(2)).unwrap();
        assert_eq!(witness.memory.len(), 1);
        let mem = witness.memory.recent(1);
        assert!(mem[0].content.contains("Overheard"));
        assert!(mem[0].content.contains("Padraig"));
    }

    #[test]
    fn test_speaker_not_given_witness_memory() {
        let mut npcs = HashMap::new();
        let speaker = make_test_npc(1, 1);
        let witness = make_test_npc(2, 1);
        npcs.insert(NpcId(1), {
            let mut n = speaker;
            n.name = "Padraig".to_string();
            n
        });
        npcs.insert(NpcId(2), {
            let mut n = witness;
            n.name = "Niamh".to_string();
            n
        });

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Hello",
            "Dia dhuit!",
            game_time,
            LocationId(1),
        );

        // Speaker should NOT have a witness memory
        let speaker = npcs.get(&NpcId(1)).unwrap();
        assert!(speaker.memory.is_empty());
    }

    #[test]
    fn test_witness_memory_only_for_present_npcs() {
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), {
            let mut n = make_test_npc(1, 1);
            n.name = "Padraig".to_string();
            n
        });
        npcs.insert(NpcId(2), {
            let mut n = make_test_npc(2, 1);
            n.name = "Niamh".to_string();
            n
        });
        npcs.insert(NpcId(3), {
            let mut n = make_test_npc(3, 2); // different location
            n.name = "Tommy".to_string();
            n
        });

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "Hello",
            "Dia dhuit!",
            game_time,
            LocationId(1),
        );

        assert_eq!(events.len(), 1); // only Niamh
        assert!(events[0].contains("Niamh"));

        // NPC at different location should NOT have memory
        let away = npcs.get(&NpcId(3)).unwrap();
        assert!(away.memory.is_empty());
    }

    #[test]
    fn test_witness_memory_content_format() {
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), {
            let mut n = make_test_npc(1, 1);
            n.name = "Padraig".to_string();
            n
        });
        npcs.insert(NpcId(2), {
            let mut n = make_test_npc(2, 1);
            n.name = "Niamh".to_string();
            n
        });

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        record_witness_memories(
            &mut npcs,
            NpcId(1),
            "Padraig",
            "What do you know about the landlord?",
            "That man is no friend of ours.",
            game_time,
            LocationId(1),
        );

        let witness = npcs.get(&NpcId(2)).unwrap();
        let mem = witness.memory.recent(1);
        assert!(mem[0].content.contains("landlord"));
        assert!(mem[0].content.contains("Padraig"));
        assert!(mem[0].content.contains("no friend"));
        // Participants should include player, speaker, and witness
        assert!(mem[0].participants.contains(&NpcId(0)));
        assert!(mem[0].participants.contains(&NpcId(1)));
        assert!(mem[0].participants.contains(&NpcId(2)));
    }
}
