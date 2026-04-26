//! Reusable dice-roll utility for probability-based game mechanics.
//!
//! Provides a [`DiceRoll`] wrapper around a `0.0..1.0` float that supports
//! threshold checks, index selection, and deterministic testing via fixed
//! values.  Higher-level helpers ([`roll_n`], [`fixed_n`]) create batches.

use rand::Rng;

/// A single probability roll in `0.0..1.0`.
///
/// Used throughout the game for threshold-based checks (encounters,
/// NPC reactions, weather transitions, etc.).
#[derive(Debug, Clone, Copy)]
pub struct DiceRoll {
    value: f64,
}

impl DiceRoll {
    /// Creates a roll with a predetermined value (for deterministic tests).
    ///
    /// The value is clamped to `0.0..1.0`.
    pub fn fixed(value: f64) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
        }
    }

    /// Rolls using the thread-local RNG.
    pub fn roll() -> Self {
        Self {
            value: rand::rng().random::<f64>(),
        }
    }

    /// Returns `true` if this roll is below `threshold` (i.e. a "success").
    ///
    /// A threshold of `0.0` never succeeds; `1.0` always succeeds.
    pub fn check(&self, threshold: f64) -> bool {
        self.value < threshold
    }

    /// The raw `0.0..1.0` value.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Picks an index in `0..len` based on this roll's value.
    ///
    /// Panics if `len` is zero.
    pub fn pick_index(&self, len: usize) -> usize {
        assert!(len > 0, "pick_index called with len 0");
        let idx = (self.value * len as f64) as usize;
        idx.min(len - 1)
    }

    /// Picks a random element from a slice.
    ///
    /// Panics if the slice is empty.
    pub fn pick<'a, T>(&self, items: &'a [T]) -> &'a T {
        &items[self.pick_index(items.len())]
    }
}

/// Rolls `n` dice using the thread-local RNG.
pub fn roll_n(n: usize) -> Vec<DiceRoll> {
    let mut rng = rand::rng();
    (0..n)
        .map(|_| DiceRoll {
            value: rng.random(),
        })
        .collect()
}

/// Creates `n` dice with predetermined values (for deterministic tests).
pub fn fixed_n(values: &[f64]) -> Vec<DiceRoll> {
    values.iter().map(|&v| DiceRoll::fixed(v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_roll_value() {
        let d = DiceRoll::fixed(0.42);
        assert!((d.value() - 0.42).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fixed_clamps_high() {
        let d = DiceRoll::fixed(1.5);
        assert!((d.value() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fixed_clamps_low() {
        let d = DiceRoll::fixed(-0.5);
        assert!(d.value().abs() < f64::EPSILON);
    }

    #[test]
    fn test_check_below_threshold() {
        let d = DiceRoll::fixed(0.3);
        assert!(d.check(0.5));
    }

    #[test]
    fn test_check_above_threshold() {
        let d = DiceRoll::fixed(0.7);
        assert!(!d.check(0.5));
    }

    #[test]
    fn test_check_at_threshold() {
        let d = DiceRoll::fixed(0.5);
        assert!(!d.check(0.5)); // not strictly less than
    }

    #[test]
    fn test_check_zero_threshold_never_passes() {
        let d = DiceRoll::fixed(0.0);
        assert!(!d.check(0.0));
    }

    #[test]
    fn test_check_one_threshold_always_passes() {
        let d = DiceRoll::fixed(0.99);
        assert!(d.check(1.0));
    }

    #[test]
    fn test_pick_index_distributes() {
        assert_eq!(DiceRoll::fixed(0.0).pick_index(4), 0);
        assert_eq!(DiceRoll::fixed(0.25).pick_index(4), 1);
        assert_eq!(DiceRoll::fixed(0.5).pick_index(4), 2);
        assert_eq!(DiceRoll::fixed(0.75).pick_index(4), 3);
        // Edge: value at 1.0 should clamp to last index
        assert_eq!(DiceRoll::fixed(1.0).pick_index(4), 3);
    }

    #[test]
    fn test_pick_single_element() {
        let items = ["only"];
        assert_eq!(*DiceRoll::fixed(0.0).pick(&items), "only");
        assert_eq!(*DiceRoll::fixed(0.99).pick(&items), "only");
    }

    #[test]
    #[should_panic(expected = "pick_index called with len 0")]
    fn test_pick_index_panics_on_zero() {
        DiceRoll::fixed(0.5).pick_index(0);
    }

    #[test]
    fn test_roll_n_count() {
        let dice = roll_n(5);
        assert_eq!(dice.len(), 5);
        for d in &dice {
            assert!((0.0..1.0).contains(&d.value()));
        }
    }

    #[test]
    fn test_fixed_n() {
        let dice = fixed_n(&[0.1, 0.5, 0.9]);
        assert_eq!(dice.len(), 3);
        assert!((dice[0].value() - 0.1).abs() < f64::EPSILON);
        assert!((dice[1].value() - 0.5).abs() < f64::EPSILON);
        assert!((dice[2].value() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_roll_produces_valid_range() {
        // Statistical test: 100 rolls should all be in [0, 1)
        for _ in 0..100 {
            let d = DiceRoll::roll();
            assert!((0.0..1.0).contains(&d.value()));
        }
    }
}
