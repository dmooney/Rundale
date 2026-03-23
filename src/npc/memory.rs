//! NPC short-term memory system.
//!
//! A ring buffer of recent interactions and observations that provides
//! context for NPC dialogue and decision-making. Old entries are evicted
//! when the buffer reaches its capacity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::npc::NpcId;
use crate::world::LocationId;

/// Maximum number of entries in short-term memory.
pub const MEMORY_CAPACITY: usize = 20;

/// A single memory entry recording something an NPC experienced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// When this happened in game time.
    pub timestamp: DateTime<Utc>,
    /// What happened (e.g. "Spoke with the traveller about the landlord").
    pub content: String,
    /// NPCs involved in this event (including the remembering NPC).
    pub participants: Vec<NpcId>,
    /// Where this happened.
    pub location: LocationId,
}

/// Ring buffer of recent NPC memories.
///
/// Holds the last [`MEMORY_CAPACITY`] entries. When full, the oldest
/// entry is evicted to make room for new ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortTermMemory {
    /// The entries, ordered oldest to newest.
    entries: VecDeque<MemoryEntry>,
}

impl ShortTermMemory {
    /// Creates an empty short-term memory.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MEMORY_CAPACITY),
        }
    }

    /// Adds a new memory entry, evicting the oldest if at capacity.
    pub fn add(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= MEMORY_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Returns the `n` most recent entries, newest last.
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        let len = self.entries.len();
        let skip = len.saturating_sub(n);
        self.entries.iter().skip(skip).collect()
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no stored entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Formats recent memories into a context string for LLM prompts.
    ///
    /// Each entry is formatted as a timestamped line. Returns an empty
    /// string if there are no memories.
    pub fn context_string(&self, n: usize) -> String {
        let recent = self.recent(n);
        if recent.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(recent.len());
        for entry in &recent {
            let time = entry.timestamp.format("%H:%M");
            lines.push(format!("- [{}] {}", time, entry.content));
        }
        lines.join("\n")
    }
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_entry(hour: u32, content: &str) -> MemoryEntry {
        MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap(),
            content: content.to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
        }
    }

    #[test]
    fn test_memory_add_and_len() {
        let mut mem = ShortTermMemory::new();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);

        mem.add(make_entry(8, "Opened the pub"));
        assert_eq!(mem.len(), 1);
        assert!(!mem.is_empty());
    }

    #[test]
    fn test_memory_eviction_at_capacity() {
        let mut mem = ShortTermMemory::new();
        for i in 0..25 {
            mem.add(make_entry(8, &format!("Event {}", i)));
        }
        assert_eq!(mem.len(), MEMORY_CAPACITY);
        // Oldest entries (0-4) should be evicted
        let recent = mem.recent(MEMORY_CAPACITY);
        assert_eq!(recent[0].content, "Event 5");
        assert_eq!(recent[MEMORY_CAPACITY - 1].content, "Event 24");
    }

    #[test]
    fn test_memory_recent() {
        let mut mem = ShortTermMemory::new();
        for i in 0..10 {
            mem.add(make_entry(8, &format!("Event {}", i)));
        }

        let recent = mem.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "Event 7");
        assert_eq!(recent[1].content, "Event 8");
        assert_eq!(recent[2].content, "Event 9");
    }

    #[test]
    fn test_memory_recent_more_than_available() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry(8, "Only one"));

        let recent = mem.recent(5);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, "Only one");
    }

    #[test]
    fn test_memory_context_string() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry(8, "Opened the pub"));
        mem.add(make_entry(9, "Spoke with a traveller"));

        let ctx = mem.context_string(5);
        assert!(ctx.contains("[08:00] Opened the pub"));
        assert!(ctx.contains("[09:00] Spoke with a traveller"));
    }

    #[test]
    fn test_memory_context_string_empty() {
        let mem = ShortTermMemory::new();
        assert_eq!(mem.context_string(5), "");
    }

    #[test]
    fn test_memory_default() {
        let mem = ShortTermMemory::default();
        assert!(mem.is_empty());
    }

    #[test]
    fn test_memory_preserves_participants() {
        let mut mem = ShortTermMemory::new();
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Joint conversation".to_string(),
            participants: vec![NpcId(1), NpcId(2), NpcId(3)],
            location: LocationId(2),
        };
        mem.add(entry);

        let recent = mem.recent(1);
        assert_eq!(recent[0].participants.len(), 3);
        assert_eq!(recent[0].location, LocationId(2));
    }
}
