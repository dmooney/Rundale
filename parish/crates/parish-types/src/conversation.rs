//! Conversation history tracking for NPC scene awareness.
//!
//! Stores recent player–NPC exchanges per location so that NPCs
//! can reference what was just said, maintaining conversational
//! continuity and scene awareness.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

use crate::ids::{LocationId, NpcId};

/// Supported machine-comparable object attribute kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RememberedObjectAttributeKind {
    Material,
    Colour,
    Marking,
}

impl std::fmt::Display for RememberedObjectAttributeKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Material => "material",
            Self::Colour => "colour",
            Self::Marking => "marking",
        })
    }
}

/// One player-established attribute of a concrete object discussed with an NPC.
/// Free-form model output never populates this structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedObjectAttribute {
    pub kind: RememberedObjectAttributeKind,
    pub value: String,
}

/// Bounded, durable object facts established by the player in conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedObjectFact {
    pub speaker_id: NpcId,
    pub location: LocationId,
    pub label: String,
    pub attributes: Vec<RememberedObjectAttribute>,
}

/// Maximum number of exchanges retained globally.
const LOG_CAPACITY: usize = 30;

/// Monotonic position immediately after an exchange in a [`ConversationLog`].
///
/// Capture a cursor before dispatching a turn, then pass it to
/// [`ConversationLog::exchanges_since`] to read only the canonical exchanges
/// recorded by that turn. Unlike a ring-buffer length, this keeps advancing
/// after the log reaches capacity and wraps.
pub type ConversationCursor = u64;

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
    /// NPCs the player has spoken with at least once during this save.
    ///
    /// This is deliberately independent of the bounded exchange ring:
    /// evicting old dialogue must not make a later meeting look like first
    /// contact again (#1786). Legacy snapshots recover contacts still present
    /// in `exchanges` through [`Self::has_exchange_with`].
    #[serde(default)]
    contacted_speakers: BTreeSet<NpcId>,
    /// Lifetime count of exchanges added to this log.
    ///
    /// `serde(default)` keeps snapshots written before the cursor was added
    /// loadable. [`Self::cursor`] normalises such legacy values to at least
    /// the number of retained exchanges.
    #[serde(default)]
    total_exchanges: ConversationCursor,
    /// Player-authored object attributes used for conservative factual
    /// continuity. This is separate from prose memories so model text can
    /// never rewrite a canonical attribute.
    #[serde(default)]
    remembered_objects: VecDeque<RememberedObjectFact>,
}

impl ConversationLog {
    /// Creates an empty conversation log.
    pub fn new() -> Self {
        Self {
            exchanges: VecDeque::with_capacity(LOG_CAPACITY),
            contacted_speakers: BTreeSet::new(),
            total_exchanges: 0,
            remembered_objects: VecDeque::new(),
        }
    }

    /// Merge one player-established object fact into the bounded durable set.
    pub fn remember_object_fact(&mut self, fact: RememberedObjectFact) {
        const MAX_OBJECT_FACTS: usize = 16;
        if let Some(existing) = self.remembered_objects.iter_mut().find(|existing| {
            existing.speaker_id == fact.speaker_id
                && existing.location == fact.location
                && existing.label.eq_ignore_ascii_case(&fact.label)
        }) {
            for attribute in fact.attributes {
                if let Some(prior) = existing
                    .attributes
                    .iter_mut()
                    .find(|prior| prior.kind == attribute.kind)
                {
                    *prior = attribute;
                } else {
                    existing.attributes.push(attribute);
                }
            }
            return;
        }
        if self.remembered_objects.len() >= MAX_OBJECT_FACTS {
            self.remembered_objects.pop_front();
        }
        self.remembered_objects.push_back(fact);
    }

    /// Facts established with one speaker at one location, oldest first.
    pub fn remembered_object_facts(
        &self,
        speaker_id: NpcId,
        location: LocationId,
    ) -> Vec<&RememberedObjectFact> {
        self.remembered_objects
            .iter()
            .filter(|fact| fact.speaker_id == speaker_id && fact.location == location)
            .collect()
    }

