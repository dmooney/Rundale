use super::build::*;
use super::*;
use parish_npc::manager::NpcManager;
use parish_npc::{Npc, NpcId};
use parish_world::WorldState;
use parish_world::events::GameEvent;
use parish_world::graph::WorldGraph;
use parish_world::time::{DayType, Season};
use std::collections::VecDeque;

/// Helper: build a minimal `InferenceDebug` for tests.
fn test_inference() -> InferenceDebug {
    InferenceDebug {
        provider_name: "ollama".to_string(),
        model_name: "test-model".to_string(),
        base_url: "http://localhost:11434".to_string(),
        cloud_provider: None,
        cloud_model: None,
        has_queue: false,
        reaction_req_id: 100_000,
        improv_enabled: false,
        call_log: vec![],
        categories: vec![],
        configured_providers: vec![],
        tier2_parse_failures_total: 0,
    }
}

#[test]
fn test_build_debug_snapshot_empty() {
    let world = WorldState::new();
    let npc_manager = NpcManager::new();
    let events = VecDeque::new();
    let game_events: VecDeque<GameEvent> = VecDeque::new();
    let inference = test_inference();

    let snapshot = build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    );

    assert!(snapshot.clock.game_time.contains("08:00"));
    assert_eq!(snapshot.clock.weather, "Clear");
    assert!(!snapshot.clock.paused);
    assert!(!snapshot.clock.inference_paused);
    assert_eq!(snapshot.weather.current, "Clear");
    assert!(snapshot.npcs.is_empty());
    assert_eq!(snapshot.tier_summary.tier1_count, 0);
    assert_eq!(snapshot.inference.provider_name, "ollama");
    assert_eq!(snapshot.gossip.item_count, 0);
    assert_eq!(snapshot.conversations.exchange_count, 0);
}

#[test]
fn test_build_debug_snapshot_with_npc() {
    let world = WorldState::new();
    let mut npc_manager = NpcManager::new();
    npc_manager.add_npc(Npc::new_test_npc());
    npc_manager.assign_tiers(&world, &[]);

    let events = VecDeque::new();
    let game_events: VecDeque<GameEvent> = VecDeque::new();
    let mut inference = test_inference();
    inference.has_queue = true;

    let snapshot = build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    );

    assert_eq!(snapshot.npcs.len(), 1);
    assert_eq!(snapshot.npcs[0].name, "Padraig O'Brien");
    assert_eq!(snapshot.npcs[0].mood, "content");
    assert_eq!(snapshot.npcs[0].state, "Present");
    assert!(!snapshot.npcs[0].introduced);
    assert_eq!(
        snapshot.npcs[0].brief_description,
        "an older man behind the bar"
    );
    // Intelligence matches new_test_npc: Intelligence::new(3, 3, 4, 4, 5, 4)
    let intel = &snapshot.npcs[0].intelligence;
    assert_eq!(intel.verbal, 3);
    assert_eq!(intel.analytical, 3);
    assert_eq!(intel.emotional, 4);
    assert_eq!(intel.practical, 4);
    assert_eq!(intel.wisdom, 5);
    assert_eq!(intel.creative, 4);
}

#[test]
fn test_build_clock_debug() {
    let world = WorldState::new();
    let clock = build_clock_debug(&world);

    assert!(clock.game_time.contains("08:00"));
    assert_eq!(clock.time_of_day, "Morning");
    assert_eq!(clock.season, "Spring");
    assert_eq!(clock.weather, "Clear");
    assert!(!clock.paused);
}

#[test]
fn test_build_tier_summary_empty() {
    let mgr = NpcManager::new();
    let summary = build_tier_summary(&mgr);
    assert_eq!(summary.tier1_count, 0);
    assert_eq!(summary.tier2_count, 0);
    assert_eq!(summary.tier3_count, 0);
    assert_eq!(summary.tier4_count, 0);
}

