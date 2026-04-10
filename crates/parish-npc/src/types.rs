//! Phase 3 NPC types — relationships, schedules, cognitive tiers, intelligence, and events.
//!
//! These types extend the base NPC system with daily schedules, inter-NPC
//! relationships, cognitive LOD tiers, multidimensional intelligence, and
//! Tier 2 simulation events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use parish_types::LocationId;
use parish_types::NpcId;
use parish_world::time::{DayType, Season};

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
    #[serde(deserialize_with = "deserialize_clamped_strength")]
    pub strength: f64,
    /// Append-only log of relationship events.
    pub history: Vec<RelationshipEvent>,
}

/// Deserializes a strength value, clamping it to the valid range -1.0..=1.0.
fn deserialize_clamped_strength<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    Ok(value.clamp(-1.0, 1.0))
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
    /// Whether this is a cuaird (visiting round) slot.
    ///
    /// When true, the location rotates among the NPC's friends' homes
    /// on different days, recreating the 1820s Irish tradition of
    /// neighbors gathering for storytelling and music.
    #[serde(default)]
    pub cuaird: bool,
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
    /// Handles overnight wraparound: an entry with `start_hour > end_hour`
    /// (e.g. 22–06) is active when `hour >= start_hour OR hour <= end_hour`.
    ///
    /// Returns `None` if no entry covers the hour (schedule gap).
    pub fn entry_at(&self, hour: u8) -> Option<&ScheduleEntry> {
        self.entries.iter().find(|e| {
            if e.start_hour <= e.end_hour {
                hour >= e.start_hour && hour <= e.end_hour
            } else {
                // Overnight: e.g. 22–06
                hour >= e.start_hour || hour <= e.end_hour
            }
        })
    }

    /// Returns the desired location at the given hour.
    pub fn location_at(&self, hour: u8) -> Option<LocationId> {
        self.entry_at(hour).map(|e| e.location)
    }
}

/// A single variant of an NPC's schedule, optionally scoped to a season and/or day type.
///
/// When both `season` and `day_type` are `None`, this is the default fallback schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleVariant {
    /// Season this variant applies to, or `None` for any season.
    #[serde(default)]
    pub season: Option<Season>,
    /// Day type this variant applies to, or `None` for any day.
    #[serde(default)]
    pub day_type: Option<DayType>,
    /// The schedule entries for this variant.
    pub entries: Vec<ScheduleEntry>,
}

/// Season- and day-aware schedule for an NPC.
///
/// Contains named schedule variants scoped by optional `(season, day_type)`.
/// Resolution order when looking up the active schedule:
///   1. Exact match: `(Some(season), Some(day_type))`
///   2. Season only: `(Some(season), None)`
///   3. Day type only: `(None, Some(day_type))`
///   4. Default: `(None, None)`
///
/// This allows NPCs to declare only the variants that differ from their
/// default routine. A publican might need only weekday/sunday variants,
/// while a farmer needs summer/winter × weekday/sunday.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonalSchedule {
    /// Schedule variants in priority order (resolution searches linearly).
    pub variants: Vec<ScheduleVariant>,
}

impl SeasonalSchedule {
    /// Resolves the best-matching schedule entries for the given context.
    ///
    /// Fallback order: exact match → season-only → day-only → default.
    pub fn resolve(&self, season: Season, day_type: DayType) -> Option<&[ScheduleEntry]> {
        // 1. Exact match: both season and day_type
        if let Some(v) = self
            .variants
            .iter()
            .find(|v| v.season == Some(season) && v.day_type == Some(day_type))
        {
            return Some(&v.entries);
        }
        // 2. Season only (any day)
        if let Some(v) = self
            .variants
            .iter()
            .find(|v| v.season == Some(season) && v.day_type.is_none())
        {
            return Some(&v.entries);
        }
        // 3. Day type only (any season)
        if let Some(v) = self
            .variants
            .iter()
            .find(|v| v.season.is_none() && v.day_type == Some(day_type))
        {
            return Some(&v.entries);
        }
        // 4. Default (both None)
        if let Some(v) = self
            .variants
            .iter()
            .find(|v| v.season.is_none() && v.day_type.is_none())
        {
            return Some(&v.entries);
        }
        None
    }

