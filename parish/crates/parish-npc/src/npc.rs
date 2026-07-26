//! NPC model: identity, schedule lookups, and the test fixture.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Datelike, Timelike, Utc};

use super::*;

/// Process-local source of unique NPC grounding revisions.
///
/// The revision is deliberately transient: it distinguishes live incarnations
/// and grounding-affecting transitions, not authored or persisted game state.
static NEXT_GROUNDING_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_grounding_revision() -> u64 {
    NEXT_GROUNDING_REVISION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("NPC grounding revision counter exhausted")
}

/// Persisted fields used to restore an [`Npc`] without exposing its live
/// grounding lineage for mutation.
///
/// [`Npc::from_persisted_fields`] always mints a fresh process-local revision
/// and clears the schedule observation, so no caller can resurrect an old
/// async-grounding token.
#[derive(Debug, Clone)]
pub struct NpcPersistedFields {
    /// Unique NPC identifier.
    pub id: NpcId,
    /// Authored full name.
    pub name: String,
    /// Anonymous pre-introduction description.
    pub brief_description: String,
    /// Age in years.
    pub age: u8,
    /// Occupation or parish role.
    pub occupation: String,
    /// Personality prompt text.
    pub personality: String,
    /// Narration pronouns.
    pub pronouns: String,
    /// Multidimensional intelligence profile.
    pub intelligence: Intelligence,
    /// Canonical current location.
    pub location: LocationId,
    /// Current mood.
    pub mood: String,
    /// Home location.
    pub home: Option<LocationId>,
    /// Workplace location.
    pub workplace: Option<LocationId>,
    /// Authored seasonal schedule.
    pub schedule: Option<SeasonalSchedule>,
    /// Relationships to other NPCs.
    pub relationships: HashMap<NpcId, Relationship>,
    /// Short-term memory ring.
    pub memory: ShortTermMemory,
    /// Persistent long-term memories.
    pub long_term_memory: LongTermMemory,
    /// Authored knowledge entries.
    pub knowledge: Vec<String>,
    /// Present or in-transit state.
    pub state: NpcState,
    /// Last cognitive-tier deflation summary.
    pub deflated_summary: Option<NpcSummary>,
    /// Last Tier-3 activity summary.
    pub last_activity: Option<String>,
    /// Current illness flag.
    pub is_ill: bool,
    /// Scheduled death time, if any.
    pub doom: Option<DateTime<Utc>>,
    /// Whether the banshee has heralded the current doom.
    pub banshee_heralded: bool,
}

/// A non-player character in the game world.
///
/// Contains identity, personality, location, schedule, relationships,
/// and short-term memory. Cognition fidelity is determined by the
/// NpcManager based on distance from the player.
#[derive(Debug, Clone)]
pub struct Npc {
    /// Unique identifier.
    pub id: NpcId,
    /// Full name.
    pub name: String,
    /// Brief anonymous description shown before the player is introduced.
    ///
    /// E.g., "a priest", "a middle-aged woman", "an older man".
    pub brief_description: String,
    /// Age in years.
    pub age: u8,
    /// Occupation or role in the parish.
    pub occupation: String,
    /// Personality description used in system prompts.
    pub personality: String,
    /// Pronouns used when narrating this NPC (e.g. `he/him`, `she/her`,
    /// `they/them`). Defaults to `they/them` for NPCs that don't state them
    /// (#1026).
    pub pronouns: String,
    /// Multidimensional intelligence profile shaping dialogue generation.
    pub intelligence: Intelligence,
    /// Current location.
    pub(crate) location: LocationId,
    /// Current emotional state.
    pub mood: String,
    /// Home location (where the NPC sleeps).
    pub home: Option<LocationId>,
    /// Workplace location (where the NPC works).
    pub workplace: Option<LocationId>,
    /// Season- and day-aware schedule defining where the NPC goes.
    pub(crate) schedule: Option<SeasonalSchedule>,
    /// Relationships to other NPCs, keyed by their id.
    pub relationships: HashMap<NpcId, Relationship>,
    /// Ring buffer of recent memories.
    pub memory: ShortTermMemory,
    /// Persistent long-term memory with keyword-based retrieval.
    pub long_term_memory: LongTermMemory,
    /// Things this NPC knows (local gossip, history, etc.).
    pub knowledge: Vec<String>,
    /// Whether the NPC is present at their location or in transit.
    pub(crate) state: NpcState,
    /// Transient process-local lineage token for asynchronous state grounding.
    ///
    /// Refreshed whenever location, transit state, or authored schedule data
    /// changes, and whenever an NPC enters a manager (including save restore).
    /// This is intentionally excluded from persisted snapshots.
    pub(crate) grounding_revision: u64,
    /// Last authored-activity fingerprint observed by the schedule tick.
    ///
    /// `None` means the live NPC has not yet been synchronized at the current
    /// clock context. This is transient process state and is never persisted.
    pub(crate) observed_activity_fingerprint: Option<u64>,
    /// Compact summary from the last tier deflation, if any.
    ///
    /// Set when the NPC drops to a lower cognitive tier; cleared when
    /// they are inflated back to a higher tier.
    pub deflated_summary: Option<NpcSummary>,
    /// Log of recent player reactions (emoji) toward this NPC.
    pub reaction_log: ReactionLog,
    /// Last activity summary from Tier 3 batch simulation.
    ///
    /// Used in deflated context and Tier 3 prompt construction.
    /// Updated each time a Tier 3 tick processes this NPC.
    pub last_activity: Option<String>,
    /// Whether the NPC is currently ill. Set by Tier 4 rules engine.
    pub is_ill: bool,
    /// Game-time at which this NPC is fated to die, if set.
    ///
    /// Populated by the Tier 4 rules engine when it rolls a `Death` event —
    /// rather than removing the NPC immediately, the doom is scheduled a few
    /// game-hours ahead so that [`crate::banshee`] can herald it with a
    /// keening cry on the night beforehand. Cleared on removal.
    pub doom: Option<chrono::DateTime<chrono::Utc>>,
    /// `true` once the banshee's cry has been emitted for the current [`Self::doom`].
    ///
    /// Prevents the same wail from being produced on every tick while the
    /// doom window is open. Reset to `false` whenever [`Self::doom`] changes.
    pub banshee_heralded: bool,
}

