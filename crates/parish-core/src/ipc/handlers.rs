//! Pure handler functions that build IPC types from game state.
//!
//! These are consumed by both the Tauri desktop backend and the axum web
//! server, keeping game-logic → IPC-type mapping in a single place.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Timelike};

use crate::game_mod::PronunciationEntry;
use crate::npc::anachronism;
use crate::npc::manager::NpcManager;
use crate::npc::mood::mood_emoji;
use crate::npc::ticks;
use crate::npc::{LanguageHint, Npc, NpcId};
use crate::world::description::render_description;
use crate::world::transport::TransportMode;
use crate::world::{LocationId, WorldState};

use super::types::{MapData, MapLocation, NpcInfo, TextLogPayload, WorldSnapshot};

/// Builds a [`WorldSnapshot`] from the current world state.
pub fn snapshot_from_world(world: &WorldState, _transport: &TransportMode) -> WorldSnapshot {
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

    let day_of_week = match now.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string();

    WorldSnapshot {
        location_name: loc.name.clone(),
        location_description: description,
        time_label: tod.to_string(),
        hour,
        minute,
        weather: weather_str,
        season: season.to_string(),
        festival,
        paused: world.clock.is_paused() || world.clock.is_inference_paused(),
        game_epoch_ms: now.timestamp_millis() as f64,
        speed_factor: world.clock.speed_factor(),
        name_hints: vec![],
        day_of_week,
    }
}

