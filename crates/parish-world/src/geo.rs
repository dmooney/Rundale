//! Geographic utilities — haversine distance and travel time calculation.
//!
//! Provides coordinate-based distance calculation between WGS-84 points
//! and conversion from distance to game-minutes at a given travel speed.

/// Earth's mean radius in meters (WGS-84 approximation).
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Calculates the Haversine distance in meters between two WGS-84 coordinate pairs.
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (dlat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

/// Converts a real-world distance in meters to game traversal minutes at a given speed.
///
/// Returns at least 1 minute and at most 120 minutes (2-hour cap).
pub fn meters_to_minutes(meters: f64, speed_m_per_s: f64) -> u16 {
    let speed_m_per_min = speed_m_per_s * 60.0;
    (meters / speed_m_per_min).ceil().clamp(1.0, 120.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_same_point() {
        let d = haversine_distance(53.618, -8.095, 53.618, -8.095);
        assert!(d.abs() < 0.01, "same point should be ~0m, got {d}");
    }

    #[test]
    fn test_haversine_known_distance() {
        // Crossroads (53.618, -8.095) to Darcy's Pub (53.6195, -8.0925)
        // Should be roughly 200-250m
        let d = haversine_distance(53.618, -8.095, 53.6195, -8.0925);
        assert!(
            d > 150.0 && d < 300.0,
            "Crossroads to Pub should be ~200m, got {d}"
        );
    }

    #[test]
    fn test_haversine_longer_distance() {
        // Crossroads (53.618, -8.095) to Lough Ree Shore (53.621, -8.043)
        // Should be ~3.5km
        let d = haversine_distance(53.618, -8.095, 53.621, -8.043);
        assert!(
            d > 3000.0 && d < 4000.0,
            "Crossroads to Lough Ree should be ~3.5km, got {d}"
        );
    }

    #[test]
    fn test_meters_to_minutes_walking() {
        // 75m at 1.25 m/s = 75/75 = 1 minute
        assert_eq!(meters_to_minutes(75.0, 1.25), 1);
    }

    #[test]
    fn test_meters_to_minutes_medium() {
        // 300m at 1.25 m/s = 300/75 = 4 minutes
        assert_eq!(meters_to_minutes(300.0, 1.25), 4);
    }

    #[test]
    fn test_meters_to_minutes_minimum() {
        // Very short distance still returns 1
        assert_eq!(meters_to_minutes(1.0, 1.25), 1);
    }

    #[test]
    fn test_meters_to_minutes_maximum() {
        // Very long distance capped at 120
        assert_eq!(meters_to_minutes(100_000.0, 1.25), 120);
    }

    #[test]
    fn test_meters_to_minutes_faster_speed() {
        // 300m at 4.0 m/s = 300/240 = 1.25 → ceil → 2 minutes
        assert_eq!(meters_to_minutes(300.0, 4.0), 2);
    }
}