    /// Records a new exchange, evicting the oldest if at capacity.
    pub fn add(&mut self, exchange: ConversationExchange) {
        // A legacy snapshot has no `total_exchanges` field. Rebase its cursor
        // to the retained length before advancing so the first post-load delta
        // does not accidentally include the whole historical buffer.
        self.total_exchanges = self.cursor().saturating_add(1);
        // Likewise, hydrate the durable contact set from a legacy snapshot's
        // retained ring before an old exchange can be evicted.
        self.contacted_speakers
            .extend(self.exchanges.iter().map(|retained| retained.speaker_id));
        self.contacted_speakers.insert(exchange.speaker_id);
        if self.exchanges.len() >= LOG_CAPACITY {
            self.exchanges.pop_front();
        }
        self.exchanges.push_back(exchange);
    }

    /// Returns the monotonic position immediately after the newest exchange.
    ///
    /// Legacy snapshots that predate `total_exchanges` deserialize it as zero;
    /// using the retained length as a floor gives those logs a valid starting
    /// cursor without a custom deserializer.
    pub fn cursor(&self) -> ConversationCursor {
        self.total_exchanges.max(self.exchanges.len() as u64)
    }

    /// Returns canonical exchanges added at or after `cursor`, oldest first.
    ///
    /// If the requested cursor is older than the retained ring window, all
    /// retained exchanges are returned. If it is ahead of the current cursor
    /// (for example, a cursor from a prior new game), it is treated as stale
    /// and the retained exchanges are returned from the start.
    pub fn exchanges_since(&self, cursor: ConversationCursor) -> Vec<&ConversationExchange> {
        let current = self.cursor();
        let retained = self.exchanges.len() as u64;
        let oldest_cursor = current.saturating_sub(retained);
        let skip = if cursor > current {
            0
        } else {
            cursor.saturating_sub(oldest_cursor).min(retained) as usize
        };
        self.exchanges.iter().skip(skip).collect()
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

    /// Checks whether this save has ever recorded an exchange with `speaker_id`.
    ///
    /// Contact history is person-scoped, not place-scoped: meeting an NPC at
    /// the farm still means a later encounter at the crossroads is not first
    /// contact (#1786).
    pub fn has_exchange_with(&self, speaker_id: NpcId) -> bool {
        self.contacted_speakers.contains(&speaker_id)
            || self
                .exchanges
                .iter()
                .any(|exchange| exchange.speaker_id == speaker_id)
    }

    /// Formats recent conversation history at a location for prompt injection.
    ///
    /// `current_npc_id` is the NPC being prompted — their own lines are
    /// phrased as "You:" while others' lines use "{Name}:".
    /// `player_label` is the name to use for the player's lines ("the newcomer"
    /// when unknown, or the player's actual name when introduced).
    pub fn context_string(
        &self,
        location: LocationId,
        current_npc_id: NpcId,
        player_label: &str,
        n: usize,
    ) -> String {
        let recent = self.recent_at(location, n);
        if recent.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(recent.len());
        for exchange in &recent {
            let time = exchange.timestamp.format("%H:%M");
            let npc_label = if exchange.speaker_id == current_npc_id {
                "You".to_string()
            } else {
                exchange.speaker_name.clone()
            };

            lines.push(format!(
                "- [{}] {}: \"{}\"\n  {}: \"{}\"",
                time, player_label, exchange.player_input, npc_label, exchange.npc_dialogue,
            ));
        }
        lines.join("\n")
    }

    /// Returns the last `n` dialogue lines spoken by `npc_id` at `location`.
    ///
    /// Oldest first. Used to build the anti-phrase-recycling prompt block
    /// (#1387): feeds the NPC's own recent lines back as a "do not repeat"
    /// list so the model cannot recycle verbatim phrases from earlier turns
    /// that fall outside the short conversation-history window.
    pub fn npc_prior_lines(&self, location: LocationId, npc_id: NpcId, n: usize) -> Vec<&str> {
        self.exchanges
            .iter()
            .filter(|e| e.location == location && e.speaker_id == npc_id)
            .rev()
            .take(n)
            .map(|e| e.npc_dialogue.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Returns the last `n` dialogue lines spoken at `location` by speakers
    /// *other than* `npc_id`, oldest first.
    ///
    /// Used to build the cross-NPC crutch-phrase suppression block (#1422):
    /// small models reach for an identical opener frame ("ye've come to the
    /// right place") across consecutive *different* NPCs in a session. The
    /// per-NPC anti-recycling guard ([`npc_prior_lines`](Self::npc_prior_lines))
    /// cannot catch a frame shared *across* NPCs, so this feeds other speakers'
    /// recent lines back as a "do not reuse these frames" list.
    pub fn other_npcs_recent_lines(
        &self,
        location: LocationId,
        npc_id: NpcId,
        n: usize,
    ) -> Vec<&str> {
        let mut lines: Vec<&str> = self
            .exchanges
            .iter()
            .filter(|e| {
                e.location == location && e.speaker_id != npc_id && e.speaker_id != NpcId(0)
            })
            .rev()
            .take(n)
            .map(|e| e.npc_dialogue.as_str())
            .collect();
        lines.reverse();
        lines
    }

    /// Returns the number of exchanges involving `npc_id` at `location`.
    ///
    /// Used to determine NPC–player familiarity level for address vocabulary
    /// selection (#1388): once sufficient prior turns exist, "stranger" is
    /// no longer an appropriate form of address.
    pub fn exchange_count_with(&self, location: LocationId, npc_id: NpcId) -> usize {
        self.exchanges
            .iter()
            .filter(|e| e.location == location && e.speaker_id == npc_id)
            .count()
    }

    /// Returns the maximum number of exchanges the log retains.
    ///
    /// Useful as the `n` argument to [`recent_at`](Self::recent_at) when a
    /// caller wants to scan the entire retained buffer (e.g. to find an NPC's
    /// own most recent line for the anti-repetition guard, #1228).
    pub const fn capacity() -> usize {
        LOG_CAPACITY
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
    pub fn all(
        &self,
    ) -> impl DoubleEndedIterator<Item = &ConversationExchange> + ExactSizeIterator {
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
    fn contact_history_follows_npc_across_locations() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(8, 1, "Padraig", "Hello", "Dia dhuit!", 7));

        assert!(log.has_exchange_with(NpcId(1)));
        assert!(!log.has_exchange_with(NpcId(2)));
        assert!(
            !log.has_recent_exchange_with(LocationId(9), NpcId(1), 2),
            "location-scoped continuity remains separate from person-scoped contact"
        );
    }

    #[test]
    fn contact_history_survives_exchange_ring_eviction_and_serde() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(
            8,
            77,
            "Earlier acquaintance",
            "Hello",
            "Good day.",
            7,
        ));
        for index in 0..LOG_CAPACITY {
            log.add(make_exchange(
                9,
                1,
                "Padraig",
                &format!("Later message {index}"),
                "Reply",
                1,
            ));
        }

        assert!(
            log.all().all(|exchange| exchange.speaker_id != NpcId(77)),
            "the acquaintance's exchange should have left the bounded ring"
        );
        assert!(log.has_exchange_with(NpcId(77)));

        let restored: ConversationLog =
            serde_json::from_str(&serde_json::to_string(&log).unwrap()).unwrap();
        assert!(restored.has_exchange_with(NpcId(77)));
    }

    #[test]
    fn legacy_retained_contact_is_hydrated_before_eviction() {
        let legacy = serde_json::json!({
            "exchanges": [
                make_exchange(
                    8,
                    77,
                    "Earlier acquaintance",
                    "Hello",
                    "Good day.",
                    7
                )
            ]
        });
        let mut log: ConversationLog = serde_json::from_value(legacy).unwrap();

        for index in 0..LOG_CAPACITY {
            log.add(make_exchange(
                9,
                1,
                "Padraig",
                &format!("Later message {index}"),
                "Reply",
                1,
            ));
        }

        assert!(log.all().all(|exchange| exchange.speaker_id != NpcId(77)));
        assert!(log.has_exchange_with(NpcId(77)));
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
    fn capacity_eviction_drops_oldest_and_preserves_order() {
        let mut log = ConversationLog::new();
        for i in 0..LOG_CAPACITY + 5 {
            log.add(make_exchange(
                8,
                1,
                "Padraig",
                &format!("msg {i}"),
                "reply",
                1,
            ));
        }

        let inputs: Vec<&str> = log
            .all()
            .map(|exchange| exchange.player_input.as_str())
            .collect();

        assert_eq!(inputs.len(), LOG_CAPACITY);
        assert_eq!(inputs.first(), Some(&"msg 5"));
        assert_eq!(inputs.last(), Some(&"msg 34"));
    }

    #[test]
    fn cursor_delta_keeps_advancing_after_ring_wrap() {
        let mut log = ConversationLog::new();
        for i in 0..LOG_CAPACITY {
            log.add(make_exchange(
                8,
                1,
                "Padraig",
                &format!("msg {i}"),
                "reply",
                1,
            ));
        }
        let before = log.cursor();
        assert_eq!(before, LOG_CAPACITY as u64);

        // The retained length stays fixed at capacity, but the monotonic
        // cursor advances and the delta contains exactly the new exchange.
        log.add(make_exchange(9, 2, "Niamh", "new turn", "new reply", 1));

        assert_eq!(log.len(), LOG_CAPACITY);
        assert_eq!(log.cursor(), before + 1);
        let delta = log.exchanges_since(before);
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].player_input, "new turn");
        assert_eq!(delta[0].npc_dialogue, "new reply");
    }