/// Builds the [`MapData`] with fog-of-war: visited locations plus the frontier.
///
/// Visited locations are fully enriched. The "frontier" — unvisited locations
/// adjacent to any visited location — also appears so the player can see
/// where they could explore next. Frontier locations are marked with
/// `visited: false` and have limited tooltip data.
pub fn build_map_data(world: &WorldState, transport: &TransportMode) -> MapData {
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

    // Frontier: unvisited locations that neighbor at least one visited location
    let mut frontier: HashSet<LocationId> = HashSet::new();
    for &v in visited {
        for (neighbor_id, _) in world.graph.neighbors(v) {
            if !visited.contains(&neighbor_id) {
                frontier.insert(neighbor_id);
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

    // Append frontier locations (limited info)
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
                travel_minutes: None,
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

    super::types::TravelStartPayload {
        waypoints,
        duration_minutes: minutes,
        destination: path.last().map(|id| id.0.to_string()).unwrap_or_default(),
    }
}

/// Builds the list of [`NpcInfo`] for NPCs at the player's current location.
pub fn build_npcs_here(world: &WorldState, npc_manager: &NpcManager) -> Vec<NpcInfo> {
    let npcs = npc_manager.npcs_at(world.player_location);
    npcs.into_iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            NpcInfo {
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
    /// Remaining player text with the mentions stripped out.
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

/// Extracts all valid `@mentions` that match NPCs at the player's location.
///
/// Matching is done against the NPCs currently present using their visible
/// display names, so multi-word lowercase descriptions like "an older man
/// behind the bar" remain parseable.
pub fn extract_npc_mentions(
    raw: &str,
    world: &WorldState,
    npc_manager: &NpcManager,
) -> MentionedNpcs {
    let candidates: Vec<String> = npc_manager
        .npcs_at(world.player_location)
        .into_iter()
        .map(|npc| npc_manager.display_name(npc).to_string())
        .collect();

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
        for name in &candidates {
            if rest.len() < name.len() {
                continue;
            }
            let candidate = &rest[..name.len()];
            if candidate.eq_ignore_ascii_case(name)
                && mention_boundary(rest[name.len()..].chars().next())
            {
                match &matched {
                    Some((len, _)) if *len >= name.len() => {}
                    _ => matched = Some((name.len(), name.clone())),
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

    if spans.is_empty() {
        return MentionedNpcs {
            names: vec![],
            remaining: raw.trim().to_string(),
        };
    }

    let mut names = Vec::new();
    let mut dedupe = HashSet::new();
    let mut remaining = String::new();
    let mut last = 0usize;
    for (start, end, name) in spans {
        if dedupe.insert(name.to_lowercase()) {
            names.push(name);
        }
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
/// Falls back to the first NPC at the current location when no names are
/// supplied. Unknown names are ignored.
pub fn resolve_npc_targets(
    world: &WorldState,
    npc_manager: &NpcManager,
    target_names: &[String],
) -> Vec<NpcId> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for name in target_names {
        if let Some(npc) = npc_manager.find_by_name(name, world.player_location)
            && seen.insert(npc.id)
        {
            targets.push(npc.id);
        }
    }

    if targets.is_empty()
        && let Some(npc) = npc_manager
            .npcs_at(world.player_location)
            .into_iter()
            .next()
    {
        targets.push(npc.id);
    }

    targets
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

/// Irish-themed canned messages shown when NPC inference fails.
///
/// Indexed by `request_id % len` so different attempts get different messages.
pub const INFERENCE_FAILURE_MESSAGES: &[&str] = &[
    "A sudden fog rolls in and swallows the conversation whole.",
    "A crow lands between you, caws loudly, and the moment is lost.",
    "The wind picks up and carries their words clean away.",
    "They open their mouth to speak, but a donkey brays so loud neither of ye can hear a thing.",
    "A clap of thunder rattles the sky and ye both forget what ye were talking about.",
    "They stare at you blankly, as if the thought simply left their head.",
    "A strange silence falls over the parish. Even the birds have stopped.",
];

/// Atmospheric messages displayed when no NPC is present at the current location.
pub const IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

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
}

/// Prepares a specific NPC's turn in an ongoing conversation.
///
/// The supplied `player_input` describes the current trigger for this turn,
/// while `transcript` carries the recent local exchange for continuity.
pub fn prepare_npc_conversation_turn(
    world: &WorldState,
    npc_manager: &mut NpcManager,
    player_input: &str,
    speaker_id: NpcId,
    transcript: &[ConversationLine],
    improv_enabled: bool,
) -> Option<NpcConversationSetup> {
    let npc = npc_manager.get(speaker_id)?.clone();
    // Mark NPC as introduced before computing display_name so first conversation
    // shows their name, not their anonymous description.
    npc_manager.mark_introduced(speaker_id);
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
    let system_prompt = ticks::build_enhanced_system_prompt_with_config(
        &npc,
        improv_enabled,
        &crate::config::NpcConfig::default(),
        &npc_names,
        Some(&roster),
    );

    let mut context = ticks::build_enhanced_context_with_config(
        &npc,
        world,
        player_input,
        &other_npcs,
        &crate::config::NpcConfig::default(),
        &npc_names,
        player_name_for_npc,
    );
    let player_label = player_name_for_npc.unwrap_or("The newcomer");
    // Transcript history first (current player input excluded — shown separately below).
    append_transcript_context(&mut context, transcript, player_label, player_input);

    // Check for anachronisms in player input and inject alert into context
    let anachronisms = anachronism::check_input(player_input);
    if let Some(alert) = anachronism::format_context_alert(&anachronisms) {
        context.push_str(&alert);
    }

    // Current player input — comes after conversation history as the triggering line.
    context.push_str("\n\n");
    context.push_str(&parish_npc::build_named_action_line(
        player_input,
        player_name_for_npc,
    ));
    context.push_str(
        "\n\nRespond to the live exchange above. You may answer the player or another nearby NPC by name when it feels natural.\n",
    );

    Some(NpcConversationSetup {
        display_name,
        npc_name: npc.name.clone(),
        npc_id: speaker_id,
        system_prompt,
        context,
    })
}

/// Backward-compatible single-target helper retained for older callers.
pub fn prepare_npc_conversation(
    world: &WorldState,
    npc_manager: &mut NpcManager,
    raw: &str,
    target_name: Option<&str>,
    improv_enabled: bool,
) -> Option<NpcConversationSetup> {
    let target_names = target_name
        .map(|name| vec![name.to_string()])
        .unwrap_or_default();
    let speaker_id = resolve_npc_targets(world, npc_manager, &target_names)
        .into_iter()
        .next()?;
    prepare_npc_conversation_turn(world, npc_manager, raw, speaker_id, &[], improv_enabled)
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
        let transport = TransportMode::walking();
        let snap = snapshot_from_world(&world, &transport);
        assert!(!snap.location_name.is_empty());
        assert!(snap.hour <= 23);
        assert!(snap.minute <= 59);
        assert!(snap.speed_factor > 0.0);
    }

    #[test]
    fn build_map_data_from_default_world() {
        let world = WorldState::new();
        let map = build_map_data(&world, &TransportMode::walking());
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

            let map = build_map_data(&world, &TransportMode::walking());

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

            // Frontier locations are not visited and have limited info
            let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();
            assert_eq!(frontier.len(), neighbor_count);
            for f in &frontier {
                assert!(f.indoor.is_none(), "frontier should not reveal indoor flag");
                assert!(
                    f.travel_minutes.is_none(),
                    "frontier should not reveal travel time"
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
                let map = build_map_data(&world, &TransportMode::walking());

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
    fn build_npcs_here_empty_manager() {
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        let npcs = build_npcs_here(&world, &npc_mgr);
        assert!(npcs.is_empty());
    }

    #[test]
    fn extract_npc_mentions_matches_visible_display_names() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.location = world.player_location;
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
        npc.location = world.player_location;
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
    fn resolve_npc_targets_preserves_order() {
        let world = WorldState::new();
        let mut npc_mgr = NpcManager::new();

        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.name = "Padraig Darcy".to_string();
        npc1.location = world.player_location;

        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.name = "Siobhan Murphy".to_string();
        npc2.location = world.player_location;

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

                let map = build_map_data(&world, &TransportMode::walking());
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
        npc.location = world.player_location;
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
        npc.location = world.player_location;
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
        npc.location = world.player_location;
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
        npc.location = world.player_location;
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
}
