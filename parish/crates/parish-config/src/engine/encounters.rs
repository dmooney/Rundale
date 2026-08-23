//! Encounter probability thresholds by time of day (`[engine.encounters]`).

use serde::Deserialize;

/// Encounter probability thresholds by time of day.
///
/// A random roll in `0.0..1.0` below the threshold triggers an encounter.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EncounterConfig {
    /// Encounter probability at dawn.
    #[serde(default = "default_encounter_dawn")]
    pub dawn: f64,
    /// Encounter probability in the morning.
    #[serde(default = "default_encounter_morning")]
    pub morning: f64,
    /// Encounter probability at midday.
    #[serde(default = "default_encounter_midday")]
    pub midday: f64,
    /// Encounter probability in the afternoon.
    #[serde(default = "default_encounter_afternoon")]
    pub afternoon: f64,
    /// Encounter probability at dusk.
    #[serde(default = "default_encounter_dusk")]
    pub dusk: f64,
    /// Encounter probability at night.
    #[serde(default = "default_encounter_night")]
    pub night: f64,
    /// Encounter probability at midnight.
    #[serde(default = "default_encounter_midnight")]
    pub midnight: f64,
}

impl Default for EncounterConfig {
    fn default() -> Self {
        Self {
            dawn: default_encounter_dawn(),
            morning: default_encounter_morning(),
            midday: default_encounter_midday(),
            afternoon: default_encounter_afternoon(),
            dusk: default_encounter_dusk(),
            night: default_encounter_night(),
            midnight: default_encounter_midnight(),
        }
    }
}

fn default_encounter_dawn() -> f64 {
    0.25
}
fn default_encounter_morning() -> f64 {
    0.25
}
fn default_encounter_midday() -> f64 {
    0.20
}
fn default_encounter_afternoon() -> f64 {
    0.20
}
fn default_encounter_dusk() -> f64 {
    0.15
}
fn default_encounter_night() -> f64 {
    0.10
}
fn default_encounter_midnight() -> f64 {
    0.05
}
