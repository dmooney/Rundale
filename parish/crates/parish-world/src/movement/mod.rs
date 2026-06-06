//! Movement system — resolving movement commands and traversal with time.
//!
//! Handles resolving player movement intents to destinations, computing
//! travel time along paths, and producing narration text for travel.
//!
//! Severe weather can block certain connections (marked with
//! [`Hazard`](super::graph::Hazard)) and slow others. The weather-aware
//! entry point is [`resolve_movement_with_weather`]; the legacy
//! [`resolve_movement`] is preserved as a `Weather::Clear` wrapper so
//! tests and older callers keep working.
//!
//! ## Submodule layout
//! - [`resolve`] — core types, target lookup, and the two public resolvers.
//! - [`transport_apply`] — weather/transport time adjustment, blocked-route
//!   fallback, and narration builders.

mod resolve;
mod transport_apply;

#[cfg(test)]
mod tests;

// Re-export the full public API so all callers importing from
// `parish_world::movement` continue to work without change.
pub use resolve::{
    MovementResult, WeatherEffect, resolve_movement, resolve_movement_with_weather, weather_effect,
};
