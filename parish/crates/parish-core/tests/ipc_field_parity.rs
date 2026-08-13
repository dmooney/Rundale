//! Contract test: Rust serde field-name parity with the TypeScript manifest.
//!
//! Serializes a representative instance of every IPC struct that the frontend
//! mirrors in `parish/apps/ui/src/lib/types.ts` and asserts that the
//! serialized JSON top-level keys contain all required field names recorded in
//! `parish/apps/ui/src/lib/types-manifest.json`.
//!
//! Companion TypeScript test: `parish/apps/ui/src/lib/types.test.ts`
//! parses `types.ts` source text and asserts each TypeScript interface
//! declares all required fields from the same manifest. The task-progression
//! contracts added in #1781 use exact matching because they have no
//! frontend-only compatibility fields.
//!
//! TD-053 / #1202

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root is two levels above crate root")
        .to_path_buf()
}

/// Load the ground-truth manifest from the UI source tree.
fn load_manifest() -> BTreeMap<String, Vec<String>> {
    let path = workspace_root().join("apps/ui/src/lib/types-manifest.json");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let raw: BTreeMap<String, Value> =
        serde_json::from_str(&src).unwrap_or_else(|e| panic!("cannot parse manifest: {e}"));
    raw.into_iter()
        .filter(|(k, _)| !k.starts_with('$')) // skip $comment
        .map(|(k, v)| {
            let fields = v
                .as_array()
                .unwrap_or_else(|| panic!("manifest entry {k} is not an array"))
                .iter()
                .map(|f| {
                    f.as_str()
                        .unwrap_or_else(|| panic!("field in {k} is not a string"))
                        .to_string()
                })
                .collect();
            (k, fields)
        })
        .collect()
}

/// Serialize `value` and return its top-level JSON key set.
fn keys(value: &impl serde::Serialize) -> BTreeSet<String> {
    let v = serde_json::to_value(value).expect("serialization failed");
    match v {
        Value::Object(map) => map.into_iter().map(|(k, _)| k).collect(),
        other => panic!("expected JSON object, got {other:?}"),
    }
}

/// Assert the serialized keys of `value` contain every field listed for
/// `struct_name` in the manifest.
fn assert_keys_contain_manifest(
    struct_name: &str,
    value: &impl serde::Serialize,
    manifest: &BTreeMap<String, Vec<String>>,
) {
    let required = manifest
        .get(struct_name)
        .unwrap_or_else(|| panic!("struct {struct_name} not found in manifest"));
    let actual = keys(value);
    let missing: Vec<&str> = required
        .iter()
        .filter(|f| !actual.contains(*f))
        .map(String::as_str)
        .collect();
    assert!(
        missing.is_empty(),
        "Rust struct {struct_name} is missing manifest fields in its serialized output: \
         {missing:?}\n\
         Fix: either the Rust field was renamed (update the Rust struct or the manifest) or \
         the field uses skip_serializing_if / serde(rename) unexpectedly.\n\
         Actual keys: {actual:?}"
    );
}

fn assert_keys_match_manifest(
    struct_name: &str,
    value: &impl serde::Serialize,
    manifest: &BTreeMap<String, Vec<String>>,
) {
    let expected: BTreeSet<String> = manifest
        .get(struct_name)
        .unwrap_or_else(|| panic!("struct {struct_name} not found in manifest"))
        .iter()
        .cloned()
        .collect();
    let actual = keys(value);
    assert_eq!(
        actual, expected,
        "Rust struct {struct_name} must exactly match its manifest entry"
    );
}

use parish_core::debug_snapshot::{
    AuthDebug, ClockDebug, ConversationExchangeDebug, ConversationsDebug, DebugEvent,
    DebugSnapshot, DeflatedSummaryDebug, EdgeTraversalDebug, EventBusDebug, GameEventDebug,
    GossipDebug, GossipItemDebug, GraphEdgeDebug, InferenceCategoryDebug, InferenceDebug,
    IntelligenceDebug, LocationDebug, LongTermMemoryDebug, MemoryDebug, NpcDebug, ReactionDebug,
    RelationshipDebug, RelationshipEventDebug, ScheduleEntryDebug, ScheduleVariantDebug,
    TierSummary, WeatherDebug, WorldDebug,
};
use parish_core::ipc::{
    BugContext, BugReportRequest, BugReportResult, DialogueCorrectedPayload, LoadingPayload,
    MapData, MapLocation, NpcInfo, NpcReactionPayload, PlayerTaskSnapshot, ReconnectState,
    SaveState, StreamEndPayload, StreamTokenPayload, StreamTurnEndPayload, TextLogPayload,
    TileSourceSnapshot, TravelStartPayload, TravelWaypoint, WorldSnapshot,
};

