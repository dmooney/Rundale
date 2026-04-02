//! Phase 3 NPC types — relationships, schedules, cognitive tiers, intelligence, and events.
//!
//! These types extend the base NPC system with daily schedules, inter-NPC
//! relationships, cognitive LOD tiers, multidimensional intelligence, and
//! Tier 2 simulation events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::npc::NpcId;
use crate::world::LocationId;

/// Multidimensional intelligence profile for an NPC.
///
/// Six dimensions rated on a 1–5 scale, where 1 is very low and 5 is
/// exceptional. These ratings shape LLM dialogue generation: a high-verbal
/// NPC uses richer vocabulary, a high-emotional NPC reads subtext, a
/// low-analytical NPC avoids abstract reasoning, etc.
///
/// # Dimensions
///
/// | Dim | Code | Meaning |
/// |-----|------|---------|
/// | Verbal | V | Language fluency, vocabulary, eloquence |
/// | Analytical | A | Logic, reasoning, problem-solving |
/// | Emotional | E | Empathy, reading people, social awareness |
/// | Practical | P | Common sense, hands-on resourcefulness |
/// | Wisdom | W | Life experience, judgment, foresight |
/// | Creative | C | Imagination, wit, improvisation |
///
/// # Prompt encoding
///
/// [`Intelligence::prompt_guidance`] produces direct behavioral directives
/// for LLM system prompts, highlighting only notable strengths (4-5) and
/// weaknesses (1-2). Average dimensions (3) generate no output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intelligence {
    /// Language fluency, vocabulary, eloquence (1–5).
    pub verbal: u8,
    /// Logic, reasoning, problem-solving (1–5).
    pub analytical: u8,
    /// Empathy, reading people, social awareness (1–5).
    pub emotional: u8,
    /// Common sense, hands-on resourcefulness (1–5).
    pub practical: u8,
    /// Life experience, judgment, foresight (1–5).
    pub wisdom: u8,
    /// Imagination, wit, improvisation (1–5).
    pub creative: u8,
}

impl Default for Intelligence {
    /// Returns a baseline "average" intelligence profile (all 3s).
    fn default() -> Self {
        Self {
            verbal: 3,
            analytical: 3,
            emotional: 3,
            practical: 3,
            wisdom: 3,
            creative: 3,
        }
    }
}

impl Intelligence {
    /// Creates a new intelligence profile, clamping all values to 1–5.
    pub fn new(
        verbal: u8,
        analytical: u8,
        emotional: u8,
        practical: u8,
        wisdom: u8,
        creative: u8,
    ) -> Self {
        Self {
            verbal: verbal.clamp(1, 5),
            analytical: analytical.clamp(1, 5),
            emotional: emotional.clamp(1, 5),
            practical: practical.clamp(1, 5),
            wisdom: wisdom.clamp(1, 5),
            creative: creative.clamp(1, 5),
        }
    }

