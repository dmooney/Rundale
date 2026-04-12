//! Conversation history tracking for NPC scene awareness.
//!
//! Stores recent player–NPC exchanges per location so that NPCs
//! can reference what was just said, maintaining conversational
//! continuity and scene awareness.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::ids::{LocationId, NpcId};

/// Maximum number of exchanges retained globally.
const LOG_CAPACITY: usize = 30;

/// A single player–NPC exchange.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationExchange {
    /// When this exchange happened in game time.
    pub timestamp: DateTime<Utc>,
    /// The NPC who responded.
    pub speaker_id: NpcId,
    /// The NPC's display name.
    pub speaker_name: String,
    /// What the player said or did.
    pub player_input: String,
    /// What the NPC said back.
    pub npc_dialogue: String,
    /// Where this exchange took place.
    pub location: LocationId,
}

/// Ring buffer of recent conversation exchanges across all locations.
///
/// Used to inject conversation history into NPC prompts, giving them
/// awareness of what's been said at their location.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversationLog {
    exchanges: VecDeque<ConversationExchange>,
}

impl ConversationLog {
    /// Creates an empty conversation log.
    pub fn new() -> Self {
        Self {
            exchanges: VecDeque::with_capacity(LOG_CAPACITY),
        }
    }

    /// Records a new exchange, evicting the oldest if at capacity.
    pub fn add(&mut self, exchange: ConversationExchange) {
        if self.exchanges.len() >= LOG_CAPACITY {
            self.exchanges.pop_front();
        }
        self.exchanges.push_back(exchange);
    }

    /// Returns the last `n` exchanges at a specific location, oldest first.
    pub fn recent_at(&self, location: LocationId, n: usize) -> Vec<&ConversationExchange> {
        self.exchanges
            .iter()
            .filter(|e| e.location == location)
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Returns the NPC id of the most recent speaker at a location, if any.
    pub fn last_speaker_at(&self, location: LocationId) -> Option<NpcId> {
        self.exchanges
            .iter()
            .rev()
            .find(|e| e.location == location)
            .map(|e| e.speaker_id)
    }

    /// Checks whether the given NPC was the speaker in any of the last `n`
    /// exchanges at this location.
    pub fn has_recent_exchange_with(
        &self,
        location: LocationId,
        speaker_id: NpcId,
        n: usize,
    ) -> bool {
        self.recent_at(location, n)
            .iter()
            .any(|e| e.speaker_id == speaker_id)
    }

    /// Formats recent conversation history at a location for prompt injection.
    ///
    /// `current_npc_id` is the NPC being prompted — their own lines are
    /// phrased as "You said..." while others' lines use "{Name} said...".
    pub fn context_string(&self, location: LocationId, current_npc_id: NpcId, n: usize) -> String {
        let recent = self.recent_at(location, n);
        if recent.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(recent.len());
        for exchange in &recent {
            let time = exchange.timestamp.format("%H:%M");
            let speaker = if exchange.speaker_id == current_npc_id {
                "You".to_string()
            } else {
                exchange.speaker_name.clone()
            };

            lines.push(format!(
                "- [{}] The traveller said: \"{}\". {} replied: \"{}\"",
                time, exchange.player_input, speaker, exchange.npc_dialogue,
            ));
        }
        lines.join("\n")
    }

    /// Returns the number of stored exchanges.
    pub fn len(&self) -> usize {
        self.exchanges.len()
    }

    /// Returns true if there are no stored exchanges.
    pub fn is_empty(&self) -> bool {
        self.exchanges.is_empty()
    }

    /// Returns all stored exchanges in chronological order (oldest first).
    ///
    /// Used by the debug panel to surface the full ring buffer.
    pub fn all(&self) -> impl Iterator<Item = &ConversationExchange> {
        self.exchanges.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_exchange(
        hour: u32,
        speaker_id: u32,
        speaker_name: &str,
        player_input: &str,
        npc_dialogue: &str,
        location: u32,
    ) -> ConversationExchange {
        ConversationExchange {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap(),
            speaker_id: NpcId(speaker_id),
            speaker_name: speaker_name.to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: npc_dialogue.to_string(),
            location: LocationId(location),
        }
    }

    #[test]
    fn test_add_and_len() {
        let mut log = ConversationLog::new();
        assert!(log.is_empty());

        log.add(make_exchange(8, 1, "Padraig", "Hello", "Dia dhuit!", 1));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn test_capacity_eviction() {
        let mut log = ConversationLog::new();
        for i in 0..35 {
            log.add(make_exchange(
                8,
                1,
                "Padraig",
                &format!("msg {}", i),
                "reply",
                1,
            ));
        }
        assert_eq!(log.len(), LOG_CAPACITY);
    }

    #[test]
    fn test_recent_at_filters_by_location() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(8, 1, "Padraig", "Hello", "Hi", 1));
        log.add(make_exchange(9, 2, "Niamh", "Howdy", "Hello", 2));
        log.add(make_exchange(10, 1, "Padraig", "Weather", "Grand", 1));

        let at_loc1 = log.recent_at(LocationId(1), 5);
        assert_eq!(at_loc1.len(), 2);
        assert_eq!(at_loc1[0].player_input, "Hello");
        assert_eq!(at_loc1[1].player_input, "Weather");

        let at_loc2 = log.recent_at(LocationId(2), 5);
        assert_eq!(at_loc2.len(), 1);
    }

    #[test]
    fn test_has_recent_exchange_with() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(8, 1, "Padraig", "Hello", "Hi", 1));
        log.add(make_exchange(9, 2, "Niamh", "Hello", "Hi", 1));

        assert!(log.has_recent_exchange_with(LocationId(1), NpcId(1), 5));
        assert!(log.has_recent_exchange_with(LocationId(1), NpcId(2), 5));
        assert!(!log.has_recent_exchange_with(LocationId(1), NpcId(3), 5));
        assert!(!log.has_recent_exchange_with(LocationId(2), NpcId(1), 5));
    }

    #[test]
    fn test_context_string_perspective() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(
            8,
            1,
            "Padraig",
            "Hello there",
            "Dia dhuit!",
            1,
        ));
        log.add(make_exchange(9, 2, "Niamh", "Good day", "Good morning", 1));

        // From Padraig's perspective
        let ctx = log.context_string(LocationId(1), NpcId(1), 5);
        assert!(ctx.contains("You replied"));
        assert!(ctx.contains("Niamh replied"));

        // From Niamh's perspective
        let ctx = log.context_string(LocationId(1), NpcId(2), 5);
        assert!(ctx.contains("Padraig replied"));
        assert!(ctx.contains("You replied"));
    }

    #[test]
    fn test_context_string_empty() {
        let log = ConversationLog::new();
        assert_eq!(log.context_string(LocationId(1), NpcId(1), 5), "");
    }

    #[test]
    fn test_recent_at_respects_limit() {
        let mut log = ConversationLog::new();
        for i in 0..10 {
            log.add(make_exchange(
                8,
                1,
                "Padraig",
                &format!("msg {}", i),
                "reply",
                1,
            ));
        }
        let recent = log.recent_at(LocationId(1), 3);
        assert_eq!(recent.len(), 3);
        // Should be the last 3
        assert_eq!(recent[0].player_input, "msg 7");
        assert_eq!(recent[2].player_input, "msg 9");
    }
}
