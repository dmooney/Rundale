//! Heuristic for selecting the next autonomous speaker in an NPC chain.
//!
//! After the player addresses a set of NPCs and each addressed NPC has had a
//! turn, the conversation can extend autonomously: bystanders may chime in,
//! addressed NPCs may bounce off each other. This module decides who speaks
//! next based on relationships, emotional state, and recent participation.
//!
//! The emotion-aware scoring is where the paper's non-linear gates land in
//! behaviour (not just in dialogue text): `public_outburst` and `effusive`
//! states amplify an NPC's score, while `withdraws_silent` suppresses it.

use crate::{Npc, NpcId};
use parish_types::EmotionState;

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
/// - +0.1 if the candidate is in a high-arousal emotional state.
/// - +0.15 if the `public_outburst` gate is active (moderate anger) OR
///   the `effusive` gate is active (high joy / strong affection at arousal).
/// - -0.4 if the `withdraws_silent` gate is active (high sadness/shame) —
///   usually enough to drop the NPC below the speak-up threshold entirely.
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
    // Back-compat wrapper — defaults emotion gates to on, matching
    // the shipping default of `NpcConfig::emotions_enabled`.
    pick_next_speaker_with_config(
        npcs_at_location,
        last_speaker_id,
        recently_spoken,
        addressed_this_turn,
        true,
    )
}