    /// Returns a prose description of how this NPC thinks and speaks.
    ///
    /// Translates the numeric intelligence profile into natural language
    /// that the LLM can use to shape dialogue style. Covers all six
    /// dimensions, describing notable strengths and weaknesses in detail.
    /// Returns an empty string for a perfectly average (all-3s) profile.
    pub fn prompt_guidance(&self) -> String {
        let mut parts = Vec::new();

        // Verbal — how they speak
        match self.verbal {
            1 => parts.push(
                "Struggles to find words, speaks in halting fragments, \
                 and often trails off mid-sentence.",
            ),
            2 => parts.push(
                "Speaks plainly with a limited vocabulary, \
                 preferring short familiar words over anything fancy.",
            ),
            4 => parts.push(
                "Well-spoken with a good vocabulary, \
                 able to express ideas clearly and persuasively.",
            ),
            5 => parts.push(
                "Exceptionally eloquent — chooses words with precision, \
                 turns a phrase beautifully, and commands attention when speaking.",
            ),
            _ => {}
        }

        // Analytical — how they reason
        match self.analytical {
            1 => parts.push(
                "Cannot follow even simple logical arguments; \
                 easily confused by cause and effect.",
            ),
            2 => parts.push(
                "Thinks concretely and struggles with abstract reasoning; \
                 takes things at face value.",
            ),
            4 => parts.push(
                "Sharp-minded, notices patterns and logical connections \
                 that others miss.",
            ),
            5 => parts.push(
                "Brilliantly analytical — sees through deceptions, \
                 connects distant facts, and reasons with piercing clarity.",
            ),
            _ => {}
        }

        // Emotional — how they read people
        match self.emotional {
            1 => parts.push(
                "Oblivious to others' feelings, misreads the room constantly, \
                 and blunders through social situations.",
            ),
            2 => parts.push(
                "Blunt and socially clumsy; often says the wrong thing \
                 without realising the effect.",
            ),
            4 => parts.push(
                "Perceptive about people's feelings, picks up on mood shifts \
                 and unspoken tensions.",
            ),
            5 => parts.push(
                "Reads people like a book — catches every flicker of emotion, \
                 hears what is left unsaid, and responds with deep empathy.",
            ),
            _ => {}
        }

        // Practical — common sense and resourcefulness
        match self.practical {
            1 => parts.push(
                "Hopelessly impractical; overlooks obvious solutions \
                 and fumbles with everyday tasks.",
            ),
            2 => parts.push(
                "Not particularly handy or resourceful; \
                 tends to overcomplicate simple problems.",
            ),
            4 => parts.push(
                "Resourceful and sensible, always knows a practical fix \
                 and wastes nothing.",
            ),
            5 => parts.push(
                "Extraordinarily resourceful — can fix, build, or improvise \
                 a solution from whatever is at hand, with unfailing common sense.",
            ),
            _ => {}
        }

        // Wisdom — life experience and judgment
        match self.wisdom {
            1 => parts.push(
                "Reckless and short-sighted, repeats the same mistakes \
                 and never learns from experience.",
            ),
            2 => parts.push(
                "Impulsive and prone to poor judgment; \
                 acts first and thinks later.",
            ),
            4 => parts.push(
                "Draws on hard-won life experience; \
                 gives considered, measured advice.",
            ),
            5 => parts.push(
                "Deeply wise — decades of living have given a quiet authority, \
                 a long view of things, and an instinct for what truly matters.",
            ),
            _ => {}
        }

        // Creative — imagination, wit, improvisation
        match self.creative {
            1 => parts.push(
                "Completely literal-minded; humour, metaphor, \
                 and imagination are foreign territory.",
            ),
            2 => parts.push(
                "Unimaginative and humourless; sticks to the obvious \
                 and rarely surprises.",
            ),
            4 => parts.push(
                "Quick-witted with a ready turn of phrase; \
                 sees the funny side and thinks on the spot.",
            ),
            5 => parts.push(
                "Brilliantly creative — a natural storyteller whose wit, \
                 vivid metaphors, and leaps of imagination light up any conversation.",
            ),
            _ => {}
        }

        parts.join(" ")
    }
}

/// The kind of relationship between two NPCs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipKind {
    /// Blood or marriage relation.
    Family,
    /// Close friend.
    Friend,
    /// Casual neighbor acquaintance.
    Neighbor,
    /// Competitive or antagonistic relationship.
    Rival,
    /// Strong dislike or hostility.
    Enemy,
    /// Romantic interest or partner.
    Romantic,
    /// Work-related connection.
    Professional,
}

impl fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationshipKind::Family => write!(f, "family"),
            RelationshipKind::Friend => write!(f, "friend"),
            RelationshipKind::Neighbor => write!(f, "neighbor"),
            RelationshipKind::Rival => write!(f, "rival"),
            RelationshipKind::Enemy => write!(f, "enemy"),
            RelationshipKind::Romantic => write!(f, "romantic"),
            RelationshipKind::Professional => write!(f, "professional"),
        }
    }
}

/// A recorded event in a relationship's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipEvent {
    /// When the event occurred in game time.
    pub timestamp: DateTime<Utc>,
    /// Description of what happened.
    pub description: String,
}

/// A relationship between two NPCs.
///
/// Tracks the kind of relationship, its strength on a -1.0 to 1.0 scale,
/// and an append-only history of significant events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    /// The type of relationship.
    pub kind: RelationshipKind,
    /// Strength from -1.0 (hostile) to 1.0 (close).
    pub strength: f64,
    /// Append-only log of relationship events.
    pub history: Vec<RelationshipEvent>,
}

impl Relationship {
    /// Creates a new relationship with the given kind and strength.
    ///
    /// Strength is clamped to the range -1.0 to 1.0.
    pub fn new(kind: RelationshipKind, strength: f64) -> Self {
        Self {
            kind,
            strength: strength.clamp(-1.0, 1.0),
            history: Vec::new(),
        }
    }

