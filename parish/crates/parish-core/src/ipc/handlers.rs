//! Pure handler functions that build IPC types from game state.
//!
//! These are consumed by both the Tauri desktop backend and the axum web
//! server, keeping game-logic → IPC-type mapping in a single place.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Timelike};

use crate::game_mod::PronunciationEntry;
use crate::npc::anachronism;
use crate::npc::manager::NpcManager;
use crate::npc::mood::mood_emoji;
use crate::npc::ticks;
use crate::npc::{LanguageHint, LanguageSettings, Npc, NpcId};
use crate::world::description::render_description;
use crate::world::transport::TransportMode;
use crate::world::{LocationId, WorldState};

use super::types::{
    MapData, MapLocation, NpcInfo, PlayerTaskSnapshot, ReconnectState, TextLogPayload,
    WorldSnapshot,
};

/// Convert a chrono weekday to its English name.
///
/// Re-exported from [`parish_types::time::weekday_name`] so the IPC layer keeps
/// its existing `crate::ipc::handlers::weekday_name` path; the implementation
/// lives in the lowest leaf crate, shared with `parish-diagnostics`.
pub(crate) use parish_types::time::weekday_name;

/// Projects non-completed tasks into the shared player-facing IPC shape.
///
/// [`PlayerProgress::active_tasks`](parish_types::PlayerProgress::active_tasks)
/// preserves assignment order, so every runtime and QA surface sees the same
/// deterministic ordering.
pub fn active_task_snapshots(world: &WorldState) -> Vec<PlayerTaskSnapshot> {
    world
        .player_progress
        .active_tasks()
        .map(PlayerTaskSnapshot::from)
        .collect()
}

/// Builds a [`WorldSnapshot`] from the current world state.
pub fn snapshot_from_world(world: &WorldState) -> WorldSnapshot {
    let now = world.clock.now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let tod = world.clock.time_of_day();
    let season = world.clock.season();
    let festival = world.clock.check_festival().map(|f| f.to_string());
    let weather_str = world.weather.to_string();

    let loc = world.current_location();
    let description = if let Some(data) = world.current_location_data() {
        render_description(data, tod, &weather_str, &[])
    } else {
        loc.description.clone()
    };

    let day_of_week = weekday_name(now.weekday()).to_string();

    WorldSnapshot {
        location_id: world.player_location.0,
        location_name: loc.name.clone(),
        location_description: description,
        time_label: tod.to_string(),
        hour,
        minute,
        weather: weather_str,
        season: season.to_string(),
        festival,
        paused: world.clock.is_paused(),
        inference_paused: world.clock.is_inference_paused(),
        game_epoch_ms: now.timestamp_millis() as f64,
        speed_factor: world.clock.speed_factor(),
        name_hints: vec![],
        active_tasks: active_task_snapshots(world),
        day_of_week,
        // The world alone does not know whether an NPC conversation turn is in
        // flight — that lives in `ConversationRuntimeState`. The reconnect-
        // resync snapshot endpoint (`GET /api/world-snapshot`) overrides this
        // from `conversation_in_progress`; everywhere else it stays `false`.
        turn_in_flight: false,
    }
}

/// Builds an all-or-nothing reconnect replacement from one state generation.
pub fn build_reconnect_state(
    world: &WorldState,
    npc_manager: &NpcManager,
    transport: &TransportMode,
    reveal_unexplored_locations: bool,
    pronunciations: &[PronunciationEntry],
    turn_in_flight: bool,
) -> ReconnectState {
    let mut world_snapshot = snapshot_from_world(world);
    world_snapshot.name_hints = compute_name_hints(world, npc_manager, pronunciations);
    world_snapshot.turn_in_flight = turn_in_flight;
    ReconnectState {
        world: world_snapshot,
        map: build_map_data(world, transport, reveal_unexplored_locations),
        npcs: build_npcs_here(world, npc_manager),
        context_epoch: world.event_bus.context_epoch(),
    }
}

/// Builds the [`MapData`] with fog-of-war: visited locations plus the frontier.
///
/// Visited locations are fully enriched. The "frontier" — unvisited locations
/// adjacent to any visited location — also appears so the player can see
/// where they could explore next. Frontier locations are marked with
/// `visited: false` and have limited tooltip data.
pub fn build_map_data(
    world: &WorldState,
    transport: &TransportMode,
    reveal_unexplored_locations: bool,
) -> MapData {
    let speed_m_per_s = transport.speed_m_per_s;
    let player_loc = world.player_location;
    let visited = &world.visited_locations;

    let adjacent_ids: HashSet<LocationId> = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let hop_map = world.graph.hop_distances(player_loc);

    // Single-pass BFS: compute travel time from the player to every reachable
    // location at once, instead of running a separate BFS per visited location.
    let travel_time_map = world.graph.travel_times_from(player_loc, speed_m_per_s);

    // Frontier: by default only unvisited locations neighboring the visited set.
    // With reveal mode enabled, include all unvisited locations.
    let mut frontier: HashSet<LocationId> = HashSet::new();
    if reveal_unexplored_locations {
        for id in world.graph.location_ids() {
            if !visited.contains(&id) {
                frontier.insert(id);
            }
        }
    } else {
        for &v in visited {
            for (neighbor_id, _) in world.graph.neighbors(v) {
                if !visited.contains(&neighbor_id) {
                    frontier.insert(neighbor_id);
                }
            }
        }
    }

    // Build visited locations (fully enriched).
    //
    // Perf: iterate `visited` directly instead of fetching every id in the
    // graph and filtering. Under fog-of-war the visited set is usually far
    // smaller than the full graph, so this skips a `Vec<LocationId>`
    // allocation and |graph| - |visited| filter rejections per call.
    let mut locations: Vec<MapLocation> = visited
        .iter()
        .copied()
        .filter_map(|id| world.graph.get(id).map(|data| (id, data)))
        .map(|(id, data)| {
            let travel_minutes = if id == player_loc {
                None
            } else {
                travel_time_map.get(&id).copied()
            };

            MapLocation {
                id: id.0.to_string(),
                name: data.name.clone(),
                lat: data.lat,
                lon: data.lon,
                adjacent: adjacent_ids.contains(&id) || id == player_loc,
                hops: *hop_map.get(&id).unwrap_or(&u32::MAX),
                indoor: Some(data.indoor),
                travel_minutes,
                visited: true,
            }
        })
        .collect();

    // Append frontier locations (limited info). Frontier entries are unvisited
    // but reachable; surface the BFS travel-time estimate so the player (and the
    // demo auto-player) can judge how far an unexplored neighbour is and choose
    // to travel there. The estimate is already computed in `travel_time_map`;
    // discarding it left adjacent-but-unexplored locations as bare "unvisited"
    // with no distance cue (#1207 findings #33/#36).
    for id in &frontier {
        if let Some(data) = world.graph.get(*id) {
            locations.push(MapLocation {
                id: id.0.to_string(),
                name: data.name.clone(),
                lat: data.lat,
                lon: data.lon,
                adjacent: adjacent_ids.contains(id),
                hops: *hop_map.get(id).unwrap_or(&u32::MAX),
                indoor: None,
                travel_minutes: travel_time_map.get(id).copied(),
                visited: false,
            });
        }
    }

    // Edges: between any two locations that are both visible (visited or frontier).
    //
    // Perf: iterate `visible` directly rather than scanning every location in
    // the graph. This avoids an extra `Vec<LocationId>` allocation and drops
    // the per-iteration `visible.contains(&loc_id)` rejection check — only
    // the inner `visible.contains(&neighbor_id)` guard is still required.
    let visible: HashSet<LocationId> = visited.union(&frontier).copied().collect();
    let mut edges: Vec<(String, String)> = Vec::new();
    for &loc_id in &visible {
        for (neighbor_id, _conn) in world.graph.neighbors(loc_id) {
            if loc_id.0 < neighbor_id.0 && visible.contains(&neighbor_id) {
                edges.push((loc_id.0.to_string(), neighbor_id.0.to_string()));
            }
        }
    }

    // Edge traversal counts for footprint rendering
    let edge_traversals: Vec<(String, String, u32)> = world
        .edge_traversals
        .iter()
        .filter(|((a, b), _)| visible.contains(a) && visible.contains(b))
        .map(|((a, b), count)| (a.0.to_string(), b.0.to_string(), *count))
        .collect();

    MapData {
        locations,
        edges,
        player_location: player_loc.0.to_string(),
        edge_traversals,
        transport_label: transport.label.clone(),
        transport_id: transport.id.clone(),
    }
}

/// Builds a [`TravelStartPayload`] from a movement path.
///
/// Extracts lat/lon coordinates from the world graph for each waypoint
/// so the frontend can animate the player's travel along the path.
pub fn build_travel_start(
    path: &[crate::world::LocationId],
    minutes: u16,
    graph: &crate::world::graph::WorldGraph,
) -> super::types::TravelStartPayload {
    let waypoints = path
        .iter()
        .filter_map(|id| {
            graph.get(*id).map(|data| super::types::TravelWaypoint {
                id: id.0.to_string(),
                lat: data.lat,
                lon: data.lon,
            })
        })
        .collect();

    let from = path
        .first()
        .and_then(|id| graph.get(*id))
        .map(|data| data.name.clone())
        .unwrap_or_default();
    let last = path.last();
    let to = last
        .and_then(|id| graph.get(*id))
        .map(|data| data.name.clone())
        .unwrap_or_default();
    let destination = last.map(|id| id.0.to_string()).unwrap_or_default();

    super::types::TravelStartPayload {
        from,
        to,
        waypoints,
        duration_minutes: minutes,
        destination,
    }
}

