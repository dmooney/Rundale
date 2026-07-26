//! Demo auto-player context.
//!
//! The demo loop simulates a human end-user player. Its LLM prompt must only
//! contain information that the human player would see in the GUI at that
//! moment — anything the GUI hides (un-introduced identity, fog-of-war map
//! entries) must be hidden from the prompt too. Issue #998.
//!
//! Concretely: the builder accepts only GUI-facing IPC payloads
//! ([`WorldSnapshot`], [`NpcInfo`], [`MapData`]) plus the text-log entries the
//! frontend already mirrors. It does **not** touch [`WorldState`] or
//! `NpcManager` directly. Adding a new field that requires raw game state is
//! a signal that the GUI should be exposing that field first.
//!
//! [`WorldState`]: crate::world::WorldState

use serde::{Deserialize, Serialize};

use super::types::{MapData, NpcInfo, PlayerTaskSnapshot, WorldSnapshot};

/// One NPC co-located with the player, shaped for the demo prompt.
///
/// Pre-introduction (`occupation == None`) the LLM only sees the
/// `display_name` (brief description). Post-introduction it sees the real
/// name and the occupation, matching what the GUI's NPC chip exposes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DemoNpcInfo {
    /// What the GUI shows for this NPC right now (brief description pre-
    /// introduction, full name post-introduction).
    pub name: String,
    /// Occupation, only populated once the player has been introduced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupation: Option<String>,
    /// Mood label — GUI always shows the mood emoji, so the LLM sees it too.
    pub mood: String,
}

/// One adjacent location the player can choose to travel to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DemoAdjacentLocation {
    pub name: String,
    /// Estimated walking time in minutes. Present for any reachable neighbour —
    /// visited or unexplored — so the auto-player can judge distance before
    /// committing to a move (#1207 #33/#36). `None` only when the map could not
    /// estimate a travel time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub travel_minutes: Option<u16>,
    /// `true` for visited locations, `false` for frontier entries.
    pub visited: bool,
}

/// Snapshot passed to the demo LLM each turn.
///
/// All fields derive from GUI-facing payloads ([`WorldSnapshot`], [`NpcInfo`],
/// [`MapData`]) — adding a new field requires the GUI to expose the source
/// data first.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DemoContextSnapshot {
    pub location_name: String,
    pub location_description: String,
    /// Pretty date+time string ("Wednesday, 14 May 1820, morning").
    pub game_time: String,
    /// Season label ("spring", "summer", ...).
    pub season: String,
    /// Weather label (matches `WorldSnapshot.weather`).
    pub weather: String,
    pub npcs_here: Vec<DemoNpcInfo>,
    pub adjacent: Vec<DemoAdjacentLocation>,
    /// Active durable work visible in the same world snapshot as the GUI.
    #[serde(default)]
    pub active_tasks: Vec<PlayerTaskSnapshot>,
    /// Recent text-log entries. The Tauri command leaves this empty; the
    /// frontend fills it from its `textLog` store before invoking the LLM.
    pub recent_log: Vec<String>,
    /// The auto-player's own most recent actions (oldest first, last 5).
    /// The Tauri command leaves this empty; the frontend fills it from the
    /// `[player]` entries of its `textLog` store before invoking the LLM.
    /// Surfaced in the rendered prompt as a `Your last actions:` block so the
    /// model can detect and break out of repetition loops (#999).
    #[serde(default)]
    pub recent_actions: Vec<String>,
    /// Operator-supplied steering text from the demo CLI flags.
    pub extra_prompt: Option<String>,
}