#[test]
fn test_build_tier_summary_with_npcs() {
    let world = WorldState::new();
    let mut mgr = NpcManager::new();
    mgr.add_npc(Npc::new_test_npc());
    mgr.assign_tiers(&world, &[]);

    let summary = build_tier_summary(&mgr);
    // Test NPC is at LocationId(1) = player location = Tier1
    assert_eq!(summary.tier1_count, 1);
    assert!(summary.tier1_names.contains(&"Padraig O'Brien".to_string()));
}

#[test]
fn test_build_world_debug() {
    let world = WorldState::new();
    let mgr = NpcManager::new();
    let w = build_world_debug(&world, &mgr);

    assert_eq!(w.player_location_id, 1);
    assert!(!w.player_location_name.is_empty());
}

#[test]
fn test_debug_event_serialize() {
    let event = DebugEvent {
        timestamp: "08:00 1820-03-20".to_string(),
        category: "system".to_string(),
        message: "Game started".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("Game started"));
    assert!(json.contains("system"));
}

#[test]
fn test_inference_log_entry_serialize() {
    let entry = InferenceLogEntry {
        request_id: 42,
        timestamp: "14:32:05".to_string(),
        model: "qwen3:14b".to_string(),
        streaming: true,
        duration_ms: 1250,
        prompt_len: 500,
        response_len: 200,
        error: None,
        system_prompt: Some("You are helpful.".to_string()),
        prompt_text: "Hello world".to_string(),
        response_text: "Hi there!".to_string(),
        max_tokens: Some(300),
        ttft_ms: Some(120),
        output_tokens: Some(40),
        temperature: Some(0.7),
        priority: parish_inference::InferencePriority::Interactive,
        ..Default::default()
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("qwen3:14b"));
    assert!(json.contains("1250"));
    assert!(json.contains("\"streaming\":true"));
}

#[test]
fn test_inference_log_entry_with_error() {
    let entry = InferenceLogEntry {
        request_id: 7,
        timestamp: "09:00:00".to_string(),
        model: "test".to_string(),
        streaming: false,
        duration_ms: 30000,
        prompt_len: 100,
        response_len: 0,
        error: Some("timeout".to_string()),
        system_prompt: None,
        prompt_text: "test prompt".to_string(),
        response_text: String::new(),
        max_tokens: None,
        ttft_ms: None,
        output_tokens: None,
        temperature: None,
        priority: parish_inference::InferencePriority::Interactive,
        ..Default::default()
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("timeout"));
    assert!(json.contains("\"response_len\":0"));
}

#[test]
fn test_call_log_included_in_snapshot() {
    let world = WorldState::new();
    let mgr = NpcManager::new();
    let events = VecDeque::new();
    let game_events: VecDeque<GameEvent> = VecDeque::new();
    let entry = InferenceLogEntry {
        request_id: 1,
        timestamp: "10:00:00".to_string(),
        model: "test-model".to_string(),
        streaming: true,
        duration_ms: 500,
        prompt_len: 100,
        response_len: 50,
        error: None,
        system_prompt: None,
        prompt_text: "test".to_string(),
        response_text: "response".to_string(),
        max_tokens: None,
        ttft_ms: None,
        output_tokens: None,
        temperature: None,
        priority: parish_inference::InferencePriority::Interactive,
        ..Default::default()
    };
    let mut inference = test_inference();
    inference.call_log = vec![entry];

    let snapshot = build_debug_snapshot(
        &world,
        &mgr,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    );
    assert_eq!(snapshot.inference.call_log.len(), 1);
    assert_eq!(snapshot.inference.call_log[0].request_id, 1);
    assert_eq!(snapshot.inference.call_log[0].duration_ms, 500);
}