/// Builds the list of [`NpcInfo`] for NPCs at the player's current location.
pub fn build_npcs_here(world: &WorldState, npc_manager: &NpcManager) -> Vec<NpcInfo> {
    let npcs = npc_manager.npcs_at(world.player_location);
    npcs.into_iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            NpcInfo {
                npc_id: npc.id.0,
                name: npc_manager.display_name(npc).to_string(),
                real_name: npc.name.clone(),
                occupation: npc.occupation.clone(),
                mood_emoji: mood_emoji(&npc.mood).to_string(),
                mood: npc.mood.clone(),
                introduced,
            }
        })
        .collect()
}

/// Capitalizes the first character of a string slice.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Maximum number of NPC LLM inference calls that may run concurrently within
/// a single `emit_npc_reactions` batch (#406).
///
/// Shared by all three runtimes (Tauri, axum, headless CLI) so a change here
/// applies everywhere without drift.
pub const NPC_REACTION_CONCURRENCY: usize = 4;

/// Monotonically increasing request ID counter for inference requests.
///
/// All three runtimes share this counter via `parish-core` so that request IDs
/// are unique across the entire process. Uses `SeqCst` ordering (the safest
/// choice) so callers need not reason about visibility guarantees.
pub static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Monotonically increasing message ID counter for text-log entries.
static MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Creates a [`TextLogPayload`] with an auto-generated unique message ID.
pub fn text_log(source: impl Into<String>, content: impl Into<String>) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        stream_turn_id: None,
        source: source.into(),
        content: content.into(),
        subtype: None,
    }
}

/// Creates a [`TextLogPayload`] tied to a specific NPC stream turn.
pub fn text_log_for_stream_turn(
    source: impl Into<String>,
    content: impl Into<String>,
    stream_turn_id: u64,
) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        stream_turn_id: Some(stream_turn_id),
        source: source.into(),
        content: content.into(),
        subtype: None,
    }
}

/// Creates a [`TextLogPayload`] tied to a specific NPC stream turn with a
/// semantic subtype for frontend styling (e.g. `"action"` for non-verbal
/// reactions such as gestures).
pub fn text_log_for_stream_turn_typed(
    source: impl Into<String>,
    content: impl Into<String>,
    stream_turn_id: u64,
    subtype: impl Into<String>,
) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        stream_turn_id: Some(stream_turn_id),
        source: source.into(),
        content: content.into(),
        subtype: Some(subtype.into()),
    }
}

/// Creates a [`TextLogPayload`] with a semantic subtype for frontend styling.
pub fn text_log_typed(
    source: impl Into<String>,
    content: impl Into<String>,
    subtype: impl Into<String>,
) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        stream_turn_id: None,
        source: source.into(),
        content: content.into(),
        subtype: Some(subtype.into()),
    }
}

/// One spoken line in a local conversation transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationLine {
    /// Speaker label shown to the player.
    pub speaker: String,
    /// Spoken text content.
    pub text: String,
}

/// Ordered NPC recipients extracted from player input at the current location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionedNpcs {
    /// Mentioned NPC display names, deduplicated while preserving order.
    pub names: Vec<String>,
    /// Remaining player text. Explicit `@mentions` are stripped; natural
    /// free-text name mentions are retained as part of the utterance.
    pub remaining: String,
}

fn canonicalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn mention_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ':' | ';'),
    }
}

#[derive(Debug, Clone)]
struct NpcMentionCandidate {
    text: String,
    target: String,
}

fn add_npc_mention_candidate(candidates: &mut Vec<(String, String)>, text: &str, target: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    candidates.push((text.to_string(), target.to_string()));
}

fn unambiguous_npc_mention_candidates(
    candidates: Vec<(String, String)>,
) -> Vec<NpcMentionCandidate> {
    let mut targets_by_text: HashMap<String, HashSet<String>> = HashMap::new();
    for (text, target) in &candidates {
        targets_by_text
            .entry(text.to_lowercase())
            .or_default()
            .insert(target.to_lowercase());
    }

    let mut seen = HashSet::new();
    let mut filtered = Vec::new();
    for (text, target) in candidates {
        let key = text.to_lowercase();
        if targets_by_text
            .get(&key)
            .is_some_and(|targets| targets.len() == 1)
            && seen.insert(key)
        {
            filtered.push(NpcMentionCandidate { text, target });
        }
    }

    filtered
}

fn npc_location_mention_candidate_pairs(
    world: &WorldState,
    npc_manager: &NpcManager,
) -> Vec<(String, String)> {
    let mut candidates = Vec::new();

    for npc in npc_manager.npcs_at(world.player_location) {
        let display = npc_manager.display_name(npc);
        add_npc_mention_candidate(&mut candidates, display, display);

        if npc_manager.is_introduced(npc.id) {
            add_npc_mention_candidate(&mut candidates, &npc.name, &npc.name);

            if let Some(first_name) = npc.name.split_whitespace().next() {
                add_npc_mention_candidate(&mut candidates, first_name, &npc.name);
            }
        }
    }

    candidates
}

fn normalized_query_tokens(raw: &str) -> Vec<String> {
    raw.chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '\'' {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

fn is_explicit_roster_presence_query(raw: &str) -> bool {
    let tokens = normalized_query_tokens(raw);
    if tokens.is_empty() {
        return false;
    }

    let starts_with_presence_question = matches!(
        tokens.first().map(String::as_str),
        Some("is" | "are" | "was" | "were")
    ) && tokens
        .iter()
        .any(|token| matches!(token.as_str(), "here" | "about" | "nearby"));
    let asks_where = tokens.windows(2).any(|window| {
        matches!(
            window,
            [first, second]
                if first == "where" && matches!(second.as_str(), "is" | "are")
        )
    });
    let asks_seen = tokens
        .windows(3)
        .any(|window| matches!(window, [a, b, c] if a == "have" && b == "you" && c == "seen"));

    starts_with_presence_question || asks_where || asks_seen
}

fn name_without_religious_title(name: &str) -> String {
    let mut parts = name.split_whitespace().collect::<Vec<_>>();
    while parts.first().is_some_and(|part| {
        matches!(
            part.trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_ascii_lowercase()
                .as_str(),
            "fr" | "father"
        )
    }) {
        parts.remove(0);
    }
    parts.join(" ")
}

fn is_priest_occupation(occupation: &str) -> bool {
    occupation
        .split_whitespace()
        .any(|token| token.eq_ignore_ascii_case("priest"))
}

fn roster_presence_mention_candidate_pairs(npc_manager: &NpcManager) -> Vec<(String, String)> {
    let mut candidates = Vec::new();

    for npc in npc_manager.all_npcs() {
        add_npc_mention_candidate(&mut candidates, &npc.name, &npc.name);
        add_npc_mention_candidate(&mut candidates, &npc.occupation, &npc.name);
        add_npc_mention_candidate(
            &mut candidates,
            &format!("the {}", npc.occupation),
            &npc.name,
        );

        let untitled = name_without_religious_title(&npc.name);
        if !untitled.is_empty() && !untitled.eq_ignore_ascii_case(&npc.name) {
            add_npc_mention_candidate(&mut candidates, &untitled, &npc.name);
        }

        if let Some(first_name) = untitled
            .split_whitespace()
            .next()
            .or_else(|| npc.name.split_whitespace().next())
        {
            add_npc_mention_candidate(&mut candidates, first_name, &npc.name);

            if is_priest_occupation(&npc.occupation) {
                add_npc_mention_candidate(
                    &mut candidates,
                    &format!("Father {first_name}"),
                    &npc.name,
                );
                add_npc_mention_candidate(&mut candidates, &format!("Fr. {first_name}"), &npc.name);
                add_npc_mention_candidate(&mut candidates, &format!("Fr {first_name}"), &npc.name);
            }
        }

        if is_priest_occupation(&npc.occupation) {
            add_npc_mention_candidate(&mut candidates, "Father", &npc.name);
            add_npc_mention_candidate(&mut candidates, "the priest", &npc.name);
            if !untitled.is_empty() {
                add_npc_mention_candidate(
                    &mut candidates,
                    &format!("Father {untitled}"),
                    &npc.name,
                );
                add_npc_mention_candidate(&mut candidates, &format!("Fr. {untitled}"), &npc.name);
                add_npc_mention_candidate(&mut candidates, &format!("Fr {untitled}"), &npc.name);
            }
        }
    }

    candidates
}

fn npc_mention_candidates(
    raw: &str,
    world: &WorldState,
    npc_manager: &NpcManager,
) -> Vec<NpcMentionCandidate> {
    let mut candidates = npc_location_mention_candidate_pairs(world, npc_manager);

    if is_explicit_roster_presence_query(raw) {
        candidates.extend(roster_presence_mention_candidate_pairs(npc_manager));
    }

    unambiguous_npc_mention_candidates(candidates)
}

fn find_natural_npc_mentions(
    raw: &str,
    candidates: &[NpcMentionCandidate],
    excluded_spans: &[(usize, usize, String)],
) -> Vec<(usize, usize, String)> {
    let raw_lower = raw.to_ascii_lowercase();
    let mut spans: Vec<(usize, usize, String)> = Vec::new();

    for candidate in candidates {
        let needle = candidate.text.to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }

        let mut cursor = 0usize;
        while cursor < raw_lower.len() {
            let Some(rel_start) = raw_lower[cursor..].find(&needle) else {
                break;
            };
            let start = cursor + rel_start;
            let end = start + candidate.text.len();
            let overlaps_excluded =
                excluded_spans
                    .iter()
                    .any(|(excluded_start, excluded_end, _)| {
                        start < *excluded_end && end > *excluded_start
                    });
            if overlaps_excluded || raw.get(start..end).is_none() {
                cursor = end;
                continue;
            }

            let before = if start == 0 {
                None
            } else {
                raw[..start].chars().next_back()
            };
            let after = raw[end..].chars().next();

            if mention_boundary(before) && mention_boundary(after) {
                spans.push((start, end, candidate.target.clone()));
            }

            cursor = end;
        }
    }

    spans.sort_by(|(a_start, a_end, _), (b_start, b_end, _)| {
        a_start
            .cmp(b_start)
            .then_with(|| (b_end - b_start).cmp(&(a_end - a_start)))
    });

    let mut filtered = Vec::new();
    let mut last_end = 0usize;
    for (start, end, target) in spans {
        if start >= last_end {
            filtered.push((start, end, target));
            last_end = end;
        }
    }

    filtered
}

/// Extracts all valid `@mentions` that match NPCs at the player's location.
///
/// Matching is done against the NPCs currently present. Explicit `@mentions`
/// and natural free-text names match introduced full/first names and visible
/// display names, so `Padraig`, `Padraig Darcy`, and multi-word lowercase
/// descriptions like `an older man behind the bar` remain parseable. Ambiguous
/// mention text is ignored rather than routed to an arbitrary co-located NPC.
/// Explicit presence/where/seen questions additionally match unambiguous
/// full-roster names and role titles so "Is Father Declan here?" can report
/// the named person's absence instead of falling through as ambient input.
pub fn extract_npc_mentions(
    raw: &str,
    world: &WorldState,
    npc_manager: &NpcManager,
) -> MentionedNpcs {
    let candidates = npc_mention_candidates(raw, world, npc_manager);

    if candidates.is_empty() {
        return MentionedNpcs {
            names: vec![],
            remaining: raw.trim().to_string(),
        };
    }

    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel_at) = raw[cursor..].find('@') {
        let at = cursor + rel_at;
        let before_ok = at == 0
            || match raw[..at].chars().next_back() {
                None => true,
                Some(ch) => ch.is_whitespace(),
            };
        if !before_ok {
            cursor = at + 1;
            continue;
        }

        let rest = &raw[at + 1..];
        let mut matched: Option<(usize, String)> = None;
        for candidate in &candidates {
            if rest.len() < candidate.text.len() {
                continue;
            }
            let candidate_len = candidate.text.len();
            let Some(text) = rest.get(..candidate_len) else {
                continue;
            };
            let Some(after) = rest.get(candidate_len..) else {
                continue;
            };
            if text.eq_ignore_ascii_case(&candidate.text) && mention_boundary(after.chars().next())
            {
                match &matched {
                    Some((len, _)) if *len >= candidate_len => {}
                    _ => matched = Some((candidate_len, candidate.target.clone())),
                }
            }
        }

        if let Some((name_len, name)) = matched {
            spans.push((at, at + 1 + name_len, name));
            cursor = at + 1 + name_len;
        } else {
            cursor = at + 1;
        }
    }

    let natural_spans = find_natural_npc_mentions(raw, &candidates, &spans);

    if spans.is_empty() {
        if natural_spans.is_empty() {
            return MentionedNpcs {
                names: vec![],
                remaining: raw.trim().to_string(),
            };
        }

        let mut names = Vec::new();
        let mut dedupe = HashSet::new();
        for (_, _, name) in natural_spans {
            if dedupe.insert(name.to_lowercase()) {
                names.push(name);
            }
        }
        return MentionedNpcs {
            names,
            remaining: raw.trim().to_string(),
        };
    }

    let mut name_spans = spans.clone();
    name_spans.extend(natural_spans);
    name_spans.sort_by_key(|(start, _, _)| *start);

    let mut names = Vec::new();
    let mut dedupe = HashSet::new();
    for (_, _, name) in name_spans {
        if dedupe.insert(name.to_lowercase()) {
            names.push(name);
        }
    }

    let mut remaining = String::new();
    let mut last = 0usize;
    for (start, end, _) in spans {
        remaining.push_str(&raw[last..start]);
        remaining.push(' ');
        last = end;
    }
    remaining.push_str(&raw[last..]);

    MentionedNpcs {
        names,
        remaining: canonicalize_whitespace(&remaining),
    }
}

