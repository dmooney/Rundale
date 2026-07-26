//! Unit tests for `NpcManager`.

use super::*;
use crate::test_helpers::{load_test_graph, make_chain_graph, make_test_npc, make_test_world};
use crate::types::{NpcState, Relationship};
use chrono::{Duration, TimeZone};
use parish_config::CognitiveTierConfig;

#[test]
fn test_manager_new_empty() {
    let mgr = NpcManager::new();
    assert_eq!(mgr.npc_count(), 0);
}

#[test]
fn test_introduction_tracking() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));

    assert!(!mgr.is_introduced(NpcId(1)));
    mgr.mark_introduced(NpcId(1));
    assert!(mgr.is_introduced(NpcId(1)));
    assert!(!mgr.is_introduced(NpcId(2)));
}

#[test]
fn test_display_name_uses_introduction_state() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));
    let npc = mgr.get(NpcId(1)).unwrap().clone();

    assert_eq!(mgr.display_name(&npc), "a person");
    mgr.mark_introduced(NpcId(1));
    let npc = mgr.get(NpcId(1)).unwrap().clone();
    assert_eq!(mgr.display_name(&npc), "NPC 1");
}

#[test]
fn test_add_and_get_npc() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));

    assert_eq!(mgr.npc_count(), 1);
    assert!(mgr.get(NpcId(1)).is_some());
    assert_eq!(mgr.get(NpcId(1)).unwrap().name, "NPC 1");
    assert!(mgr.get(NpcId(99)).is_none());
}

#[test]
fn relationship_tone_hints_project_speaker_relationships_with_target_names() {
    let mut mgr = NpcManager::new();
    let mut speaker = make_test_npc(1, 2);
    speaker.relationships.insert(
        NpcId(2),
        Relationship::new(crate::types::RelationshipKind::Rival, -0.4),
    );
    speaker.relationships.insert(
        NpcId(99),
        Relationship::new(crate::types::RelationshipKind::Friend, 0.7),
    );

    let mut target = make_test_npc(2, 2);
    target.name = "Mick Flanagan".to_string();

    mgr.add_npc(speaker);
    mgr.add_npc(target);

    let hints = mgr.relationship_tone_hints(NpcId(1));

    assert_eq!(hints.len(), 1, "dangling relationship targets are ignored");
    assert_eq!(hints[0].target_name, "Mick Flanagan");
    assert_eq!(hints[0].kind, crate::types::RelationshipKind::Rival);
    assert_eq!(hints[0].strength, -0.4);
}

#[test]
fn test_npcs_at_location() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));
    mgr.add_npc(make_test_npc(2, 2));
    mgr.add_npc(make_test_npc(3, 3));

    assert_eq!(mgr.npcs_at(LocationId(2)).len(), 2);
    assert_eq!(mgr.npcs_at(LocationId(3)).len(), 1);
    assert!(mgr.npcs_at(LocationId(99)).is_empty());
}

#[test]
fn test_in_transit_excluded_from_npcs_at() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.set_state(NpcState::InTransit {
        from: LocationId(2),
        to: LocationId(3),
        arrives_at: chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(),
        activity: None,
    });
    mgr.add_npc(npc);

    assert!(mgr.npcs_at(LocationId(2)).is_empty());
    assert!(mgr.npcs_at(LocationId(3)).is_empty());
}

#[test]
fn test_default_manager() {
    let mgr = NpcManager::default();
    assert_eq!(mgr.npc_count(), 0);
}

#[test]
fn test_find_by_name_exact_match() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.name = "Padraig Darcy".to_string();
    mgr.add_npc(npc);
    mgr.mark_introduced(NpcId(1));

    let found = mgr.find_by_name("Padraig Darcy", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
}

#[test]
fn test_find_by_name_case_insensitive() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.name = "Padraig Darcy".to_string();
    mgr.add_npc(npc);
    mgr.mark_introduced(NpcId(1));

    assert!(mgr.find_by_name("padraig darcy", LocationId(2)).is_some());
}

#[test]
fn test_find_by_name_first_name_match() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.name = "Padraig Darcy".to_string();
    mgr.add_npc(npc);
    mgr.mark_introduced(NpcId(1));

    let found = mgr.find_by_name("Padraig", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
}

