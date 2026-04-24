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

use parish_types::LocationId;
use parish_types::NpcId;

/// Maximum number of entries in short-term memory.
pub const MEMORY_CAPACITY: usize = 20;

/// Categorizes what kind of event a memory records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemoryKind {
    /// Spoke directly with the player.
    SpokeWithPlayer,
    /// Spoke with another NPC.
    SpokeWithNpc(NpcId),
    /// Overheard a conversation nearby.
    OverheardConversation,
    /// Received gossip from another NPC.
    ReceivedGossip,
}

/// A single memory entry recording something an NPC experienced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// When this happened in game time.
    pub timestamp: DateTime<Utc>,
    /// What happened (e.g. "Spoke with the newcomer about the landlord").
    pub content: String,
    /// NPCs involved in this event (including the remembering NPC).
    pub participants: Vec<NpcId>,
    /// Where this happened.
    pub location: LocationId,
    /// What kind of event this was (None for legacy entries).
    #[serde(default)]
    pub kind: Option<MemoryKind>,
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

    /// Returns all entries as a slice-like iterator.
    pub fn entries(&self) -> impl Iterator<Item = &MemoryEntry> {
        self.entries.iter()
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

    /// Formats recent memories with friendly relative timestamps.
    ///
    /// Uses human-readable relative dates ("a few minutes ago", "yesterday",
    /// etc.) instead of bare clock times. Returns an empty string if there
    /// are no memories.
    pub fn context_string_with_now(&self, n: usize, now: DateTime<Utc>) -> String {
        let recent = self.recent(n);
        if recent.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(recent.len());
        for entry in &recent {
            let label = relative_time_label(entry.timestamp, now);
            lines.push(format!("- [{}] {}", label, entry.content));
        }
        lines.join("\n")
    }
}

/// Formats a game timestamp relative to `now` as a human-readable label.
fn relative_time_label(ts: DateTime<Utc>, now: DateTime<Utc>) -> String {
    use chrono::Timelike;
    if ts > now {
        tracing::warn!(
            timestamp = %ts,
            now = %now,
            "future timestamp in NPC memory — clock skew or restored save?"
        );
        return "just now".to_string();
    }
    let diff = now.signed_duration_since(ts);
    let mins = diff.num_minutes();
    match mins {
        0 => "just now".to_string(),
        1..=59 => format!("{} min ago", mins),
        60..=1439 => format!("{} hr ago", diff.num_hours()),
        1440..=2879 => format!("yesterday, {:02}:{:02}", ts.hour(), ts.minute()),
        2880..=20159 => format!("{} days ago", diff.num_days()),
        20160..=86399 => format!("{} weeks ago", diff.num_weeks()),
        _ => format!("{} months ago", diff.num_days() / 30),
    }
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimum importance score for a memory to be stored long-term.
const PROMOTION_THRESHOLD: f32 = 0.5;

/// Default maximum number of entries held in a single NPC's long-term memory.
///
/// Matches the short-term cap and the designer's stated intent (issue #341).
/// When this cap is reached, the lowest-scoring (importance, oldest-first)
/// entry is evicted to make room.
pub const LONG_TERM_CAPACITY: usize = 50;

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
    ///
    /// Invariant: keywords are stored pre-lowercased (see [`extract_keywords`]).
    /// [`LongTermMemory::recall`] relies on this to avoid per-lookup
    /// allocations.
    pub keywords: Vec<String>,
}

/// Long-term memory store with keyword-based retrieval.
///
/// Stores important memories that survive short-term eviction. Retrieval
/// scores entries by keyword overlap weighted by importance.
///
/// Bounded to [`LONG_TERM_CAPACITY`] entries per NPC (issue #341). When the
/// cap is reached, [`Self::store`] evicts the lowest-importance entry
/// (oldest first on ties) to keep the most salient memories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongTermMemory {
    entries: Vec<LongTermEntry>,
    #[serde(default = "default_long_term_capacity")]
    max_entries: usize,
}

/// Serde default for [`LongTermMemory::max_entries`]; matches the in-memory default
/// and keeps pre-existing save files compatible.
fn default_long_term_capacity() -> usize {
    LONG_TERM_CAPACITY
}

