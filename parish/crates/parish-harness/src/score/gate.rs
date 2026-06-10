//! Deterministic hard-fail gates.
//!
//! Gates are the "is the game even functioning?" layer. They are evaluated from
//! observable, non-LLM signals (transport failures, the command `outcome`, and
//! whether engine state actually advances), so they are fully deterministic and
//! unit-testable. A tripped gate short-circuits the run, records the reason and
//! the turn it fired on, and forces `quality_score = NULL` — a broken run is
//! never given a quality number.

use crate::client::Outcome;
use crate::config::GateCfg;

/// Why a run was gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateReason {
    /// Transport failure or 5xx mid-run — no game to talk to.
    Crash,
    /// Too many `rejected` outcomes (parser could not act on well-formed input).
    ParserReject,
    /// A command timed out.
    Timeout,
    /// A response body could not be parsed.
    JsonParseFail,
    /// The player burned through the empty/no-progress tolerance — stuck.
    EmptyTurnBurn,
    /// The judge flagged a session-breaking (critical) finding.
    JudgeCritical,
}

impl GateReason {
    /// Stable snake_case label for the DB / dashboard.
    pub fn label(self) -> &'static str {
        match self {
            GateReason::Crash => "crash",
            GateReason::ParserReject => "parser_reject",
            GateReason::Timeout => "timeout",
            GateReason::JsonParseFail => "json_parse_fail",
            GateReason::EmptyTurnBurn => "empty_turn_burn",
            GateReason::JudgeCritical => "judge_critical",
        }
    }

    /// Parse a gate reason from its snake_case label. Unknown labels degrade to
    /// `Crash` (the most conservative "broken run" reason) rather than failing —
    /// an ingested run is always recordable even if its reason string drifted.
    pub fn from_label(s: &str) -> GateReason {
        match s.trim().to_ascii_lowercase().as_str() {
            "parser_reject" => GateReason::ParserReject,
            "timeout" => GateReason::Timeout,
            "json_parse_fail" => GateReason::JsonParseFail,
            "empty_turn_burn" => GateReason::EmptyTurnBurn,
            "judge_critical" => GateReason::JudgeCritical,
            _ => GateReason::Crash,
        }
    }
}

/// A tripped gate: the reason, the turn index it fired on, and a human detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateTrip {
    pub reason: GateReason,
    pub turn: u32,
    pub detail: String,
}

/// What happened on one turn, reduced to the signals the gates care about.
#[derive(Debug, Clone)]
pub enum TurnEvent {
    /// The client could not reach the backend / got a 5xx.
    Crash(String),
    /// The response body could not be parsed.
    JsonParseFail(String),
    /// A parsed command response, with the engine-state signature taken right
    /// after it (used for no-progress detection).
    Response {
        outcome: Outcome,
        /// A hash/fingerprint of the engine state after this turn. `None` when
        /// the engine state could not be read (treated as "no change").
        state_sig: Option<String>,
    },
}

/// Running gate state across a session.
pub struct GateState {
    cfg: GateCfg,
    reject_count: u32,
    no_progress_streak: u32,
    last_sig: Option<String>,
}

impl GateState {
    pub fn new(cfg: GateCfg) -> Self {
        Self {
            cfg,
            reject_count: 0,
            no_progress_streak: 0,
            last_sig: None,
        }
    }

    /// Feed one turn's event. Returns `Some(GateTrip)` if a hard fail tripped on
    /// this turn (the caller should stop the run), else `None`.
    pub fn observe(&mut self, turn: u32, event: &TurnEvent) -> Option<GateTrip> {
        match event {
            TurnEvent::Crash(detail) => Some(GateTrip {
                reason: GateReason::Crash,
                turn,
                detail: detail.clone(),
            }),
            TurnEvent::JsonParseFail(detail) => Some(GateTrip {
                reason: GateReason::JsonParseFail,
                turn,
                detail: detail.clone(),
            }),
            TurnEvent::Response { outcome, state_sig } => {
                self.observe_outcome(turn, *outcome, state_sig.as_deref())
            }
        }
    }

