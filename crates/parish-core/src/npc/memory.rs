//! NPC memory system — short-term ring buffer and long-term keyword store.
//!
//! Short-term memory is a ring buffer of recent interactions and observations
//! that provides context for NPC dialogue. Old entries are evicted when the
//! buffer reaches its capacity, and may be promoted to long-term memory based
//! on importance scoring.
//!
//! Long-term memory stores important events with keyword-based retrieval,
//! allowing NPCs to recall relevant past experiences during conversations.

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
    /// Maximum number of entries before eviction.
    #[serde(default = "default_max_capacity")]
    max_capacity: usize,
}

/// Serde default for `max_capacity`.
fn default_max_capacity() -> usize {
    MEMORY_CAPACITY
}

impl ShortTermMemory {
    /// Creates an empty short-term memory with the default capacity.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MEMORY_CAPACITY),
            max_capacity: MEMORY_CAPACITY,
        }
    }

    /// Creates an empty short-term memory with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            max_capacity: capacity,
        }
    }

    /// Adds a new memory entry, evicting the oldest if at capacity.
    ///
    /// Returns the evicted entry (if any) so the caller can score it
    /// for potential promotion to long-term memory.
    pub fn add(&mut self, entry: MemoryEntry) -> Option<MemoryEntry> {
        let evicted = if self.entries.len() >= self.max_capacity {
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

/// Minimum importance score for a memory to be stored long-term.
const PROMOTION_THRESHOLD: f32 = 0.5;

/// Words that signal emotionally significant events.
const EMOTION_WORDS: &[&str] = &[
    "angry",
    "furious",
    "love",
    "hate",
    "death",
    "dead",
    "died",
    "secret",
    "afraid",
    "terrified",
    "sobbing",
    "weeping",
    "joy",
    "grief",
    "betrayed",
    "married",
    "pregnant",
    "murdered",
    "stolen",
    "cursed",
    "blessed",
];

/// A long-term memory entry with importance scoring and keyword tagging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongTermEntry {
    /// When this was originally experienced.
    pub timestamp: DateTime<Utc>,
    /// What happened.
    pub content: String,
    /// Importance score from 0.0 (trivial) to 1.0 (life-changing).
    pub importance: f32,
    /// Keywords for retrieval (NPC names, locations, event types).
    pub keywords: Vec<String>,
}

/// Long-term memory store with keyword-based retrieval.
///
/// Stores important memories that survive short-term eviction. Retrieval
/// scores entries by keyword overlap weighted by importance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LongTermMemory {
    entries: Vec<LongTermEntry>,
}

impl LongTermMemory {
    /// Creates an empty long-term memory.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Stores an entry if it meets the importance threshold.
    ///
    /// Returns `true` if the entry was stored, `false` if it was below
    /// the promotion threshold.
    pub fn store(&mut self, entry: LongTermEntry) -> bool {
        if entry.importance >= PROMOTION_THRESHOLD {
            self.entries.push(entry);
            true
        } else {
            false
        }
    }

    /// Retrieves the top `limit` entries matching the query by keyword overlap.
    ///
    /// Scoring: count of query keywords that appear in entry keywords,
    /// weighted by importance. Higher scores first. Only returns entries
    /// with at least one keyword match.
    pub fn recall(&self, query_keywords: &[&str], limit: usize) -> Vec<&LongTermEntry> {
        if query_keywords.is_empty() || self.entries.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(f32, &LongTermEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let keyword_matches = query_keywords
                    .iter()
                    .filter(|qk| {
                        let qk_lower = qk.to_lowercase();
                        entry
                            .keywords
                            .iter()
                            .any(|ek| ek.to_lowercase() == qk_lower)
                    })
                    .count();

                if keyword_matches > 0 {
                    let score = keyword_matches as f32 * entry.importance;
                    Some((score, entry))
                } else {
                    None
                }
            })
            .collect();

        // Sort descending by score
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// Returns the total number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Formats recalled memories into a context string for LLM prompts.
    ///
    /// Returns an empty string if no memories match.
    pub fn recall_context_string(&self, query_keywords: &[&str], limit: usize) -> String {
        let recalled = self.recall(query_keywords, limit);
        if recalled.is_empty() {
            return String::new();
        }

        let lines: Vec<String> = recalled.iter().map(|entry| entry.content.clone()).collect();
        format!("You recall: {}", lines.join(". "))
    }
}

