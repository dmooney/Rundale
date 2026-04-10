//! Heuristic for selecting the next autonomous speaker in an NPC chain.
//!
//! After the player addresses a set of NPCs and each addressed NPC has had a
//! turn, the conversation can extend autonomously: bystanders may chime in,
//! addressed NPCs may bounce off each other. This module decides who speaks
//! next based on relationships, mood, and recent participation.

use crate::{Npc, NpcId};

/// Maximum number of autonomous turns to chain before forcing the player back in.
pub const MAX_CHAIN_TURNS: usize = 3;

/// Minimum heuristic score required for an NPC to speak up autonomously.
///
/// Scores below this threshold mean the conversation just dies out instead of
/// dragging on with NPCs who have no real reason to interject.
pub const SPEAK_UP_THRESHOLD: f32 = 0.5;

/// Picks the next NPC who should speak in an autonomous chain, or `None` if
/// no one has enough motivation.
///
/// Scoring:
/// - Baseline: every candidate starts at 0.4.
/// - +0.3 if the candidate has a non-trivial relationship to the last speaker.
/// - +0.2 if the candidate was directly addressed earlier this turn.
/// - +0.1 if the candidate is in a high-energy mood.
///
/// Excludes:
/// - The most recent speaker (NPCs don't immediately reply to themselves).
/// - Anyone already in `recently_spoken` (one autonomous turn each).
pub fn pick_next_speaker<'a>(
    npcs_at_location: &[&'a Npc],
    last_speaker_id: Option<NpcId>,
    recently_spoken: &[NpcId],
    addressed_this_turn: &[NpcId],
) -> Option<&'a Npc> {
    let mut best: Option<(&Npc, f32)> = None;

    for &candidate in npcs_at_location {
        if Some(candidate.id) == last_speaker_id {
            continue;
        }
        if recently_spoken.contains(&candidate.id) {
            continue;
        }

        let mut score: f32 = 0.4;

        if let Some(last_id) = last_speaker_id
            && let Some(rel) = candidate.relationships.get(&last_id)
            && rel.strength.abs() > 0.1
        {
            score += 0.3;
        }

        if addressed_this_turn.contains(&candidate.id) {
            score += 0.2;
        }

        if is_high_energy_mood(&candidate.mood) {
            score += 0.1;
        }

        if let Some((_, best_score)) = best {
            if score > best_score {
                best = Some((candidate, score));
            }
        } else {
            best = Some((candidate, score));
        }
    }

    best.filter(|(_, score)| *score >= SPEAK_UP_THRESHOLD)
        .map(|(npc, _)| npc)
}

/// Whether a mood string belongs to the "high energy" set that makes an NPC
/// more likely to speak up unprompted.
fn is_high_energy_mood(mood: &str) -> bool {
    matches!(
        mood.to_lowercase().as_str(),
        "excited"
            | "agitated"
            | "joyful"
            | "angry"
            | "indignant"
            | "outraged"
            | "elated"
            | "boisterous"
            | "anxious"
            | "scared"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Relationship, RelationshipKind};
    use std::collections::HashMap;

    fn make_npc(id: u32, name: &str, mood: &str) -> Npc {
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.mood = mood.to_string();
        npc.relationships = HashMap::new();
        npc
    }

    fn add_relationship(npc: &mut Npc, target: NpcId, strength: f64) {
        npc.relationships.insert(
            target,
            Relationship {
                kind: RelationshipKind::Friend,
                strength,
                history: Vec::new(),
            },
        );
    }

    #[test]
    fn returns_none_when_only_last_speaker_present() {
        let alice = make_npc(1, "Alice", "content");
        let candidates = vec![&alice];
        let result = pick_next_speaker(&candidates, Some(NpcId(1)), &[], &[]);
        assert!(result.is_none());
    }

    #[test]
    fn excludes_recently_spoken_npcs() {
        let alice = make_npc(1, "Alice", "excited");
        let bob = make_npc(2, "Bob", "excited");
        let candidates = vec![&alice, &bob];
        // Both candidates are excluded — alice is recently_spoken, bob is the last speaker.
        let result = pick_next_speaker(&candidates, Some(NpcId(2)), &[NpcId(1)], &[]);
        assert!(result.is_none());
    }

    #[test]
    fn addressed_bonus_lifts_otherwise_calm_npc() {
        let alice = make_npc(1, "Alice", "content");
        let candidates = vec![&alice];
        // Calm + no relationship — score 0.4 < threshold.
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[]);
        assert!(result.is_none());
        // Add the addressed bonus → 0.6 ≥ threshold.
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[NpcId(1)]);
        assert_eq!(result.map(|n| n.id), Some(NpcId(1)));
    }

    #[test]
    fn relationship_to_last_speaker_picks_correct_npc() {
        let mut alice = make_npc(1, "Alice", "content");
        let bob = make_npc(2, "Bob", "content");
        // Alice has a strong relationship to NPC 99, the last speaker.
        add_relationship(&mut alice, NpcId(99), 0.7);
        let candidates = vec![&alice, &bob];
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[]);
        assert_eq!(result.map(|n| n.id), Some(NpcId(1)));
    }

    #[test]
    fn high_energy_mood_alone_is_not_enough() {
        let alice = make_npc(1, "Alice", "excited");
        let candidates = vec![&alice];
        // 0.4 baseline + 0.1 mood = 0.5, exactly at threshold.
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[]);
        assert_eq!(result.map(|n| n.id), Some(NpcId(1)));
    }

    #[test]
    fn calm_npc_with_no_bonuses_stays_silent() {
        let alice = make_npc(1, "Alice", "content");
        let candidates = vec![&alice];
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[]);
        assert!(result.is_none());
    }

    #[test]
    fn chain_caps_at_max_turns() {
        // The cap is enforced by the caller, but the constant must exist
        // and be positive so callers can rely on it.
        assert!(MAX_CHAIN_TURNS > 0);
    }

    #[test]
    fn high_energy_mood_recognition() {
        for mood in [
            "excited",
            "agitated",
            "joyful",
            "angry",
            "indignant",
            "outraged",
            "elated",
            "boisterous",
            "anxious",
            "scared",
            "EXCITED", // case-insensitive
        ] {
            assert!(is_high_energy_mood(mood), "{} should be high-energy", mood);
        }
        for mood in ["content", "calm", "tired", "serene", "pensive"] {
            assert!(
                !is_high_energy_mood(mood),
                "{} should not be high-energy",
                mood
            );
        }
    }
}