    fn observe_outcome(
        &mut self,
        turn: u32,
        outcome: Outcome,
        state_sig: Option<&str>,
    ) -> Option<GateTrip> {
        // Timeout trips immediately — a hung turn means the game stopped
        // responding within its own deadline.
        if outcome == Outcome::Timeout {
            return Some(GateTrip {
                reason: GateReason::Timeout,
                turn,
                detail: "command outcome was timeout".into(),
            });
        }

        // Rejects accumulate against a tolerance.
        if outcome == Outcome::Rejected {
            self.reject_count += 1;
            if self.reject_count > self.cfg.max_rejects {
                return Some(GateTrip {
                    reason: GateReason::ParserReject,
                    turn,
                    detail: format!(
                        "{} rejected outcomes exceeded tolerance {}",
                        self.reject_count, self.cfg.max_rejects
                    ),
                });
            }
        }

        // No-progress streak: an explicit Empty outcome, or engine state that
        // did not change from the previous turn, counts toward being "stuck".
        let no_progress = outcome == Outcome::Empty
            || match (self.last_sig.as_deref(), state_sig) {
                (Some(prev), Some(curr)) => prev == curr,
                _ => false,
            };
        if no_progress {
            self.no_progress_streak += 1;
        } else {
            self.no_progress_streak = 0;
        }
        self.last_sig = state_sig.map(|s| s.to_string());

        if self.no_progress_streak >= self.cfg.empty_burn_limit {
            return Some(GateTrip {
                reason: GateReason::EmptyTurnBurn,
                turn,
                detail: format!(
                    "{} consecutive empty/no-progress turns reached limit {}",
                    self.no_progress_streak, self.cfg.empty_burn_limit
                ),
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GateCfg {
        GateCfg {
            max_rejects: 3,
            empty_burn_limit: 5,
        }
    }

    fn resp(outcome: Outcome, sig: &str) -> TurnEvent {
        TurnEvent::Response {
            outcome,
            state_sig: Some(sig.to_string()),
        }
    }

    #[test]
    fn gate_crash_trips_immediately() {
        let mut g = GateState::new(cfg());
        let trip = g
            .observe(0, &TurnEvent::Crash("connection refused".into()))
            .expect("crash trips");
        assert_eq!(trip.reason, GateReason::Crash);
        assert_eq!(trip.turn, 0);
    }

    #[test]
    fn gate_timeout_trips_immediately() {
        let mut g = GateState::new(cfg());
        let trip = g.observe(2, &resp(Outcome::Timeout, "x")).expect("trips");
        assert_eq!(trip.reason, GateReason::Timeout);
    }

    #[test]
    fn gate_parser_reject_tolerates_then_trips() {
        let mut g = GateState::new(cfg());
        // 3 rejects within tolerance.
        for i in 0..3 {
            assert!(
                g.observe(i, &resp(Outcome::Rejected, &format!("s{i}")))
                    .is_none()
            );
        }
        // 4th exceeds tolerance of 3.
        let trip = g.observe(3, &resp(Outcome::Rejected, "s3")).expect("trips");
        assert_eq!(trip.reason, GateReason::ParserReject);
    }

    #[test]
    fn gate_empty_burn_trips_on_repeated_empty() {
        let mut g = GateState::new(cfg());
        for i in 0..4 {
            assert!(g.observe(i, &resp(Outcome::Empty, "same")).is_none());
        }
        let trip = g.observe(4, &resp(Outcome::Empty, "same")).expect("trips");
        assert_eq!(trip.reason, GateReason::EmptyTurnBurn);
    }

    #[test]
    fn gate_no_progress_resets_when_state_changes() {
        let mut g = GateState::new(cfg());
        // Alternate no-change / change so the streak never reaches the limit.
        for i in 0..20 {
            let sig = if i % 2 == 0 { "a" } else { "b" };
            assert!(
                g.observe(i, &resp(Outcome::Ok, sig)).is_none(),
                "turn {i} should not trip"
            );
        }
    }

    #[test]
    fn gate_ok_turns_never_trip() {
        let mut g = GateState::new(cfg());
        for i in 0..50 {
            assert!(g.observe(i, &resp(Outcome::Ok, &format!("s{i}"))).is_none());
        }
    }
}
