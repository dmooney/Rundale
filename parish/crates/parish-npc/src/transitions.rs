//! NPC state inflation and deflation for cognitive tier transitions.
//!
//! When an NPC is promoted to a higher cognitive tier (closer to the
//! player), we **inflate** their context by injecting a synthetic
//! memory entry summarizing recent events they were involved in.
//!
//! When an NPC is demoted to a lower tier (farther from the player),
//! we **deflate** their state by capturing a compact summary that can
//! be used to quickly rebuild context if they return.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Npc, NpcId};
use parish_types::LocationId;
use parish_world::events::GameEvent;

/// A compact summary of an NPC's state at the time of deflation.
///
/// Stored on `Npc::deflated_summary` when the NPC drops to a lower
/// cognitive tier. Used during inflation to quickly rebuild narrative
/// context without replaying the full event history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NpcSummary {
    /// Which NPC this summary is for.
    pub npc_id: NpcId,
    /// Location at the time of deflation.
    pub location: LocationId,
    /// Mood at the time of deflation.
    pub mood: String,
    /// Short summaries of recent activity (up to 3).
    pub recent_activity: Vec<String>,
    /// Notable relationship changes since last inflation.
    pub key_relationship_changes: Vec<String>,
}

/// Inflates an NPC's context after promotion to a higher cognitive tier.
///
/// Filters recent [`GameEvent`]s for events involving this NPC, builds
/// a narrative summary, and injects it as a synthetic [`MemoryEntry`]
/// so the LLM has context about what happened while the NPC was in a
/// lower tier.
///
/// Returns `true` if a memory was injected, `false` if there were no
/// relevant events to summarize.
pub fn inflate_npc_context(
    npc: &mut Npc,
    recent_events: &[GameEvent],
    game_time: DateTime<Utc>,
) -> bool {
    let relevant = filter_events_for_npc(npc.id, recent_events);
    if relevant.is_empty() {
        return false;
    }

    // Build narrative summary from relevant events
    let summaries: Vec<String> = relevant
        .iter()
        .map(|e| summarize_event_for_npc(npc.id, e))
        .collect();
    let narrative = summaries.join(" ");

    // Inject as a synthetic memory entry (no promotion — recap is synthetic)
    use crate::memory::MemoryEntry;
    let _ = npc.memory.add(MemoryEntry {
        timestamp: game_time,
        content: format!("[Context recap] {}", narrative),
        participants: vec![npc.id],
        location: npc.location,
        kind: None,
    });

    // Clear the deflated summary since we've now inflated
    npc.deflated_summary = None;

    true
}

/// Deflates an NPC's state after demotion to a lower cognitive tier.
///
/// Captures the NPC's current mood, location, recent memories, and
/// any relationship changes from recent events into a compact
/// [`NpcSummary`].
pub fn deflate_npc_state(npc: &Npc, recent_events: &[GameEvent]) -> NpcSummary {
    // Extract up to 3 most recent memory entries as activity summaries
    let recent_activity: Vec<String> = npc
        .memory
        .recent(3)
        .iter()
        .map(|m| m.content.clone())
        .collect();

    // Extract relationship changes from recent events
    let key_relationship_changes: Vec<String> = recent_events
        .iter()
        .filter_map(|e| match e {
            GameEvent::RelationshipChanged {
                npc_a,
                npc_b,
                delta,
                ..
            } if *npc_a == npc.id || *npc_b == npc.id => {
                let other = if *npc_a == npc.id { npc_b } else { npc_a };
                let direction = if *delta > 0.0 { "improved" } else { "worsened" };
                Some(format!(
                    "Relationship with NPC {} {} by {:.2}",
                    other.0,
                    direction,
                    delta.abs()
                ))
            }
            _ => None,
        })
        .collect();

    NpcSummary {
        npc_id: npc.id,
        location: npc.location,
        mood: npc.mood.clone(),
        recent_activity,
        key_relationship_changes,
    }
}

