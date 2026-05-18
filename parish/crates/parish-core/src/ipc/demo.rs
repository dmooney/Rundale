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

use super::types::{MapData, NpcInfo, WorldSnapshot};

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
    /// Walking time in minutes. `None` for frontier entries (matches the GUI
    /// map tooltip, which hides travel time until the location has been
    /// visited).
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
    /// Recent text-log entries. The Tauri command leaves this empty; the
    /// frontend fills it from its `textLog` store before invoking the LLM.
    pub recent_log: Vec<String>,
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
            travel_minutes: if loc.visited {
                loc.travel_minutes
            } else {
                None
            },
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
        recent_log: Vec::new(),
        extra_prompt,
    }
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
    parts.push(format!("Weather: {}", ctx.weather));

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
                (false, _) => format!("  - {} — unvisited", a.name),
            })
            .collect();
        parts.push(format!("Adjacent locations:\n{}", lines.join("\n")));
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
            day_of_week: "Wednesday".to_string(),
        }
    }

    fn map_data(adjacent_unvisited: Vec<&str>, adjacent_visited: Vec<(&str, u16)>) -> MapData {
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
        for name in adjacent_unvisited {
            locations.push(MapLocation {
                id: next_id.to_string(),
                name: name.to_string(),
                lat: 0.0,
                lon: 0.0,
                adjacent: true,
                hops: 1,
                indoor: None,
                travel_minutes: None,
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
    fn adjacent_frontier_omits_travel_minutes() {
        let ctx = build_demo_context(
            &world_snapshot(),
            &[],
            &map_data(vec!["The Mill"], vec![("The Crossroads", 12)]),
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
        assert!(
            mill.travel_minutes.is_none(),
            "travel_minutes leaked on frontier"
        );

        let xroads = ctx
            .adjacent
            .iter()
            .find(|a| a.name == "The Crossroads")
            .expect("visited entry present");
        assert!(xroads.visited);
        assert_eq!(xroads.travel_minutes, Some(12));

        let prompt = render_user_prompt(&ctx);
        assert!(prompt.contains("The Mill — unvisited"));
        assert!(prompt.contains("The Crossroads — 12 min away, visited"));
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
}