#[test]
fn test_find_by_name_ambiguous_first_name_returns_none() {
    let mut mgr = NpcManager::new();
    let mut a = make_test_npc(1, 2);
    a.name = "Mary Byrne".to_string();
    let mut b = make_test_npc(2, 2);
    b.name = "Mary Kelly".to_string();
    mgr.add_npc(a);
    mgr.add_npc(b);
    mgr.mark_introduced(NpcId(1));
    mgr.mark_introduced(NpcId(2));

    assert!(mgr.find_by_name("Mary", LocationId(2)).is_none());
}

#[test]
fn test_find_by_name_wrong_location() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.name = "Padraig Darcy".to_string();
    mgr.add_npc(npc);
    mgr.mark_introduced(NpcId(1));

    assert!(mgr.find_by_name("Padraig", LocationId(99)).is_none());
}

#[test]
fn test_find_by_name_no_match() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));
    mgr.mark_introduced(NpcId(1));

    assert!(mgr.find_by_name("Nobody", LocationId(2)).is_none());
}

#[test]
fn test_find_by_role_at_unique_match_resolves() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.occupation = "Widow".to_string();
    mgr.add_npc(npc);

    let found = mgr.find_by_role_at("Widow", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
}

#[test]
fn test_find_by_role_at_case_insensitive() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.occupation = "Father".to_string();
    mgr.add_npc(npc);

    assert!(mgr.find_by_role_at("father", LocationId(2)).is_some());
    assert!(mgr.find_by_role_at("FATHER", LocationId(2)).is_some());
}

#[test]
fn test_find_by_role_at_ambiguous_returns_none() {
    let mut mgr = NpcManager::new();
    let mut a = make_test_npc(1, 2);
    a.occupation = "Farmer".to_string();
    let mut b = make_test_npc(2, 2);
    b.occupation = "Farmer".to_string();
    mgr.add_npc(a);
    mgr.add_npc(b);

    assert!(
        mgr.find_by_role_at("Farmer", LocationId(2)).is_none(),
        "two NPCs share the role — resolver must refuse to guess"
    );
}

#[test]
fn test_find_by_role_at_wrong_location_returns_none() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.occupation = "Widow".to_string();
    mgr.add_npc(npc);

    assert!(mgr.find_by_role_at("Widow", LocationId(99)).is_none());
}

#[test]
fn test_find_by_role_at_token_overlap_priest_matches_parish_priest() {
    // Player addresses "Priest" — Rundale data uses "Parish Priest".
    let mut mgr = NpcManager::new();
    let mut tierney = make_test_npc(1, 2);
    tierney.occupation = "Parish Priest".to_string();
    mgr.add_npc(tierney);

    let found = mgr.find_by_role_at("Priest", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
}

#[test]
fn test_find_by_role_at_token_overlap_constable_matches_retired_constable() {
    let mut mgr = NpcManager::new();
    let mut flanagan = make_test_npc(1, 2);
    flanagan.occupation = "Retired Constable".to_string();
    mgr.add_npc(flanagan);

    assert!(mgr.find_by_role_at("Constable", LocationId(2)).is_some());
}

#[test]
fn test_find_by_role_at_alias_father_routes_to_priest() {
    // "Father, a word" — period-correct vocative for a Catholic priest.
    let mut mgr = NpcManager::new();
    let mut tierney = make_test_npc(1, 2);
    tierney.occupation = "Parish Priest".to_string();
    mgr.add_npc(tierney);

    let found = mgr.find_by_role_at("Father", LocationId(2));
    assert!(found.is_some(), "Father vocative should resolve to priest");
    assert_eq!(found.unwrap().id, NpcId(1));

    // "Fr." / "Fr" abbreviations.
    assert!(mgr.find_by_role_at("Fr.", LocationId(2)).is_some());
    assert!(mgr.find_by_role_at("fr", LocationId(2)).is_some());
}

#[test]
fn test_find_by_role_at_alias_refuses_when_ambiguous_across_priests() {
    let mut mgr = NpcManager::new();
    let mut priest = make_test_npc(1, 2);
    priest.occupation = "Parish Priest".to_string();
    let mut curate = make_test_npc(2, 2);
    curate.occupation = "Curate Priest".to_string();
    mgr.add_npc(priest);
    mgr.add_npc(curate);

    assert!(
        mgr.find_by_role_at("Father", LocationId(2)).is_none(),
        "two priests share the vocative — must refuse"
    );
}

#[test]
fn test_find_by_role_at_unknown_alias_does_not_match() {
    let mut mgr = NpcManager::new();
    let mut publican = make_test_npc(1, 2);
    publican.occupation = "Publican".to_string();
    mgr.add_npc(publican);

    // "Sir" is not a registered alias and isn't a token of "Publican".
    assert!(mgr.find_by_role_at("Sir", LocationId(2)).is_none());
}

#[test]
fn test_find_by_role_at_empty_input_returns_none() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.occupation = "Widow".to_string();
    mgr.add_npc(npc);

    assert!(mgr.find_by_role_at("", LocationId(2)).is_none());
    assert!(mgr.find_by_role_at("   ", LocationId(2)).is_none());
}