/// Computes an importance score for a memory entry using heuristics.
///
/// Scoring rules (no LLM needed):
/// - Base: 0.2
/// - Player involved (NpcId(0) in participants): +0.3
/// - Multiple participants (>2): +0.1
/// - Contains relationship-change language: +0.2
/// - Contains strong emotion words: +0.2
///
/// The score is clamped to \[0.0, 1.0\].
pub fn compute_importance(entry: &MemoryEntry) -> f32 {
    let mut score: f32 = 0.2;

    // Player involved — NpcId(0) is conventionally "the player"
    if entry.participants.iter().any(|p| p.0 == 0) {
        score += 0.3;
    }

    // Multiple participants
    if entry.participants.len() > 2 {
        score += 0.1;
    }

    let lower = entry.content.to_lowercase();

    // Relationship-change language
    if lower.contains("relationship")
        || lower.contains("friend")
        || lower.contains("enemy")
        || lower.contains("trust")
        || lower.contains("quarrel")
        || lower.contains("forgave")
    {
        score += 0.2;
    }

    // Emotion words
    if EMOTION_WORDS.iter().any(|w| lower.contains(w)) {
        score += 0.2;
    }

    score.min(1.0)
}

/// Extracts keywords from a memory entry for long-term indexing.
///
/// Extracts:
/// - NPC names from participants (requires a name lookup function)
/// - Location name
/// - Words over 4 characters from content (simple heuristic)
pub fn extract_keywords(
    entry: &MemoryEntry,
    participant_names: &[String],
    location_name: &str,
) -> Vec<String> {
    let mut keywords: Vec<String> = Vec::new();

    // Add participant names
    for name in participant_names {
        if !name.is_empty() {
            keywords.push(name.to_lowercase());
        }
    }

    // Add location name
    if !location_name.is_empty() {
        keywords.push(location_name.to_lowercase());
    }

    // Extract content words >4 chars (simple noun/verb heuristic)
    let stop_words = [
        "about", "after", "again", "being", "between", "could", "doing", "during", "every",
        "found", "going", "heard", "their", "there", "these", "thing", "think", "those", "under",
        "until", "wants", "which", "while", "would", "spoke", "asked", "should",
    ];
    for word in entry.content.split_whitespace() {
        let cleaned: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if cleaned.len() > 4
            && !stop_words.contains(&cleaned.as_str())
            && !keywords.contains(&cleaned)
        {
            keywords.push(cleaned);
        }
    }

    keywords
}