    /// Returns the schedule entry active at the given hour for the given context.
    ///
    /// Handles overnight wraparound: an entry with `start_hour > end_hour`
    /// (e.g. 22–06) is active when `hour >= start_hour OR hour <= end_hour`.
    pub fn entry_at(&self, hour: u8, season: Season, day_type: DayType) -> Option<&ScheduleEntry> {
        let entries = self.resolve(season, day_type)?;
        entries.iter().find(|e| {
            if e.start_hour <= e.end_hour {
                hour >= e.start_hour && hour <= e.end_hour
            } else {
                // Overnight: e.g. 22–06
                hour >= e.start_hour || hour <= e.end_hour
            }
        })
    }

    /// Returns the desired location at the given hour for the given context.
    pub fn location_at(&self, hour: u8, season: Season, day_type: DayType) -> Option<LocationId> {
        self.entry_at(hour, season, day_type).map(|e| e.location)
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

/// The result of a Tier 3 batch simulation for a single NPC.
///
/// Produced by the batch LLM call that simulates many distant NPCs at once.
/// Each update describes what one NPC did during the simulated period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3Update {
    /// Which NPC this update is for.
    pub npc_id: NpcId,
    /// New location (if NPC moved during the simulated period).
    #[serde(default)]
    pub new_location: Option<LocationId>,
    /// Updated mood string.
    #[serde(default)]
    pub mood: String,
    /// Summary of what the NPC did during the simulated period.
    #[serde(default)]
    pub activity_summary: String,
    /// Relationship changes: (from, to, strength_delta).
    #[serde(default)]
    pub relationship_changes: Vec<RelationshipChange>,
}

/// The full response from a Tier 3 batch LLM call.
///
/// Contains an array of updates, one per NPC in the batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier3Response {
    /// Per-NPC updates from the batch simulation.
    #[serde(default)]
    pub updates: Vec<Tier3Update>,
}