#[test]
fn test_find_by_name_unintroduced_uses_brief_description() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.brief_description = "an older man behind the bar".to_string();
    mgr.add_npc(npc);

    let found = mgr.find_by_name("an older man behind the bar", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
}

#[test]
fn test_find_by_name_unintroduced_allows_exact_canonical_target() {
    let mut mgr = NpcManager::new();
    let mut npc = make_test_npc(1, 2);
    npc.name = "Padraig Darcy".to_string();
    npc.brief_description = "an older man behind the bar".to_string();
    mgr.add_npc(npc);

    let found = mgr.find_by_name("Padraig Darcy", LocationId(2));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, NpcId(1));
    assert!(mgr.find_by_name("Padraig", LocationId(2)).is_none());
}

#[test]
fn test_known_roster_unions_home_and_work_matches() {
    let mut mgr = NpcManager::new();
    let mut subject = make_test_npc(1, 10);
    subject.home = Some(LocationId(10));
    subject.workplace = Some(LocationId(20));
    mgr.add_npc(subject.clone());

    let mut home_mate = make_test_npc(2, 30);
    home_mate.home = Some(LocationId(10));
    home_mate.workplace = Some(LocationId(30));
    mgr.add_npc(home_mate);

    let mut work_mate = make_test_npc(3, 20);
    work_mate.home = Some(LocationId(40));
    work_mate.workplace = Some(LocationId(20));
    mgr.add_npc(work_mate);

    let mut visitor = make_test_npc(4, 10);
    visitor.home = Some(LocationId(50));
    mgr.add_npc(visitor);

    let mut both = make_test_npc(5, 10);
    both.home = Some(LocationId(10));
    both.workplace = Some(LocationId(20));
    mgr.add_npc(both);

    let mut stranger = make_test_npc(6, 99);
    stranger.home = Some(LocationId(99));
    stranger.workplace = Some(LocationId(98));
    mgr.add_npc(stranger);

    let roster = mgr.known_roster(&subject);
    let ids: HashSet<NpcId> = roster.iter().map(|(id, _, _)| *id).collect();

    assert!(ids.contains(&NpcId(2)), "home-mate should be in roster");
    assert!(ids.contains(&NpcId(3)), "work-mate should be in roster");
    assert!(
        ids.contains(&NpcId(4)),
        "co-present at home should be in roster"
    );
    assert!(
        ids.contains(&NpcId(5)),
        "sharing both home and work should be in roster"
    );
    assert!(
        !ids.contains(&NpcId(6)),
        "unrelated NPC must not be in roster"
    );
    assert!(
        !ids.contains(&NpcId(1)),
        "subject must not be in its own roster"
    );
    assert_eq!(ids.len(), roster.len(), "no duplicates");
}

#[test]
fn known_roster_descriptor_carries_pronouns_and_age() {
    // #1506: the roster descriptor must ground the model in pronouns + age so
    // it never guesses gender from a name.
    let mut mgr = NpcManager::new();
    let mut subject = make_test_npc(1, 10);
    subject.home = Some(LocationId(10));
    mgr.add_npc(subject.clone());

    let mut mate = make_test_npc(2, 10);
    mate.home = Some(LocationId(10));
    // Explicit, not relying on make_test_npc defaults (gemini review #1516).
    mate.pronouns = "she/her".to_string();
    mate.age = 42;
    mate.occupation = "Weaver".to_string();
    mgr.add_npc(mate);

    let roster = mgr.known_roster(&subject);
    let entry = roster
        .iter()
        .find(|(id, _, _)| *id == NpcId(2))
        .expect("home-mate must be in the roster");
    let descriptor = &entry.2;
    assert!(
        descriptor.contains("she/her"),
        "descriptor must carry pronouns: {descriptor:?}"
    );
    assert!(
        descriptor.contains("42"),
        "descriptor must carry age: {descriptor:?}"
    );
    assert!(
        descriptor.contains("Weaver"),
        "descriptor must keep occupation: {descriptor:?}"
    );
}