/// Attempts to promote an evicted short-term memory entry to long-term storage.
///
/// Scores the entry's importance and, if above threshold, extracts keywords
/// and stores it. Returns `true` if the entry was promoted.
pub fn try_promote(
    ltm: &mut LongTermMemory,
    entry: &MemoryEntry,
    participant_names: &[String],
    location_name: &str,
) -> bool {
    let importance = compute_importance(entry);
    if importance < PROMOTION_THRESHOLD {
        return false;
    }

    let keywords = extract_keywords(entry, participant_names, location_name);
    ltm.store(LongTermEntry {
        timestamp: entry.timestamp,
        content: entry.content.clone(),
        importance,
        keywords,
    })
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

    #[test]
    fn test_with_capacity_custom() {
        let mut mem = ShortTermMemory::with_capacity(5);
        for i in 0..10 {
            mem.add(make_entry(8, &format!("Event {}", i)));
        }
        assert_eq!(mem.len(), 5);
        let recent = mem.recent(5);
        assert_eq!(recent[0].content, "Event 5");
        assert_eq!(recent[4].content, "Event 9");
    }

    #[test]
    fn test_with_capacity_larger() {
        let mut mem = ShortTermMemory::with_capacity(30);
        for i in 0..25 {
            mem.add(make_entry(8, &format!("Event {}", i)));
        }
        // All 25 should fit since capacity is 30
        assert_eq!(mem.len(), 25);
    }

    #[test]
    fn test_short_term_eviction_returns_entry() {
        let mut mem = ShortTermMemory::with_capacity(3);
        assert!(mem.add(make_entry(8, "First")).is_none());
        assert!(mem.add(make_entry(9, "Second")).is_none());
        assert!(mem.add(make_entry(10, "Third")).is_none());

        // Fourth entry should evict "First"
        let evicted = mem.add(make_entry(11, "Fourth"));
        assert!(evicted.is_some());
        assert_eq!(evicted.unwrap().content, "First");
    }

    #[test]
    fn test_short_term_no_eviction_under_capacity() {
        let mut mem = ShortTermMemory::new();
        for i in 0..MEMORY_CAPACITY {
            assert!(mem.add(make_entry(8, &format!("Event {}", i))).is_none());
        }
    }

    // ── Long-term memory tests ──────────────────────────────────────

    fn make_lt_entry(content: &str, importance: f32, keywords: &[&str]) -> LongTermEntry {
        LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: content.to_string(),
            importance,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_long_term_store_and_recall() {
        let mut ltm = LongTermMemory::new();
        assert!(ltm.is_empty());

        ltm.store(make_lt_entry(
            "Padraig argued with the landlord",
            0.8,
            &["padraig", "landlord", "crossroads"],
        ));
        ltm.store(make_lt_entry(
            "Sheep escaped from the field",
            0.6,
            &["sheep", "field"],
        ));
        ltm.store(make_lt_entry(
            "Padraig told a story about fairies",
            0.7,
            &["padraig", "fairies", "story"],
        ));

        assert_eq!(ltm.len(), 3);

        // Recall by "padraig" — should return 2 entries, higher importance first
        let results = ltm.recall(&["padraig"], 5);
        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("argued")); // 0.8 > 0.7
        assert!(results[1].content.contains("story"));
    }

    #[test]
    fn test_long_term_importance_threshold() {
        let mut ltm = LongTermMemory::new();

        // Below threshold (0.5)
        let stored = ltm.store(make_lt_entry("trivial event", 0.3, &["test"]));
        assert!(!stored);
        assert!(ltm.is_empty());

        // At threshold
        let stored = ltm.store(make_lt_entry("important event", 0.5, &["test"]));
        assert!(stored);
        assert_eq!(ltm.len(), 1);
    }

    #[test]
    fn test_long_term_keyword_scoring() {
        let mut ltm = LongTermMemory::new();

        // Entry with many matching keywords should rank higher
        ltm.store(make_lt_entry(
            "Padraig at crossroads",
            0.6,
            &["padraig", "crossroads"],
        ));
        ltm.store(make_lt_entry("Only sheep", 0.6, &["sheep"]));

        // Query with both "padraig" and "crossroads" — first entry matches 2 keywords
        let results = ltm.recall(&["padraig", "crossroads"], 5);
        assert_eq!(results.len(), 1); // only first entry matches
        assert!(results[0].content.contains("Padraig"));
    }

    #[test]
    fn test_long_term_recall_empty() {
        let ltm = LongTermMemory::new();
        let results = ltm.recall(&["anything"], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_long_term_recall_no_match() {
        let mut ltm = LongTermMemory::new();
        ltm.store(make_lt_entry("something happened", 0.7, &["padraig"]));
        let results = ltm.recall(&["nonexistent"], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_long_term_recall_context_string() {
        let mut ltm = LongTermMemory::new();
        ltm.store(make_lt_entry("Saw the landlord", 0.8, &["landlord"]));
        ltm.store(make_lt_entry("Talked to Mary", 0.7, &["mary"]));

        let ctx = ltm.recall_context_string(&["landlord"], 3);
        assert!(ctx.starts_with("You recall: "));
        assert!(ctx.contains("Saw the landlord"));

        let empty = ltm.recall_context_string(&["nobody"], 3);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_compute_importance_base() {
        let entry = make_entry(8, "Nothing special happened");
        let score = compute_importance(&entry);
        assert!((score - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_compute_importance_player_involved() {
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Spoke about the weather".to_string(),
            participants: vec![NpcId(0), NpcId(1)], // NpcId(0) = player
            location: LocationId(1),
        };
        let score = compute_importance(&entry);
        assert!((score - 0.5).abs() < 0.01); // 0.2 base + 0.3 player
    }

    #[test]
    fn test_compute_importance_emotion_words() {
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "The farmer was angry about the stolen sheep".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
        };
        let score = compute_importance(&entry);
        // 0.2 base + 0.2 emotion ("angry" + "stolen")
        assert!((score - 0.4).abs() < 0.01);
    }

    #[test]
    fn test_compute_importance_clamped() {
        // Player + multiple participants + relationship + emotion = would exceed 1.0
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "The friend was angry and betrayed".to_string(),
            participants: vec![NpcId(0), NpcId(1), NpcId(2), NpcId(3)],
            location: LocationId(1),
        };
        let score = compute_importance(&entry);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_extract_keywords() {
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Argued fiercely about the landlord's cattle".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
        };

        let keywords = extract_keywords(&entry, &["Padraig".to_string()], "The Crossroads");

        assert!(keywords.contains(&"padraig".to_string()));
        assert!(keywords.contains(&"the crossroads".to_string()));
        assert!(keywords.contains(&"argued".to_string()));
        assert!(keywords.contains(&"fiercely".to_string()));
        assert!(
            keywords.contains(&"landlord's".to_string())
                || keywords.contains(&"landlords".to_string())
        );
        assert!(keywords.contains(&"cattle".to_string()));
    }

    #[test]
    fn test_try_promote_above_threshold() {
        let mut ltm = LongTermMemory::new();
        let entry = MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Spoke with the traveller about the secret".to_string(),
            participants: vec![NpcId(0), NpcId(1)], // player involved
            location: LocationId(1),
        };
        // Player (0.3) + base (0.2) + emotion "secret" (0.2) = 0.7
        let promoted = try_promote(&mut ltm, &entry, &["Padraig".to_string()], "The Crossroads");
        assert!(promoted);
        assert_eq!(ltm.len(), 1);
    }

    #[test]
    fn test_try_promote_below_threshold() {
        let mut ltm = LongTermMemory::new();
        let entry = make_entry(8, "Nothing happened");
        // Base only = 0.2, below 0.5 threshold
        let promoted = try_promote(&mut ltm, &entry, &[], "");
        assert!(!promoted);
        assert!(ltm.is_empty());
    }
}
