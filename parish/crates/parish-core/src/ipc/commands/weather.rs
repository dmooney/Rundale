//! Weather command — report or force weather state.

use parish_types::Weather;

use crate::world::WorldState;

use super::CommandResult;

/// Handles the `/weather` command.
///
/// With no argument, reports the current weather and how long it has been
/// running. With an argument, forces a weather state — useful for testing
/// weather-gated travel and demonstrations. Accepts the canonical names
/// plus common shorthands (e.g. `rain`, `partly`).
pub(super) fn handle_weather_command(
    world: &mut WorldState,
    arg: Option<String>,
) -> CommandResult {
    let parse_weather = |s: &str| -> Option<Weather> {
        match s.trim().to_lowercase().as_str() {
            "clear" | "sun" | "sunny" => Some(Weather::Clear),
            "partly" | "partly cloudy" | "partlycloudy" => Some(Weather::PartlyCloudy),
            "overcast" | "cloudy" => Some(Weather::Overcast),
            "light rain" | "lightrain" | "rain" | "drizzle" => Some(Weather::LightRain),
            "heavy rain" | "heavyrain" | "pour" | "pouring" => Some(Weather::HeavyRain),
            "fog" | "foggy" | "mist" => Some(Weather::Fog),
            "storm" | "stormy" | "gale" => Some(Weather::Storm),
            _ => None,
        }
    };

    match arg {
        None => {
            let hours = world.weather_engine.duration_hours(world.clock.now());
            CommandResult::text(format!(
                "The weather is {} (running for {:.1} game-hour{}).\nUsage: /weather <clear|fog|rain|heavy rain|storm|overcast|partly cloudy>",
                world.weather,
                hours,
                if (hours - 1.0).abs() < f64::EPSILON {
                    ""
                } else {
                    "s"
                }
            ))
        }
        Some(name) => match parse_weather(&name) {
            Some(w) => {
                world.weather = w;
                world.weather_engine.force(w, world.clock.now());
                CommandResult::text(format!("The weather shifts to {}.", w))
            }
            None => CommandResult::text(format!(
                "Unknown weather '{}'. Try: clear, partly cloudy, overcast, light rain, heavy rain, fog, storm.",
                name
            )),
        },
    }
}