/// Filters events to only those involving a specific NPC.
fn filter_events_for_npc(npc_id: NpcId, events: &[GameEvent]) -> Vec<&GameEvent> {
    events
        .iter()
        .filter(|e| event_involves_npc(npc_id, e))
        .collect()
}

/// Returns whether a game event involves a specific NPC.
fn event_involves_npc(npc_id: NpcId, event: &GameEvent) -> bool {
    match event {
        GameEvent::DialogueOccurred { npc_id: id, .. }
        | GameEvent::MoodChanged { npc_id: id, .. }
        | GameEvent::NpcArrived { npc_id: id, .. }
        | GameEvent::NpcDeparted { npc_id: id, .. }
        | GameEvent::LifeEvent { npc_id: id, .. } => *id == npc_id,
        GameEvent::RelationshipChanged { npc_a, npc_b, .. } => *npc_a == npc_id || *npc_b == npc_id,
        GameEvent::WeatherChanged { .. } | GameEvent::FestivalStarted { .. } => false,
    }
}

/// Produces a short human-readable summary of an event for a specific NPC.
fn summarize_event_for_npc(npc_id: NpcId, event: &GameEvent) -> String {
    match event {
        GameEvent::DialogueOccurred { summary, .. } => {
            format!("Had a conversation: {summary}")
        }
        GameEvent::MoodChanged { new_mood, .. } => {
            format!("Mood shifted to {new_mood}.")
        }
        GameEvent::RelationshipChanged {
            npc_a,
            npc_b,
            delta,
            ..
        } => {
            let other = if *npc_a == npc_id { npc_b.0 } else { npc_a.0 };
            let direction = if *delta > 0.0 {
                "grew closer to"
            } else {
                "grew distant from"
            };
            format!("{direction} NPC {other}.")
        }
        GameEvent::NpcArrived { location, .. } => {
            format!("Arrived at location {}.", location.0)
        }
        GameEvent::NpcDeparted { location, .. } => {
            format!("Left location {}.", location.0)
        }
        GameEvent::LifeEvent { description, .. } => {
            format!("Experienced: {description}")
        }
        GameEvent::WeatherChanged { .. } | GameEvent::FestivalStarted { .. } => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::types::{Intelligence, NpcState};
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;

    fn test_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
    }

    fn make_npc(id: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {id}"),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            intelligence: Intelligence::default(),
            location: LocationId(1),
            mood: "calm".to_string(),
            emotion: parish_types::EmotionState::default(),
            temperament: parish_types::Temperament::default(),
            home: Some(LocationId(1)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
            deflated_summary: None,
            reaction_log: crate::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    #[test]
    fn test_inflate_with_relevant_events() {
        let mut npc = make_npc(1);
        let events = vec![
            GameEvent::MoodChanged {
                npc_id: NpcId(1),
                new_mood: "happy".to_string(),
                timestamp: test_time(),
            },
            GameEvent::DialogueOccurred {
                npc_id: NpcId(1),
                summary: "discussed the weather".to_string(),
                timestamp: test_time(),
            },
        ];

        let injected = inflate_npc_context(&mut npc, &events, test_time());
        assert!(injected);
        let memories = npc.memory.recent(10);
        assert_eq!(memories.len(), 1);
        assert!(memories[0].content.contains("[Context recap]"));
        assert!(memories[0].content.contains("happy"));
    }

    #[test]
    fn test_inflate_no_relevant_events() {
        let mut npc = make_npc(1);
        let events = vec![GameEvent::MoodChanged {
            npc_id: NpcId(99), // different NPC
            new_mood: "angry".to_string(),
            timestamp: test_time(),
        }];

        let injected = inflate_npc_context(&mut npc, &events, test_time());
        assert!(!injected);
        assert!(npc.memory.recent(10).is_empty());
    }

    #[test]
    fn test_inflate_clears_deflated_summary() {
        let mut npc = make_npc(1);
        npc.deflated_summary = Some(NpcSummary {
            npc_id: NpcId(1),
            location: LocationId(1),
            mood: "calm".to_string(),
            recent_activity: vec![],
            key_relationship_changes: vec![],
        });

        let events = vec![GameEvent::NpcArrived {
            npc_id: NpcId(1),
            location: LocationId(2),
            timestamp: test_time(),
        }];

        inflate_npc_context(&mut npc, &events, test_time());
        assert!(npc.deflated_summary.is_none());
    }

    #[test]
    fn test_deflate_captures_state() {
        let mut npc = make_npc(1);
        npc.mood = "anxious".to_string();
        npc.location = LocationId(5);

        // Add some memories
        use crate::memory::MemoryEntry;
        npc.memory.add(MemoryEntry {
            timestamp: test_time(),
            content: "Saw a rabbit".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(5),
            kind: None,
        });

        let events = vec![GameEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: 0.3,
            timestamp: test_time(),
        }];

        let summary = deflate_npc_state(&npc, &events);
        assert_eq!(summary.npc_id, NpcId(1));
        assert_eq!(summary.location, LocationId(5));
        assert_eq!(summary.mood, "anxious");
        assert_eq!(summary.recent_activity.len(), 1);
        assert!(summary.recent_activity[0].contains("rabbit"));
        assert_eq!(summary.key_relationship_changes.len(), 1);
        assert!(summary.key_relationship_changes[0].contains("improved"));
    }

    #[test]
    fn test_deflate_empty_state() {
        let npc = make_npc(1);
        let summary = deflate_npc_state(&npc, &[]);
        assert_eq!(summary.npc_id, NpcId(1));
        assert!(summary.recent_activity.is_empty());
        assert!(summary.key_relationship_changes.is_empty());
    }

    #[test]
    fn test_event_involves_npc_dialogue() {
        let event = GameEvent::DialogueOccurred {
            npc_id: NpcId(3),
            summary: "test".to_string(),
            timestamp: test_time(),
        };
        assert!(event_involves_npc(NpcId(3), &event));
        assert!(!event_involves_npc(NpcId(1), &event));
    }

    #[test]
    fn test_event_involves_npc_relationship() {
        let event = GameEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: 0.1,
            timestamp: test_time(),
        };
        assert!(event_involves_npc(NpcId(1), &event));
        assert!(event_involves_npc(NpcId(2), &event));
        assert!(!event_involves_npc(NpcId(3), &event));
    }

    #[test]
    fn test_weather_event_involves_no_npc() {
        let event = GameEvent::WeatherChanged {
            new_weather: "Storm".to_string(),
            timestamp: test_time(),
        };
        assert!(!event_involves_npc(NpcId(1), &event));
    }

    #[test]
    fn test_summarize_mood_event() {
        let event = GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "joyful".to_string(),
            timestamp: test_time(),
        };
        let summary = summarize_event_for_npc(NpcId(1), &event);
        assert!(summary.contains("joyful"));
    }

    #[test]
    fn test_summarize_relationship_event() {
        let event = GameEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: -0.2,
            timestamp: test_time(),
        };
        let summary = summarize_event_for_npc(NpcId(1), &event);
        assert!(summary.contains("grew distant from"));
        assert!(summary.contains("NPC 2"));
    }

    #[test]
    fn test_filter_events_for_npc() {
        let events = vec![
            GameEvent::MoodChanged {
                npc_id: NpcId(1),
                new_mood: "happy".to_string(),
                timestamp: test_time(),
            },
            GameEvent::MoodChanged {
                npc_id: NpcId(2),
                new_mood: "sad".to_string(),
                timestamp: test_time(),
            },
            GameEvent::WeatherChanged {
                new_weather: "Rain".to_string(),
                timestamp: test_time(),
            },
        ];
        let filtered = filter_events_for_npc(NpcId(1), &events);
        assert_eq!(filtered.len(), 1);
    }
}