/// Flag-aware variant of [`pick_next_speaker`].
///
/// When `emotions_enabled` is true, the full gate-aware scoring is
/// used ([`energy_bonus`] including `public_outburst`, `effusive`,
/// and `withdraws_silent` contributions). When false, only the
/// arousal component applies — matching the approximate behaviour of
/// the old string-matching `is_high_energy_mood` without the gate
/// penalties that can suppress a grief-struck NPC.
pub fn pick_next_speaker_with_config<'a>(
    npcs_at_location: &[&'a Npc],
    last_speaker_id: Option<NpcId>,
    recently_spoken: &[NpcId],
    addressed_this_turn: &[NpcId],
    emotions_enabled: bool,
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

        score += energy_bonus(&candidate.emotion, emotions_enabled);

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

/// Emotion-derived score bonus (or penalty) for an NPC's likelihood
/// to speak up unprompted.
///
/// When `use_gates` is false, only the arousal component fires —
/// preserving approximate legacy behaviour under the `emotions`
/// kill-switch. When true, all three contributions apply:
/// - +0.1 for high arousal (the classic "activation" component of PAD)
/// - +0.15 for `public_outburst` or `effusive` gates
/// - -0.4 for `withdraws_silent`
///
/// Net with gates on: a simmering-angry NPC gets +0.25 (visibly more
/// likely to interject), a grief-struck NPC gets -0.3 to -0.4
/// (unlikely to chime in unless directly addressed).
pub fn energy_bonus(emotion: &EmotionState, use_gates: bool) -> f32 {
    let mut bonus = 0.0;

    if emotion.arousal > 0.3 {
        bonus += 0.1;
    }
    if use_gates {
        let gates = emotion.gates();
        if gates.public_outburst || gates.effusive {
            bonus += 0.15;
        }
        if gates.withdraws_silent {
            bonus -= 0.4;
        }
    }

    bonus
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
        // Keep the structured emotion state in sync with the legacy
        // mood string so the speaker-selection heuristic — which now
        // reads from `emotion` — sees the intended affect instead of
        // new_test_npc's baseline "content" state.
        npc.emotion =
            parish_types::EmotionState::initial_from(&parish_types::Temperament::default(), mood);
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
        // Lock the actual value — if this number ever changes, every frontend
        // (tauri, server) must be reviewed because they read this constant.
        assert_eq!(
            MAX_CHAIN_TURNS, 3,
            "MAX_CHAIN_TURNS is a load-bearing constant — bump with care"
        );
    }

    /// Full simulation of the caller-side chain loop (matches the shape used
    /// by parish-tauri/commands.rs and parish-server/routes.rs):
    /// iterate `pick_next_speaker`, append to `recently_spoken`, bail on
    /// `None`, and break at `MAX_CHAIN_TURNS`. With enough high-energy,
    /// related NPCs present, the loop MUST terminate at exactly
    /// `MAX_CHAIN_TURNS` speakers — no more.
    #[test]
    fn chain_terminates_at_max_chain_turns_with_abundant_speakers() {
        // Build 6 NPCs so the speaker pool is strictly larger than the cap.
        // Each is high-energy AND has a strong mutual relationship to the
        // "initial" speaker (id 99) so their heuristic score is well above
        // threshold and the chain never dies from insufficient motivation.
        let mut npcs: Vec<Npc> = (1..=6)
            .map(|i| {
                let mut n = make_npc(i, &format!("NPC{i}"), "excited");
                add_relationship(&mut n, NpcId(99), 0.8);
                n
            })
            .collect();
        // Add cross-relationships so once the "initial" speaker is exhausted
        // as a relationship target, candidates still have ties to the most
        // recent speakers.
        for i in 0..npcs.len() {
            for j in 0..npcs.len() {
                if i != j {
                    let target = npcs[j].id;
                    add_relationship(&mut npcs[i], target, 0.7);
                }
            }
        }

        let candidates: Vec<&Npc> = npcs.iter().collect();
        let mut recently_spoken: Vec<NpcId> = Vec::new();
        let mut last_speaker = Some(NpcId(99));

        // Mirror the caller loop verbatim.
        for _ in 0..MAX_CHAIN_TURNS {
            let Some(next) = pick_next_speaker(&candidates, last_speaker, &recently_spoken, &[])
            else {
                break;
            };
            recently_spoken.push(next.id);
            last_speaker = Some(next.id);
        }

        assert_eq!(
            recently_spoken.len(),
            MAX_CHAIN_TURNS,
            "chain should fill to exactly MAX_CHAIN_TURNS speakers when candidates are abundant"
        );

        // Crucially: a *fourth* call beyond the loop cap would still succeed
        // if we ran it — the cap is what prevents runaway chatter, NOT the
        // heuristic. Verify that's still true so we don't accidentally shift
        // the enforcement boundary.
        let fourth = pick_next_speaker(&candidates, last_speaker, &recently_spoken, &[]);
        assert!(
            fourth.is_some(),
            "cap must be enforced by the loop, not by the heuristic — \
             with 6 strong candidates and only 3 spoken, a fourth should \
             still be available"
        );
    }

    /// Negative case: if nobody has a strong enough reason to speak, the
    /// chain must terminate EARLY — well before `MAX_CHAIN_TURNS`.
    #[test]
    fn chain_terminates_early_when_no_motivated_speakers() {
        // Three calm NPCs, no relationships, no addressed bonus — all score
        // 0.4, below threshold.
        let alice = make_npc(1, "Alice", "content");
        let bob = make_npc(2, "Bob", "content");
        let carol = make_npc(3, "Carol", "content");
        let candidates = vec![&alice, &bob, &carol];

        let mut recently_spoken: Vec<NpcId> = Vec::new();
        let mut last_speaker = Some(NpcId(99));
        let mut turns_taken = 0usize;

        for _ in 0..MAX_CHAIN_TURNS {
            let Some(next) = pick_next_speaker(&candidates, last_speaker, &recently_spoken, &[])
            else {
                break;
            };
            recently_spoken.push(next.id);
            last_speaker = Some(next.id);
            turns_taken += 1;
        }

        assert_eq!(
            turns_taken, 0,
            "calm unrelated NPCs should produce zero autonomous turns"
        );
    }

    /// Each NPC should only speak once per autonomous chain even if their
    /// score would otherwise make them the highest-ranked candidate on every
    /// iteration. This guards the `recently_spoken` exclusion.
    #[test]
    fn chain_does_not_repeat_speakers() {
        // Two high-energy NPCs, one calm. If `recently_spoken` weren't
        // honored, NPC 1 (highest score) would speak twice in a row.
        let mut alice = make_npc(1, "Alice", "excited");
        add_relationship(&mut alice, NpcId(99), 0.9);
        let mut bob = make_npc(2, "Bob", "excited");
        add_relationship(&mut bob, NpcId(99), 0.5);
        let candidates = vec![&alice, &bob];

        let mut recently_spoken: Vec<NpcId> = Vec::new();
        let mut last_speaker = Some(NpcId(99));

        for _ in 0..MAX_CHAIN_TURNS {
            let Some(next) = pick_next_speaker(&candidates, last_speaker, &recently_spoken, &[])
            else {
                break;
            };
            recently_spoken.push(next.id);
            last_speaker = Some(next.id);
        }

        // Only 2 unique speakers exist — chain must cap at 2, not loop back
        // to Alice after Bob.
        assert_eq!(
            recently_spoken.len(),
            2,
            "only two unique speakers available"
        );
        assert_eq!(recently_spoken[0], NpcId(1));
        assert_eq!(recently_spoken[1], NpcId(2));
    }

    #[test]
    fn energy_bonus_rewards_high_arousal_states() {
        // "angry", "scared", "joyful" all seed initial_from to high
        // arousal; the bonus should reflect that.
        for mood in &["angry", "scared", "joyful"] {
            let npc = make_npc(1, "Test", mood);
            assert!(
                energy_bonus(&npc.emotion, true) > 0.05,
                "mood {mood:?} should produce a positive energy bonus"
            );
        }
    }

    #[test]
    fn energy_bonus_neutral_states_add_little() {
        for mood in &["content", "calm", "tired", "serene"] {
            let npc = make_npc(1, "Test", mood);
            let b = energy_bonus(&npc.emotion, true);
            assert!(
                b.abs() < 0.05,
                "mood {mood:?} should produce a near-zero bonus, got {b}"
            );
        }
    }

    #[test]
    fn energy_bonus_penalises_withdrawn_states() {
        // Grief-struck NPC: high sadness → withdraws_silent gate
        // → large negative bonus.
        let mut npc = Npc::new_test_npc();
        npc.emotion.families.sadness = 0.9;
        let b = energy_bonus(&npc.emotion, true);
        assert!(
            b < -0.2,
            "grief-struck NPC should have a strongly negative bonus, got {b}"
        );
    }

    #[test]
    fn withdrawn_npc_stays_silent_even_when_addressed() {
        // Integration: a grief-struck NPC directly addressed earlier
        // this turn would ordinarily pass the threshold (0.4 baseline
        // + 0.2 addressed = 0.6). With the withdraws_silent penalty
        // (-0.4), they fall back below threshold — mirroring the
        // paper's "high sadness causes withdrawal" finding.
        let mut alice = Npc::new_test_npc();
        alice.id = NpcId(1);
        alice.name = "Alice".to_string();
        alice.relationships = HashMap::new();
        alice.emotion.families.sadness = 0.9;

        let candidates = vec![&alice];
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[NpcId(1)]);
        assert!(
            result.is_none(),
            "withdrawn NPC should not speak up even when addressed"
        );
    }

    #[test]
    fn outburst_npc_speaks_up_unprompted() {
        // Integration: an NPC in the public_outburst band (moderate
        // anger) should clear the threshold even with no relationship
        // bonus and no addressed bonus — 0.4 baseline + 0.1 arousal
        // + 0.15 gate = 0.65.
        let mut alice = Npc::new_test_npc();
        alice.id = NpcId(1);
        alice.name = "Alice".to_string();
        alice.relationships = HashMap::new();
        alice.emotion.families.anger = 0.7;
        alice.emotion.arousal = 0.6;

        let candidates = vec![&alice];
        let result = pick_next_speaker(&candidates, Some(NpcId(99)), &[], &[]);
        assert_eq!(
            result.map(|n| n.id),
            Some(NpcId(1)),
            "outburst-band NPC should chime in unprompted"
        );
    }
}