/// Builds the demo snapshot from GUI-facing payloads.
///
/// The four payload parameters are exactly what the frontend already
/// consumes — passing anything else is a layering violation and the only
/// thing that lets occupation/identity leak into the prompt (the bug at the
/// root of issue #998).
pub fn build_demo_context(
    snapshot: &WorldSnapshot,
    npcs: &[NpcInfo],
    map: &MapData,
    game_time: String,
    season: String,
    extra_prompt: Option<String>,
) -> DemoContextSnapshot {
    let npcs_here = npcs
        .iter()
        .map(|n| DemoNpcInfo {
            name: n.name.clone(),
            occupation: if n.introduced {
                Some(n.occupation.clone())
            } else {
                None
            },
            mood: n.mood.clone(),
        })
        .collect();

    // Adjacent locations: take only entries the map already considers
    // adjacent to the player (covers visited neighbours + frontier
    // neighbours). Anything beyond the frontier never appears.
    let adjacent = map
        .locations
        .iter()
        .filter(|loc| loc.adjacent && loc.id != map.player_location)
        .map(|loc| DemoAdjacentLocation {
            name: loc.name.clone(),
            // Carry the travel estimate for unexplored neighbours too — the map
            // now provides it (#1207 #33/#36), so the auto-player sees how far
            // an unvisited adjacent location is rather than a bare "unvisited".
            travel_minutes: loc.travel_minutes,
            visited: loc.visited,
        })
        .collect();

    DemoContextSnapshot {
        location_name: snapshot.location_name.clone(),
        location_description: snapshot.location_description.clone(),
        game_time,
        season,
        weather: snapshot.weather.clone(),
        npcs_here,
        adjacent,
        active_tasks: snapshot.active_tasks.clone(),
        recent_log: Vec::new(),
        recent_actions: Vec::new(),
        extra_prompt,
    }
}

/// Dedupes consecutive identical entries in `actions`, returning each unique
/// run with its repeat count. Used by [`render_user_prompt`] to surface
/// repetition signals to the LLM without flooding the prompt with copies.
///
/// Input is oldest-first; output preserves order. Empty/whitespace entries
/// are dropped to avoid `(last 0 turns)` artefacts.
fn collapse_consecutive_actions(actions: &[String]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for action in actions {
        let trimmed = action.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut()
            && last.0 == trimmed
        {
            last.1 += 1;
            continue;
        }
        out.push((trimmed.to_string(), 1));
    }
    out
}

