//! Per-NPC familiarity tracking — "The Blow-In Arc".
//!
//! In 1820s rural Ireland a stranger ("a blow-in") is not embraced on the
//! first Sunday. Trust is earned slowly, in shared time rather than in
//! scripted quests. This module models that arc: every NPC accrues an
//! encounter counter as the player shares their location on successive
//! game-days, and that counter maps to a discrete [`FamiliarityTier`]
//! which the reaction system uses to pick warmer or cooler greetings.
//!
//! The counter advances **at most once per game-day per NPC** so the
//! payoff is measured in real game-time lived together, not in how many
//! times the player opens the inn door in a single afternoon.
//!
//! ## Reserve
//!
//! Some NPCs warm up faster than others. A warm publican moves from
//! "stranger" to "known" after a handful of visits; a gruff, suspicious
//! farmer needs twice that. The per-NPC [`Reserve`] value (derived from
//! personality keywords) scales the effective encounter count used for
//! tier lookups.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::NpcId;

/// Discrete familiarity tiers the reaction system can switch on.
///
/// Tiers are ordered from least to most familiar; the reaction system
/// treats higher tiers as unlocking warmer greeting templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FamiliarityTier {
    /// Never shared space on a previous game-day — a true blow-in.
    Stranger,
    /// Seen once or twice. NPC acknowledges but does not warm up yet.
    Acquaintance,
    /// Familiar face. NPC greets by name, makes small-talk.
    Known,
    /// Welcome presence. NPC invites the player to sit, to stay.
    Welcome,
    /// As if you belonged here. NPC treats you like family or a neighbour.
    Familiar,
}

impl FamiliarityTier {
    /// Returns a short label for debug output (`/standing`, snapshots, etc.).
    pub fn label(self) -> &'static str {
        match self {
            Self::Stranger => "stranger",
            Self::Acquaintance => "acquaintance",
            Self::Known => "known",
            Self::Welcome => "welcome",
            Self::Familiar => "familiar",
        }
    }
}

/// Encounter thresholds (exclusive upper bounds) for each tier transition.
///
/// With `reserve = 0` these are:
/// - 0 encounters → `Stranger`
/// - 1-2 → `Acquaintance`
/// - 3-7 → `Known`
/// - 8-14 → `Welcome`
/// - 15+ → `Familiar`
///
/// Tuned for a roughly week-to-month payoff at the parish visit cadence
/// observed during play-testing (~2–4 meaningful NPC encounters per game-day).
const ACQUAINTANCE_MIN: u16 = 1;
const KNOWN_MIN: u16 = 3;
const WELCOME_MIN: u16 = 8;
const FAMILIAR_MIN: u16 = 15;

/// How reserved an NPC is — slows familiarity payoff.
///
/// `0` means "warm from day one" (an innkeeper, a priest), `100` means
/// "tell me your grandmother's maiden name and I might nod" (a wary
/// farmer, a bitter old hermit). The effective encounter count used
/// for tier lookups is `encounters * 100 / (100 + reserve)`, so a
/// reserve-80 NPC needs roughly 1.8× as many shared days to reach the
/// same tier as a reserve-0 NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Reserve(pub u8);

impl Reserve {
    /// Default reserve for an unmarked NPC. A nudge above zero so that
    /// *most* villagers take at least a little while to thaw — the warm
    /// exceptions (publican, priest) earn their low reserve explicitly.
    pub const DEFAULT: Self = Self(30);

    /// Scales `encounters` downward by the reserve factor.
    pub fn scale(self, encounters: u16) -> u16 {
        let denom = 100u32 + self.0 as u32;
        ((encounters as u32 * 100) / denom) as u16
    }
}

