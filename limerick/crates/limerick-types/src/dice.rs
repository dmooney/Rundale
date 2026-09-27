//! Reusable dice-roll utility for probability-based game mechanics.
//!
//! Provides a [`DiceRoll`] wrapper around a `0.0..1.0` float that supports
//! threshold checks, index selection, and deterministic testing via fixed
//! values. Game rolls are seeded from the game state they depend on
//! ([`seed`], [`DiceRoll::seeded`], [`seeded_n`]): the odds are unchanged,
//! and the same state always rolls the same way, so a replayed session is
//! byte-identical. [`fixed_n`] creates batches for tests.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

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

    /// Rolls from `seed` (see [`seed`]): uniform in `0.0..1.0`, and the
    /// same seed always gives the same roll.
    pub fn seeded(seed: u64) -> Self {
        Self {
            value: StdRng::seed_from_u64(seed).random::<f64>(),
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

/// Rolls `n` independent dice from `seed` (see [`seed`]).
pub fn seeded_n(seed: u64, n: usize) -> Vec<DiceRoll> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n)
        .map(|_| DiceRoll {
            value: rng.random(),
        })
        .collect()
}

/// Builds a roll seed from a purpose label and the game state the roll
/// depends on, such as game minutes and a location id.
///
/// The label keeps different rolls made from the same state independent
/// (an encounter and an arrival reaction at the same place and minute).
pub fn seed(purpose: &str, parts: &[u64]) -> u64 {
    // FNV-1a over the label, then a splitmix64 step per part.
    let mut state = purpose
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    for &part in parts {
        state = splitmix64(state ^ part);
    }
    splitmix64(state)
}

fn splitmix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
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
    fn seeded_n_count_and_range() {
        let dice = seeded_n(seed("test", &[1]), 5);
        assert_eq!(dice.len(), 5);
        for d in &dice {
            assert!((0.0..1.0).contains(&d.value()));
        }
    }

    #[test]
    fn same_seed_same_rolls() {
        let a = seeded_n(seed("arrival", &[600, 15]), 4);
        let b = seeded_n(seed("arrival", &[600, 15]), 4);
        let values = |dice: &[DiceRoll]| dice.iter().map(DiceRoll::value).collect::<Vec<_>>();
        assert_eq!(values(&a), values(&b));
        assert_eq!(
            DiceRoll::seeded(seed("x", &[1])).value(),
            DiceRoll::seeded(seed("x", &[1])).value()
        );
    }

    #[test]
    fn seed_depends_on_label_parts_and_their_order() {
        let base = seed("encounter", &[600, 1, 2]);
        assert_ne!(base, seed("arrival", &[600, 1, 2]));
        assert_ne!(base, seed("encounter", &[601, 1, 2]));
        assert_ne!(base, seed("encounter", &[600, 2, 1]));
        assert_ne!(base, seed("encounter", &[600, 1]));
    }

    #[test]
    fn seeded_rolls_keep_uniform_odds() {
        // Seeds from consecutive game minutes, as the game makes them: the
        // share of rolls under a threshold matches the threshold.
        let n = 20_000;
        for threshold in [0.1, 0.3, 0.6] {
            let hits = (0..n)
                .filter(|&minute| DiceRoll::seeded(seed("odds", &[minute, 15])).check(threshold))
                .count();
            let share = hits as f64 / n as f64;
            assert!((share - threshold).abs() < 0.015, "{threshold}: {share}");
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
    fn test_seeded_produces_valid_range() {
        for minute in 0..100 {
            let d = DiceRoll::seeded(seed("range", &[minute]));
            assert!((0.0..1.0).contains(&d.value()));
        }
    }
}