#[test]
fn test_load_from_file() {
    let path = std::path::Path::new("data/npcs.json");
    if !path.exists() {
        return;
    }
    let mgr = NpcManager::load_from_file(path).unwrap();
    assert_eq!(mgr.npc_count(), 23);
}

// ── Tick state management ────────────────────────────────────────────────

#[test]
fn test_needs_tier2_tick() {
    let mgr = NpcManager::new();
    let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    assert!(mgr.needs_tier2_tick(now));
}

#[test]
fn test_tier2_tick_interval() {
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    mgr.record_tier2_tick(t0);

    assert!(!mgr.needs_tier2_tick(t0 + Duration::minutes(3)));
    assert!(mgr.needs_tier2_tick(t0 + Duration::minutes(5)));
    assert!(mgr.needs_tier2_tick(t0 + Duration::minutes(10)));
}

#[test]
fn test_needs_tier2_tick_with_config_custom_interval() {
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    mgr.record_tier2_tick(t0);

    let config = CognitiveTierConfig {
        tier2_tick_interval_minutes: 10,
        ..CognitiveTierConfig::default()
    };
    assert!(!mgr.needs_tier2_tick_with_config(t0 + Duration::minutes(5), &config));
    assert!(mgr.needs_tier2_tick_with_config(t0 + Duration::minutes(10), &config));
}

#[test]
fn test_needs_tier2_tick_with_config_first_tick() {
    let mgr = NpcManager::new();
    let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    let config = CognitiveTierConfig {
        tier2_tick_interval_minutes: 10,
        ..CognitiveTierConfig::default()
    };
    assert!(mgr.needs_tier2_tick_with_config(now, &config));
}

#[test]
fn test_tier2_in_flight_tracking() {
    let mut mgr = NpcManager::new();
    assert!(!mgr.tier2_in_flight());
    mgr.set_tier2_in_flight(true);
    assert!(mgr.tier2_in_flight());
    mgr.set_tier2_in_flight(false);
    assert!(!mgr.tier2_in_flight());
}

#[test]
fn test_tier3_tick_interval() {
    let config = CognitiveTierConfig::default();
    let mgr = NpcManager::new();
    let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    assert!(mgr.needs_tier3_tick_with_config(now, &config));
}

#[test]
fn test_tier3_tick_not_yet_due() {
    let config = CognitiveTierConfig::default();
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
    mgr.record_tier3_tick(t0);
    let t1 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    assert!(!mgr.needs_tier3_tick_with_config(t1, &config));
}

#[test]
fn test_tier3_tick_due() {
    let config = CognitiveTierConfig::default();
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
    mgr.record_tier3_tick(t0);
    let t1 = chrono::Utc.with_ymd_and_hms(1820, 3, 21, 0, 0, 0).unwrap();
    assert!(mgr.needs_tier3_tick_with_config(t1, &config));
}

#[test]
fn test_tier3_in_flight_tracking() {
    let mut mgr = NpcManager::new();
    assert!(!mgr.tier3_in_flight());
    mgr.set_tier3_in_flight(true);
    assert!(mgr.tier3_in_flight());
    mgr.set_tier3_in_flight(false);
    assert!(!mgr.tier3_in_flight());
}

#[test]
fn test_tier4_tick_never_ticked() {
    let mgr = NpcManager::new();
    let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    assert!(mgr.needs_tier4_tick(now));
    assert!(mgr.last_tier4_game_time().is_none());
}

#[test]
fn test_tier4_tick_not_yet_due() {
    let config = CognitiveTierConfig::default();
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
    mgr.record_tier4_tick(t0);
    let t1 = chrono::Utc.with_ymd_and_hms(1820, 4, 19, 0, 0, 0).unwrap();
    assert!(!mgr.needs_tier4_tick_with_config(t1, &config));
    assert_eq!(mgr.last_tier4_game_time(), Some(t0));
}

