//! NPC short-term and long-term memory systems.
//!
//! Short-term memory is a ring buffer of recent interactions and observations
//! that provides context for NPC dialogue and decision-making. Old entries are
//! evicted when the buffer reaches its capacity.
//!
//! Long-term memory stores important memories with keyword-based recall,
//! allowing NPCs to remember significant events across many interactions.
//! Entries are scored by importance and can be recalled by keyword overlap.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

use crate::npc::NpcId;
use crate::world::LocationId;

/// Common stop words excluded from keyword extraction.
const STOP_WORDS: &[&str] = &[
    "the", "a", "is", "was", "to", "and", "of", "in", "at", "for", "on", "with", "it", "he", "she",
    "they", "this", "that",
];

/// Maximum number of entries in short-term memory.
pub const MEMORY_CAPACITY: usize = 20;

/// A single memory entry recording something an NPC experienced.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    ///
    /// Returns the evicted entry if one was removed, or `None` otherwise.
    pub fn add(&mut self, entry: MemoryEntry) -> Option<MemoryEntry> {
        let evicted = if self.entries.len() >= MEMORY_CAPACITY {
            self.entries.pop_front()
        } else {
            None
        };
        self.entries.push_back(entry);
        evicted
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

/// A single long-term memory entry with keyword-based recall.
///
/// Long-term entries are scored by importance and can be recalled
/// by matching query words against stored keywords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermEntry {
    /// When this memory was formed.
    pub timestamp: DateTime<Utc>,
    /// What happened.
    pub content: String,
    /// Importance score from 0.0 (trivial) to 1.0 (critical).
    pub importance: f32,
    /// Significant words extracted from the content for recall matching.
    pub keywords: Vec<String>,
}

/// Long-term memory store with keyword-based recall.
///
/// Stores up to [`MAX_CAPACITY`](LongTermMemory::MAX_CAPACITY) entries.
/// When full, the least important entry is evicted to make room.
/// Entries can be recalled by scoring keyword overlap with a query string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemory {
    /// Stored long-term memory entries.
    entries: Vec<LongTermEntry>,
}

impl LongTermMemory {
    /// Maximum number of long-term memory entries.
    pub const MAX_CAPACITY: usize = 100;