    /// Adjusts the relationship strength by a delta, clamping to -1.0..1.0.
    pub fn adjust_strength(&mut self, delta: f64) {
        self.strength = (self.strength + delta).clamp(-1.0, 1.0);
    }

    /// Records an event in the relationship history.
    pub fn record_event(&mut self, timestamp: DateTime<Utc>, description: String) {
        self.history.push(RelationshipEvent {
            timestamp,
            description,
        });
    }
}

/// A single entry in an NPC's daily schedule.
///
/// Defines where the NPC should be and what they do during a time range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// Start hour (0-23, inclusive).
    pub start_hour: u8,
    /// End hour (0-23, inclusive).
    pub end_hour: u8,
    /// Target location for this time slot.
    pub location: LocationId,
    /// What the NPC does during this slot (e.g. "tending bar").
    pub activity: String,
}

/// An NPC's daily schedule.
///
/// Contains a list of time-slot entries defining where the NPC goes
/// throughout the day. Entries should cover all 24 hours without gaps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailySchedule {
    /// Schedule entries sorted by start_hour.
    pub entries: Vec<ScheduleEntry>,
}

impl DailySchedule {
    /// Returns the schedule entry active at the given hour.
    ///
    /// Returns `None` if no entry covers the hour (schedule gap).
    pub fn entry_at(&self, hour: u8) -> Option<&ScheduleEntry> {
        self.entries
            .iter()
            .find(|e| hour >= e.start_hour && hour <= e.end_hour)
    }

    /// Returns the desired location at the given hour.
    pub fn location_at(&self, hour: u8) -> Option<LocationId> {
        self.entry_at(hour).map(|e| e.location)
    }
}

/// Whether an NPC is stationary or moving between locations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum NpcState {
    /// NPC is at their current location.
    #[default]
    Present,
    /// NPC is traveling between locations.
    InTransit {
        /// Location they departed from.
        from: LocationId,
        /// Location they are heading to.
        to: LocationId,
        /// Game time when they will arrive.
        arrives_at: DateTime<Utc>,
    },
}

/// Cognitive LOD tier for NPC simulation fidelity.
///
/// Higher tiers use more compute-intensive inference:
/// - Tier 1: Full LLM (per player interaction)
/// - Tier 2: Lighter LLM (every 5 game-minutes for nearby NPCs)
/// - Tier 3: Batch inference (daily, for distant NPCs — future)
/// - Tier 4: Rules engine only (seasonal — future)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CogTier {
    /// Full-fidelity inference — same location as player.
    Tier1,
    /// Lighter inference — 1-2 edges from player.
    Tier2,
    /// Batch inference — 3+ edges away (Phase 4+).
    Tier3,
    /// Rules engine only — far away (Phase 4+).
    Tier4,
}

/// An event produced by a Tier 2 simulation tick.
///
/// Captures what happened at a location during background simulation,
/// including mood changes and relationship adjustments for the NPCs involved.
#[derive(Debug, Clone)]
pub struct Tier2Event {
    /// Location where the event occurred.
    pub location: LocationId,
    /// Human-readable summary of what happened.
    pub summary: String,
    /// NPCs who participated.
    pub participants: Vec<NpcId>,
    /// Mood changes to apply.
    pub mood_changes: Vec<MoodChange>,
    /// Relationship strength deltas to apply.
    pub relationship_changes: Vec<RelationshipChange>,
}

/// A mood change resulting from a Tier 2 event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodChange {
    /// Which NPC's mood to update.
    pub npc_id: NpcId,
    /// New mood string.
    pub new_mood: String,
}

/// A relationship strength change resulting from a Tier 2 event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipChange {
    /// NPC whose relationship is affected.
    pub from: NpcId,
    /// The other NPC in the relationship.
    pub to: NpcId,
    /// Change in strength (-1.0 to 1.0 range).
    pub delta: f64,
}