/// Resolves ordered conversation targets from extracted display names.
///
/// Falls back to the first NPC at the current location only when no names
/// were supplied at all. If names were given but none match a co-located
/// NPC, returns empty so callers can surface "no one here by that name"
/// rather than silently routing to whoever happens to be present.
///
/// Note: this convenience wrapper preserves the fallback even when the caller
/// supplied names but none matched a co-located NPC. New call sites that need
/// to distinguish "no names" from "named but absent" — so they can emit a
/// "{name} is not here." system message instead of letting the wrong NPC speak
/// (#985) — should use [`resolve_addressed_targets`] instead.
pub fn resolve_npc_targets(
    world: &WorldState,
    npc_manager: &NpcManager,
    target_names: &[String],
) -> Vec<NpcId> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for name in target_names {
        // Primary: literal name (exact or first-name prefix).
        // Fallback: occupation/role vocative ("Father", "Widow") when
        // exactly one co-located NPC matches that role — issue #998.
        let resolved = npc_manager
            .find_by_name(name, world.player_location)
            .or_else(|| npc_manager.find_by_role_at(name, world.player_location));
        if let Some(npc) = resolved
            && seen.insert(npc.id)
        {
            targets.push(npc.id);
        }
    }

    if targets.is_empty()
        && target_names.is_empty()
        && let Some(npc) = npc_manager
            .npcs_at(world.player_location)
            .into_iter()
            .next()
    {
        targets.push(npc.id);
    }

    targets
}

/// Result of resolving an explicitly-addressed set of conversation targets.
///
/// Unlike [`resolve_npc_targets`], this does **not** silently fall back to the
/// first co-located NPC when every named target is absent. Instead, callers
/// can inspect [`AddressedTargets::absent`] and inform the player which named
/// NPC was not at the current location (#985).
#[derive(Debug, Default, Clone)]
pub struct AddressedTargets {
    /// NPC ids that matched a co-located NPC, in the order names were supplied
    /// (deduplicated).
    pub resolved: Vec<NpcId>,
    /// Display names that did not match any co-located NPC, in order of first
    /// occurrence (deduplicated by case-insensitive comparison).
    pub absent: Vec<String>,
}

/// Resolves explicitly-addressed conversation targets without a fallback.
///
/// For every name in `target_names`:
/// - If it matches a co-located NPC, its id is appended to `resolved`.
/// - Otherwise the name is appended to `absent` so the caller can surface
///   "{name} is not here." rather than let a different co-located NPC reply.
///
/// Both lists preserve the caller's name order and deduplicate
/// (case-insensitively for `absent`).
pub fn resolve_addressed_targets(
    world: &WorldState,
    npc_manager: &NpcManager,
    target_names: &[String],
) -> AddressedTargets {
    let mut resolved = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut absent = Vec::new();
    let mut seen_absent = HashSet::new();
    for name in target_names {
        // Primary: literal name (exact or first-name prefix).
        // Fallback: occupation/role vocative ("Father", "Widow") when
        // exactly one co-located NPC matches that role — mirrors the same
        // fallback in `resolve_npc_targets` so explicit `addressed_to`
        // names resolve identically regardless of which resolver is used
        // (#1221).
        let npc = npc_manager
            .find_by_name(name, world.player_location)
            .or_else(|| npc_manager.find_by_role_at(name, world.player_location));
        if let Some(npc) = npc {
            if seen_ids.insert(npc.id) {
                resolved.push(npc.id);
            }
        } else if seen_absent.insert(name.to_lowercase()) {
            absent.push(name.clone());
        }
    }
    AddressedTargets { resolved, absent }
}

fn append_transcript_context(
    context: &mut String,
    transcript: &[ConversationLine],
    player_label: &str,
    current_player_input: &str,
) {
    let current_trimmed = current_player_input.trim();
    // Exclude the player's current message — it's already been pushed to the transcript
    // before this call (commands.rs), but it will be rendered separately below as the
    // triggering "just said" line. Showing it in both places creates duplication.
    let lines: Vec<&ConversationLine> = transcript
        .iter()
        .filter(|line| {
            !(line.text.trim().is_empty()
                || line.speaker == "You" && line.text.trim() == current_trimmed)
        })
        .collect();
    if lines.is_empty() {
        return;
    }

    context.push_str("\n\nRecent conversation here:\n");
    for line in &lines {
        // "You" in the transcript refers to the player (the caller's perspective),
        // but from the NPC's perspective "You" = the NPC themselves. Remap it to
        // the player's name so the NPC doesn't mistake the player's words for their own.
        let speaker = if line.speaker == "You" {
            player_label
        } else {
            line.speaker.as_str()
        };
        context.push_str(&format!("- {}: {}\n", speaker, line.text.trim()));
    }
    // No CTA here — the caller appends the triggering "just said" line and CTA after.
}

/// Neutral fallback shown when NPC inference fails and the active base mod
/// provides no `inference_failure_messages` of its own. Themed flavour
/// (e.g. Rundale's Hiberno-English atmosphere) belongs in the mod.
///
/// Indexed by `request_id % len` so different attempts get different messages.
pub const INFERENCE_FAILURE_MESSAGES: &[&str] = &["…"];

/// Neutral fallback shown when no NPC is present and the active base mod
/// provides no `idle_messages` of its own. Themed flavour belongs in the
/// mod.
pub const IDLE_MESSAGES: &[&str] = &[""];

/// Helper to mask an API key for display (shows first 4 and last 4 chars).
pub fn mask_key(key: &str) -> String {
    let char_count = key.chars().count();
    if char_count > 8 {
        let prefix: String = key.chars().take(4).collect();
        let suffix: String = key.chars().skip(char_count - 4).collect();
        format!("{}...{}", prefix, suffix)
    } else {
        "(set, too short to mask)".to_string()
    }
}

// ── NPC conversation setup ──────────────────────────────────────────────────

