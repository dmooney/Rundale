//! Scoring: deterministic gates, quality axes, rubric pinning, and findings.

pub mod axes;
pub mod finding;
pub mod gate;
pub mod rubric;

pub use axes::{Axis, AxisScore, weighted_quality};
pub use finding::{Finding, RawFinding, Severity, canonical_signature};
pub use gate::{GateReason, GateState, GateTrip, TurnEvent};
pub use rubric::{Rubric, load_version, sha256_hex};