impl Npc {
    /// Restores persisted gameplay state into a fresh live NPC incarnation.
    ///
    /// Transient reaction history and all async-grounding state deliberately
    /// start fresh.
    pub fn from_persisted_fields(fields: NpcPersistedFields) -> Self {
        let NpcPersistedFields {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            deflated_summary,
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
        } = fields;

        Self {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            grounding_revision: next_grounding_revision(),
            observed_activity_fingerprint: None,
            deflated_summary,
            reaction_log: ReactionLog::default(),
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
        }
    }

    /// Captures every persisted field while deliberately excluding transient
    /// reaction and async-grounding state.
    pub fn persisted_fields(&self) -> NpcPersistedFields {
        let Self {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            grounding_revision: _,
            observed_activity_fingerprint: _,
            deflated_summary,
            reaction_log: _,
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
        } = self;

        NpcPersistedFields {
            id: *id,
            name: name.clone(),
            brief_description: brief_description.clone(),
            age: *age,
            occupation: occupation.clone(),
            personality: personality.clone(),
            pronouns: pronouns.clone(),
            intelligence: *intelligence,
            location: *location,
            mood: mood.clone(),
            home: *home,
            workplace: *workplace,
            schedule: schedule.clone(),
            relationships: relationships.clone(),
            memory: memory.clone(),
            long_term_memory: long_term_memory.clone(),
            knowledge: knowledge.clone(),
            state: state.clone(),
            deflated_summary: deflated_summary.clone(),
            last_activity: last_activity.clone(),
            is_ill: *is_ill,
            doom: *doom,
            banshee_heralded: *banshee_heralded,
        }
    }