impl Default for Reserve {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Heuristically derives a reserve value from an NPC's personality text
/// and occupation.
///
/// This is intentionally simple: the NPC data files do not yet carry an
/// explicit "reserve" field, so we scan the existing free-text
/// personality prose for a small vocabulary of warmth/guarded cues.
/// Occupation acts as a tie-breaker: publicans, priests, teachers,
/// healers, and shopkeepers all trend warm; farmers and labourers sit
/// around the default; and anyone described as "bitter", "wary",
/// "suspicious", or "proud" gets a bump toward the cautious end.
///
/// Matches are ASCII-case-insensitive.
pub fn derive_reserve(personality: &str, occupation: &str) -> Reserve {
    let p = personality.to_ascii_lowercase();
    let o = occupation.to_ascii_lowercase();

    let mut score: i32 = 30; // start at default

    // Warmth cues pull the score down (less reserved).
    for cue in [
        "warm",
        "welcoming",
        "hospitable",
        "cheerful",
        "jovial",
        "friendly",
        "generous",
        "open-hearted",
        "sociable",
        "hearty",
    ] {
        if p.contains(cue) {
            score -= 15;
        }
    }

    // Guarded cues push it up (more reserved).
    for cue in [
        "gruff",
        "wary",
        "suspicious",
        "bitter",
        "proud",
        "stern",
        "curt",
        "taciturn",
        "reserved",
        "sullen",
        "withdrawn",
        "aloof",
        "reticent",
    ] {
        if p.contains(cue) {
            score += 18;
        }
    }

    // Occupations that professionally meet strangers skew warm.
    if o.contains("publican")
        || o.contains("innkeeper")
        || o.contains("shopkeep")
        || o.contains("priest")
        || o.contains("curate")
        || o.contains("teacher")
        || o.contains("hedge")
        || o.contains("healer")
        || o.contains("bean feasa")
        || o.contains("midwife")
    {
        score -= 12;
    }

    // A few occupations are historically guarded toward outsiders.
    if o.contains("agent")
        || o.contains("constable")
        || o.contains("bailiff")
        || o.contains("magistrate")
    {
        score += 20;
    }

    Reserve(score.clamp(0, 100) as u8)
}

/// Per-NPC familiarity counter plus last-encounter bookkeeping.
///
/// `last_encounter_day` holds the ordinal game-day on which the counter
/// was last bumped so we can enforce the "at most one bump per day per
/// NPC" rule. `None` means the NPC has never encountered the player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Familiarity {
    /// Number of distinct game-days the player has shared space with this NPC.
    pub encounters: u16,
    /// Ordinal day (days since the Unix epoch) of the most recent bump.
    ///
    /// Used purely for the once-per-day gate; never exposed to the player.
    #[serde(default)]
    pub last_encounter_day: Option<i32>,
}

impl Familiarity {
    /// Returns the tier this counter maps to under the given reserve.
    pub fn tier(self, reserve: Reserve) -> FamiliarityTier {
        tier_for(self.encounters, reserve)
    }

    /// Attempts to register an encounter on the given ordinal day.
    ///
    /// Returns `true` if the counter advanced (i.e. this is the first
    /// encounter of a new game-day), `false` if the NPC was already
    /// credited for `day`.
    pub fn bump(&mut self, day: i32) -> bool {
        if self.last_encounter_day == Some(day) {
            return false;
        }
        self.last_encounter_day = Some(day);
        self.encounters = self.encounters.saturating_add(1);
        true
    }
}

/// Maps a raw encounter count plus reserve to a [`FamiliarityTier`].
pub fn tier_for(encounters: u16, reserve: Reserve) -> FamiliarityTier {
    let effective = reserve.scale(encounters);
    if effective >= FAMILIAR_MIN {
        FamiliarityTier::Familiar
    } else if effective >= WELCOME_MIN {
        FamiliarityTier::Welcome
    } else if effective >= KNOWN_MIN {
        FamiliarityTier::Known
    } else if effective >= ACQUAINTANCE_MIN {
        FamiliarityTier::Acquaintance
    } else {
        FamiliarityTier::Stranger
    }
}

/// Collection of per-NPC familiarity counters owned by the NPC manager.
///
/// Small wrapper over a `HashMap` so snapshot save/restore has one
/// stable type to round-trip.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamiliarityMap {
    #[serde(default)]
    entries: HashMap<NpcId, Familiarity>,
}

impl FamiliarityMap {
    /// Returns the [`Familiarity`] for `id`, or the default (stranger,
    /// never encountered) if none is recorded.
    pub fn get(&self, id: NpcId) -> Familiarity {
        self.entries.get(&id).copied().unwrap_or_default()
    }

    /// Sets the familiarity for `id` directly. Intended for snapshot
    /// restoration and tests, not gameplay.
    pub fn set(&mut self, id: NpcId, familiarity: Familiarity) {
        self.entries.insert(id, familiarity);
    }

