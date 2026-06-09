//! Cost accumulator scaffolding.
//!
//! The `runs` table already carries `cost_usd`, `player_tokens`, and
//! `judge_tokens` columns (defaulting to 0). Real per-call token usage is not
//! yet surfaced by `AnyClient::generate` — wiring awaits usage-field support in
//! `parish-inference`'s `AnyClient` (tracked separately). Until then these
//! counters accumulate from whatever callers explicitly record.
//!
//! `CostTracker` is an in-process accumulator used during a run to collect
//! usage as it becomes available; `Db::cost_summary` sums the persisted columns
//! for the dashboard.

use serde::Serialize;

/// In-process cost accumulator for a single run.
///
/// # Known limitation
/// Token usage wiring awaits `AnyClient` usage exposure. The fields will be
/// non-zero only once callers explicitly feed counts into `add(...)`. The seam
/// is here; the plumbing to `AnyClient` is not yet implemented.
#[derive(Debug, Default, Clone)]
pub struct CostTracker {
    cost_usd: f64,
    player_tokens: u64,
    judge_tokens: u64,
}

impl CostTracker {
    /// Create a fresh, zeroed tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accumulate a cost observation.
    pub fn add(&mut self, cost_usd: f64, player_tokens: u64, judge_tokens: u64) {
        self.cost_usd += cost_usd;
        self.player_tokens += player_tokens;
        self.judge_tokens += judge_tokens;
    }

    /// Total USD cost accumulated so far.
    pub fn cost_usd(&self) -> f64 {
        self.cost_usd
    }

    /// Total player tokens accumulated so far.
    pub fn player_tokens(&self) -> u64 {
        self.player_tokens
    }

    /// Total judge tokens accumulated so far.
    pub fn judge_tokens(&self) -> u64 {
        self.judge_tokens
    }
}

/// Aggregate cost summary across all persisted runs (served at `GET /api/cost`).
#[derive(Debug, Clone, Serialize)]
pub struct CostSummary {
    pub total_runs: u64,
    pub total_cost_usd: f64,
    pub total_player_tokens: u64,
    pub total_judge_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_tracker_accumulates_correctly() {
        let mut t = CostTracker::new();
        assert_eq!(t.cost_usd(), 0.0);
        assert_eq!(t.player_tokens(), 0);
        assert_eq!(t.judge_tokens(), 0);

        t.add(0.01, 100, 200);
        t.add(0.02, 300, 400);

        assert!((t.cost_usd() - 0.03).abs() < 1e-10, "cost_usd should sum");
        assert_eq!(t.player_tokens(), 400);
        assert_eq!(t.judge_tokens(), 600);
    }

    #[test]
    fn cost_tracker_default_is_zero() {
        let t = CostTracker::default();
        assert_eq!(t.cost_usd(), 0.0);
        assert_eq!(t.player_tokens(), 0);
        assert_eq!(t.judge_tokens(), 0);
    }

    #[test]
    fn cost_tracker_add_multiple_zeros_stays_zero() {
        let mut t = CostTracker::new();
        t.add(0.0, 0, 0);
        t.add(0.0, 0, 0);
        assert_eq!(t.cost_usd(), 0.0);
        assert_eq!(t.player_tokens(), 0);
        assert_eq!(t.judge_tokens(), 0);
    }
}