    /// Creates a test NPC for Phase 1 development.
    ///
    /// Padraig O'Brien is a 58-year-old publican at The Crossroads,
    /// known for his storytelling and dry wit.
    pub fn new_test_npc() -> Self {
        Self {
            id: NpcId(1),
            name: "Padraig O'Brien".to_string(),
            brief_description: "an older man behind the bar".to_string(),
            age: 58,
            occupation: "Publican".to_string(),
            personality: "A gruff but warm-hearted publican who has run the crossroads \
                pub for thirty years. Known for his dry wit, encyclopedic knowledge of \
                local history, and tendency to offer unsolicited advice. He speaks with \
                a thick Roscommon accent and peppers his speech with Irish phrases."
                .to_string(),
            pronouns: "he/him".to_string(),
            intelligence: Intelligence::new(3, 3, 4, 4, 5, 4),
            location: LocationId(1),
            mood: "content".to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
            grounding_revision: next_grounding_revision(),
            observed_activity_fingerprint: None,
            deflated_summary: None,
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    /// Returns the name to display to the player.
    ///
    /// Before the NPC is introduced, returns the brief anonymous description
    /// (e.g., "a priest"). After introduction, returns the full name.
    pub fn display_name(&self, introduced: bool) -> &str {
        if introduced {
            &self.name
        } else {
            &self.brief_description
        }
    }

    /// Returns the NPC's desired location based on their schedule and the current context.
    ///
    /// Returns `None` if the NPC has no schedule or no entry covers the hour.
    pub fn desired_location(
        &self,
        hour: u8,
        season: Season,
        day_type: DayType,
    ) -> Option<LocationId> {
        self.schedule.as_ref()?.location_at(hour, season, day_type)
    }

    /// Returns the active schedule entry for the current context.
    ///
    /// Returns `None` if the NPC has no schedule or no entry covers the hour.
    pub fn schedule_entry(
        &self,
        hour: u8,
        season: Season,
        day_type: DayType,
    ) -> Option<&types::ScheduleEntry> {
        self.schedule.as_ref()?.entry_at(hour, season, day_type)
    }

    /// Canonical current location.
    pub const fn location(&self) -> LocationId {
        self.location
    }

    /// Authored seasonal schedule, if any.
    pub fn schedule(&self) -> Option<&SeasonalSchedule> {
        self.schedule.as_ref()
    }

    /// Current present/transit state.
    pub const fn state(&self) -> &NpcState {
        &self.state
    }

    /// Current process-local async-grounding lineage token.
    pub const fn grounding_revision(&self) -> u64 {
        self.grounding_revision
    }

    /// Last schedule-interval fingerprint observed by the production tick.
    pub const fn observed_activity_fingerprint(&self) -> Option<u64> {
        self.observed_activity_fingerprint
    }

    /// Allocates a fresh grounding revision for trusted in-crate constructors.
    pub(crate) fn fresh_grounding_revision() -> u64 {
        next_grounding_revision()
    }

    /// Invalidates any asynchronous result grounded in this NPC's prior
    /// location, transit state, schedule, or live incarnation.
    pub(crate) fn refresh_grounding_revision(&mut self) {
        self.grounding_revision = next_grounding_revision();
    }

    pub(crate) fn authored_activity_entry_at(
        &self,
        hour: u8,
        season: Season,
        day_type: DayType,
    ) -> Option<&types::ScheduleEntry> {
        self.schedule_entry(hour, season, day_type)
            .filter(|entry| entry.location == self.location)
            .filter(|entry| !entry.activity.trim().is_empty())
    }

    pub(crate) fn canonical_activity_fingerprint_at(&self, game_time: DateTime<Utc>) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x00000100000001b3;

        let mut hash = FNV_OFFSET;
        let mut feed = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(FNV_PRIME);
        };

        let game_date = game_time.date_naive();
        let hour = game_time.hour() as u8;
        let season = Season::from_date(game_date);
        let day_type = DayType::from_date(game_date);

        feed(&self.id.0.to_le_bytes());
        feed(&self.location.0.to_le_bytes());
        match self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.entry_with_index_at(hour, season, day_type))
        {
            Some((variant_index, entry_index, entry)) => {
                // The after-midnight portion of a wrapping entry belongs to
                // the interval that started on the previous game date.
                let interval_date = if entry.start_hour > entry.end_hour && hour <= entry.end_hour {
                    game_date.pred_opt().unwrap_or(game_date)
                } else {
                    game_date
                };
                feed(b"authored-schedule-interval");
                feed(&interval_date.num_days_from_ce().to_le_bytes());
                feed(&(variant_index as u64).to_le_bytes());
                feed(&(entry_index as u64).to_le_bytes());
                feed(&[entry.start_hour, entry.end_hour]);
                feed(&entry.location.0.to_le_bytes());
                feed(entry.activity.trim().as_bytes());
                feed(&[u8::from(entry.cuaird)]);
            }
            None => {
                feed(b"no-authored-schedule-interval");
                feed(&game_date.num_days_from_ce().to_le_bytes());
                feed(&[hour]);
            }
        }

        hash
    }

    pub(crate) fn authored_activity_fingerprint_at(&self, game_time: DateTime<Utc>) -> u64 {
        self.canonical_activity_fingerprint_at(game_time)
    }

    /// Synchronizes the schedule-derived activity token for this clock
    /// context. Returns `true` only when an already-observed activity changed.
    pub(crate) fn observe_authored_activity_at(&mut self, game_time: DateTime<Utc>) -> bool {
        let current = self.authored_activity_fingerprint_at(game_time);
        match self.observed_activity_fingerprint {
            None => {
                self.observed_activity_fingerprint = Some(current);
                false
            }
            Some(previous) if previous != current => {
                self.observed_activity_fingerprint = Some(current);
                self.refresh_grounding_revision();
                true
            }
            Some(_) => false,
        }
    }

    pub(crate) fn authored_activity_observation_is_current(
        &self,
        game_time: DateTime<Utc>,
    ) -> bool {
        self.observed_activity_fingerprint == Some(self.authored_activity_fingerprint_at(game_time))
    }

    pub(crate) fn reset_authored_activity_observation(&mut self) {
        self.observed_activity_fingerprint = None;
    }

    /// Replaces the canonical location, refreshing grounding only on change.
    pub fn set_location(&mut self, location: LocationId) {
        if self.location != location {
            self.location = location;
            self.reset_authored_activity_observation();
            self.refresh_grounding_revision();
        }
    }

    /// Replaces the present/transit state, refreshing grounding only on change.
    pub fn set_state(&mut self, state: NpcState) {
        if self.state != state {
            self.state = state;
            self.reset_authored_activity_observation();
            self.refresh_grounding_revision();
        }
    }

    /// Atomically replaces location and present/transit state with one
    /// grounding refresh.
    pub fn set_location_and_state(&mut self, location: LocationId, state: NpcState) {
        if self.location != location || self.state != state {
            self.location = location;
            self.state = state;
            self.reset_authored_activity_observation();
            self.refresh_grounding_revision();
        }
    }

    /// Replaces authored schedule data, invalidating its activity anchor.
    pub fn set_schedule(&mut self, schedule: Option<SeasonalSchedule>) {
        if self.schedule != schedule {
            self.schedule = schedule;
            self.reset_authored_activity_observation();
            self.refresh_grounding_revision();
        }
    }
}
