//! Transport modes — configurable movement speeds for travel time calculation.
//!
//! Each transport mode defines a speed (m/s) and a display label used in
//! travel narration. Mods provide a `transport.toml` with available modes;
//! if absent, the engine falls back to walking (1.25 m/s, "on foot").

use serde::{Deserialize, Serialize};

/// Default walking speed in meters per second (~4.5 km/h).
const DEFAULT_WALKING_SPEED: f64 = 1.25;

/// A method of travel with a speed and display label.
///
/// Used to calculate coordinate-based travel times and to format
/// narration text (e.g., "23 minutes on foot").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportMode {
    /// Machine identifier (e.g., `"walking"`, `"jaunting_car"`).
    pub id: String,
    /// Display label for narration (e.g., `"on foot"`, `"in a jaunting car"`).
    pub label: String,
    /// Travel speed in meters per second.
    pub speed_m_per_s: f64,
}

impl TransportMode {
    /// Returns the default walking transport mode.
    pub fn walking() -> Self {
        Self {
            id: "walking".to_string(),
            label: "on foot".to_string(),
            speed_m_per_s: DEFAULT_WALKING_SPEED,
        }
    }
}

/// Collection of available transport modes loaded from a mod's `transport.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Which mode id is the default for player travel.
    pub default: String,
    /// All available transport modes.
    pub modes: Vec<TransportMode>,
}

impl TransportConfig {
    /// Returns the default transport mode.
    pub fn default_mode(&self) -> &TransportMode {
        self.modes
            .iter()
            .find(|m| m.id == self.default)
            .unwrap_or_else(|| {
                self.modes
                    .first()
                    .expect("TransportConfig must have at least one mode")
            })
    }

    /// Looks up a transport mode by id.
    pub fn get_mode(&self, id: &str) -> Option<&TransportMode> {
        self.modes.iter().find(|m| m.id == id)
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            default: "walking".to_string(),
            modes: vec![TransportMode::walking()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walking_default() {
        let mode = TransportMode::walking();
        assert_eq!(mode.id, "walking");
        assert_eq!(mode.label, "on foot");
        assert!((mode.speed_m_per_s - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.default, "walking");
        assert_eq!(config.modes.len(), 1);
        assert_eq!(config.default_mode().id, "walking");
    }

    #[test]
    fn test_transport_config_deserialize() {
        let toml_str = r#"
default = "walking"

[[modes]]
id = "walking"
label = "on foot"
speed_m_per_s = 1.25

[[modes]]
id = "jaunting_car"
label = "in a jaunting car"
speed_m_per_s = 4.0
"#;
        let config: TransportConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.modes.len(), 2);
        assert_eq!(config.default_mode().id, "walking");

        let car = config.get_mode("jaunting_car").unwrap();
        assert_eq!(car.label, "in a jaunting car");
        assert!((car.speed_m_per_s - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_mode_not_found() {
        let config = TransportConfig::default();
        assert!(config.get_mode("horse").is_none());
    }

    #[test]
    fn test_default_mode_fallback() {
        let config = TransportConfig {
            default: "nonexistent".to_string(),
            modes: vec![TransportMode::walking()],
        };
        // Falls back to first mode when default id not found
        assert_eq!(config.default_mode().id, "walking");
    }
}
