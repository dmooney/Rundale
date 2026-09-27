//! Deterministic scripted player + judge.
//!
//! No API key, no network — used to prove the run loop end-to-end and in CI.
//! The player cycles a fixed set of safe commands (look / npcs / status / time
//! / wait); the `/wait` every fifth turn advances the clock so engine state
//! changes and the empty-turn-burn gate never false-trips. The judge scores
//! axes deterministically from transcript statistics so a "completed" run
//! always carries a real, reproducible quality number.

use async_trait::async_trait;

use crate::error::Result;
use crate::score::{Axis, AxisScore, RawFinding};

use super::traits::{Judge, Player};
use super::types::{JudgeVerdict, Observation, PlayerMove, RunTranscript};

/// A scripted player that cycles a safe command rotation.
pub struct ScriptedPlayer {
    cycle: Vec<String>,
}

impl Default for ScriptedPlayer {
    fn default() -> Self {
        Self {
            cycle: vec![
                "look".to_string(),
                "/npcs".to_string(),
                "/status".to_string(),
                "/time".to_string(),
                "/wait 20".to_string(),
            ],
        }
    }
}

impl ScriptedPlayer {
    /// The command this player issues on a given turn (pure, testable).
    pub fn nth(&self, turn: u32) -> &str {
        &self.cycle[(turn as usize) % self.cycle.len()]
    }
}

#[async_trait]
impl Player for ScriptedPlayer {
    async fn choose_action(&self, obs: &Observation<'_>) -> Result<PlayerMove> {
        Ok(PlayerMove::new(self.nth(obs.turn_index).to_string()))
    }
}

/// A scripted judge: deterministic axis scores derived from transcript stats.
pub struct ScriptedJudge {
    rubric_sha256: String,
}

impl ScriptedJudge {
    pub fn new(rubric_sha256: impl Into<String>) -> Self {
        Self {
            rubric_sha256: rubric_sha256.into(),
        }
    }
}

#[async_trait]
impl Judge for ScriptedJudge {
    fn rubric_sha256(&self) -> &str {
        &self.rubric_sha256
    }

    async fn judge_run(&self, transcript: &RunTranscript) -> Result<JudgeVerdict> {
        let total = transcript.turns.len().max(1) as f64;
        let ok = transcript
            .turns
            .iter()
            .filter(|t| t.outcome == "ok")
            .count() as f64;
        let rejected = transcript
            .turns
            .iter()
            .filter(|t| t.outcome == "rejected")
            .count();

        // Fraction of turns whose ground-truth state changed from the previous
        // turn — a proxy for world responsiveness.
        let mut changed = 0u32;
        for w in transcript.turns.windows(2) {
            if w[0].state_summary != w[1].state_summary {
                changed += 1;
            }
        }
        let responsiveness = ((changed as f64 / total) * 100.0).round().clamp(0.0, 100.0) as u8;
        let ok_pct = ((ok / total) * 100.0).round().clamp(0.0, 100.0) as u8;

        let score_for = |axis: Axis| -> u8 {
            match axis {
                Axis::WorldResponsiveness => responsiveness,
                Axis::IntentFidelity => ok_pct,
                // The scripted judge is not a quality oracle — it returns stable
                // mid-high baselines for the qualitative axes so a healthy
                // smoke run scores in a sensible band, while the two computed
                // axes above carry the real signal.
                Axis::NarrativeCoherence => 80,
                Axis::CharacterFidelity => 78,
                Axis::Immersion => 75,
                Axis::Progression => 74,
                Axis::CommonSense => 80,
            }
        };

        let axes = Axis::ALL
            .into_iter()
            .map(|axis| AxisScore {
                axis,
                score: score_for(axis),
                rationale: format!(
                    "scripted: ok={ok_pct}% state_change={responsiveness}% over {} turns",
                    transcript.turns.len()
                ),
            })
            .collect();

        // Emit a low-severity finding when any input was rejected, so the
        // finding-persistence path is exercised deterministically.
        let mut findings = Vec::new();
        if rejected > 0 {
            findings.push(
                RawFinding {
                    category: "intent_parse".into(),
                    turn_index: None,
                    severity: "low".into(),
                    description: format!("{rejected} player inputs were rejected by the parser"),
                    evidence_quote: String::new(),
                    signature_hint: "scripted player input rejected".into(),
                }
                .canonicalize(),
            );
        }

        Ok(JudgeVerdict { axes, findings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::types::TurnTranscript;

    fn transcript(outcomes: &[&str]) -> RunTranscript {
        RunTranscript {
            persona: "tester".into(),
            turns: outcomes
                .iter()
                .enumerate()
                .map(|(i, o)| TurnTranscript {
                    turn_index: i as u32,
                    player_input: "look".into(),
                    narrative: "n".into(),
                    outcome: (*o).into(),
                    kind: "looked".into(),
                    state_summary: format!("s{i}"),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn scripted_judge_scores_all_axes() {
        let judge = ScriptedJudge::new("sha");
        let verdict = judge
            .judge_run(&transcript(&["ok", "ok", "ok"]))
            .await
            .unwrap();
        assert_eq!(verdict.axes.len(), 7);
        assert!(verdict.axes.iter().all(|a| a.score <= 100));
    }

    #[tokio::test]
    async fn scripted_judge_flags_rejects() {
        let judge = ScriptedJudge::new("sha");
        let verdict = judge
            .judge_run(&transcript(&["ok", "rejected", "ok"]))
            .await
            .unwrap();
        assert_eq!(verdict.findings.len(), 1);
        assert_eq!(verdict.findings[0].category, "intent_parse");
    }

    #[test]
    fn scripted_player_cycles_safe_commands_with_wait_every_fifth() {
        let player = ScriptedPlayer::default();
        // Every command is non-empty and the rotation includes a `/wait` within
        // each window of 5 (keeps engine state advancing → no false empty-burn).
        for i in 0..15u32 {
            assert!(!player.nth(i).is_empty());
        }
        assert_eq!(player.nth(4), "/wait 20");
        assert_eq!(player.nth(9), "/wait 20");
        // Wraps around.
        assert_eq!(player.nth(0), player.nth(5));
    }
}