/// Priority levels for inference requests.
///
/// Lower values = higher priority. Used to ensure player-facing dialogue
/// is never delayed by background batch simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferencePriority {
    /// Tier 1: player-facing dialogue (highest priority).
    Interactive = 0,
    /// Tier 2: nearby NPC background simulation.
    Background = 1,
    /// Tier 3: distant NPC batch simulation (lowest LLM priority).
    Batch = 2,
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
                    cuaird: false,
                },
                ScheduleEntry {
                    start_hour: 12,
                    end_hour: 22,
                    location: LocationId(2),
                    activity: "tending bar".to_string(),
                    cuaird: false,
                },
                ScheduleEntry {
                    start_hour: 23,
                    end_hour: 5,
                    location: LocationId(1),
                    activity: "sleeping".to_string(),
                    cuaird: false,
                },
            ],
        };

        let entry = schedule.entry_at(8).unwrap();
        assert_eq!(entry.activity, "opening the pub");

        let entry = schedule.entry_at(15).unwrap();
        assert_eq!(entry.activity, "tending bar");

        // Overnight entry (23-5): hours 23 and 3 should both match.
        let entry = schedule.entry_at(23).unwrap();
        assert_eq!(entry.activity, "sleeping");

        let entry = schedule.entry_at(3).unwrap();
        assert_eq!(entry.activity, "sleeping");

        // Hour 6 is the start of "opening the pub", not covered by the overnight entry.
        let entry = schedule.entry_at(6).unwrap();
        assert_eq!(entry.activity, "opening the pub");

        // Hour 5 is covered by the overnight sleeping entry (end_hour=5).
        let entry = schedule.entry_at(5).unwrap();
        assert_eq!(entry.activity, "sleeping");
    }

    #[test]
    fn test_schedule_entry_at_overnight_only() {
        // A schedule with a single overnight entry (22–06).
        let schedule = DailySchedule {
            entries: vec![ScheduleEntry {
                start_hour: 22,
                end_hour: 6,
                location: LocationId(1),
                activity: "sleeping".to_string(),
                cuaird: false,
            }],
        };

        // Hours in the evening portion (after midnight rollover)
        assert!(schedule.entry_at(22).is_some());
        assert!(schedule.entry_at(23).is_some());
        // Hours in the early-morning portion (before end_hour)
        assert!(schedule.entry_at(0).is_some());
        assert!(schedule.entry_at(3).is_some());
        assert!(schedule.entry_at(6).is_some());
        // Hour 7 is outside the range
        assert!(schedule.entry_at(7).is_none());
        assert!(schedule.entry_at(12).is_none());
        assert!(schedule.entry_at(21).is_none());
    }

    #[test]
    fn test_schedule_location_at() {
        let schedule = DailySchedule {
            entries: vec![ScheduleEntry {
                start_hour: 8,
                end_hour: 17,
                location: LocationId(3),
                activity: "teaching".to_string(),
                cuaird: false,
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

    // --- SeasonalSchedule tests ---

    fn make_entry(start: u8, end: u8, loc: u32, activity: &str) -> ScheduleEntry {
        ScheduleEntry {
            start_hour: start,
            end_hour: end,
            location: LocationId(loc),
            activity: activity.to_string(),
            cuaird: false,
        }
    }

    fn make_variant(
        season: Option<Season>,
        day_type: Option<DayType>,
        entries: Vec<ScheduleEntry>,
    ) -> ScheduleVariant {
        ScheduleVariant {
            season,
            day_type,
            entries,
        }
    }

    #[test]
    fn test_seasonal_resolve_exact_match() {
        let sched = SeasonalSchedule {
            variants: vec![
                make_variant(None, None, vec![make_entry(0, 23, 1, "default")]),
                make_variant(
                    Some(Season::Winter),
                    Some(DayType::Sunday),
                    vec![make_entry(0, 23, 3, "winter sunday mass")],
                ),
            ],
        };
        let entries = sched.resolve(Season::Winter, DayType::Sunday).unwrap();
        assert_eq!(entries[0].activity, "winter sunday mass");
    }

    #[test]
    fn test_seasonal_resolve_season_only_fallback() {
        let sched = SeasonalSchedule {
            variants: vec![
                make_variant(None, None, vec![make_entry(0, 23, 1, "default")]),
                make_variant(
                    Some(Season::Winter),
                    None,
                    vec![make_entry(0, 23, 2, "winter routine")],
                ),
            ],
        };
        // Winter weekday should match season-only variant
        let entries = sched.resolve(Season::Winter, DayType::Weekday).unwrap();
        assert_eq!(entries[0].activity, "winter routine");
    }

    #[test]
    fn test_seasonal_resolve_day_only_fallback() {
        let sched = SeasonalSchedule {
            variants: vec![
                make_variant(None, None, vec![make_entry(0, 23, 1, "default")]),
                make_variant(
                    None,
                    Some(DayType::Sunday),
                    vec![make_entry(0, 23, 3, "sunday routine")],
                ),
            ],
        };
        // Summer sunday should match day-only variant
        let entries = sched.resolve(Season::Summer, DayType::Sunday).unwrap();
        assert_eq!(entries[0].activity, "sunday routine");
    }

    #[test]
    fn test_seasonal_resolve_default_fallback() {
        let sched = SeasonalSchedule {
            variants: vec![make_variant(
                None,
                None,
                vec![make_entry(0, 23, 1, "default")],
            )],
        };
        // Any combination should match the default
        let entries = sched.resolve(Season::Autumn, DayType::MarketDay).unwrap();
        assert_eq!(entries[0].activity, "default");
    }

    #[test]
    fn test_seasonal_resolve_priority_order() {
        // Exact match should win over season-only, day-only, and default
        let sched = SeasonalSchedule {
            variants: vec![
                make_variant(None, None, vec![make_entry(0, 23, 1, "default")]),
                make_variant(
                    Some(Season::Summer),
                    None,
                    vec![make_entry(0, 23, 2, "summer")],
                ),
                make_variant(
                    None,
                    Some(DayType::Sunday),
                    vec![make_entry(0, 23, 3, "sunday")],
                ),
                make_variant(
                    Some(Season::Summer),
                    Some(DayType::Sunday),
                    vec![make_entry(0, 23, 4, "summer sunday")],
                ),
            ],
        };
        let entries = sched.resolve(Season::Summer, DayType::Sunday).unwrap();
        assert_eq!(entries[0].activity, "summer sunday");
        assert_eq!(entries[0].location, LocationId(4));
    }

    #[test]
    fn test_seasonal_resolve_none_when_empty() {
        let sched = SeasonalSchedule {
            variants: Vec::new(),
        };
        assert!(sched.resolve(Season::Spring, DayType::Weekday).is_none());
    }

    #[test]
    fn test_seasonal_location_at() {
        let sched = SeasonalSchedule {
            variants: vec![
                make_variant(
                    None,
                    None,
                    vec![
                        make_entry(0, 7, 10, "sleeping"),
                        make_entry(8, 17, 9, "working"),
                        make_entry(18, 23, 10, "evening"),
                    ],
                ),
                make_variant(
                    None,
                    Some(DayType::Sunday),
                    vec![
                        make_entry(0, 7, 10, "sleeping"),
                        make_entry(8, 10, 3, "mass"),
                        make_entry(11, 23, 2, "pub"),
                    ],
                ),
            ],
        };
        // Weekday at 10am -> working at location 9
        assert_eq!(
            sched.location_at(10, Season::Spring, DayType::Weekday),
            Some(LocationId(9))
        );
        // Sunday at 10am -> mass at location 3
        assert_eq!(
            sched.location_at(10, Season::Spring, DayType::Sunday),
            Some(LocationId(3))
        );
        // Sunday at 20pm -> pub at location 2
        assert_eq!(
            sched.location_at(20, Season::Spring, DayType::Sunday),
            Some(LocationId(2))
        );
    }

    #[test]
    fn test_seasonal_entry_at_returns_cuaird() {
        let mut entry = make_entry(19, 21, 2, "cuaird visiting");
        entry.cuaird = true;
        let sched = SeasonalSchedule {
            variants: vec![make_variant(None, None, vec![entry])],
        };
        let resolved = sched
            .entry_at(20, Season::Summer, DayType::Weekday)
            .unwrap();
        assert!(resolved.cuaird);
        assert_eq!(resolved.activity, "cuaird visiting");
    }

    // --- Tier 3 type tests ---

    #[test]
    fn test_tier3_update_deserialize_full() {
        use super::{Tier3Response, Tier3Update};

        let json = r#"{
            "updates": [{
                "npc_id": 1,
                "mood": "content",
                "activity_summary": "Worked in the field.",
                "new_location": 5,
                "relationship_changes": [{"from": 1, "to": 2, "delta": 0.1}]
            }]
        }"#;
        let resp: Tier3Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.updates.len(), 1);
        let u = &resp.updates[0];
        assert_eq!(u.npc_id, NpcId(1));
        assert_eq!(u.mood, "content");
        assert_eq!(u.activity_summary, "Worked in the field.");
        assert_eq!(u.new_location, Some(LocationId(5)));
        assert_eq!(u.relationship_changes.len(), 1);
    }

    #[test]
    fn test_tier3_update_deserialize_minimal() {
        use super::Tier3Update;

        let json = r#"{"npc_id": 3}"#;
        let u: Tier3Update = serde_json::from_str(json).unwrap();
        assert_eq!(u.npc_id, NpcId(3));
        assert_eq!(u.mood, "");
        assert_eq!(u.activity_summary, "");
        assert!(u.new_location.is_none());
        assert!(u.relationship_changes.is_empty());
    }

    #[test]
    fn test_tier3_response_empty() {
        use super::Tier3Response;

        let json = r#"{}"#;
        let resp: Tier3Response = serde_json::from_str(json).unwrap();
        assert!(resp.updates.is_empty());
    }

    #[test]
    fn test_inference_priority_ordering() {
        use super::InferencePriority;

        assert!(InferencePriority::Interactive < InferencePriority::Background);
        assert!(InferencePriority::Background < InferencePriority::Batch);
    }
}