#[test]
fn test_tier4_tick_due_after_interval() {
    let config = CognitiveTierConfig::default();
    let mut mgr = NpcManager::new();
    let t0 = chrono::Utc.with_ymd_and_hms(1820, 1, 1, 0, 0, 0).unwrap();
    mgr.record_tier4_tick(t0);
    let t1 = chrono::Utc.with_ymd_and_hms(1820, 4, 1, 0, 0, 0).unwrap();
    assert!(mgr.needs_tier4_tick_with_config(t1, &config));
}

// ── remove_npc reference-scrubbing (#339) ────────────────────────────────

#[test]
fn remove_npc_scrubs_all_references() {
    let mut mgr = NpcManager::new();
    for id in [10, 20, 30] {
        mgr.add_npc(make_test_npc(id, 0));
    }
    mgr.tier_assignments.insert(NpcId(20), CogTier::Tier1);
    mgr.introduced_npcs.insert(NpcId(20));
    mgr.npcs_who_know_player_name.insert(NpcId(20));

    mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
        NpcId(20),
        Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
    );
    mgr.npcs.get_mut(&NpcId(30)).unwrap().relationships.insert(
        NpcId(20),
        Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
    );
    mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
        NpcId(30),
        Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
    );

    let removed = mgr.remove_npc(NpcId(20));
    assert!(removed.is_some());

    assert!(mgr.get(NpcId(20)).is_none());
    assert!(!mgr.tier_assignments.contains_key(&NpcId(20)));
    assert!(!mgr.introduced_npcs.contains(&NpcId(20)));
    assert!(!mgr.npcs_who_know_player_name.contains(&NpcId(20)));

    let n10 = mgr.get(NpcId(10)).unwrap();
    assert!(!n10.relationships.contains_key(&NpcId(20)));
    assert!(n10.relationships.contains_key(&NpcId(30)));
    let n30 = mgr.get(NpcId(30)).unwrap();
    assert!(!n30.relationships.contains_key(&NpcId(20)));
}

#[test]
fn remove_npc_returns_none_for_missing_id() {
    let mut mgr = NpcManager::new();
    assert!(mgr.remove_npc(NpcId(9_999_999)).is_none());
}

// ── Integration tests (manager coordinates subsystems) ───────────────────

/// tier2_groups depends on both assign_tiers (writes tier_assignments)
/// and npcs state — tested here as an integration of both.
#[test]
fn test_tier2_groups() {
    let graph = match load_test_graph() {
        Some(g) => g,
        None => return,
    };
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));
    mgr.add_npc(make_test_npc(2, 2));
    mgr.add_npc(make_test_npc(3, 3));

    let world = make_test_world(graph, 1);
    mgr.assign_tiers(&world, &[]);

    let groups = mgr.tier2_groups();
    assert_eq!(groups.get(&LocationId(2)).map(|v| v.len()), Some(2));
}

/// #1025: tier2_groups returns only locations with >=2 co-located
/// Tier 2 NPCs; a solo Tier 2 NPC's location is gated out. Uses a
/// chain graph so the assertion runs without the optional data file.
#[test]
fn test_tier2_groups_excludes_solo() {
    use parish_world::WorldState;

    let graph = make_chain_graph(4); // 0 — 1 — 2 — 3 — 4
    let mut mgr = NpcManager::new();
    // Player at loc 0. Tier 2 = BFS distance 1..=2 (engine defaults).
    mgr.add_npc(make_test_npc(1, 1)); // loc 1, dist 1 → Tier2 (group)
    mgr.add_npc(make_test_npc(2, 1)); // loc 1, dist 1 → Tier2 (group)
    mgr.add_npc(make_test_npc(3, 2)); // loc 2, dist 2 → Tier2 (solo)
    mgr.add_npc(make_test_npc(4, 3)); // loc 3, dist 3 → Tier3

    let mut world = WorldState::new();
    world.player_location = LocationId(0);
    world.graph = graph;
    mgr.assign_tiers(&world, &[]);

    // Sanity: the solo NPC really is Tier 2, so its exclusion is the gate.
    assert_eq!(mgr.tier_of(NpcId(3)), Some(CogTier::Tier2));

    let groups = mgr.tier2_groups();
    // Location 1 holds two co-located Tier 2 NPCs → included with count 2.
    assert_eq!(groups.get(&LocationId(1)).map(|v| v.len()), Some(2));
    // Location 2 holds a single Tier 2 NPC → excluded.
    assert!(!groups.contains_key(&LocationId(2)));
    // No surviving group has fewer than two members.
    assert!(groups.values().all(|ids| ids.len() >= 2));
    assert_eq!(groups.len(), 1);
}