#[test]
fn test_events_included_in_snapshot() {
    let world = WorldState::new();
    let mgr = NpcManager::new();
    let mut events = VecDeque::new();
    let game_events: VecDeque<GameEvent> = VecDeque::new();
    events.push_back(DebugEvent {
        timestamp: "08:00".to_string(),
        category: "system".to_string(),
        message: "Test event".to_string(),
    });
    events.push_back(DebugEvent {
        timestamp: "08:05".to_string(),
        category: "schedule".to_string(),
        message: "NPC moved".to_string(),
    });
    let inference = test_inference();

    let snapshot = build_debug_snapshot(
        &world,
        &mgr,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    );
    assert_eq!(snapshot.events.len(), 2);
    assert_eq!(snapshot.events[0].message, "Test event");
    assert_eq!(snapshot.events[1].category, "schedule");
}

#[test]
fn test_npc_debug_relationships_sorted() {
    use parish_npc::types::{Relationship, RelationshipKind};

    let mut npc = Npc::new_test_npc();
    npc.relationships
        .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));
    npc.relationships
        .insert(NpcId(3), Relationship::new(RelationshipKind::Rival, -0.3));

    let mut mgr = NpcManager::new();
    mgr.add_npc(npc);

    let graph = WorldGraph::new();
    let npcs = build_npc_debug_list(&mgr, &graph, 10, Season::Spring, DayType::Weekday);
    assert_eq!(npcs.len(), 1);
    // Relationships should be sorted by strength descending
    assert_eq!(npcs[0].relationships.len(), 2);
    assert!(npcs[0].relationships[0].strength > npcs[0].relationships[1].strength);
}

#[test]
fn test_npc_debug_new_fields() {
    let world = WorldState::new();
    let mut mgr = NpcManager::new();
    let npc = Npc::new_test_npc();
    mgr.add_npc(npc);
    mgr.assign_tiers(&world, &[]);

    let graph = WorldGraph::new();
    let npcs = build_npc_debug_list(&mgr, &graph, 10, Season::Spring, DayType::Weekday);
    assert_eq!(npcs.len(), 1);
    // New fields: is_ill should be false for a healthy NPC
    assert!(!npcs[0].is_ill);
    // No deflated summary on a fresh NPC
    assert!(npcs[0].deflated_summary.is_none());
    // Long-term memory starts empty
    assert!(npcs[0].long_term_memories.is_empty());
}

#[test]
fn test_tier_summary_new_fields() {
    let mgr = NpcManager::new();
    let summary = build_tier_summary(&mgr);
    // New fields: defaults
    assert!(!summary.tier2_in_flight);
    assert!(summary.last_tier2_tick.is_none());
    assert_eq!(summary.tier3_pending_count, 0);
    assert!(summary.tier4_recent_events.is_empty());
}

#[test]
fn test_gossip_debug_empty() {
    let world = WorldState::new();
    let mgr = NpcManager::new();
    let g = build_gossip_debug(&world, &mgr);
    assert_eq!(g.item_count, 0);
    assert!(g.items.is_empty());
}

#[test]
fn test_gossip_debug_serializes_in_snapshot() {
    let world = WorldState::new();
    let mgr = NpcManager::new();
    let events = VecDeque::new();
    let game_events: VecDeque<GameEvent> = VecDeque::new();
    let inference = InferenceDebug {
        provider_name: "test".to_string(),
        model_name: "test".to_string(),
        base_url: "http://localhost".to_string(),
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
    let snapshot = build_debug_snapshot(
        &world,
        &mgr,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    );
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("gossip"));
    assert!(json.contains("item_count"));
}

#[test]
fn test_recent_tier4_events_in_tier_summary() {
    use chrono::Utc;
    use parish_npc::tier4::Tier4Event;

    let world = WorldState::new();
    let mut mgr = NpcManager::new();
    let npc = Npc::new_test_npc();
    let npc_id = npc.id;
    mgr.add_npc(npc);
    mgr.assign_tiers(&world, &[]);

    // Apply an Illness event — should populate ring buffer
    let events = vec![Tier4Event::Illness { npc_id }];
    mgr.apply_tier4_events(&events, Utc::now(), true);

    let summary = build_tier_summary(&mgr);
    assert_eq!(summary.tier4_recent_events.len(), 1);
    assert!(summary.tier4_recent_events[0].contains("ill"));
}