    /// Attempts to bump the counter for `id` on the given ordinal day.
    ///
    /// See [`Familiarity::bump`] for the once-per-day semantics. Returns
    /// `true` if the counter advanced.
    pub fn bump(&mut self, id: NpcId, day: i32) -> bool {
        self.entries.entry(id).or_default().bump(day)
    }

    /// Total number of NPCs with any recorded familiarity.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has any entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all `(NpcId, Familiarity)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NpcId, Familiarity)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stranger_when_never_encountered() {
        let f = Familiarity::default();
        assert_eq!(f.tier(Reserve::DEFAULT), FamiliarityTier::Stranger);
    }

    #[test]
    fn bump_advances_once_per_day() {
        let mut f = Familiarity::default();
        assert!(f.bump(1));
        assert_eq!(f.encounters, 1);
        // Same day — no advance.
        assert!(!f.bump(1));
        assert_eq!(f.encounters, 1);
        // Next day — advance.
        assert!(f.bump(2));
        assert_eq!(f.encounters, 2);
    }

    #[test]
    fn saturates_at_u16_max() {
        let mut f = Familiarity {
            encounters: u16::MAX,
            last_encounter_day: Some(0),
        };
        assert!(f.bump(1));
        assert_eq!(f.encounters, u16::MAX);
    }

    #[test]
    fn reserve_zero_hits_tiers_on_canonical_boundaries() {
        let r = Reserve(0);
        assert_eq!(tier_for(0, r), FamiliarityTier::Stranger);
        assert_eq!(tier_for(1, r), FamiliarityTier::Acquaintance);
        assert_eq!(tier_for(2, r), FamiliarityTier::Acquaintance);
        assert_eq!(tier_for(3, r), FamiliarityTier::Known);
        assert_eq!(tier_for(7, r), FamiliarityTier::Known);
        assert_eq!(tier_for(8, r), FamiliarityTier::Welcome);
        assert_eq!(tier_for(14, r), FamiliarityTier::Welcome);
        assert_eq!(tier_for(15, r), FamiliarityTier::Familiar);
        assert_eq!(tier_for(200, r), FamiliarityTier::Familiar);
    }

    #[test]
    fn high_reserve_slows_the_climb() {
        // reserve 100 halves the effective count.
        let r = Reserve(100);
        assert_eq!(tier_for(1, r), FamiliarityTier::Stranger);
        assert_eq!(tier_for(5, r), FamiliarityTier::Acquaintance);
        assert_eq!(tier_for(15, r), FamiliarityTier::Known);
        assert_eq!(tier_for(30, r), FamiliarityTier::Familiar);
    }

    #[test]
    fn derive_reserve_warm_occupation_is_low() {
        let r = derive_reserve("warm-hearted publican with a ready laugh", "Publican");
        assert!(
            r.0 < 20,
            "expected warm publican to be low reserve, got {r:?}"
        );
    }

    #[test]
    fn derive_reserve_gruff_farmer_is_high() {
        let r = derive_reserve(
            "A gruff, suspicious farmer who is wary of outsiders and speaks with curt bitterness.",
            "Farmer",
        );
        assert!(
            r.0 > 60,
            "expected gruff farmer to be high reserve, got {r:?}"
        );
    }

    #[test]
    fn derive_reserve_bailiff_is_very_high() {
        let r = derive_reserve("a stern agent of the landlord", "Bailiff");
        assert!(r.0 > 60, "expected bailiff to be high reserve, got {r:?}");
    }

    #[test]
    fn familiarity_map_bump_respects_once_per_day() {
        let mut map = FamiliarityMap::default();
        let id = NpcId(42);
        assert!(map.bump(id, 10));
        assert!(!map.bump(id, 10));
        assert_eq!(map.get(id).encounters, 1);
        assert!(map.bump(id, 11));
        assert_eq!(map.get(id).encounters, 2);
    }

    #[test]
    fn tier_labels_are_stable_strings() {
        assert_eq!(FamiliarityTier::Stranger.label(), "stranger");
        assert_eq!(FamiliarityTier::Familiar.label(), "familiar");
    }

    #[test]
    fn reserve_scales_correctly() {
        // 10 encounters at reserve 0 => 10
        assert_eq!(Reserve(0).scale(10), 10);
        // 10 encounters at reserve 100 => 5
        assert_eq!(Reserve(100).scale(10), 5);
        // 0 encounters stays 0
        assert_eq!(Reserve(80).scale(0), 0);
    }
}