/// Data needed to start an NPC conversation, returned by [`prepare_npc_conversation`].
#[derive(Debug, Clone)]
pub struct NpcConversationSetup {
    /// Display name of the NPC (for UI labels — may be a brief description if not introduced).
    pub display_name: String,
    /// Actual NPC name (always the real name, used for conversation log speaker_name).
    pub npc_name: String,
    /// NPC's unique ID.
    pub npc_id: NpcId,
    /// The assembled system prompt for the LLM.
    pub system_prompt: String,
    /// The assembled context string for the LLM.
    pub context: String,
    /// Names from the full parish person registry plus the player when known.
    /// Used by the post-generation person-confirmation guard (#1459) to detect
    /// when the NPC's reply affirms a fabricated person not on this list.
    pub known_person_names: Vec<String>,
    /// Full roster as (name, occupation) pairs, including the speaker.
    /// Used by the wrong-speaker-identity guard (#1475) to detect when this
    /// NPC's reply claims to be a different roster member.
    pub roster_names_occupations: Vec<(String, String)>,
    /// Current player location name.
    /// Used by the wrong-location-reference guard (#1477) to detect when an NPC
    /// names a different settlement in "here in X" / "village of X" collocations.
    pub location_name: String,
    /// All known location names in the world graph.
    /// Used by the invented-place-confirmation guard (#1530) to detect when an
    /// NPC confirms a place name that does not exist in the world.
    pub known_location_names: Vec<String>,
    /// Player's name as currently known from world state.
    /// Passed through to post-generation guards so the player's own name is
    /// never treated as a fabricated third-party person (#1553).
    pub player_name: Option<String>,
    /// Whether this NPC already had a canonical conversation exchange with the
    /// player before the current turn. Kept separate from identity knowledge:
    /// an NPC can have met the player without having said their name (#1776,
    /// #1786).
    pub had_prior_exchange: bool,
    /// Canonical authored work facts for post-generation referral validation:
    /// `(name, occupation, workplace name)`.
    pub work_roster: Vec<(String, String, Option<String>)>,
}

/// Prepares a specific NPC's turn in an ongoing conversation.
///
/// The supplied `player_input` describes the current trigger for this turn,
/// while `transcript` carries the recent local exchange for continuity.
/// `npc_cfg` is forwarded to the prompt builders so runtime feature-flag
/// overrides (e.g. `dialogue-quality-continuity` kill-switch and
/// `npc-dialogue-grounding`) take effect.
// TD-029 migrated most params to `Tier1ContextParams`; both the
// `dialogue_quality_continuity` (#1387/#1388) and `grounding_enabled` (#1394)
// flags now live on `NpcConfig` and are threaded via `npc_cfg`.
#[allow(clippy::too_many_arguments)]
pub fn prepare_npc_conversation_turn(
    world: &WorldState,
    npc_manager: &mut NpcManager,
    player_input: &str,
    speaker_id: NpcId,
    transcript: &[ConversationLine],
    improv_enabled: bool,
    language: &LanguageSettings,
    npc_cfg: &crate::config::NpcConfig,
) -> Option<NpcConversationSetup> {
    let npc = npc_manager.get(speaker_id)?.clone();
    // Identity knowledge and prior contact are separate. Merely beginning an
    // exchange must not reveal the NPC's authored name (#1776); the shared
    // apply seam marks identity only after the delivered dialogue explicitly
    // establishes it. Prior-contact grounding uses the conversation log
    // independently so an unnamed NPC does not claim this is a first meeting
    // forever (#1786).
    let was_introduced = npc_manager.is_introduced(speaker_id);
    let had_prior_exchange = world.conversation_log.has_exchange_with(speaker_id);
    let display_name = npc_manager.display_name(&npc).to_string();
    let other_npcs: Vec<&Npc> = npc_manager
        .npcs_at(world.player_location)
        .into_iter()
        .filter(|other| other.id != npc.id)
        .collect();

    let npc_names: std::collections::HashMap<NpcId, String> = npc_manager
        .all_npcs()
        .map(|n| (n.id, n.name.clone()))
        .collect();
    // Determine if this NPC knows the player's name
    let player_name_for_npc = if npc_manager.knows_player_name(speaker_id) {
        world.player_name.as_deref()
    } else {
        None
    };

    // Build roster; if NPC knows the player, inject the player at the front
    // so they appear in PEOPLE YOU KNOW with a clear "currently speaking with" note.
    let mut roster = npc_manager.known_roster(&npc);
    if let Some(pname) = player_name_for_npc {
        roster.insert(
            0,
            (
                NpcId(0),
                pname.to_string(),
                "newcomer to the parish".to_string(),
            ),
        );
    }
    // Location grounding (#1394): build the place-name list from the world
    // graph when grounding is enabled, so the system prompt can instruct the
    // NPC not to confirm nonexistent places/people.
    let location_names: Option<Vec<String>> = if npc_cfg.grounding_enabled {
        let mut names: Vec<String> = world
            .graph
            .location_ids()
            .into_iter()
            .filter_map(|id| world.graph.get(id).map(|d| d.name.clone()))
            .collect();
        names.sort();
        Some(names)
    } else {
        None
    };
    // Prompt grounding (#1563): the "PEOPLE YOU KNOW" block is the model's
    // primary allow-list for real names. The personal relationship roster is
    // too small for that purpose: a real parish-wide figure absent from this
    // NPC's local roster (e.g. a publican) can otherwise be denied as
    // nonexistent. Keep relationship entries first, then append every other
    // real parish NPC as a "real parish person" entry so the model may
    // recognise the name without claiming close acquaintance.
    let mut prompt_roster = roster.clone();
    for (id, _, descriptor) in &mut prompt_roster {
        if id.0 == 0 {
            continue;
        }
        if let Some(workplace_name) = npc_manager
            .get(*id)
            .and_then(|person| person.workplace)
            .and_then(|location_id| world.graph.get(location_id))
            .map(|location| location.name.as_str())
        {
            descriptor.push_str(&format!("; workplace: {workplace_name}"));
        }
    }
    if npc_cfg.grounding_enabled {
        let mut parish_people: Vec<(NpcId, String, String)> = npc_manager
            .all_npcs()
            .filter(|other| other.id != npc.id)
            .filter(|other| !prompt_roster.iter().any(|(id, _, _)| *id == other.id))
            .map(|other| {
                let mut descriptor = other.occupation.clone();
                if let Some(workplace_name) = other
                    .workplace
                    .and_then(|location_id| world.graph.get(location_id))
                    .map(|location| location.name.as_str())
                {
                    descriptor.push_str(&format!("; workplace: {workplace_name}"));
                }
                (other.id, other.name.clone(), descriptor)
            })
            .collect();
        parish_people.sort_by_key(|(id, _, _)| id.0);
        prompt_roster.extend(parish_people);
    }

    let system_prompt = ticks::build_enhanced_system_prompt_with_config(
        &npc,
        improv_enabled,
        language,
        npc_cfg,
        &npc_names,
        Some(&prompt_roster),
        location_names.as_deref(),
    );

    let mut context = ticks::build_enhanced_context_with_config(ticks::Tier1ContextParams {
        npc: &npc,
        world,
        player_input,
        other_npcs: &other_npcs,
        language,
        config: npc_cfg,
        npc_names: &npc_names,
        player_name_for_npc,
        was_introduced,
    });
    let player_label = player_name_for_npc.unwrap_or("The newcomer");
    // Transcript history first (current player input excluded — shown separately below).
    append_transcript_context(&mut context, transcript, player_label, player_input);

    // Check for anachronisms in player input and inject alert into context
    let anachronisms = anachronism::check_input(player_input);
    if let Some(alert) = anachronism::format_context_alert(&anachronisms) {
        context.push_str(&alert);
    }

    // Modern-register echo guard (TODO #55) — separate from the
    // technology/slang anachronism path. Fires when the player uses a
    // 21st-century phrase from MODERN_REGISTER_TERMS so the NPC doesn't
    // echo it back and trip the post-reply validator.
    if let Some(alert) = crate::npc::quality::format_player_register_alert(player_input) {
        context.push_str(&alert);
    }

    // Current player input — comes after conversation history as the triggering line.
    context.push_str("\n\n");
    context.push_str(&parish_npc::build_named_action_line(
        player_input,
        player_name_for_npc,
    ));
    context.push_str(
        "\n\nRespond to the live exchange above. Address ONLY ONE person in your \
         reply — either the player or one specific co-located NPC by name. Do NOT \
         say goodbye to one person and then continue speaking to another in the \
         same reply, and do NOT mix farewells with ongoing chat. One addressee, \
         one tone, one beat.\n",
    );
    context.push_str(&ticks::live_turn_contract_block(
        &npc,
        had_prior_exchange,
        was_introduced,
        player_input,
    ));

    // Extract plain name strings for the person-confirmation guard (#1459, #1488).
    //
    // The guard must never emit a false denial for a REAL parish NPC — even one
    // the speaking NPC does not personally know. `roster` only contains the NPCs
    // this NPC has a relationship/co-residence with ("PEOPLE YOU KNOW"), so a
    // shopkeeper recommended by one NPC but absent from another NPC's roster
    // would wrongly trigger the guard (#1488: Roisin false-denial bug).
    //
    // Fix: seed the known list from ALL parish NPC names, then append the
    // player entry from the personal roster. This makes the guard conservative:
    // it only fires for names that do not appear anywhere in the parish registry.
    let mut known_person_names: Vec<String> =
        npc_manager.all_npcs().map(|n| n.name.clone()).collect();
    // Also include the player's name if injected at the front of `roster`.
    // The player entry has NpcId(0); include it to avoid false-positive on the
    // player's own name appearing in a question.
    for (id, name, _) in &roster {
        if *id == NpcId(0) && !known_person_names.iter().any(|n| n == name) {
            known_person_names.push(name.clone());
        }
    }

    // For the wrong-speaker-identity guard (#1475): (name, occupation) pairs
    // for all roster members. Exclude the player entry (NpcId(0)) since the
    // guard is about NPC identities, not the player.
    let roster_names_occupations: Vec<(String, String)> = roster
        .iter()
        .filter(|(id, _, _)| id.0 != 0)
        .map(|(_, name, occ)| (name.clone(), occ.clone()))
        .collect();

    let location_name = world
        .graph
        .get(world.player_location)
        .map(|d| d.name.clone())
        .unwrap_or_default();

    // All location names in the world graph, for the invented-place guard (#1530).
    // Reuse the already-computed `location_names` if grounding was enabled;
    // otherwise build it fresh so the guard always has the full list.
    let known_location_names: Vec<String> = location_names.unwrap_or_else(|| {
        let mut names: Vec<String> = world
            .graph
            .location_ids()
            .into_iter()
            .filter_map(|id| world.graph.get(id).map(|d| d.name.clone()))
            .collect();
        names.sort();
        names
    });

    let mut work_roster_with_ids: Vec<(NpcId, String, String, Option<String>)> = npc_manager
        .all_npcs()
        .map(|person| {
            let workplace = person
                .workplace
                .and_then(|location_id| world.graph.get(location_id))
                .map(|location| location.name.clone());
            (
                person.id,
                person.name.clone(),
                person.occupation.clone(),
                workplace,
            )
        })
        .collect();
    work_roster_with_ids.sort_by_key(|(id, _, _, _)| id.0);
    let work_roster = work_roster_with_ids
        .into_iter()
        .map(|(_, name, occupation, workplace)| (name, occupation, workplace))
        .collect();

    Some(NpcConversationSetup {
        display_name,
        npc_name: npc.name.clone(),
        npc_id: speaker_id,
        system_prompt,
        context,
        known_person_names,
        roster_names_occupations,
        location_name,
        known_location_names,
        player_name: world.player_name.clone(),
        had_prior_exchange,
        work_roster,
    })
}