#[test]
fn test_tier2_dispatch_wiring_cycle() {
    use parish_world::WorldState;

    let graph = make_chain_graph(4);
    let mut mgr = NpcManager::new();
    // Two co-located Tier 2 NPCs so tier2_groups yields a >=2 group (#1025).
    mgr.add_npc(make_test_npc(20, 2)); // distance 2 → Tier2
    mgr.add_npc(make_test_npc(21, 2)); // distance 2 → Tier2

    let mut world = WorldState::new();
    world.player_location = LocationId(0);
    world.graph = graph;
    mgr.assign_tiers(&world, &[]);

    assert_eq!(mgr.tier_of(NpcId(20)), Some(CogTier::Tier2));

    let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

    assert!(mgr.needs_tier2_tick(now));
    assert!(!mgr.tier2_in_flight());
    assert!(mgr.needs_tier2_tick(now) && !mgr.tier2_in_flight());

    mgr.set_tier2_in_flight(true);
    assert!(!mgr.needs_tier2_tick(now) || mgr.tier2_in_flight());

    let groups = mgr.tier2_groups();
    assert!(!groups.is_empty());

    mgr.record_tier2_tick(now);
    mgr.set_tier2_in_flight(false);

    assert_eq!(mgr.last_tier2_game_time(), Some(now));
    assert!(!mgr.tier2_in_flight());
    assert!(!mgr.needs_tier2_tick(now));
}

#[test]
fn test_tier3_dispatch_wiring_cycle() {
    use crate::ticks::tier3_snapshot_from_npc;
    use parish_world::WorldState;

    let graph = make_chain_graph(6);
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(10, 4)); // distance 4 → Tier3

    let mut world = WorldState::new();
    world.player_location = LocationId(0);
    world.graph = graph;
    mgr.assign_tiers(&world, &[]);

    assert_eq!(mgr.tier_of(NpcId(10)), Some(CogTier::Tier3));

    let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

    assert!(mgr.needs_tier3_tick(now));
    assert!(!mgr.tier3_in_flight());
    mgr.set_tier3_in_flight(true);
    assert!(!mgr.needs_tier3_tick(now) || mgr.tier3_in_flight());

    let tier3_ids = mgr.npcs_in_tier(CogTier::Tier3);
    assert!(!tier3_ids.is_empty());
    let npc_names: std::collections::HashMap<_, _> =
        mgr.all_npcs().map(|n| (n.id, n.name.clone())).collect();
    let snapshots: Vec<_> = tier3_ids
        .iter()
        .filter_map(|id| mgr.get(*id))
        .map(|npc| tier3_snapshot_from_npc(npc, &world.graph, &npc_names))
        .collect();
    assert!(!snapshots.is_empty());

    mgr.record_tier3_tick(now);
    mgr.set_tier3_in_flight(false);

    assert_eq!(mgr.last_tier3_game_time(), Some(now));
    assert!(!mgr.tier3_in_flight());
    assert!(!mgr.needs_tier3_tick(now));
}

#[test]
fn test_tier4_dispatch_wiring_cycle() {
    use crate::tier4::tick_tier4;
    use parish_world::WorldState;
    use std::collections::HashSet;

    let graph = make_chain_graph(6);
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(99, 6)); // distance 6 → Tier4

    let mut world = WorldState::new();
    world.player_location = LocationId(0);
    world.graph = graph;
    mgr.assign_tiers(&world, &[]);

    assert_eq!(mgr.tier_of(NpcId(99)), Some(CogTier::Tier4));

    let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();
    assert!(mgr.needs_tier4_tick(now));

    let tier4_ids: HashSet<NpcId> = mgr.npcs_in_tier(CogTier::Tier4).into_iter().collect();
    let events = {
        let mut tier4_refs: Vec<&mut Npc> = mgr
            .npcs_mut()
            .values_mut()
            .filter(|n| tier4_ids.contains(&n.id))
            .collect();
        let season = world.clock.season();
        let game_date = now.date_naive();
        let mut rng = rand::rng();
        tick_tier4(&mut tier4_refs, season, game_date, &mut rng)
    };
    let game_events = mgr.apply_tier4_events(&events, now, true);
    for evt in game_events {
        world.event_bus.publish(evt);
    }
    mgr.record_tier4_tick(now);

    assert_eq!(mgr.last_tier4_game_time(), Some(now));
    assert!(!mgr.needs_tier4_tick(now));
}

