//! NPC short-term memory system.
//!
//! A ring buffer of recent experiences that provides context
//! for NPC dialogue and decision-making. Oldest entries are
//! evicted when the buffer is full.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::NpcId;
use crate::world::LocationId;

/// Default maximum number of memory entries.
const DEFAULT_CAPACITY: usize = 20;

/// A single memory entry representing something an NPC experienced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// When the event occurred (game time).
    pub timestamp: DateTime<Utc>,
    /// What happened.
    pub content: String,
    /// Other NPCs involved.
    #[serde(default)]
    pub participants: Vec<NpcId>,
    /// Where it happened.
    pub location: LocationId,
}

/// Ring buffer of recent memories for an NPC.
///
/// When the buffer reaches capacity, adding a new entry
/// evicts the oldest one. This provides a sliding window
/// of recent context for LLM prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemory {
    /// The memory entries (front = oldest, back = newest).
    entries: VecDeque<MemoryEntry>,
    /// Maximum number of entries before eviction.
    capacity: usize,
}

impl ShortTermMemory {
    /// Creates a new empty short-term memory with the default capacity (20).
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
            capacity: DEFAULT_CAPACITY,
        }
    }

    /// Creates a new empty short-term memory with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Adds a memory entry, evicting the oldest if at capacity.
    pub fn add(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Returns the most recent `n` entries (newest first).
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the memory is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Formats all memories as a string suitable for inclusion in an LLM prompt.
    ///
    /// Returns an empty string if there are no memories.
    pub fn context_string(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        lines.push("Recent memories:".to_string());
        for entry in &self.entries {
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
    use chrono::{TimeZone, Utc};

    fn make_entry(content: &str, hour: u32) -> MemoryEntry {
        MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 3, 20, hour, 0, 0).unwrap(),
            content: content.to_string(),
            participants: Vec::new(),
            location: LocationId(1),
        }
    }

    #[test]
    fn test_memory_add_and_len() {
        let mut mem = ShortTermMemory::new();
        assert!(mem.is_empty());
        mem.add(make_entry("Arrived at pub", 8));
        assert_eq!(mem.len(), 1);
        assert!(!mem.is_empty());
    }

    #[test]
    fn test_memory_eviction() {
        let mut mem = ShortTermMemory::with_capacity(3);
        mem.add(make_entry("A", 8));
        mem.add(make_entry("B", 9));
        mem.add(make_entry("C", 10));
        assert_eq!(mem.len(), 3);

        mem.add(make_entry("D", 11));
        assert_eq!(mem.len(), 3);

        // "A" should have been evicted
        let recent = mem.recent(3);
        assert_eq!(recent[0].content, "D");
        assert_eq!(recent[1].content, "C");
        assert_eq!(recent[2].content, "B");
    }

    #[test]
    fn test_memory_eviction_at_20() {
        let mut mem = ShortTermMemory::new();
        for i in 0..25 {
            mem.add(make_entry(&format!("Event {}", i), 8));
        }
        assert_eq!(mem.len(), 20);

        // First 5 should have been evicted
        let recent = mem.recent(1);
        assert_eq!(recent[0].content, "Event 24");
    }

    #[test]
    fn test_memory_recent() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry("First", 8));
        mem.add(make_entry("Second", 9));
        mem.add(make_entry("Third", 10));

        let recent = mem.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "Third");
        assert_eq!(recent[1].content, "Second");
    }

    #[test]
    fn test_memory_recent_more_than_available() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry("Only one", 8));
        let recent = mem.recent(5);
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn test_memory_context_string_empty() {
        let mem = ShortTermMemory::new();
        assert_eq!(mem.context_string(), "");
    }

    #[test]
    fn test_memory_context_string() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry("Talked to Padraig", 8));
        mem.add(make_entry("Saw Siobhan at the shop", 9));
        let ctx = mem.context_string();
        assert!(ctx.contains("Recent memories:"));
        assert!(ctx.contains("Talked to Padraig"));
        assert!(ctx.contains("Saw Siobhan"));
        assert!(ctx.contains("[08:00]"));
        assert!(ctx.contains("[09:00]"));
    }

    #[test]
    fn test_memory_with_participants() {
        let mut mem = ShortTermMemory::new();
        let entry = MemoryEntry {
            timestamp: Utc::now(),
            content: "Had a chat".to_string(),
            participants: vec![NpcId(1), NpcId(2)],
            location: LocationId(1),
        };
        mem.add(entry);
        let recent = mem.recent(1);
        assert_eq!(recent[0].participants.len(), 2);
    }

    #[test]
    fn test_memory_serialize_deserialize() {
        let mut mem = ShortTermMemory::new();
        mem.add(make_entry("Test event", 8));
        let json = serde_json::to_string(&mem).unwrap();
        let deser: ShortTermMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.len(), 1);
    }

    #[test]
    fn test_memory_default() {
        let mem = ShortTermMemory::default();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
    }
}