/// Renders the demo snapshot into the user-prompt body shown to the LLM.
///
/// The format is deliberately sentence-shaped (no `Name (Title)` parentheses)
/// so the model can't mistake a metadata field for a vocative — issue #998
/// surfaced exactly that confusion when occupation was rendered as
/// `(Widow)`.
pub fn render_user_prompt(ctx: &DemoContextSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("Location: {}", ctx.location_name));
    if !ctx.location_description.is_empty() {
        parts.push(ctx.location_description.clone());
    }
    parts.push(format!("Date and time: {} | {}", ctx.game_time, ctx.season));
    // TODO #15: weather is already carried in `location_description` via the
    // {weather} placeholder substitution (14 of the 22 Rundale location
    // templates include it inline). A standalone `Weather: ...` line here
    // duplicated that signal for those locations and confused the demo
    // auto-player into "the weather is X. ... the weather is X." parroting.
    // For the 8 templates that don't carry weather inline, the prompt's
    // `Date and time:` line still anchors the broader sky/season context,
    // and the in-game NPC dialogue layer surfaces weather independently
    // via `ANACHRONISM ALERT` / NPC personality.

    if ctx.npcs_here.is_empty() {
        parts.push("NPCs here: none".to_string());
    } else {
        let lines: Vec<String> = ctx
            .npcs_here
            .iter()
            .map(|n| match &n.occupation {
                Some(occ) => format!("  - {}, {}, feeling {}", n.name, occ, n.mood),
                None => format!("  - {}, feeling {}", n.name, n.mood),
            })
            .collect();
        parts.push(format!("NPCs here:\n{}", lines.join("\n")));
    }

    if !ctx.adjacent.is_empty() {
        let lines: Vec<String> = ctx
            .adjacent
            .iter()
            .map(|a| match (a.visited, a.travel_minutes) {
                (true, Some(m)) => format!("  - {} — {} min away, visited", a.name, m),
                (true, None) => format!("  - {} — visited", a.name),
                (false, Some(m)) => {
                    format!("  - {} — about {} min away, unexplored", a.name, m)
                }
                (false, None) => format!("  - {} — unexplored", a.name),
            })
            .collect();
        parts.push(format!("Adjacent locations:\n{}", lines.join("\n")));
    }

    if !ctx.active_tasks.is_empty() {
        let lines: Vec<String> = ctx
            .active_tasks
            .iter()
            .map(|task| {
                format!(
                    "  - #{} [{}] {}",
                    task.id,
                    task.status_label().replace('_', " "),
                    task.description
                )
            })
            .collect();
        parts.push(format!("Active tasks:\n{}", lines.join("\n")));
    }

    let collapsed = collapse_consecutive_actions(&ctx.recent_actions);
    if !collapsed.is_empty() {
        let lines: Vec<String> = collapsed
            .iter()
            .map(|(text, count)| match count {
                1 => format!("  - {}", text),
                n => format!("  - {} (last {} turns)", text, n),
            })
            .collect();
        parts.push(format!(
            "Your last actions (do not repeat these):\n{}",
            lines.join("\n")
        ));
    }

    if !ctx.recent_log.is_empty() {
        let lines: Vec<String> = ctx.recent_log.iter().map(|l| format!("> {}", l)).collect();
        parts.push(format!("Recent events:\n{}", lines.join("\n")));
    }

    parts.push("Action (complete the JSON):\n{\"action\": \"".to_string());
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::types::{MapLocation, NpcInfo};

    fn world_snapshot() -> WorldSnapshot {
        WorldSnapshot {
            location_id: 15,
            location_name: "Kilteevan".to_string(),
            location_description: "A handful of whitewashed cottages.".to_string(),
            time_label: "morning".to_string(),
            hour: 8,
            minute: 30,
            weather: "clear sky".to_string(),
            season: "spring".to_string(),
            festival: None,
            paused: false,
            inference_paused: false,
            game_epoch_ms: 0.0,
            speed_factor: 1.0,
            name_hints: vec![],
            active_tasks: vec![],
            day_of_week: "Wednesday".to_string(),
            turn_in_flight: false,
        }
    }

    fn map_data(
        adjacent_unvisited: Vec<(&str, u16)>,
        adjacent_visited: Vec<(&str, u16)>,
    ) -> MapData {
        let mut locations = vec![MapLocation {
            id: "0".to_string(),
            name: "Kilteevan".to_string(),
            lat: 0.0,
            lon: 0.0,
            adjacent: false,
            hops: 0,
            indoor: Some(false),
            travel_minutes: None,
            visited: true,
        }];
        let mut next_id = 1u32;
        for (name, mins) in adjacent_visited {
            locations.push(MapLocation {
                id: next_id.to_string(),
                name: name.to_string(),
                lat: 0.0,
                lon: 0.0,
                adjacent: true,
                hops: 1,
                indoor: Some(false),
                travel_minutes: Some(mins),
                visited: true,
            });
            next_id += 1;
        }
        for (name, mins) in adjacent_unvisited {
            locations.push(MapLocation {
                id: next_id.to_string(),
                name: name.to_string(),
                lat: 0.0,
                lon: 0.0,
                adjacent: true,
                hops: 1,
                indoor: None,
                travel_minutes: Some(mins),
                visited: false,
            });
            next_id += 1;
        }
        MapData {
            locations,
            edges: vec![],
            player_location: "0".to_string(),
            edge_traversals: vec![],
            transport_label: "on foot".to_string(),
            transport_id: "walking".to_string(),
        }
    }

    fn unintroduced_widow() -> NpcInfo {
        NpcInfo {
            name: "a small, sharp-eyed old woman wrapped in a shawl".to_string(),
            real_name: "Peig Hannigan".to_string(),
            occupation: "Widow".to_string(),
            mood: "sharp".to_string(),
            introduced: false,
            mood_emoji: "😐".to_string(),
        }
    }

    fn introduced_publican() -> NpcInfo {
        NpcInfo {
            name: "Padraig O'Brien".to_string(),
            real_name: "Padraig O'Brien".to_string(),
            occupation: "Publican".to_string(),
            mood: "warm".to_string(),
            introduced: true,
            mood_emoji: "🙂".to_string(),
        }
    }

    #[test]
    fn pre_introduction_npc_hides_occupation_and_real_name() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[unintroduced_widow()],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        assert_eq!(ctx.npcs_here.len(), 1);
        let npc = &ctx.npcs_here[0];
        assert!(npc.occupation.is_none(), "occupation leaked pre-intro");
        assert_eq!(npc.name, "a small, sharp-eyed old woman wrapped in a shawl");
        assert!(
            !serde_json::to_string(npc).unwrap().contains("Peig"),
            "real name leaked into serialised payload"
        );
    }

    #[test]
    fn rendered_prompt_does_not_contain_widow_pre_intro() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[unintroduced_widow()],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let prompt = render_user_prompt(&ctx);
        assert!(
            !prompt.contains("Widow"),
            "occupation leaked into rendered prompt:\n{prompt}"
        );
        assert!(
            !prompt.contains("Peig"),
            "real name leaked into rendered prompt:\n{prompt}"
        );
        assert!(prompt.contains("a small, sharp-eyed old woman wrapped in a shawl"));
    }

    #[test]
    fn rendered_prompt_avoids_parenthetical_vocative_format() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[unintroduced_widow()],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let prompt = render_user_prompt(&ctx);
        let npc_block = prompt
            .split("NPCs here:")
            .nth(1)
            .and_then(|s| s.split("\n\n").next())
            .unwrap_or("");
        assert!(
            !npc_block.contains('('),
            "NPC block uses parens that can be misread as vocative:\n{npc_block}"
        );
    }

    #[test]
    fn introduced_npc_exposes_occupation() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[introduced_publican()],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let npc = &ctx.npcs_here[0];
        assert_eq!(npc.occupation.as_deref(), Some("Publican"));
        assert_eq!(npc.name, "Padraig O'Brien");
        let prompt = render_user_prompt(&ctx);
        assert!(prompt.contains("Padraig O'Brien, Publican, feeling warm"));
    }

    #[test]
    fn adjacent_frontier_carries_travel_estimate() {
        // #1207 #33/#36: unexplored adjacent locations now surface their travel
        // estimate (was previously dropped, leaving a bare "unvisited" with no
        // distance cue the auto-player could act on).
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec![("The Mill", 8)], vec![("The Crossroads", 12)]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let mill = ctx
            .adjacent
            .iter()
            .find(|a| a.name == "The Mill")
            .expect("frontier entry present");
        assert!(!mill.visited);
        assert_eq!(
            mill.travel_minutes,
            Some(8),
            "frontier entry should carry its travel estimate"
        );

        let xroads = ctx
            .adjacent
            .iter()
            .find(|a| a.name == "The Crossroads")
            .expect("visited entry present");
        assert!(xroads.visited);
        assert_eq!(xroads.travel_minutes, Some(12));

        let prompt = render_user_prompt(&ctx);
        assert!(prompt.contains("The Mill — about 8 min away, unexplored"));
        assert!(prompt.contains("The Crossroads — 12 min away, visited"));
    }

    #[test]
    fn active_tasks_are_visible_to_demo_player() {
        use chrono::{TimeZone, Utc};

        let mut snapshot = world_snapshot();
        snapshot.active_tasks.push(PlayerTaskSnapshot {
            id: 7,
            description: "weed the potato patch".to_string(),
            assigned_by: 11,
            location_id: 0,
            status: parish_types::TaskStatus::InProgress,
            assigned_at: Utc.with_ymd_and_hms(1820, 5, 14, 8, 0, 0).unwrap(),
            started_at: Some(Utc.with_ymd_and_hms(1820, 5, 14, 8, 30, 0).unwrap()),
            completed_at: None,
            last_matching_action: Some("I weed the potato patch".to_string()),
        });
        let ctx = build_demo_context(
            &snapshot,
            &[],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );

        assert_eq!(ctx.active_tasks, snapshot.active_tasks);
        let prompt = render_user_prompt(&ctx);
        assert!(
            prompt.contains("Active tasks:\n  - #7 [in progress] weed the potato patch"),
            "active task missing from demo prompt:\n{prompt}"
        );
    }

    #[test]
    fn locations_beyond_frontier_are_omitted() {
        // Map carries an extra entry that isn't adjacent — should be filtered.
        let mut map = map_data(vec![], vec![("The Crossroads", 12)]);
        map.locations.push(MapLocation {
            id: "99".to_string(),
            name: "Distant Hamlet".to_string(),
            lat: 0.0,
            lon: 0.0,
            adjacent: false,
            hops: 5,
            indoor: None,
            travel_minutes: None,
            visited: false,
        });
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map,
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        assert!(
            ctx.adjacent.iter().all(|a| a.name != "Distant Hamlet"),
            "non-adjacent location leaked"
        );
    }

    #[test]
    fn build_demo_context_initialises_empty_recent_actions() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        assert!(
            ctx.recent_actions.is_empty(),
            "Tauri/server build path must leave recent_actions empty; frontend fills it"
        );
    }

    /// TODO #15 — the standalone `Weather: ...` line was removed
    /// because every Rundale location template that mentions weather
    /// already substitutes `{weather}` inside its
    /// `location_description`. Keeping the duplicate line caused the
    /// demo auto-player to parrot weather adjectives.
    #[test]
    fn render_user_prompt_drops_standalone_weather_line() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let prompt = render_user_prompt(&ctx);
        assert!(
            !prompt.contains("Weather:"),
            "Weather: line should be absent — templates already carry weather inline:\n{prompt}"
        );
        // Date and time line still present.
        assert!(
            prompt.contains("Date and time:"),
            "Date and time: line must remain:\n{prompt}"
        );
    }

    #[test]
    fn render_user_prompt_omits_block_when_no_recent_actions() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        let prompt = render_user_prompt(&ctx);
        assert!(
            !prompt.contains("Your last actions"),
            "empty recent_actions must not render the header:\n{prompt}"
        );
    }

    /// #999 regression guard: when the auto-player has repeated the same
    /// greeting two turns in a row, the rendered prompt must show that
    /// repetition with a turn-count suffix so the model can break the loop.
    #[test]
    fn render_user_prompt_collapses_consecutive_recent_actions_with_count() {
        let mut ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec![], vec![]),
            "Wednesday".to_string(),
            "spring".to_string(),
            None,
        );
        ctx.recent_actions = vec![
            "Good morning".to_string(),
            "Good morning".to_string(),
            "go to the mill".to_string(),
        ];
        let prompt = render_user_prompt(&ctx);
        assert!(
            prompt.contains("Your last actions (do not repeat these):"),
            "missing header:\n{prompt}"
        );
        assert!(
            prompt.contains("Good morning (last 2 turns)"),
            "missing collapsed repeat:\n{prompt}"
        );
        assert!(
            prompt.contains("- go to the mill"),
            "missing singleton action:\n{prompt}"
        );
    }

    #[test]
    fn collapse_consecutive_actions_handles_runs_and_singletons() {
        let input = vec![
            "a".to_string(),
            "a".to_string(),
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "  ".to_string(), // whitespace dropped
            "a".to_string(),
        ];
        let collapsed = collapse_consecutive_actions(&input);
        assert_eq!(
            collapsed,
            vec![
                ("a".to_string(), 3),
                ("b".to_string(), 1),
                ("a".to_string(), 2),
            ]
        );
    }
}