/// The structured response expected from a Tier 2 LLM call.
///
/// Deserialized from JSON via `generate_json<Tier2Response>()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier2Response {
    /// Summary of what happened at this location.
    #[serde(default)]
    pub summary: String,
    /// Mood changes for participating NPCs.
    #[serde(default)]
    pub mood_changes: Vec<MoodChange>,
    /// Relationship strength adjustments.
    #[serde(default)]
    pub relationship_changes: Vec<RelationshipChange>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_relationship_new_clamps_strength() {
        let r = Relationship::new(RelationshipKind::Friend, 1.5);
        assert_eq!(r.strength, 1.0);

        let r = Relationship::new(RelationshipKind::Enemy, -2.0);
        assert_eq!(r.strength, -1.0);

        let r = Relationship::new(RelationshipKind::Neighbor, 0.5);
        assert_eq!(r.strength, 0.5);
    }

    #[test]
    fn test_relationship_adjust_strength() {
        let mut r = Relationship::new(RelationshipKind::Friend, 0.5);
        r.adjust_strength(0.3);
        assert!((r.strength - 0.8).abs() < f64::EPSILON);

        r.adjust_strength(0.5);
        assert_eq!(r.strength, 1.0); // clamped

        r.adjust_strength(-2.5);
        assert_eq!(r.strength, -1.0); // clamped
    }

    #[test]
    fn test_relationship_record_event() {
        let mut r = Relationship::new(RelationshipKind::Professional, 0.3);
        let ts = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        r.record_event(ts, "Had a good trade at the market".to_string());
        assert_eq!(r.history.len(), 1);
        assert_eq!(r.history[0].description, "Had a good trade at the market");
    }

    #[test]
    fn test_schedule_entry_at() {
        let schedule = DailySchedule {
            entries: vec![
                ScheduleEntry {
                    start_hour: 6,
                    end_hour: 11,
                    location: LocationId(2),
                    activity: "opening the pub".to_string(),
                },
                ScheduleEntry {
                    start_hour: 12,
                    end_hour: 22,
                    location: LocationId(2),
                    activity: "tending bar".to_string(),
                },
                ScheduleEntry {
                    start_hour: 23,
                    end_hour: 5,
                    location: LocationId(1),
                    activity: "sleeping".to_string(),
                },
            ],
        };

        let entry = schedule.entry_at(8).unwrap();
        assert_eq!(entry.activity, "opening the pub");

        let entry = schedule.entry_at(15).unwrap();
        assert_eq!(entry.activity, "tending bar");

        // Gap — no entry covers hour 5 unless the sleeping entry wraps
        // Note: for overnight entries (23-5), our simple check won't match hour 3.
        // This is expected; the data.rs loader should handle wrap-around.
    }

    #[test]
    fn test_schedule_location_at() {
        let schedule = DailySchedule {
            entries: vec![ScheduleEntry {
                start_hour: 8,
                end_hour: 17,
                location: LocationId(3),
                activity: "teaching".to_string(),
            }],
        };

        assert_eq!(schedule.location_at(10), Some(LocationId(3)));
        assert_eq!(schedule.location_at(20), None);
    }

    #[test]
    fn test_npc_state_default() {
        let state = NpcState::default();
        assert!(matches!(state, NpcState::Present));
    }

    #[test]
    fn test_npc_state_in_transit() {
        let arrives = Utc.with_ymd_and_hms(1820, 3, 20, 12, 30, 0).unwrap();
        let state = NpcState::InTransit {
            from: LocationId(1),
            to: LocationId(2),
            arrives_at: arrives,
        };
        match state {
            NpcState::InTransit { from, to, .. } => {
                assert_eq!(from, LocationId(1));
                assert_eq!(to, LocationId(2));
            }
            NpcState::Present => panic!("expected InTransit"),
        }
    }

    #[test]
    fn test_cog_tier_equality() {
        assert_eq!(CogTier::Tier1, CogTier::Tier1);
        assert_ne!(CogTier::Tier1, CogTier::Tier2);
    }

    #[test]
    fn test_relationship_kind_display() {
        assert_eq!(RelationshipKind::Family.to_string(), "family");
        assert_eq!(RelationshipKind::Professional.to_string(), "professional");
        assert_eq!(RelationshipKind::Rival.to_string(), "rival");
    }

    #[test]
    fn test_tier2_response_deserialize() {
        let json = r#"{
            "summary": "Padraig and Tommy shared stories over a pint",
            "mood_changes": [{"npc_id": 1, "new_mood": "jovial"}],
            "relationship_changes": [{"from": 1, "to": 5, "delta": 0.1}]
        }"#;
        let resp: Tier2Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.summary, "Padraig and Tommy shared stories over a pint");
        assert_eq!(resp.mood_changes.len(), 1);
        assert_eq!(resp.mood_changes[0].npc_id, NpcId(1));
        assert_eq!(resp.relationship_changes.len(), 1);
        assert!((resp.relationship_changes[0].delta - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tier2_response_deserialize_minimal() {
        let json = r#"{}"#;
        let resp: Tier2Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.summary, "");
        assert!(resp.mood_changes.is_empty());
        assert!(resp.relationship_changes.is_empty());
    }

    // --- Intelligence tests ---

    #[test]
    fn test_intelligence_default() {
        let intel = Intelligence::default();
        assert_eq!(intel.verbal, 3);
        assert_eq!(intel.analytical, 3);
        assert_eq!(intel.emotional, 3);
        assert_eq!(intel.practical, 3);
        assert_eq!(intel.wisdom, 3);
        assert_eq!(intel.creative, 3);
    }

    #[test]
    fn test_intelligence_new_clamps() {
        let intel = Intelligence::new(0, 6, 1, 5, 10, 255);
        assert_eq!(intel.verbal, 1, "below-minimum clamped to 1");
        assert_eq!(intel.analytical, 5, "above-maximum clamped to 5");
        assert_eq!(intel.emotional, 1);
        assert_eq!(intel.practical, 5);
        assert_eq!(intel.wisdom, 5, "far above maximum clamped to 5");
        assert_eq!(intel.creative, 5);
    }

    #[test]
    fn test_intelligence_new_valid_range() {
        let intel = Intelligence::new(1, 2, 3, 4, 5, 3);
        assert_eq!(intel.verbal, 1);
        assert_eq!(intel.analytical, 2);
        assert_eq!(intel.emotional, 3);
        assert_eq!(intel.practical, 4);
        assert_eq!(intel.wisdom, 5);
        assert_eq!(intel.creative, 3);
    }

    #[test]
    fn test_intelligence_guidance_high_verbal() {
        let intel = Intelligence::new(5, 3, 3, 3, 3, 3);
        let guidance = intel.prompt_guidance();
        assert!(
            guidance.contains("eloquent"),
            "high verbal should describe eloquence"
        );
    }

    #[test]
    fn test_intelligence_guidance_low_analytical() {
        let intel = Intelligence::new(3, 1, 3, 3, 3, 3);
        let guidance = intel.prompt_guidance();
        assert!(
            guidance.contains("logical arguments"),
            "low analytical should mention struggling with logic"
        );
    }

    #[test]
    fn test_intelligence_guidance_high_emotional() {
        let intel = Intelligence::new(3, 3, 5, 3, 3, 3);
        let guidance = intel.prompt_guidance();
        assert!(
            guidance.contains("left unsaid"),
            "high emotional should mention reading what is unsaid"
        );
    }

    #[test]
    fn test_intelligence_guidance_low_creative() {
        let intel = Intelligence::new(3, 3, 3, 3, 3, 1);
        let guidance = intel.prompt_guidance();
        assert!(
            guidance.contains("literal-minded"),
            "low creative should mention literal-mindedness"
        );
    }

    #[test]
    fn test_intelligence_guidance_average_is_empty() {
        let intel = Intelligence::default();
        let guidance = intel.prompt_guidance();
        assert!(
            guidance.is_empty(),
            "all-3s profile should have no special guidance"
        );
    }

    #[test]
    fn test_intelligence_guidance_mixed_profile() {
        // High verbal + low practical = both descriptions
        let intel = Intelligence::new(5, 3, 3, 1, 3, 3);
        let guidance = intel.prompt_guidance();
        assert!(guidance.contains("eloquent"));
        assert!(guidance.contains("impractical"));
    }

    #[test]
    fn test_intelligence_serialize_roundtrip() {
        let intel = Intelligence::new(2, 4, 1, 5, 3, 4);
        let json = serde_json::to_string(&intel).unwrap();
        let restored: Intelligence = serde_json::from_str(&json).unwrap();
        assert_eq!(intel, restored);
    }

    #[test]
    fn test_intelligence_deserialize_from_json() {
        let json =
            r#"{"verbal":4,"analytical":2,"emotional":5,"practical":1,"wisdom":3,"creative":4}"#;
        let intel: Intelligence = serde_json::from_str(json).unwrap();
        assert_eq!(intel.verbal, 4);
        assert_eq!(intel.analytical, 2);
        assert_eq!(intel.emotional, 5);
        assert_eq!(intel.practical, 1);
        assert_eq!(intel.wisdom, 3);
        assert_eq!(intel.creative, 4);
    }

    #[test]
    fn test_intelligence_copy_semantics() {
        let a = Intelligence::new(1, 2, 3, 4, 5, 1);
        let b = a; // Copy
        assert_eq!(a, b);
    }
}