    /// Creates an empty long-term memory.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Stores a new entry, evicting the lowest-importance entry if at capacity.
    pub fn store(&mut self, entry: LongTermEntry) {
        if self.entries.len() >= Self::MAX_CAPACITY {
            // Find and remove the entry with the lowest importance
            if let Some(min_idx) = self
                .entries
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.importance
                        .partial_cmp(&b.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
            {
                self.entries.remove(min_idx);
            }
        }
        self.entries.push(entry);
    }

    /// Recalls entries matching a query, scored by keyword overlap.
    ///
    /// Each entry is scored by counting how many query words appear in its
    /// keywords. Returns the top `limit` entries sorted by score descending,
    /// with ties broken by more recent timestamp.
    pub fn recall(&self, query: &str, limit: usize) -> Vec<&LongTermEntry> {
        let query_words: HashSet<String> =
            query.split_whitespace().map(|w| w.to_lowercase()).collect();

        let mut scored: Vec<(usize, &LongTermEntry)> = self
            .entries
            .iter()
            .map(|entry| {
                let score = entry
                    .keywords
                    .iter()
                    .filter(|kw| query_words.contains(kw.as_str()))
                    .count();
                (score, entry)
            })
            .filter(|(score, _)| *score > 0)
            .collect();

        scored.sort_by(|(score_a, a), (score_b, b)| {
            score_b
                .cmp(score_a)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });

        scored.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// Returns all entries for debug inspection.
    pub fn all_entries(&self) -> &[LongTermEntry] {
        &self.entries
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no stored entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Formats recalled memories into a context string for LLM prompts.
    ///
    /// Recalls entries matching `query`, formats up to `limit` as timestamped
    /// lines. Returns an empty string if no memories match.
    pub fn context_string(&self, query: &str, limit: usize) -> String {
        let recalled = self.recall(query, limit);
        if recalled.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(recalled.len());
        for entry in &recalled {
            let time = entry.timestamp.format("%Y-%m-%d %H:%M");
            lines.push(format!("- [{}] {}", time, entry.content));
        }
        lines.join("\n")
    }
}

impl Default for LongTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Extracts significant keywords from content text.
///
/// Lowercases all words, removes common stop words, and deduplicates.
/// Returns a sorted, deduplicated list of significant words.
pub fn extract_keywords(content: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut keywords = Vec::new();

    for word in content.split_whitespace() {
        let lower = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if !lower.is_empty() && !stop.contains(lower.as_str()) && seen.insert(lower.clone()) {
            keywords.push(lower);
        }
    }
    keywords
}

/// Heuristic importance scoring for memory content.
///
/// Base score of 0.3, plus 0.1 per 20 characters, capped at 1.0.
/// Adds a bonus for capitalized words that aren't at the start of a sentence
/// (likely proper names).
pub fn score_importance(content: &str) -> f32 {
    let base = 0.3 + (content.len() as f32 / 20.0) * 0.1;

    // Count capitalized words not at start of sentences as likely names
    let name_bonus = content
        .split_whitespace()
        .enumerate()
        .filter(|(i, word)| {
            if *i == 0 {
                return false;
            }
            // Check if preceded by sentence-ending punctuation
            let chars: Vec<char> = word.chars().collect();
            if chars.is_empty() {
                return false;
            }
            chars[0].is_uppercase()
        })
        .count() as f32
        * 0.05;

    (base + name_bonus).min(1.0)
}

/// Promotes an evicted short-term memory entry to a long-term entry.
///
/// Extracts keywords and scores importance. If importance exceeds 0.3
/// (a low threshold to retain most memories), returns `Some(LongTermEntry)`.
/// Otherwise returns `None`.
pub fn promote_from_short_term(evicted: &MemoryEntry) -> Option<LongTermEntry> {
    let keywords = extract_keywords(&evicted.content);
    let importance = score_importance(&evicted.content);

    if importance > 0.3 {
        Some(LongTermEntry {
            timestamp: evicted.timestamp,
            content: evicted.content.clone(),
            importance,
            keywords,
        })
    } else {
        None
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

    fn make_long_term_entry(hour: u32, content: &str, importance: f32) -> LongTermEntry {
        LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap(),
            content: content.to_string(),
            importance,
            keywords: extract_keywords(content),
        }
    }

    #[test]
    fn test_long_term_store_and_recall() {
        let mut mem = LongTermMemory::new();
        mem.store(make_long_term_entry(
            8,
            "Spoke with Padraig about the landlord",
            0.7,
        ));
        mem.store(make_long_term_entry(
            9,
            "Saw a stranger at the crossroads",
            0.5,
        ));
        mem.store(make_long_term_entry(
            10,
            "Heard gossip about the landlord from Siobhan",
            0.8,
        ));

        let results = mem.recall("landlord", 5);
        assert_eq!(results.len(), 2);
        // Higher score (more keyword overlap) or more recent should come first
        assert!(results[0].content.contains("landlord"));
        assert!(results[1].content.contains("landlord"));
    }

    #[test]
    fn test_long_term_capacity_eviction() {
        let mut mem = LongTermMemory::new();
        // Fill to capacity with importance = 0.5
        for i in 0..LongTermMemory::MAX_CAPACITY {
            mem.store(make_long_term_entry(8, &format!("Event number {}", i), 0.5));
        }
        assert_eq!(mem.len(), LongTermMemory::MAX_CAPACITY);

        // Add one more with higher importance — should evict one with lowest importance
        mem.store(make_long_term_entry(9, "Very important event", 0.9));
        assert_eq!(mem.len(), LongTermMemory::MAX_CAPACITY);

        // The new entry should be present
        let results = mem.recall("important event", 1);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Very important"));
    }

    #[test]
    fn test_keyword_extraction() {
        let keywords = extract_keywords("The landlord was at the crossroads with a stranger");
        // "the", "was", "at", "with", "a" are stop words
        assert!(keywords.contains(&"landlord".to_string()));
        assert!(keywords.contains(&"crossroads".to_string()));
        assert!(keywords.contains(&"stranger".to_string()));
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"was".to_string()));
        assert!(!keywords.contains(&"a".to_string()));

        // Test deduplication
        let keywords2 = extract_keywords("cat cat cat dog dog");
        assert_eq!(keywords2.len(), 2);
    }

    #[test]
    fn test_importance_scoring() {
        // Short content should have a lower score
        let short_score = score_importance("Hi");
        assert!(short_score >= 0.3, "short score should be at least 0.3");
        assert!(short_score < 0.5, "short score should be low");

        // Longer content should score higher
        let long_score = score_importance(
            "The landlord came to the village and spoke at great length about the rent increases affecting every family in the parish",
        );
        assert!(
            long_score > short_score,
            "longer content should score higher"
        );

        // Content with names (capitalized words) should get a bonus
        let name_score =
            score_importance("Spoke with Padraig about Siobhan and the landlord Murphy");
        let no_name_score =
            score_importance("spoke with someone about something and the landlord today");
        assert!(
            name_score > no_name_score,
            "content with names should score higher"
        );

        // Score should cap at 1.0
        let very_long = "word ".repeat(500);
        let capped = score_importance(&very_long);
        assert!(
            (capped - 1.0).abs() < f32::EPSILON,
            "score should cap at 1.0"
        );
    }

    #[test]
    fn test_promote_from_short_term() {
        let entry = make_entry(10, "Spoke with Padraig about the landlord situation");
        let promoted = promote_from_short_term(&entry);
        assert!(promoted.is_some(), "should promote non-trivial memory");

        let lt = promoted.unwrap();
        assert_eq!(lt.timestamp, entry.timestamp);
        assert_eq!(lt.content, entry.content);
        assert!(!lt.keywords.is_empty());
        assert!(lt.importance > 0.3);
        assert!(lt.keywords.contains(&"padraig".to_string()));
        assert!(lt.keywords.contains(&"landlord".to_string()));
    }

    #[test]
    fn test_short_term_add_returns_evicted() {
        let mut mem = ShortTermMemory::new();
        // Fill to capacity
        for i in 0..MEMORY_CAPACITY {
            let result = mem.add(make_entry(8, &format!("Event {}", i)));
            assert!(result.is_none(), "should not evict before capacity");
        }
        assert_eq!(mem.len(), MEMORY_CAPACITY);

        // Next add should evict
        let result = mem.add(make_entry(9, "Overflow event"));
        assert!(result.is_some(), "should evict when at capacity");
        assert_eq!(result.unwrap().content, "Event 0");
    }

    #[test]
    fn test_long_term_context_string() {
        let mut mem = LongTermMemory::new();
        mem.store(make_long_term_entry(
            8,
            "Spoke about the landlord situation",
            0.7,
        ));
        mem.store(make_long_term_entry(9, "Discussed weather and crops", 0.4));

        let ctx = mem.context_string("landlord", 5);
        assert!(ctx.contains("landlord situation"));
        assert!(
            !ctx.contains("weather"),
            "unrelated memory should not appear"
        );

        // Should contain timestamp formatting
        assert!(ctx.contains("[1820-03-20 08:00]"));
    }

    #[test]
    fn test_long_term_empty_recall() {
        let mem = LongTermMemory::new();
        let results = mem.recall("anything", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_long_term_default() {
        let mem = LongTermMemory::default();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
    }
}