/// Single-target convenience wrapper around [`prepare_npc_conversation_turn`].
///
/// Used by headless CLI and other callers that address exactly one NPC.
pub fn prepare_npc_conversation(
    world: &WorldState,
    npc_manager: &mut NpcManager,
    raw: &str,
    target_name: Option<&str>,
    improv_enabled: bool,
    language: &LanguageSettings,
    npc_cfg: &crate::config::NpcConfig,
) -> Option<NpcConversationSetup> {
    let target_names = target_name
        .map(|name| vec![name.to_string()])
        .unwrap_or_default();
    let speaker_id = resolve_npc_targets(world, npc_manager, &target_names)
        .into_iter()
        .next()?;
    prepare_npc_conversation_turn(
        world,
        npc_manager,
        raw,
        speaker_id,
        &[],
        improv_enabled,
        language,
        npc_cfg,
    )
}

/// Detects if the player is introducing themselves and records the name.
///
/// Call this before `prepare_npc_conversation_turn` so the NPC prompt can
/// use the player's name. If detected, sets `world.player_name` (if not
/// already set) and teaches the speaking NPC the player's name.
pub fn detect_and_record_player_name(
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    player_input: &str,
    speaker_id: NpcId,
) {
    if let Some(name) = crate::npc::detect_player_name(player_input) {
        // Don't overwrite a previously set player name
        if world.player_name.is_none() {
            tracing::info!("Player introduced themselves as: {}", name);
            world.player_name = Some(name);
        }
        npc_manager.teach_player_name(speaker_id);
    }
}

/// Checks an NPC response for hallucinated names and returns a corrective
/// system prompt addendum if any are found.
///
/// Call this after parsing the NPC response. If it returns `Some(correction)`,
/// append the correction to the system prompt and re-submit once. If the
/// retry also hallucinates, accept and log.
pub fn check_for_hallucinated_names(
    response: &crate::npc::NpcStreamResponse,
    known_roster: &[(NpcId, String, String)],
    player_name: Option<&str>,
) -> Option<String> {
    let mentioned = response
        .metadata
        .as_ref()
        .map(|m| &m.mentioned_people)
        .filter(|mp| !mp.is_empty())?;

    let hallucinated = crate::npc::validate_mentioned_people(mentioned, known_roster, player_name);
    if hallucinated.is_empty() {
        return None;
    }

    let names = hallucinated.join(", ");
    tracing::warn!("NPC hallucinated names: {}", names);
    Some(format!(
        "\n\nCORRECTION: Your previous response mentioned '{}', \
        who does not exist in this parish. Regenerate your dialogue \
        without inventing names for people not in your PEOPLE YOU KNOW list.",
        names
    ))
}

// ── Pronunciation hints ────────────────────────────────────────────────────

