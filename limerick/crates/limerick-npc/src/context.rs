//! Tier 1 world-context block construction.

use super::*;

/// Builds the Tier 1 context prompt for an NPC interaction.
///
/// Renders the location description template (substituting `{time}`,
/// `{weather}`, and `{npcs_present}` placeholders) and includes the
/// full game date and time so NPCs have precise temporal context.
/// The player's action is intentionally omitted here so callers can
/// append it at the end of the full context (after memory, history, etc.).
pub fn build_tier1_context(world: &WorldState) -> String {
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();
    let now = world.clock.now();

    // Render the location description with current time/weather substituted.
    let rendered_desc = if let Some(loc_data) = world.current_location_data() {
        render_description(loc_data, time_of_day, &world.weather.to_string(), &[])
    } else {
        world.current_location().description.clone()
    };

    let date_time_str = format!(
        "{day_of_week} {day} {month} {year} | {hour:02}:{minute:02} | {season}",
        day_of_week = now.format("%A"),
        day = now.day(),
        month = now.format("%B"),
        year = now.year(),
        hour = now.hour(),
        minute = now.minute(),
        season = season,
    );

    // Regression (fixed: #13, reinforced: #1225): explicit time-of-day cue so
    // the model picks the right greeting register. NPCs were saying "good
    // morning" at Dusk because the only time signal in the context was the bare
    // 17:30 HH:MM — without an English label the model defaulted to "morning".
    // Spell out the bucket and direct the model to greet accordingly. A
    // forbidden-greeting directive is also injected for non-morning buckets
    // because small models need negative examples to override the training-data
    // majority bias toward "good morning".
    let time_of_day_label = time_of_day.to_string();
    let loc_details = location_details_block(world);
    let forbidden = forbidden_greeting_directive(time_of_day)
        .map(|d| format!(" {d}"))
        .unwrap_or_default();
    // #1451: season is present in the date string but underweighted, so NPCs
    // reference wrong seasons. Inject a dedicated directive so models cannot
    // treat season as an inert label.
    format!(
        "Your Location: {loc_name} — {loc_desc}{loc_details}\n\
        Date and time: {date_time}\n\
        Time of day: {tod} ({hour:02}:{minute:02}) — greet and refer to the time of day accordingly.{forbidden}\n\
        CURRENT SEASON: {season}. Do not reference any other season as if it were now.",
        loc_name = world.current_location().name,
        loc_desc = rendered_desc,
        loc_details = loc_details,
        date_time = date_time_str,
        tod = time_of_day_label,
        hour = now.hour(),
        minute = now.minute(),
        forbidden = forbidden,
        season = season,
    )
}

/// Builds the situational location block appended to the Tier 1 context:
/// indoor/outdoor + public/private framing, any mythological note, and the
/// paths leading away — each neighbour by name, its prose path, an on-foot
/// travel estimate, and a live weather-hazard note when the current weather
/// blocks or slows that edge.
///
/// Returns an empty string when the current location is absent from the world
/// graph, so callers fall back to the legacy name + description text unchanged.
fn location_details_block(world: &WorldState) -> String {
    let Some(loc) = world.current_location_data() else {
        return String::new();
    };

    let setting = if loc.indoor {
        "an enclosed indoor space"
    } else {
        "an open outdoor place"
    };
    let access = if loc.public {
        "open to all"
    } else {
        "a private place"
    };
    let mut out = format!("\nThis is {setting}, {access}.");

    if let Some(sig) = &loc.mythological_significance {
        out.push_str(&format!("\nOf local note: {sig}"));
    }

    let walking_speed = TransportMode::walking().speed_m_per_s;
    let neighbors = world.graph.neighbors(world.player_location);
    if neighbors.is_empty() {
        out.push_str("\nThere are no paths leading away from here.");
        return out;
    }

    out.push_str("\nPaths from here:");
    for (target, conn) in neighbors {
        let Some(dest) = world.graph.get(target) else {
            continue;
        };
        let mut minutes =
            world
                .graph
                .edge_travel_minutes(world.player_location, target, walking_speed);
        let hazard = match weather_effect(conn, world.weather) {
            WeatherEffect::Impassable { reason } => {
                format!(" — impassable in this weather: {reason}")
            }
            WeatherEffect::Slowed { factor, note } => {
                // Mirror movement::weather_adjusted_path so the quoted time
                // matches the slowdown the player will actually experience.
                minutes = ((minutes as f64 / factor).ceil() as u16).max(minutes);
                format!(" — slow going today: {note}")
            }
            WeatherEffect::Clear => String::new(),
        };
        out.push_str(&format!(
            "\n- {name} — {path} (about {minutes} min on foot){hazard}",
            name = dest.name,
            path = conn.path_description,
        ));
    }

    out
}