#[test]
fn ipc_field_parity() {
    let manifest = load_manifest();

    // ── IPC core types ──────────────────────────────────────────────────────

    assert_keys_match_manifest(
        "WorldSnapshot",
        &WorldSnapshot {
            location_id: 1,
            location_name: "Crossroads".into(),
            location_description: "A dusty crossroads.".into(),
            time_label: "Morning".into(),
            hour: 8,
            minute: 30,
            weather: "Clear".into(),
            season: "Summer".into(),
            festival: None,
            paused: false,
            inference_paused: false,
            game_epoch_ms: 1_234_567_890.0,
            speed_factor: 36.0,
            name_hints: vec![],
            active_tasks: vec![],
            day_of_week: "Monday".into(),
            turn_in_flight: false,
        },
        &manifest,
    );

    assert_keys_match_manifest(
        "PlayerTaskSnapshot",
        &PlayerTaskSnapshot {
            id: 7,
            description: "weed the potato patch".into(),
            assigned_by: 11,
            location_id: 3,
            status: parish_types::TaskStatus::InProgress,
            assigned_at: Utc.with_ymd_and_hms(1820, 5, 14, 8, 0, 0).unwrap(),
            started_at: Some(Utc.with_ymd_and_hms(1820, 5, 14, 8, 30, 0).unwrap()),
            completed_at: None,
            last_matching_action: Some("I weed the potato patch".into()),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "MapLocation",
        &MapLocation {
            id: "1".into(),
            name: "Church".into(),
            lat: 53.0,
            lon: -7.0,
            adjacent: true,
            hops: 0,
            indoor: None,
            travel_minutes: None,
            visited: true,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "MapData",
        &MapData {
            locations: vec![],
            edges: vec![],
            player_location: "1".into(),
            edge_traversals: vec![],
            transport_label: "on foot".into(),
            transport_id: "walking".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "NpcInfo",
        &NpcInfo {
            npc_id: 7,
            name: "Seán".into(),
            real_name: "Seán Ó Briain".into(),
            occupation: "Farmer".into(),
            mood: "content".into(),
            introduced: true,
            mood_emoji: "😌".into(),
        },
        &manifest,
    );

    assert_keys_match_manifest(
        "ReconnectState",
        &ReconnectState {
            world: WorldSnapshot {
                location_id: 1,
                location_name: "Crossroads".into(),
                location_description: "A dusty crossroads.".into(),
                time_label: "Morning".into(),
                hour: 8,
                minute: 30,
                weather: "Clear".into(),
                season: "Spring".into(),
                festival: None,
                paused: false,
                inference_paused: false,
                game_epoch_ms: 0.0,
                speed_factor: 36.0,
                name_hints: vec![],
                active_tasks: vec![],
                day_of_week: "Monday".into(),
                turn_in_flight: false,
            },
            map: MapData {
                locations: vec![],
                edges: vec![],
                player_location: "1".into(),
                edge_traversals: vec![],
                transport_label: "on foot".into(),
                transport_id: "walking".into(),
            },
            npcs: vec![],
            context_epoch: 0,
        },
        &manifest,
    );

    assert!(
        keys(&NpcInfo {
            npc_id: 7,
            name: "Seán".into(),
            real_name: "Seán Ó Briain".into(),
            occupation: "Farmer".into(),
            mood: "content".into(),
            introduced: true,
            mood_emoji: "😌".into(),
        })
        .contains("npc_id"),
        "NpcInfo must carry its stable numeric identity across IPC"
    );

    assert_keys_contain_manifest(
        "StreamTokenPayload",
        &StreamTokenPayload {
            token: "hello".into(),
            turn_id: 1,
            source: "Siobhan".into(),
            message_id: Some("msg-1".into()),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "TextLogPayload",
        &TextLogPayload {
            id: "msg-1".into(),
            stream_turn_id: None,
            source: "system".into(),
            content: "Welcome".into(),
            subtype: None,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "StreamEndPayload",
        &StreamEndPayload { hints: vec![] },
        &manifest,
    );

    assert_keys_contain_manifest(
        "StreamTurnEndPayload",
        &StreamTurnEndPayload { turn_id: 1 },
        &manifest,
    );

    assert_keys_contain_manifest(
        "DialogueCorrectedPayload",
        &DialogueCorrectedPayload {
            turn_id: 1,
            corrected_text: "Good day to ye.".into(),
            message_id: Some("msg-1".into()),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "NpcReactionPayload",
        &NpcReactionPayload {
            message_id: "msg-1".into(),
            emoji: "👍".into(),
            source: "Seán".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "LoadingPayload",
        &LoadingPayload {
            active: true,
            spinner: None,
            phrase: None,
            color: None,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "TravelWaypoint",
        &TravelWaypoint {
            id: "1".into(),
            lat: 53.0,
            lon: -7.0,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "TravelStartPayload",
        &TravelStartPayload {
            from: "Home".into(),
            to: "Church".into(),
            waypoints: vec![],
            duration_minutes: 10,
            destination: "2".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "TileSource",
        &TileSourceSnapshot {
            id: "osm".into(),
            label: "OpenStreetMap".into(),
            url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".into(),
            tile_size: 256,
            minzoom: 0,
            maxzoom: 19,
            attribution: "© OpenStreetMap contributors".into(),
            raster_saturation: 0.0,
            raster_opacity: 1.0,
            tms: false,
        },
        &manifest,
    );

    // ── Bug report types ────────────────────────────────────────────────────

    assert_keys_contain_manifest(
        "BugReportRequest",
        &BugReportRequest {
            title: "Something broke".into(),
            description: "It broke badly.".into(),
            screenshot_data_url: None,
            context: None,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "BugContext",
        &BugContext {
            kind: "inference".into(),
            label: "Call #42".into(),
            detail: serde_json::Value::Null,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "BugReportResult",
        &BugReportResult {
            created: false,
            issue_url: None,
            issue_number: None,
            screenshot_url: None,
            bundle_path: None,
            message: "Dry-run: bundle written to disk.".into(),
        },
        &manifest,
    );

    // ── Debug snapshot types ────────────────────────────────────────────────

    let clock = ClockDebug {
        game_time: "08:30 1820-03-20".into(),
        time_of_day: "Morning".into(),
        season: "Spring".into(),
        festival: None,
        weather: "Clear".into(),
        paused: false,
        inference_paused: false,
        speed_factor: 36.0,
        speed_name: None,
        day_of_week: "Monday".into(),
        day_type: "Weekday".into(),
        start_game_time: "08:00 1820-03-20".into(),
        paused_game_time: "08:30 1820-03-20".into(),
        real_elapsed_secs: 0.0,
    };
    assert_keys_contain_manifest("ClockDebug", &clock, &manifest);

    let weather = WeatherDebug {
        current: "Clear".into(),
        since: "06:00 1820-03-20".into(),
        duration_hours: 2.5,
        min_duration_hours: 1.0,
        last_check_hour: None,
    };
    assert_keys_contain_manifest("WeatherDebug", &weather, &manifest);

    assert_keys_contain_manifest(
        "EdgeTraversalDebug",
        &EdgeTraversalDebug {
            from_name: "Church".into(),
            to_name: "Crossroads".into(),
            count: 3,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "GraphEdgeDebug",
        &GraphEdgeDebug {
            target_id: 2,
            target_name: "Church".into(),
            path_description: "A narrow path".into(),
            walking_minutes: 10,
        },
        &manifest,
    );

    let location = LocationDebug {
        id: 1,
        name: "Church".into(),
        indoor: true,
        public: true,
        connection_count: 2,
        npcs_here: vec![],
        visited: true,
        edges: vec![],
    };
    assert_keys_contain_manifest("LocationDebug", &location, &manifest);

    let world = WorldDebug {
        player_location_name: "Church".into(),
        player_location_id: 1,
        location_count: 10,
        visited_count: 3,
        visited_locations: vec![],
        edge_traversals: vec![],
        text_log_tail: vec![],
        text_log_len: 0,
        locations: vec![],
        player_name: None,
    };
    assert_keys_contain_manifest("WorldDebug", &world, &manifest);

    assert_keys_contain_manifest(
        "LongTermMemoryDebug",
        &LongTermMemoryDebug {
            timestamp: "08:00".into(),
            content: "Met the priest.".into(),
            importance: 0.8,
            keywords: vec!["priest".into()],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "ReactionDebug",
        &ReactionDebug {
            direction: parish_core::ReactionDirection::PlayerToNpc,
            timestamp: "08:00".into(),
            emoji: "😠".into(),
            description: "looked angry".into(),
            context: "You said something rude.".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "DeflatedSummaryDebug",
        &DeflatedSummaryDebug {
            location_name: "Church".into(),
            mood: "content".into(),
            recent_activity: vec![],
            key_relationship_changes: vec![],
        },
        &manifest,
    );

    let intel = IntelligenceDebug {
        verbal: 3,
        analytical: 4,
        emotional: 2,
        practical: 5,
        wisdom: 3,
        creative: 2,
    };
    assert_keys_contain_manifest("IntelligenceDebug", &intel, &manifest);

    assert_keys_contain_manifest(
        "ScheduleEntryDebug",
        &ScheduleEntryDebug {
            start_hour: 9,
            end_hour: 12,
            location_name: "Church".into(),
            activity: "Prayer".into(),
            is_current: true,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "ScheduleVariantDebug",
        &ScheduleVariantDebug {
            season: None,
            day_type: None,
            is_active: true,
            entries: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "RelationshipEventDebug",
        &RelationshipEventDebug {
            timestamp: "08:00".into(),
            description: "Had a disagreement.".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "RelationshipDebug",
        &RelationshipDebug {
            target_name: "Brigid".into(),
            kind: "friend".into(),
            strength: 0.7,
            history_count: 0,
            history: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "MemoryDebug",
        &MemoryDebug {
            timestamp: "08:00".into(),
            content: "Met the player.".into(),
            location_name: "Church".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "NpcDebug",
        &NpcDebug {
            id: 1,
            name: "Seán".into(),
            brief_description: "A weathered farmer.".into(),
            introduced: true,
            age: 45,
            occupation: "Farmer".into(),
            personality: "Quiet and reserved.".into(),
            location_name: "Church".into(),
            location_id: 1,
            home_name: None,
            workplace_name: None,
            mood: "content".into(),
            is_ill: false,
            state: "Present".into(),
            tier: "Tier1".into(),
            schedule: vec![],
            relationships: vec![],
            memories: vec![],
            long_term_memories: vec![],
            reactions: vec![],
            deflated_summary: None,
            knowledge: vec![],
            intelligence: IntelligenceDebug {
                verbal: 3,
                analytical: 3,
                emotional: 3,
                practical: 3,
                wisdom: 3,
                creative: 3,
            },
            last_activity: None,
            knows_player_name: false,
        },
        &manifest,
    );

    let tier_summary = TierSummary {
        tier1_count: 0,
        tier2_count: 0,
        tier3_count: 0,
        tier4_count: 0,
        tier1_names: vec![],
        tier2_names: vec![],
        tier3_names: vec![],
        tier4_names: vec![],
        tier3_in_flight: false,
        last_tier2_tick: None,
        last_tier3_tick: None,
        last_tier4_tick: None,
        introduced_count: 0,
        tier2_in_flight: false,
        tier3_pending_count: 0,
        tier4_recent_events: vec![],
    };
    assert_keys_contain_manifest("TierSummary", &tier_summary, &manifest);

    assert_keys_contain_manifest(
        "GameEventDebug",
        &GameEventDebug {
            timestamp: "08:00".into(),
            kind: "WeatherChanged".into(),
            summary: "Weather changed to clear.".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "EventBusDebug",
        &EventBusDebug {
            subscriber_count: 0,
            recent_events: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "GossipItemDebug",
        &GossipItemDebug {
            id: 1,
            content: "The priest is ill.".into(),
            source_name: "Brigid".into(),
            distortion_level: 0,
            known_by: vec![],
            timestamp: "08:00".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "GossipDebug",
        &GossipDebug {
            item_count: 0,
            items: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "ConversationExchangeDebug",
        &ConversationExchangeDebug {
            timestamp: "08:00".into(),
            speaker_id: 1,
            speaker_name: "Seán".into(),
            location_name: "Church".into(),
            player_input: "Hello".into(),
            npc_dialogue: "Good morning.".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "ConversationsDebug",
        &ConversationsDebug {
            exchange_count: 0,
            exchanges: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "DebugEvent",
        &DebugEvent {
            timestamp: "08:00".into(),
            category: "schedule".into(),
            message: "Seán moved to Church.".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "InferenceCategoryDebug",
        &InferenceCategoryDebug {
            role: "dialogue".into(),
            provider: None,
            model: None,
            base_url: None,
            thinking_level: parish_config::ThinkingLevel::Medium,
            max_output_tokens: 4_096,
            service_tier: parish_config::ServiceTier::Standard,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "AuthDebug",
        &AuthDebug {
            oauth_enabled: false,
            logged_in: false,
            provider: None,
            display_name: None,
            session_id: None,
        },
        &manifest,
    );

    let inference_debug = InferenceDebug {
        provider_name: "ollama".into(),
        model_name: "llama3".into(),
        base_url: "http://localhost:11434".into(),
        cloud_provider: None,
        cloud_model: None,
        has_queue: false,
        reaction_req_id: 0,
        improv_enabled: false,
        call_log: vec![],
        categories: vec![],
        configured_providers: vec![],
        tier2_parse_failures_total: 0,
    };
    assert_keys_contain_manifest("InferenceDebug", &inference_debug, &manifest);

    // InferenceLogEntry — from the inference crate.
    let log_entry = parish_inference::InferenceLogEntry {
        request_id: 1,
        timestamp: "14:32:05".into(),
        model: "llama3".into(),
        streaming: true,
        duration_ms: 250,
        prompt_len: 1024,
        response_len: 256,
        error: None,
        system_prompt: None,
        prompt_text: "Hello".into(),
        response_text: "World".into(),
        max_tokens: None,
        ttft_ms: None,
        output_tokens: None,
        temperature: None,
        priority: parish_inference::InferencePriority::Interactive,
        ..Default::default()
    };
    assert_keys_contain_manifest("InferenceLogEntry", &log_entry, &manifest);

    // DebugSnapshot as a whole.
    assert_keys_contain_manifest(
        "DebugSnapshot",
        &DebugSnapshot {
            clock,
            weather,
            world,
            npcs: vec![],
            tier_summary,
            event_bus: EventBusDebug::default(),
            gossip: GossipDebug::default(),
            conversations: ConversationsDebug::default(),
            events: vec![],
            inference: inference_debug,
            auth: AuthDebug::disabled(),
        },
        &manifest,
    );

    // ── Persistence types ───────────────────────────────────────────────────

    assert_keys_contain_manifest(
        "SnapshotCell",
        &parish_persistence::picker::SnapshotCell {
            id: 1,
            game_date: "20 Mar 1820, Morning".into(),
            location: None,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "SaveBranchDisplay",
        &parish_persistence::picker::SaveBranchDisplay {
            name: "main".into(),
            id: 1,
            parent_name: None,
            snapshot_count: 0,
            latest_location: None,
            latest_game_date: None,
            snapshots: vec![],
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "SaveFileInfo",
        &parish_persistence::picker::SaveFileInfo {
            path: std::path::PathBuf::from("parish_001.db"),
            filename: "parish_001.db".into(),
            file_size: "12 KB".into(),
            branches: vec![],
            locked: false,
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "SaveState",
        &SaveState {
            filename: None,
            branch_id: None,
            branch_name: None,
        },
        &manifest,
    );

    // ── Demo types ──────────────────────────────────────────────────────────

    assert_keys_contain_manifest(
        "DemoNpcInfo",
        &parish_core::ipc::demo::DemoNpcInfo {
            name: "Seán".into(),
            occupation: None,
            mood: "content".into(),
        },
        &manifest,
    );

    assert_keys_contain_manifest(
        "DemoAdjacentLocation",
        &parish_core::ipc::demo::DemoAdjacentLocation {
            name: "Church".into(),
            travel_minutes: None,
            visited: true,
        },
        &manifest,
    );

    assert_keys_match_manifest(
        "DemoContextSnapshot",
        &parish_core::ipc::demo::DemoContextSnapshot {
            location_name: "Crossroads".into(),
            location_description: "A dusty crossroads.".into(),
            game_time: "Wednesday, 14 May 1820, morning".into(),
            season: "spring".into(),
            weather: "Clear".into(),
            npcs_here: vec![],
            adjacent: vec![],
            active_tasks: vec![],
            recent_log: vec![],
            recent_actions: vec![],
            extra_prompt: None,
        },
        &manifest,
    );
}