/// Computes contextual name pronunciation hints for the current location.
///
/// Matches pronunciation entries against the current location name and
/// any introduced NPC names present at the player's location.
pub fn compute_name_hints(
    world: &WorldState,
    npc_manager: &NpcManager,
    pronunciations: &[PronunciationEntry],
) -> Vec<LanguageHint> {
    if pronunciations.is_empty() {
        tracing::debug!("compute_name_hints: no pronunciation entries loaded");
        return vec![];
    }
    let loc = world.current_location();
    let mut names: Vec<&str> = vec![&loc.name];
    let npcs = npc_manager.npcs_at(world.player_location);
    let npc_names: Vec<String> = npcs
        .iter()
        .filter(|n| npc_manager.is_introduced(n.id))
        .map(|n| n.name.clone())
        .collect();
    for name in &npc_names {
        names.push(name);
    }
    let hints: Vec<LanguageHint> = pronunciations
        .iter()
        .filter(|entry| entry.matches_any(&names))
        .map(|entry| entry.to_hint())
        .collect();
    tracing::debug!(
        location = %loc.name,
        npc_names = ?npc_names,
        pronunciation_count = pronunciations.len(),
        matched_hints = hints.len(),
        "compute_name_hints"
    );
    hints
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_first_works() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("ABC"), "ABC");
    }

    #[test]
    fn mask_key_works() {
        assert_eq!(mask_key("abcdefghij"), "abcd...ghij");
        assert_eq!(mask_key("short"), "(set, too short to mask)");
        assert_eq!(mask_key("123456789"), "1234...6789");
    }

    #[test]
    fn mask_key_non_ascii() {
        // Multi-byte UTF-8 characters must not panic
        let key = "αβγδεζηθικ"; // 10 Greek letters, each 2 bytes
        let result = mask_key(key);
        assert_eq!(result, "αβγδ...ηθικ");
        // Exactly 8 chars → too short to mask
        assert_eq!(mask_key("αβγδεζηθ"), "(set, too short to mask)");
    }

    #[test]
    fn snapshot_from_default_world() {
        let world = WorldState::new();
        let snap = snapshot_from_world(&world);
        assert_eq!(snap.location_id, world.player_location.0);
        assert!(!snap.location_name.is_empty());
        assert!(snap.hour <= 23);
        assert!(snap.minute <= 59);
        assert!(snap.speed_factor > 0.0);
    }

    #[test]
    fn snapshot_keeps_inference_pause_separate_from_player_pause() {
        let mut world = WorldState::new();
        world.clock.inference_pause();

        let snap = snapshot_from_world(&world);

        assert!(!snap.paused);
        assert!(snap.inference_paused);
    }

    #[test]
    fn build_map_data_from_default_world() {
        let world = WorldState::new();
        let map = build_map_data(&world, &TransportMode::walking(), false);
        assert!(!map.player_location.is_empty());
        // At least the player's location should exist
        assert!(
            map.locations.iter().any(|l| l.id == map.player_location) || map.locations.is_empty()
        );
    }

    #[test]
    fn fog_of_war_shows_frontier() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let world = crate::game_mod::world_state_from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            let neighbor_count = world.graph.neighbors(start).len();

            let map = build_map_data(&world, &TransportMode::walking(), false);

            // Start location (visited) + its neighbors (frontier)
            assert_eq!(
                map.locations.len(),
                1 + neighbor_count,
                "should show start + frontier neighbors"
            );

            // The start location is visited
            let start_loc = map
                .locations
                .iter()
                .find(|l| l.id == map.player_location)
                .unwrap();
            assert!(start_loc.visited);
            assert!(start_loc.indoor.is_some());
            assert!(start_loc.travel_minutes.is_none());

            // Frontier locations are not visited and reveal limited info: the
            // indoor flag stays hidden, but the travel-time estimate is surfaced
            // so the player can judge how far an unexplored neighbour is
            // (#1207 #33/#36).
            let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();
            assert_eq!(frontier.len(), neighbor_count);
            for f in &frontier {
                assert!(f.indoor.is_none(), "frontier should not reveal indoor flag");
                assert!(
                    f.travel_minutes.is_some(),
                    "frontier should surface a travel-time estimate"
                );
            }

            // Edges must include start→frontier neighbors; may also include
            // edges between frontier nodes that are connected to each other.
            let start_str = start.0.to_string();
            for f in &frontier {
                let connected = map.edges.iter().any(|(a, b)| {
                    (a == &start_str && b == &f.id) || (a == &f.id && b == &start_str)
                });
                assert!(
                    connected,
                    "start should be connected to frontier node {}",
                    f.id
                );
            }
            assert!(map.edges.len() >= neighbor_count);
        }
    }

    #[test]
    fn fog_of_war_reveals_after_visit() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let mut world =
                crate::game_mod::world_state_from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            // Visit a neighbor
            let neighbors = world.graph.neighbors(start);
            if let Some((neighbor_id, _)) = neighbors.first() {
                world.mark_visited(*neighbor_id);
                let map = build_map_data(&world, &TransportMode::walking(), false);

                // Visited locations should have visited=true
                let visited: Vec<_> = map.locations.iter().filter(|l| l.visited).collect();
                assert_eq!(visited.len(), 2);

                // The non-player visited location should have travel_minutes
                let other = visited
                    .iter()
                    .find(|l| l.id != map.player_location)
                    .unwrap();
                assert!(other.travel_minutes.is_some());
                assert!(other.indoor.is_some());

                // Frontier locations exist for unvisited neighbors of both visited locs
                let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();
                assert!(
                    !frontier.is_empty() || map.locations.len() == 2,
                    "frontier should appear unless all neighbors are visited"
                );
            }
        }
    }

    #[test]
    fn reveal_unexplored_shows_entire_graph_as_frontier_plus_visited() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let world = crate::game_mod::world_state_from_mod(&game_mod).expect("world from mod");
            let full_count = world.graph.location_ids().len();
            let visited_count = world.visited_locations.len();

            let map = build_map_data(&world, &TransportMode::walking(), true);
            assert_eq!(map.locations.len(), full_count);

            let visited_rendered = map.locations.iter().filter(|l| l.visited).count();
            let frontier_rendered = map.locations.iter().filter(|l| !l.visited).count();
            assert_eq!(visited_rendered, visited_count);
            assert_eq!(visited_rendered + frontier_rendered, full_count);
        }
    }

    #[test]
    fn build_npcs_here_empty_manager() {
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        let npcs = build_npcs_here(&world, &npc_mgr);
        assert!(npcs.is_empty());
    }

    #[test]
    fn build_npcs_here_carries_numeric_npc_identity() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(42);
        npc.location = world.player_location;
        npc_mgr.add_npc(npc);

        let npcs = build_npcs_here(&world, &npc_mgr);

        assert_eq!(npcs.len(), 1);
        assert_eq!(npcs[0].npc_id, 42);
    }

    #[test]
    fn extract_npc_mentions_matches_visible_display_names() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc_mgr.add_npc(npc);
        npc_mgr.mark_introduced(NpcId(1));

        let extracted = extract_npc_mentions(
            "@Padraig O'Brien @padraig o'brien tell me the news",
            &world,
            &npc_mgr,
        );

        assert_eq!(extracted.names, vec!["Padraig O'Brien".to_string()]);
        assert_eq!(extracted.remaining, "tell me the news");
    }

    #[test]
    fn extract_npc_mentions_handles_unintroduced_descriptions() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc.brief_description = "an older man behind the bar".to_string();
        npc_mgr.add_npc(npc);

        let extracted = extract_npc_mentions(
            "@an older man behind the bar what have you heard?",
            &world,
            &npc_mgr,
        );

        assert_eq!(
            extracted.names,
            vec!["an older man behind the bar".to_string()]
        );
        assert_eq!(extracted.remaining, "what have you heard?");
    }

    #[test]
    fn extract_npc_mentions_does_not_match_unintroduced_real_name() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc.name = "Padraig O'Brien".to_string();
        npc.brief_description = "an older man behind the bar".to_string();
        npc_mgr.add_npc(npc);

        let raw = "@Padraig O'Brien what have you heard?";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert!(extracted.names.is_empty());
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn extract_npc_mentions_ignores_ambiguous_first_names() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.name = "Mary Byrne".to_string();
        npc1.set_location(world.player_location);

        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.name = "Mary Kelly".to_string();
        npc2.set_location(world.player_location);

        npc_mgr.add_npc(npc1);
        npc_mgr.add_npc(npc2);
        npc_mgr.mark_introduced(NpcId(1));
        npc_mgr.mark_introduced(NpcId(2));

        let raw = "@Mary could I ask ye both something?";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert!(extracted.names.is_empty());
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn extract_npc_mentions_detects_free_text_names_in_order() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.name = "Padraig Darcy".to_string();
        npc1.set_location(world.player_location);

        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.name = "Niamh Darcy".to_string();
        npc2.set_location(world.player_location);

        npc_mgr.add_npc(npc1);
        npc_mgr.add_npc(npc2);
        npc_mgr.mark_introduced(NpcId(1));
        npc_mgr.mark_introduced(NpcId(2));

        let raw = "Good morning, Padraig and good day, Niamh Darcy.";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert_eq!(
            extracted.names,
            vec!["Padraig Darcy".to_string(), "Niamh Darcy".to_string()]
        );
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn extract_npc_mentions_free_text_only_matches_co_located_npcs() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(1);
        npc.name = "Padraig Darcy".to_string();
        npc.set_location(LocationId(world.player_location.0 + 1));
        npc_mgr.add_npc(npc);
        npc_mgr.mark_introduced(NpcId(1));

        let raw = "I saw Padraig Darcy yesterday.";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert!(extracted.names.is_empty());
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn extract_npc_mentions_presence_query_matches_absent_rostered_priest_alias() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut priest = Npc::new_test_npc();
        priest.id = NpcId(10);
        priest.name = "Fr. Declan Tierney".to_string();
        priest.occupation = "Parish Priest".to_string();
        priest.set_location(LocationId(world.player_location.0 + 1));
        npc_mgr.add_npc(priest);

        let raw = "Is Father Declan here? I should like to introduce myself to the parish priest.";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert_eq!(extracted.names, vec!["Fr. Declan Tierney".to_string()]);
        assert_eq!(extracted.remaining, raw);

        let casual = "I saw Father Declan on the road yesterday.";
        let extracted = extract_npc_mentions(casual, &world, &npc_mgr);
        assert!(
            extracted.names.is_empty(),
            "full-roster aliases should stay gated to explicit presence queries"
        );
        assert_eq!(extracted.remaining, casual);
    }

    #[test]
    fn extract_npc_mentions_detects_free_text_display_description() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc.brief_description = "an older man behind the bar".to_string();
        npc_mgr.add_npc(npc);

        let raw = "Could I ask an older man behind the bar about the harvest?";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert_eq!(
            extracted.names,
            vec!["an older man behind the bar".to_string()]
        );
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn extract_npc_mentions_merges_at_mentions_with_free_text_names() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.name = "Padraig Darcy".to_string();
        npc1.set_location(world.player_location);

        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.name = "Niamh Darcy".to_string();
        npc2.set_location(world.player_location);

        npc_mgr.add_npc(npc1);
        npc_mgr.add_npc(npc2);
        npc_mgr.mark_introduced(NpcId(1));
        npc_mgr.mark_introduced(NpcId(2));

        let extracted = extract_npc_mentions("@Padraig Darcy hello Niamh", &world, &npc_mgr);

        assert_eq!(
            extracted.names,
            vec!["Padraig Darcy".to_string(), "Niamh Darcy".to_string()]
        );
        assert_eq!(extracted.remaining, "hello Niamh");
    }

    #[test]
    fn extract_npc_mentions_ignores_non_boundary_multibyte_prefix() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.name = "A".to_string();
        npc.set_location(world.player_location);
        npc_mgr.add_npc(npc);
        npc_mgr.mark_introduced(NpcId(1));

        let raw = "Áine says hello";
        let extracted = extract_npc_mentions(raw, &world, &npc_mgr);

        assert!(extracted.names.is_empty());
        assert_eq!(extracted.remaining, raw);
    }

    #[test]
    fn resolve_npc_targets_preserves_order() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.name = "Padraig Darcy".to_string();
        npc1.set_location(world.player_location);

        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.name = "Siobhan Murphy".to_string();
        npc2.set_location(world.player_location);

        npc_mgr.add_npc(npc1);
        npc_mgr.add_npc(npc2);
        npc_mgr.mark_introduced(NpcId(1));
        npc_mgr.mark_introduced(NpcId(2));

        let targets = resolve_npc_targets(
            &world,
            &npc_mgr,
            &["Siobhan Murphy".to_string(), "Padraig Darcy".to_string()],
        );

        assert_eq!(targets, vec![NpcId(2), NpcId(1)]);
    }

    /// `resolve_addressed_targets` must classify present vs absent names
    /// without ever silently substituting a different co-located NPC for an
    /// absent target (#985).
    #[test]
    fn resolve_addressed_targets_separates_present_and_absent() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        // Peig is co-located, Aoife is elsewhere.
        let mut peig = Npc::new_test_npc();
        peig.id = NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.set_location(world.player_location);

        let mut aoife = Npc::new_test_npc();
        aoife.id = NpcId(2);
        aoife.name = "Aoife Brennan".to_string();
        aoife.set_location(LocationId(world.player_location.0 + 99));

        npc_mgr.add_npc(peig);
        npc_mgr.add_npc(aoife);
        npc_mgr.mark_introduced(NpcId(1));
        npc_mgr.mark_introduced(NpcId(2));

        let result = resolve_addressed_targets(
            &world,
            &npc_mgr,
            &["Aoife Brennan".to_string(), "Peig Hannigan".to_string()],
        );

        assert_eq!(result.resolved, vec![NpcId(1)]);
        assert_eq!(result.absent, vec!["Aoife Brennan".to_string()]);
    }

    /// All addressed names are absent — neither falls back to a co-located
    /// NPC (the regression behaviour from #985 that lets the wrong NPC speak).
    #[test]
    fn resolve_addressed_targets_no_fallback_when_all_absent() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        // Peig is co-located but the player addressed only the absent Aoife.
        let mut peig = Npc::new_test_npc();
        peig.id = NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.set_location(world.player_location);
        npc_mgr.add_npc(peig);
        npc_mgr.mark_introduced(NpcId(1));

        let result = resolve_addressed_targets(&world, &npc_mgr, &["Aoife Brennan".to_string()]);

        assert!(
            result.resolved.is_empty(),
            "resolved must be empty so the caller can emit `Aoife Brennan is not here.`; \
             got {:?}",
            result.resolved,
        );
        assert_eq!(result.absent, vec!["Aoife Brennan".to_string()]);
    }

    /// Empty input → empty result. The caller is responsible for any ambient
    /// fallback (e.g. first co-located NPC), which it can do via
    /// `resolve_npc_targets`.
    #[test]
    fn resolve_addressed_targets_empty_input_returns_empty() {
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        let result = resolve_addressed_targets(&world, &npc_mgr, &[]);
        assert!(result.resolved.is_empty());
        assert!(result.absent.is_empty());
    }

    #[test]
    fn resolve_npc_targets_no_names_falls_back_to_first_present() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(1);
        npc.name = "Peig Hannigan".to_string();
        npc.set_location(world.player_location);
        npc_mgr.add_npc(npc);
        npc_mgr.mark_introduced(NpcId(1));

        let targets = resolve_npc_targets(&world, &npc_mgr, &[]);
        assert_eq!(targets, vec![NpcId(1)]);
    }

    #[test]
    fn resolve_npc_targets_named_but_absent_returns_empty() {
        // Regression: player says "talk to Aoife" while only Peig is here.
        // Previously the fallback would route to Peig; now we return empty so
        // the caller can emit "no one here by that name".
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut peig = Npc::new_test_npc();
        peig.id = NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.set_location(world.player_location);
        npc_mgr.add_npc(peig);
        npc_mgr.mark_introduced(NpcId(1));

        let targets = resolve_npc_targets(&world, &npc_mgr, &["Aoife Brennan".to_string()]);
        assert!(targets.is_empty());
    }

    #[test]
    fn resolve_npc_targets_role_vocative_resolves_when_unambiguous() {
        // Issue #998: "Good mornin', Widow." with only Peig (occupation = Widow)
        // co-located. Should resolve to Peig, not return empty.
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut peig = Npc::new_test_npc();
        peig.id = NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.occupation = "Widow".to_string();
        peig.set_location(world.player_location);
        npc_mgr.add_npc(peig);

        let targets = resolve_npc_targets(&world, &npc_mgr, &["Widow".to_string()]);
        assert_eq!(targets, vec![NpcId(1)]);
    }

    #[test]
    fn resolve_npc_targets_role_vocative_refuses_when_ambiguous() {
        // Two co-located NPCs share a role — resolver must NOT silently pick.
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut a = Npc::new_test_npc();
        a.id = NpcId(1);
        a.name = "Siobhan Murphy".to_string();
        a.occupation = "Farmer".to_string();
        a.set_location(world.player_location);

        let mut b = Npc::new_test_npc();
        b.id = NpcId(2);
        b.name = "Liam Murphy".to_string();
        b.occupation = "Farmer".to_string();
        b.set_location(world.player_location);

        npc_mgr.add_npc(a);
        npc_mgr.add_npc(b);

        let targets = resolve_npc_targets(&world, &npc_mgr, &["Farmer".to_string()]);
        assert!(
            targets.is_empty(),
            "ambiguous role-vocative must not resolve silently"
        );
    }

    #[test]
    fn resolve_npc_targets_role_vocative_case_insensitive() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut tierney = Npc::new_test_npc();
        tierney.id = NpcId(1);
        tierney.name = "Fr. Declan Tierney".to_string();
        tierney.occupation = "Parish Priest".to_string();
        tierney.set_location(world.player_location);
        npc_mgr.add_npc(tierney);

        let targets = resolve_npc_targets(&world, &npc_mgr, &["parish priest".to_string()]);
        assert_eq!(targets, vec![NpcId(1)]);
    }

    #[test]
    fn build_travel_start_basic() {
        use crate::world::graph::WorldGraph;

        let json = r#"{"locations": [
            {"id": 1, "name": "A", "description_template": ".", "indoor": false, "public": true, "lat": 53.6, "lon": -8.1, "connections": [{"target": 2, "path_description": "road"}]},
            {"id": 2, "name": "B", "description_template": ".", "indoor": false, "public": true, "lat": 53.61, "lon": -8.09, "connections": [{"target": 1, "path_description": "back"}]}
        ]}"#;
        let graph = WorldGraph::load_from_str(json).unwrap();
        let path = vec![LocationId(1), LocationId(2)];
        let payload = build_travel_start(&path, 5, &graph);
        assert_eq!(payload.waypoints.len(), 2);
        assert_eq!(payload.waypoints[0].id, "1");
        assert_eq!(payload.waypoints[1].id, "2");
        assert_eq!(payload.duration_minutes, 5);
        assert_eq!(payload.destination, "2");
        assert!((payload.waypoints[0].lat - 53.6).abs() < 0.001);
    }

    #[test]
    fn build_map_data_includes_edge_traversals() {
        use crate::game_mod::{GameMod, find_default_mod};

        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let mut world =
                crate::game_mod::world_state_from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            let neighbor_id = world.graph.neighbors(start).first().map(|(id, _)| *id);
            if let Some(neighbor_id) = neighbor_id {
                // Traverse the edge twice
                world.record_path_traversal(&[start, neighbor_id]);
                world.record_path_traversal(&[start, neighbor_id]);
                world.mark_visited(neighbor_id);

                let map = build_map_data(&world, &TransportMode::walking(), false);
                assert!(
                    !map.edge_traversals.is_empty(),
                    "should include edge traversals"
                );
                // Find the traversal for start<->neighbor
                let start_str = start.0.to_string();
                let neighbor_str = neighbor_id.0.to_string();
                let found = map.edge_traversals.iter().any(|(a, b, count)| {
                    ((a == &start_str && b == &neighbor_str)
                        || (a == &neighbor_str && b == &start_str))
                        && *count == 2
                });
                assert!(found, "should find traversal count of 2");
            }
        }
    }

    // ── Additional coverage for text_log helpers and supporting functions ───

    #[test]
    fn capitalize_first_handles_unicode() {
        // Irish — initial letter has an acute accent.
        assert_eq!(capitalize_first("éire"), "Éire");
        // Leading whitespace is preserved.
        assert_eq!(capitalize_first(" hello"), " hello");
    }

    #[test]
    fn mask_key_boundary_conditions() {
        // Exactly 8 chars still falls into the short branch.
        assert_eq!(mask_key("12345678"), "(set, too short to mask)");
        // 9 chars reveals first 4 and last 4.
        assert_eq!(mask_key("123456789"), "1234...6789");
        // Empty.
        assert_eq!(mask_key(""), "(set, too short to mask)");
    }

    #[test]
    fn text_log_assigns_unique_monotonic_ids() {
        let a = text_log("system", "first");
        let b = text_log("system", "second");
        assert!(a.id.starts_with("msg-"));
        assert!(b.id.starts_with("msg-"));
        assert_ne!(a.id, b.id);
        assert_eq!(a.source, "system");
        assert_eq!(a.content, "first");
        assert!(a.subtype.is_none());
        assert!(a.stream_turn_id.is_none());
    }

    #[test]
    fn text_log_for_stream_turn_carries_turn_id() {
        let payload = text_log_for_stream_turn("npc", "hello", 42);
        assert_eq!(payload.stream_turn_id, Some(42));
        assert_eq!(payload.source, "npc");
        assert_eq!(payload.content, "hello");
        assert!(payload.subtype.is_none());
    }

    #[test]
    fn text_log_typed_sets_subtype() {
        let payload = text_log_typed("system", "A wren hops by.", "ambient");
        assert_eq!(payload.subtype.as_deref(), Some("ambient"));
        assert_eq!(payload.content, "A wren hops by.");
        assert!(payload.stream_turn_id.is_none());
    }

    // ── detect_and_record_player_name ───────────────────────────────────────

    #[test]
    fn detect_player_name_records_first_introduction() {
        let mut world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        let speaker = npc.id;
        npc_mgr.add_npc(npc);

        assert!(world.player_name.is_none());
        detect_and_record_player_name(&mut world, &mut npc_mgr, "My name is Ciaran.", speaker);
        assert_eq!(world.player_name.as_deref(), Some("Ciaran"));
        assert!(npc_mgr.knows_player_name(speaker));
    }

    #[test]
    fn detect_player_name_does_not_overwrite() {
        let mut world = WorldState::new();
        world.player_name = Some("Aoife".to_string());
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        let speaker = npc.id;
        npc_mgr.add_npc(npc);

        detect_and_record_player_name(&mut world, &mut npc_mgr, "My name is Ciaran.", speaker);
        assert_eq!(world.player_name.as_deref(), Some("Aoife"));
        // The speaker still gets taught the name because detection fired.
        assert!(npc_mgr.knows_player_name(speaker));
    }

    #[test]
    fn detect_player_name_skips_non_introductions() {
        let mut world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        let speaker = npc.id;
        npc_mgr.add_npc(npc);

        detect_and_record_player_name(&mut world, &mut npc_mgr, "Tell me the news.", speaker);
        assert!(world.player_name.is_none());
        assert!(!npc_mgr.knows_player_name(speaker));
    }

    // ── compute_name_hints ───────────────────────────────────────────────────

    #[test]
    fn compute_name_hints_empty_when_no_pronunciations() {
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        let hints = compute_name_hints(&world, &npc_mgr, &[]);
        assert!(hints.is_empty());
    }

    #[test]
    fn compute_name_hints_matches_location_name() {
        use crate::game_mod::PronunciationEntry;
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        // Match the default crossroads location.
        let entries = vec![PronunciationEntry {
            word: "Crossroads".to_string(),
            pronunciation: "KROSS-rohds".to_string(),
            meaning: Some("meeting of ways".to_string()),
            matches: vec!["crossroads".to_string()],
        }];
        let hints = compute_name_hints(&world, &npc_mgr, &entries);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].word, "Crossroads");
    }

    #[test]
    fn compute_name_hints_ignores_unintroduced_npcs() {
        use crate::game_mod::PronunciationEntry;
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(world.player_location);
        npc.name = "Siobhan".to_string();
        let npc_id = npc.id;
        npc_mgr.add_npc(npc);
        // Do NOT mark introduced.

        let entries = vec![PronunciationEntry {
            word: "Siobhan".to_string(),
            pronunciation: "shi-VAWN".to_string(),
            meaning: None,
            matches: vec!["siobhan".to_string()],
        }];
        let hints = compute_name_hints(&world, &npc_mgr, &entries);
        assert!(hints.is_empty(), "unintroduced NPC names must not leak");

        // After introduction, the hint appears.
        npc_mgr.mark_introduced(npc_id);
        let hints = compute_name_hints(&world, &npc_mgr, &entries);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].word, "Siobhan");
    }

    // ── check_for_hallucinated_names ─────────────────────────────────────────

    #[test]
    fn check_hallucinated_returns_none_when_metadata_absent() {
        let response = crate::npc::NpcStreamResponse {
            dialogue: "Hello.".to_string(),
            metadata: None,
        };
        let roster: Vec<(NpcId, String, String)> = vec![];
        let result = check_for_hallucinated_names(&response, &roster, None);
        assert!(result.is_none());
    }

    // ── resolve_addressed_targets role-vocative parity (#1221) ───────────────

    /// Regression (#1221): `resolve_addressed_targets` must use the same
    /// role-vocative fallback as `resolve_npc_targets` so that "Hello Father"
    /// routes to the priest instead of producing "Father is not here.".
    #[test]
    fn resolve_addressed_targets_role_vocative_father_resolves_to_priest() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut priest = Npc::new_test_npc();
        priest.id = NpcId(10);
        priest.name = "Fr. Declan Tierney".to_string();
        priest.occupation = "Parish Priest".to_string();
        priest.set_location(world.player_location);
        npc_mgr.add_npc(priest);

        // "Father" vocative via built-in alias — must resolve, not absent.
        let result = resolve_addressed_targets(&world, &npc_mgr, &["Father".to_string()]);
        assert!(
            result.absent.is_empty(),
            "\"Father\" must NOT be absent when a priest is co-located; absent={:?}",
            result.absent
        );
        assert_eq!(
            result.resolved,
            vec![NpcId(10)],
            "\"Father\" must resolve to the co-located priest"
        );
    }

    #[test]
    fn resolve_addressed_targets_role_vocative_widow_resolves() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(11);
        npc.name = "Peig Hannigan".to_string();
        npc.occupation = "Widow".to_string();
        npc.set_location(world.player_location);
        npc_mgr.add_npc(npc);

        let result = resolve_addressed_targets(&world, &npc_mgr, &["Widow".to_string()]);
        assert!(result.absent.is_empty());
        assert_eq!(result.resolved, vec![NpcId(11)]);
    }

    #[test]
    fn resolve_addressed_targets_role_vocative_absent_when_truly_absent() {
        // Named NPC still absent when not at location — no false positive.
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        // Priest at a different location.
        let mut priest = Npc::new_test_npc();
        priest.id = NpcId(10);
        priest.name = "Fr. Declan Tierney".to_string();
        priest.occupation = "Parish Priest".to_string();
        priest.set_location(LocationId(999)); // different loc
        npc_mgr.add_npc(priest);

        let result = resolve_addressed_targets(&world, &npc_mgr, &["Father".to_string()]);
        assert!(
            result.resolved.is_empty(),
            "priest at wrong location must not resolve"
        );
        assert_eq!(result.absent, vec!["Father".to_string()]);
    }

    #[test]
    fn resolve_addressed_targets_ambiguous_role_is_absent() {
        // Two priests co-located → ambiguous → must NOT silently pick one.
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        for id in [12u32, 13] {
            let mut p = Npc::new_test_npc();
            p.id = NpcId(id);
            p.name = format!("Priest {id}");
            p.occupation = "Parish Priest".to_string();
            p.set_location(world.player_location);
            npc_mgr.add_npc(p);
        }

        let result = resolve_addressed_targets(&world, &npc_mgr, &["Father".to_string()]);
        assert!(
            result.resolved.is_empty(),
            "ambiguous role must not resolve; resolved={:?}",
            result.resolved
        );
        // "Father" is absent (ambiguous, treated as unresolvable).
        assert_eq!(result.absent, vec!["Father".to_string()]);
    }

    /// AC-1 (#1488): `prepare_npc_conversation_turn` must include ALL parish
    /// NPC names in `known_person_names`, not just those in the speaking NPC's
    /// personal relationship roster. This prevents the person-confirmation guard
    /// from emitting a false denial when NPC-A accurately describes NPC-C, who
    /// is a real parish NPC but not in NPC-A's personal roster.
    ///
    /// Repro: priest (NPC 1) has NO relationship with Roisin (NPC 2). When the
    /// player asks the priest about Roisin, the guard must NOT fire — Roisin is
    /// in the parish registry even if she's absent from the priest's roster.
    #[test]
    fn known_person_names_includes_all_parish_npcs_not_just_roster() {
        use crate::config::NpcConfig;
        use crate::npc::{Npc, manager::NpcManager};
        use crate::world::WorldState;

        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        // NPC 1: priest (the speaker — has no relationship with Roisin).
        let mut priest = Npc::new_test_npc();
        priest.id = NpcId(1);
        priest.name = "Father Brennan".to_string();
        priest.occupation = "Parish Priest".to_string();
        priest.set_location(world.player_location);
        npc_mgr.add_npc(priest);
        npc_mgr.mark_introduced(NpcId(1));

        // NPC 2: Roisin — a shopkeeper the priest does NOT have a relationship with.
        let mut roisin = Npc::new_test_npc();
        roisin.id = NpcId(2);
        roisin.name = "Roisin Malone".to_string();
        roisin.occupation = "Shopkeeper".to_string();
        // Roisin is at a different location — not co-located with the priest.
        // So she would NOT appear in the priest's `known_roster`.
        roisin.set_location(
            world
                .graph
                .location_ids()
                .into_iter()
                .find(|l| *l != world.player_location)
                .unwrap_or(world.player_location),
        );
        npc_mgr.add_npc(roisin);

        let npc_cfg = NpcConfig::default();
        let language = crate::npc::LanguageSettings::english_only();

        let setup = prepare_npc_conversation_turn(
            &world,
            &mut npc_mgr,
            "Where can I find Roisin Malone?",
            NpcId(1), // priest speaks
            &[],
            false,
            &language,
            &npc_cfg,
        );

        let setup = setup.expect("setup must succeed for co-located NPC");

        // The fix: Roisin must appear in both the hidden guard allow-list and
        // the system prompt even though she is NOT in the priest's personal
        // relationship roster.
        assert!(
            setup
                .known_person_names
                .iter()
                .any(|n| n == "Roisin Malone"),
            "Roisin Malone (a real parish NPC) must appear in known_person_names \
             even though she has no relationship with the speaking priest (#1488); \
             got: {:?}",
            setup.known_person_names
        );
        assert!(
            setup.system_prompt.contains("Roisin Malone, Shopkeeper")
                && setup.system_prompt.contains("real parish person"),
            "Roisin Malone must appear in the prompt as a real parish person \
             so the model is not instructed to deny her (#1563):\n{}",
            setup.system_prompt
        );

        // Validate that the person-confirmation guard does NOT fire on a good
        // reply about Roisin, because she is now in the known list.
        let good_reply = "Roisin Malone keeps a shop at the crossroads. She is a fine woman.";
        let guarded = crate::npc::guard_fabricated_person_confirmation(
            good_reply,
            "Where can I find Roisin Malone?",
            &setup.known_person_names,
            &[],
            None,
            0,
        );
        assert_eq!(
            guarded, good_reply,
            "person-confirmation guard must NOT fire on a real parish NPC (Roisin Malone) \
             after #1488 fix; got: {guarded:?}"
        );
    }

    #[test]
    fn conversation_setup_keeps_contact_and_identity_as_separate_state() {
        use crate::config::NpcConfig;
        use crate::npc::{LanguageSettings, Npc, manager::NpcManager};
        use parish_types::conversation::ConversationExchange;

        let mut world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut peig = Npc::new_test_npc();
        peig.id = NpcId(22);
        peig.name = "Peig Hannigan".to_string();
        peig.brief_description = "an elderly widow".to_string();
        peig.occupation = "Widow".to_string();
        peig.set_location(world.player_location);
        npc_mgr.add_npc(peig);

        let setup = prepare_npc_conversation_turn(
            &world,
            &mut npc_mgr,
            "Might I ask your name?",
            NpcId(22),
            &[],
            false,
            &LanguageSettings::english_only(),
            &NpcConfig::default(),
        )
        .expect("speaker exists");
        assert_eq!(setup.display_name, "an elderly widow");
        assert!(!setup.had_prior_exchange);
        assert!(setup.context.contains("FIRST CONTACT"));
        assert!(
            !npc_mgr.is_introduced(NpcId(22)),
            "prompt preparation alone must not reveal identity"
        );

        world.conversation_log.add(ConversationExchange {
            timestamp: world.clock.now(),
            speaker_id: NpcId(22),
            speaker_name: "Peig Hannigan".to_string(),
            player_input: "Might I ask your name?".to_string(),
            npc_dialogue: "Good morning. What brings ye here?".to_string(),
            // Contact follows the person across locations. This deliberately
            // differs from the player's current location (#1786).
            location: LocationId(999),
        });
        let follow_up = prepare_npc_conversation_turn(
            &world,
            &mut npc_mgr,
            "Ye never gave me your name.",
            NpcId(22),
            &[],
            false,
            &LanguageSettings::english_only(),
            &NpcConfig::default(),
        )
        .expect("speaker exists");
        assert!(follow_up.had_prior_exchange);
        assert_eq!(follow_up.display_name, "an elderly widow");
        assert!(!follow_up.context.contains("FIRST CONTACT"));
        assert!(!npc_mgr.is_introduced(NpcId(22)));
    }
}