impl Default for LongTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl LongTermMemory {
    /// Creates an empty long-term memory with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(LONG_TERM_CAPACITY)
    }

    /// Creates an empty long-term memory with the given capacity.
    ///
    /// A value of 0 is clamped to 1 so that `store` can always keep the
    /// highest-importance entry seen so far.
    pub fn with_capacity(max_entries: usize) -> Self {
        let cap = max_entries.max(1);
        Self {
            entries: Vec::with_capacity(cap),
            max_entries: cap,
        }
    }

    /// Returns the configured maximum number of stored entries.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Returns all entries.
    pub fn entries(&self) -> &[LongTermEntry] {
        &self.entries
    }

    /// Stores an entry if it meets the importance threshold.
    ///
    /// Returns `true` if the entry was stored, `false` if it was below
    /// the promotion threshold or if every existing entry is more
    /// important than the incoming one at capacity.
    ///
    /// When at capacity, the lowest-importance existing entry is evicted
    /// to make room. Ties are broken by preferring to evict the oldest
    /// entry. If the incoming entry's importance is strictly lower than
    /// every stored entry's importance, it is rejected and the log is
    /// unchanged.
    pub fn store(&mut self, entry: LongTermEntry) -> bool {
        if !entry.importance.is_finite() || entry.importance < PROMOTION_THRESHOLD {
            return false;
        }

        // #419 — purge any already-stored entries whose importance is not a
        // finite number (could arrive from a corrupted save file, since
        // deserialization does not validate the field). They poison the
        // eviction scan: `partial_cmp` returns `None` for NaN and our
        // `unwrap_or(Equal)` means a NaN entry is never seen as smaller
        // than a real number, so a valid 0.3 entry would be evicted ahead
        // of it. Doing the purge here — just before we consult capacity —
        // lets a fresh store reclaim the slot they occupied.
        self.entries.retain(|e| e.importance.is_finite());

        if self.entries.len() >= self.max_entries {
            // Find the least-important, oldest entry. `position` returns the
            // first index, so ties naturally favour eviction of the oldest.
            let (evict_idx, evict_importance) = self
                .entries
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1.importance
                        .partial_cmp(&b.1.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.timestamp.cmp(&b.1.timestamp))
                })
                .map(|(i, e)| (i, e.importance))
                .expect("entries is non-empty when at capacity");

            // Refuse to evict a more-important entry for a less-important one.
            if entry.importance < evict_importance {
                return false;
            }
            self.entries.remove(evict_idx);
        }

        self.entries.push(entry);
        true
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

        // Perf: lowercase each query keyword exactly once, up-front. The previous
        // implementation re-lowercased every query keyword AND every entry
        // keyword inside the per-entry loop, producing
        // O(entries × query × (1 + entry_keywords)) transient String
        // allocations. Entry keywords are guaranteed pre-lowercased (see
        // `extract_keywords` and the `LongTermEntry::keywords` invariant), so
        // comparison reduces to a cheap `Vec::contains` against the already-
        // lowered query. This is on the NPC dialogue hot path
        // (`ticks::build_enhanced_context_with_config` → `recall_context_string`,
        // invoked per conversation turn per NPC via `ipc::handlers`).
        let query_lower: Vec<String> = query_keywords.iter().map(|qk| qk.to_lowercase()).collect();

        let mut scored: Vec<(f32, &LongTermEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let keyword_matches = query_lower
                    .iter()
                    .filter(|qk| entry.keywords.contains(*qk))
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

    /// Returns all entries for debug display, sorted newest first.
    pub fn all_entries(&self) -> Vec<&LongTermEntry> {
        let mut entries: Vec<&LongTermEntry> = self.entries.iter().collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
        entries
    }

    /// Formats recalled memories into a context string for LLM prompts.
    ///
    /// Returns an empty string if no memories match.
    pub fn recall_context_string(&self, query_keywords: &[&str], limit: usize) -> String {
        let recalled = self.recall(query_keywords, limit);
        if recalled.is_empty() {
            return String::new();
        }

        // Single allocation, zero clones: borrow content strings directly instead of
        // cloning into a Vec<String> + join + format.
        let prefix = "You recall: ";
        let sep = ". ";
        let cap = prefix.len()
            + recalled.iter().map(|e| e.content.len()).sum::<usize>()
            + recalled.len().saturating_sub(1) * sep.len();
        let mut result = String::with_capacity(cap);
        result.push_str(prefix);
        for (i, entry) in recalled.iter().enumerate() {
            if i > 0 {
                result.push_str(sep);
            }
            result.push_str(&entry.content);
        }
        result
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

    // First encounters / direct player interaction — always promote
    if let Some(MemoryKind::SpokeWithPlayer) = &entry.kind {
        score += 0.1;
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
        // Perf: collect filtered chars lowercased in one pass. The previous
        // `collect::<String>().to_lowercase()` chain heap-allocated twice per
        // word (once for the filtered String, once for its lowercase copy).
        // `flat_map(char::to_lowercase)` lowercases each kept char inline and
        // collects directly, saving one allocation per word. Called per
        // memory promotion (`try_promote`) — once per evicted short-term
        // entry, per NPC, per conversation turn.
        let cleaned: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();
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
            kind: None,
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
        mem.add(make_entry(9, "Spoke with a newcomer"));

        let ctx = mem.context_string(5);
        assert!(ctx.contains("[08:00] Opened the pub"));
        assert!(ctx.contains("[09:00] Spoke with a newcomer"));
    }

    #[test]
    fn test_memory_context_string_empty() {
        let mem = ShortTermMemory::new();
        assert_eq!(mem.context_string(5), "");
    }

    #[test]
    fn test_context_string_with_now_empty() {
        let mem = ShortTermMemory::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert_eq!(mem.context_string_with_now(5, now), "");
    }

    #[test]
    fn test_context_string_with_now_just_now() {
        let mut mem = ShortTermMemory::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        // Entry timestamped at the same time as "now"
        mem.add(MemoryEntry {
            timestamp: now,
            content: "Said hello".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(ctx.contains("[just now] Said hello"), "got: {ctx}");
    }

    #[test]
    fn test_context_string_with_now_minutes_ago() {
        let mut mem = ShortTermMemory::new();
        let ts = Utc.with_ymd_and_hms(1820, 3, 20, 11, 45, 0).unwrap();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(); // 15 min later
        mem.add(MemoryEntry {
            timestamp: ts,
            content: "Bought bread".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(ctx.contains("[15 min ago] Bought bread"), "got: {ctx}");
    }

    #[test]
    fn test_context_string_with_now_hours_ago() {
        let mut mem = ShortTermMemory::new();
        let ts = Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(); // 4 hr later
        mem.add(MemoryEntry {
            timestamp: ts,
            content: "Opened the shop".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(ctx.contains("[4 hr ago] Opened the shop"), "got: {ctx}");
    }

    #[test]
    fn test_context_string_with_now_yesterday() {
        let mut mem = ShortTermMemory::new();
        let ts = Utc.with_ymd_and_hms(1820, 3, 19, 8, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(); // 28 hr later
        mem.add(MemoryEntry {
            timestamp: ts,
            content: "Met the priest".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(
            ctx.contains("[yesterday, 08:00] Met the priest"),
            "got: {ctx}"
        );
    }

    #[test]
    fn test_context_string_with_now_days_ago() {
        let mut mem = ShortTermMemory::new();
        let ts = Utc.with_ymd_and_hms(1820, 3, 15, 10, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(); // 5 days later
        mem.add(MemoryEntry {
            timestamp: ts,
            content: "Attended the fair".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(ctx.contains("[5 days ago] Attended the fair"), "got: {ctx}");
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
            kind: None,
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
    fn test_long_term_recall_mixed_case_query() {
        // Query keywords may arrive mixed-case from player input
        // (see `ticks::build_enhanced_context_with_config`). The recall
        // path must lower-case them to match the lowercased stored keywords.
        let mut ltm = LongTermMemory::new();
        ltm.store(make_lt_entry("Saw the landlord", 0.8, &["landlord"]));

        let upper = ltm.recall(&["LANDLORD"], 5);
        assert_eq!(upper.len(), 1, "uppercase query should match");

        let mixed = ltm.recall(&["LandLord"], 5);
        assert_eq!(mixed.len(), 1, "mixed-case query should match");
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

    // ── Long-term memory capacity (issue #341) ──────────────────────

    fn make_lt_entry_at(ts_hour: u32, importance: f32) -> LongTermEntry {
        LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, ts_hour, 0, 0).unwrap(),
            content: format!("event at hour {ts_hour} (score {importance})"),
            importance,
            keywords: vec!["test".to_string()],
        }
    }

    #[test]
    fn long_term_memory_default_capacity_matches_const() {
        let ltm = LongTermMemory::new();
        assert_eq!(ltm.max_entries(), LONG_TERM_CAPACITY);
    }

    #[test]
    fn long_term_memory_is_bounded_by_cap() {
        let mut ltm = LongTermMemory::with_capacity(3);
        for h in 0..10 {
            ltm.store(make_lt_entry_at(h, 0.7));
        }
        assert_eq!(ltm.len(), 3, "must not grow past configured cap");
    }

    #[test]
    fn long_term_memory_evicts_lowest_importance_first() {
        let mut ltm = LongTermMemory::with_capacity(3);
        assert!(ltm.store(make_lt_entry_at(0, 0.6))); // idx 0
        assert!(ltm.store(make_lt_entry_at(1, 0.9))); // idx 1
        assert!(ltm.store(make_lt_entry_at(2, 0.7))); // idx 2
        // Pushing a new 0.8 at capacity: the 0.6 entry (hour 0) is the
        // lowest-importance entry and must be evicted to make room.
        assert!(ltm.store(make_lt_entry_at(3, 0.8)));
        let remaining_importance: Vec<f32> = ltm.entries().iter().map(|e| e.importance).collect();
        assert!(!remaining_importance.iter().any(|i| (*i - 0.6).abs() < 1e-6));
        assert_eq!(ltm.len(), 3);
    }

    #[test]
    fn long_term_memory_evicts_oldest_on_importance_tie() {
        let mut ltm = LongTermMemory::with_capacity(2);
        assert!(ltm.store(make_lt_entry_at(0, 0.6)));
        assert!(ltm.store(make_lt_entry_at(1, 0.6)));
        // New 0.6 entry at capacity: a tie on importance — the oldest
        // (hour 0) should be evicted.
        assert!(ltm.store(make_lt_entry_at(2, 0.6)));
        let kept_hours: Vec<u32> = ltm
            .entries()
            .iter()
            .map(|e| e.timestamp.format("%H").to_string().parse().unwrap())
            .collect();
        assert!(!kept_hours.contains(&0));
        assert!(kept_hours.contains(&1));
        assert!(kept_hours.contains(&2));
    }

    #[test]
    fn long_term_memory_rejects_lower_importance_when_full() {
        let mut ltm = LongTermMemory::with_capacity(2);
        assert!(ltm.store(make_lt_entry_at(0, 0.9)));
        assert!(ltm.store(make_lt_entry_at(1, 0.8)));
        // Incoming 0.6 is strictly below every stored importance — reject.
        assert!(!ltm.store(make_lt_entry_at(2, 0.6)));
        assert_eq!(ltm.len(), 2);
    }

    #[test]
    fn long_term_memory_preserves_importance_threshold_even_when_empty_room() {
        let mut ltm = LongTermMemory::with_capacity(2);
        assert!(!ltm.store(make_lt_entry_at(0, 0.49)));
        assert!(ltm.is_empty());
    }

    #[test]
    fn long_term_memory_deserialises_legacy_format_without_max_entries() {
        // Legacy save files only have `entries`. Verify #[serde(default)]
        // fills in the capacity so autosave round-trips don't regress.
        let legacy = serde_json::json!({
            "entries": [
                {
                    "timestamp": "1820-03-20T10:00:00Z",
                    "content": "legacy",
                    "importance": 0.8,
                    "keywords": ["legacy"]
                }
            ]
        });
        let ltm: LongTermMemory = serde_json::from_value(legacy).unwrap();
        assert_eq!(ltm.len(), 1);
        assert_eq!(ltm.max_entries(), LONG_TERM_CAPACITY);
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
            kind: None,
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
            kind: None,
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
            kind: None,
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
            kind: None,
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
            content: "Spoke with the newcomer about the secret".to_string(),
            participants: vec![NpcId(0), NpcId(1)], // player involved
            location: LocationId(1),
            kind: None,
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

    // ── Issue #461: NaN importance must not evict valid entries ─────

    #[test]
    fn long_term_memory_rejects_nan_importance() {
        let mut ltm = LongTermMemory::with_capacity(2);
        assert!(ltm.store(make_lt_entry_at(0, 0.8)));
        assert!(ltm.store(make_lt_entry_at(1, 0.7)));

        let stored = ltm.store(make_lt_entry("NaN entry", f32::NAN, &["test"]));
        assert!(!stored, "NaN importance must be rejected");
        assert_eq!(ltm.len(), 2, "existing entries must be preserved");
    }

    #[test]
    fn long_term_memory_rejects_infinite_importance() {
        let mut ltm = LongTermMemory::with_capacity(2);
        assert!(ltm.store(make_lt_entry_at(0, 0.8)));
        assert!(ltm.store(make_lt_entry_at(1, 0.7)));

        assert!(!ltm.store(make_lt_entry("inf", f32::INFINITY, &["test"])));
        assert!(!ltm.store(make_lt_entry("-inf", f32::NEG_INFINITY, &["test"])));
        assert_eq!(ltm.len(), 2);
    }

    // ── Issue #419: a pre-existing NaN entry (e.g. from a corrupted save
    // file that bypassed store()'s is_finite gate on load) must not
    // indefinitely poison eviction and force real entries to be rejected.

    #[test]
    fn long_term_memory_evicts_preexisting_nan_entry_before_valid_ones() {
        // Cap=2 with two NaN entries + one valid entry all injected
        // directly (simulating a corrupted save-file load that bypassed
        // store()'s incoming-importance gate). The next real store() must
        // purge both NaN entries and succeed, even at a modest importance
        // that would otherwise be rejected because the valid 0.6 entry
        // would look like the eviction candidate.
        let mut ltm = LongTermMemory::with_capacity(2);
        ltm.entries.push(LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap(),
            content: "corrupted-A".into(),
            importance: f32::NAN,
            keywords: vec!["legacy".into()],
        });
        ltm.entries.push(LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 1, 0, 0).unwrap(),
            content: "corrupted-B".into(),
            importance: f32::NAN,
            keywords: vec!["legacy".into()],
        });
        // Note: bypassing store() here, so the valid entry sits alongside
        // the two NaN entries at len=3 even though capacity is 2 — this
        // is exactly the kind of inconsistent on-disk state a corrupted
        // save could produce.
        ltm.entries.push(LongTermEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 2, 0, 0).unwrap(),
            content: "real entry".into(),
            importance: 0.6,
            keywords: vec!["test".into()],
        });
        assert_eq!(ltm.len(), 3);

        // Store a fresh entry at importance 0.55. Pre-fix behavior: the
        // NaN entries were sorted as Equal, `min_by` picked the first of
        // them, `evict_importance = NaN`, `0.55 < NaN` is false, so no
        // early return — but then we'd evict a NaN entry (actually OK).
        // The real bug bites when the min_by picks a VALID entry over a
        // NaN because iteration order puts NaN later; pre-fix this meant
        // the real 0.6 got evicted ahead of the NaN entries. Post-fix
        // the retain() purges both NaN entries first, leaving only the
        // real 0.6 — then capacity=2 fits the new 0.55 freely.
        assert!(
            ltm.store(make_lt_entry("fresh", 0.55, &["test"])),
            "fresh entry must fit after NaN entries are purged"
        );

        assert!(
            ltm.entries.iter().all(|e| e.importance.is_finite()),
            "NaN-importance entries must be purged on store()"
        );
        // The valid 0.6 and the freshly-stored 0.55 both survive.
        assert_eq!(ltm.len(), 2);
    }

    #[test]
    fn long_term_memory_purges_multiple_nan_entries_at_once() {
        let mut ltm = LongTermMemory::with_capacity(5);
        for _ in 0..3 {
            ltm.entries.push(LongTermEntry {
                timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap(),
                content: "corrupt".into(),
                importance: f32::NAN,
                keywords: vec![],
            });
        }
        // Only NaN entries present; a store purges all three and adds the
        // new one.
        assert!(ltm.store(make_lt_entry("first-real", 0.6, &["t"])));
        assert_eq!(ltm.len(), 1);
        assert!(ltm.entries.iter().all(|e| e.importance.is_finite()));
    }

    // ── Issue #462: future timestamps must not silently pass ────────

    #[test]
    fn relative_time_label_future_timestamp_clamps_to_just_now() {
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(1820, 3, 20, 13, 0, 0).unwrap();
        let label = relative_time_label(future, now);
        assert_eq!(label, "just now");
    }

    #[test]
    fn context_string_with_now_future_entry() {
        let mut mem = ShortTermMemory::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(1820, 3, 20, 14, 0, 0).unwrap();
        mem.add(MemoryEntry {
            timestamp: future,
            content: "Future event".to_string(),
            participants: vec![],
            location: LocationId(1),
            kind: None,
        });
        let ctx = mem.context_string_with_now(5, now);
        assert!(ctx.contains("[just now] Future event"), "got: {ctx}");
    }
}