    #[test]
    fn cursor_older_than_retained_window_returns_retained_history() {
        let mut log = ConversationLog::new();
        for i in 0..LOG_CAPACITY + 5 {
            log.add(make_exchange(
                8,
                1,
                "Padraig",
                &format!("msg {i}"),
                "reply",
                1,
            ));
        }

        let retained = log.exchanges_since(0);
        assert_eq!(retained.len(), LOG_CAPACITY);
        assert_eq!(retained[0].player_input, "msg 5");
        assert_eq!(retained.last().unwrap().player_input, "msg 34");
    }

    #[test]
    fn legacy_serialized_log_rebases_cursor_before_next_add() {
        let legacy = serde_json::json!({
            "exchanges": [
                make_exchange(8, 1, "Padraig", "first", "reply 1", 1),
                make_exchange(9, 2, "Niamh", "second", "reply 2", 1)
            ]
        });
        let mut log: ConversationLog = serde_json::from_value(legacy).unwrap();

        assert_eq!(log.cursor(), 2);
        let before = log.cursor();
        log.add(make_exchange(10, 1, "Padraig", "third", "reply 3", 1));

        assert_eq!(log.cursor(), 3);
        let delta = log.exchanges_since(before);
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].player_input, "third");
    }

    #[test]
    fn conversation_log_serde_round_trip_preserves_chronological_order() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(8, 1, "Padraig", "first", "reply 1", 1));
        log.add(make_exchange(9, 2, "Niamh", "second", "reply 2", 2));
        log.add(make_exchange(10, 1, "Padraig", "third", "reply 3", 1));

        let json = serde_json::to_string(&log).unwrap();
        let restored: ConversationLog = serde_json::from_str(&json).unwrap();
        let inputs: Vec<&str> = restored
            .all()
            .map(|exchange| exchange.player_input.as_str())
            .collect();

        assert_eq!(inputs, vec!["first", "second", "third"]);
        assert_eq!(restored, log);
    }

    #[test]
    fn remembered_object_facts_merge_by_scope_and_survive_serde() {
        let mut log = ConversationLog::new();
        log.remember_object_fact(RememberedObjectFact {
            speaker_id: NpcId(1),
            location: LocationId(2),
            label: "ribbon".to_string(),
            attributes: vec![RememberedObjectAttribute {
                kind: RememberedObjectAttributeKind::Material,
                value: "wool".to_string(),
            }],
        });
        log.remember_object_fact(RememberedObjectFact {
            speaker_id: NpcId(1),
            location: LocationId(2),
            label: "Ribbon".to_string(),
            attributes: vec![RememberedObjectAttribute {
                kind: RememberedObjectAttributeKind::Colour,
                value: "red".to_string(),
            }],
        });
        log.remember_object_fact(RememberedObjectFact {
            speaker_id: NpcId(2),
            location: LocationId(2),
            label: "ribbon".to_string(),
            attributes: vec![RememberedObjectAttribute {
                kind: RememberedObjectAttributeKind::Material,
                value: "silk".to_string(),
            }],
        });

        let encoded = serde_json::to_string(&log).unwrap();
        let restored: ConversationLog = serde_json::from_str(&encoded).unwrap();
        let scoped = restored.remembered_object_facts(NpcId(1), LocationId(2));
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].attributes.len(), 2);
        assert!(scoped[0]
            .attributes
            .iter()
            .any(|attribute| attribute.kind == RememberedObjectAttributeKind::Material && attribute.value == "wool"));
        assert!(scoped[0]
            .attributes
            .iter()
            .any(|attribute| attribute.kind == RememberedObjectAttributeKind::Colour && attribute.value == "red"));
        assert_eq!(
            restored.remembered_object_facts(NpcId(2), LocationId(2))[0].attributes[0].value,
            "silk"
        );
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
        let ctx = log.context_string(LocationId(1), NpcId(1), "the newcomer", 5);
        assert!(ctx.contains("You:"), "got: {ctx}");
        assert!(ctx.contains("Niamh:"), "got: {ctx}");

        // From Niamh's perspective
        let ctx = log.context_string(LocationId(1), NpcId(2), "the newcomer", 5);
        assert!(ctx.contains("Padraig:"), "got: {ctx}");
        assert!(ctx.contains("You:"), "got: {ctx}");
    }

    #[test]
    fn test_context_string_uses_player_label() {
        let mut log = ConversationLog::new();
        log.add(make_exchange(
            8,
            1,
            "Padraig",
            "Hello there",
            "Dia dhuit!",
            1,
        ));

        let ctx = log.context_string(LocationId(1), NpcId(1), "Ciaran", 5);
        assert!(ctx.contains("Ciaran:"), "got: {ctx}");
        assert!(ctx.contains("Hello there"), "got: {ctx}");
    }

    #[test]
    fn test_context_string_empty() {
        let log = ConversationLog::new();
        assert_eq!(
            log.context_string(LocationId(1), NpcId(1), "the newcomer", 5),
            ""
        );
    }

    #[test]
    fn test_other_npcs_recent_lines_excludes_self_and_player() {
        let mut log = ConversationLog::new();
        // NPC 1 (self), NPC 2 (other), and player (id 0) all speak at loc 1.
        log.add(make_exchange(
            8,
            1,
            "Peig",
            "hi",
            "Ye've come to the right place.",
            1,
        ));
        log.add(make_exchange(
            9,
            2,
            "Roisin",
            "hi",
            "Ye've come to the right place too.",
            1,
        ));
        log.add(make_exchange(10, 0, "Player", "hi", "player line", 1));
        // Different location — must be excluded.
        log.add(make_exchange(11, 3, "Maire", "hi", "elsewhere line", 2));

        let lines = log.other_npcs_recent_lines(LocationId(1), NpcId(1), 6);
        assert_eq!(lines, vec!["Ye've come to the right place too."]);
        assert!(
            !lines.iter().any(|l| l.contains("player line")),
            "player (id 0) lines must be excluded: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("elsewhere")),
            "other-location lines must be excluded: {lines:?}"
        );
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