// ── Reaction-emoji diversity sensor (issue #995) ────────────────────────

#[test]
fn reaction_emoji_buffer_caps_at_capacity() {
    let mut mgr = NpcManager::new();
    for _ in 0..(REACTION_EMOJI_BUFFER_CAPACITY + 4) {
        mgr.record_reaction_emoji("🤔");
    }
    assert_eq!(
        mgr.reaction_emoji_buffer().len(),
        REACTION_EMOJI_BUFFER_CAPACITY,
        "buffer must cap at REACTION_EMOJI_BUFFER_CAPACITY"
    );
}

#[test]
fn reaction_emoji_diverse_history_does_not_flag() {
    // Eight distinct emoji → distinct_count=8, dominant_ratio=1/8 = 0.125
    // → detector returns None, no WARN, no active state.
    let mut mgr = NpcManager::new();
    for e in &["🤔", "😊", "😢", "😡", "😏", "👀", "🍺", "✝️"] {
        mgr.record_reaction_emoji(e);
    }
    assert!(
        !mgr.reaction_monoculture_active,
        "diverse buffer must leave the sensor un-flagged"
    );
}

#[test]
fn reaction_emoji_monoculture_flips_active_state() {
    let mut mgr = NpcManager::new();
    for _ in 0..REACTION_EMOJI_BUFFER_CAPACITY {
        mgr.record_reaction_emoji("🤔");
    }
    assert!(
        mgr.reaction_monoculture_active,
        "sustained same-emoji push must flip the sensor to active"
    );
}

#[test]
fn reaction_emoji_monoculture_clears_when_diversity_returns() {
    let mut mgr = NpcManager::new();
    for _ in 0..REACTION_EMOJI_BUFFER_CAPACITY {
        mgr.record_reaction_emoji("🤔");
    }
    assert!(mgr.reaction_monoculture_active);

    // Flush the buffer with distinct emoji until ratio falls back below 0.7.
    for e in &["😊", "😢", "😡", "😏", "👀", "🍺", "✝️", "😳"] {
        mgr.record_reaction_emoji(e);
    }
    assert!(
        !mgr.reaction_monoculture_active,
        "recovering diversity must clear the sensor so it can fire again later"
    );
}

/// AC-2 / AC-3 (#1396): `clear_introduced_for_session` resets the in-memory
/// set so NPCs that were introduced in a previous (restored) session must be
/// re-introduced this session.
#[test]
fn test_clear_introduced_for_session_resets_set() {
    let mut mgr = NpcManager::new();
    mgr.add_npc(make_test_npc(1, 2));
    mgr.add_npc(make_test_npc(2, 2));

    // Simulate a prior session: both NPCs were introduced.
    mgr.mark_introduced(NpcId(1));
    mgr.mark_introduced(NpcId(2));
    assert!(mgr.is_introduced(NpcId(1)));
    assert!(mgr.is_introduced(NpcId(2)));

    // Simulate session reload: clear the in-memory set.
    mgr.clear_introduced_for_session();

    // Neither NPC is introduced at the start of the new session (AC-2).
    assert!(
        !mgr.is_introduced(NpcId(1)),
        "NPC 1 must not be introduced after session reset"
    );
    assert!(
        !mgr.is_introduced(NpcId(2)),
        "NPC 2 must not be introduced after session reset"
    );
    assert_eq!(mgr.introduced_count(), 0);

    // After a real meeting this session, introduced flips to true (AC-3).
    mgr.mark_introduced(NpcId(1));
    assert!(
        mgr.is_introduced(NpcId(1)),
        "NPC 1 must be introduced after actual meeting"
    );
    assert!(
        !mgr.is_introduced(NpcId(2)),
        "NPC 2 still unintroduced — only NPC 1 was met"
    );
}
