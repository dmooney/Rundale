//! Cross-subsystem integration test for gossip propagation.
//!
//! Closes the "gossip propagation across NPCs" regression gap identified
//! in the engine audit. The individual pieces (`GossipNetwork::create`,
//! `GossipNetwork::propagate`, the grounded Tier-2 apply seam,
//! `propagate_gossip_at_location`) all have unit tests, but nothing
//! asserted that a Tier 2 event from NPC A actually surfaces in NPC B's
//! known-gossip set via the wiring these functions are supposed to form.
//!
//! This test runs the wiring end to end and asserts that a notable Tier 2
//! event originating at NPC A materialises in NPC B's `known_by` set after
//! a co-located propagation pass.

use std::collections::HashMap;

use parish_config::NpcConfig;
use parish_npc::Npc;
use parish_npc::ticks::{
    GroundedTier2ApplyOutcome, apply_grounded_tier2_event_with_config,
    propagate_gossip_at_location, tier2_activity_fingerprint_from_npc_at,
};
use parish_npc::types::{RelationshipChange, Tier2Event, Tier2ParticipantGrounding};
use parish_types::events::{EventBus, GameEvent};
use parish_types::{GossipNetwork, LocationId, NpcId};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn game_time() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
}

fn apply_grounded_event(
    event: &Tier2Event,
    network: &mut GossipNetwork,
    event_bus: &EventBus,
) -> GroundedTier2ApplyOutcome {
    let game_time = game_time();
    let mut npcs: HashMap<NpcId, Npc> = event
        .participants
        .iter()
        .copied()
        .map(|id| {
            let mut npc = Npc::new_test_npc();
            npc.id = id;
            npc.name = format!("NPC {}", id.0);
            npc.set_location(event.location);
            (id, npc)
        })
        .collect();
    let mut grounded = event.clone();
    grounded.grounding = grounded
        .participants
        .iter()
        .map(|id| {
            let npc = &npcs[id];
            Tier2ParticipantGrounding {
                npc_id: *id,
                location: grounded.location,
                grounding_revision: npc.grounding_revision(),
                activity_fingerprint: tier2_activity_fingerprint_from_npc_at(npc, game_time),
            }
        })
        .collect();

    apply_grounded_tier2_event_with_config(
        &grounded,
        &mut npcs,
        game_time,
        &NpcConfig::default(),
        event_bus,
        network,
    )
}

/// A notable Tier 2 event (big relationship change) originating at NPC A
/// must:
///   1. Seed the gossip network with A as the source.
///   2. Propagate to NPC B when they are co-located during propagation.
///
/// This test verifies the structural invariants (source recorded, listener
/// added to known_by) on a known-transmitting seed, and separately asserts
/// that the overall transmission rate across 200 seeds is within the expected
/// ~60% range — catching both wiring breaks and probability regressions.
#[test]
fn tier2_event_seeds_gossip_and_propagates_to_colocated_npc() {
    let mut network = GossipNetwork::new();
    let alice = NpcId(1);
    let bob = NpcId(2);

    // Step 1 — a notable Tier 2 event occurs, with Alice as the first participant.
    let event = Tier2Event {
        location: LocationId(2),
        summary: "Alice confronted the landlord about the rent".to_string(),
        participants: vec![alice, bob],
        mood_changes: Vec::new(),
        relationship_changes: vec![RelationshipChange {
            from: alice,
            to: NpcId(99),
            delta: 0.5, // > 0.3 → notable
        }],
        grounding: Vec::new(),
    };
    let outcome = apply_grounded_event(&event, &mut network, &EventBus::new());
    assert!(matches!(outcome, GroundedTier2ApplyOutcome::Applied(_)));

    assert_eq!(
        network.len(),
        1,
        "notable event should seed one gossip item"
    );
    let alice_gossip = network.known_by(alice);
    assert_eq!(alice_gossip.len(), 1, "source NPC should know the gossip");
    assert_eq!(
        alice_gossip[0].source, alice,
        "first participant must be recorded as source"
    );
    assert!(
        network.known_by(bob).is_empty(),
        "listener must not know the gossip until propagation runs"
    );

    // Step 2 — verify transmission rate over 200 deterministic seeds.
    //
    // propagate_gossip_at_location with {alice, bob} and one alice-owned item
    // calls gossip_network.propagate(alice, bob, rng) which makes one RNG draw
    // per item (rng.random::<f64>() < TRANSMISSION_CHANCE = 0.60).
    // Across 200 independent seeds the rate should fall between 50% and 70%.
    // If the transmission probability is silently dropped to 0%, this catches it.
    let participants = [alice, bob];
    let mut transmitted_count = 0usize;
    let trials = 200u64;

    // Also check structural invariants on the first seed that does transmit.
    let mut invariants_verified = false;

    for seed in 0..trials {
        let mut net = network.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        let transmitted = propagate_gossip_at_location(&participants, &mut net, &mut rng);
        if transmitted > 0 {
            transmitted_count += 1;
            if !invariants_verified {
                // Bob's known_by set must now include the gossip item.
                let bob_gossip = net.known_by(bob);
                assert_eq!(
                    bob_gossip.len(),
                    1,
                    "listener should know exactly the one gossip item after propagation (seed={seed})"
                );
                assert!(
                    bob_gossip[0].known_by.contains(&alice),
                    "original source must still be in known_by (seed={seed})"
                );
                assert!(
                    bob_gossip[0].known_by.contains(&bob),
                    "listener must now be in known_by (seed={seed})"
                );
                invariants_verified = true;
            }
        }
    }

    assert!(
        invariants_verified,
        "structural invariants must be verified on at least one transmission across {trials} seeds"
    );

    let rate = transmitted_count as f64 / trials as f64;
    assert!(
        (0.50..=0.70).contains(&rate),
        "transmission rate over {trials} seeds should be ~60%, got {:.1}% ({transmitted_count}/{trials})",
        rate * 100.0
    );
}

/// Trivial events (no significant relationship change and short summaries)
/// must NOT seed gossip. This guards the "what counts as notable" threshold.
#[test]
fn trivial_tier2_event_does_not_seed_gossip() {
    let mut network = GossipNetwork::new();
    let event = Tier2Event {
        location: LocationId(2),
        summary: "brief nod".to_string(), // < 30 chars, no relationship changes
        participants: vec![NpcId(1), NpcId(2)],
        mood_changes: Vec::new(),
        relationship_changes: vec![RelationshipChange {
            from: NpcId(1),
            to: NpcId(2),
            delta: 0.05, // below the 0.3 notability threshold
        }],
        grounding: Vec::new(),
    };
    let outcome = apply_grounded_event(&event, &mut network, &EventBus::new());
    assert_eq!(
        network.len(),
        0,
        "trivial events must not seed gossip items"
    );
    assert!(matches!(outcome, GroundedTier2ApplyOutcome::Applied(_)));
}

/// Empty-participants guard (TD-031): a Tier 2 event with no participants is
/// degenerate — there is no speaker to attribute gossip to. The function must
/// bail (`participants.first()?`) rather than default the source to `NpcId(0)`,
/// which is the player and would mint gossip falsely attributed to them.
///
/// This pins the load-bearing `let &source = event.participants.first()?;`
/// guard: a regression back to `unwrap_or(&NpcId(0))` would seed gossip here
/// and fail this test. The summary is deliberately notable (> 30 chars + a
/// large relationship delta) so only the empty-participants bail can explain a
/// `None` return and an empty network.
#[test]
fn empty_participants_tier2_event_does_not_seed_gossip() {
    let mut network = GossipNetwork::new();
    let event = Tier2Event {
        location: LocationId(3),
        // Notable on both axes — would seed gossip if a source existed.
        summary: "A long, substantive summary that easily clears the threshold".to_string(),
        participants: vec![], // degenerate: no speaker
        mood_changes: Vec::new(),
        relationship_changes: vec![RelationshipChange {
            from: NpcId(1),
            to: NpcId(2),
            delta: 0.9,
        }],
        grounding: Vec::new(),
    };
    let outcome = apply_grounded_event(&event, &mut network, &EventBus::new());
    assert!(matches!(
        outcome,
        GroundedTier2ApplyOutcome::Rejected(reason) if reason == "event has no participants"
    ));
    assert_eq!(
        network.len(),
        0,
        "events without participants must not seed gossip (no NpcId(0)/player misattribution)"
    );
}

/// A notable Tier 2 event must return a `GameEvent::GossipSpread` so the
/// caller can publish it on the world event bus. The payload must reflect
/// the originating NPC, the conversation location, and the summary that
/// became gossip.
#[test]
fn notable_tier2_event_returns_gossip_spread_event() {
    let mut network = GossipNetwork::new();
    let event_bus = EventBus::new();
    let mut events = event_bus.subscribe();
    let alice = NpcId(1);
    let event = Tier2Event {
        location: LocationId(7),
        summary: "Alice confronted the landlord about the rent".to_string(),
        participants: vec![alice, NpcId(2)],
        mood_changes: Vec::new(),
        relationship_changes: vec![RelationshipChange {
            from: alice,
            to: NpcId(99),
            delta: 0.5,
        }],
        grounding: Vec::new(),
    };
    let outcome = apply_grounded_event(&event, &mut network, &event_bus);
    assert!(matches!(outcome, GroundedTier2ApplyOutcome::Applied(_)));
    let mut gossip_spread = None;
    while let Ok(published) = events.try_recv() {
        if let GameEvent::GossipSpread {
            source,
            location,
            content,
            ..
        } = published
        {
            gossip_spread = Some((source, location, content));
        }
    }
    assert_eq!(
        gossip_spread,
        Some((
            alice,
            LocationId(7),
            "Alice confronted the landlord about the rent".to_string()
        ))
    );
}

/// Empty-participants guard: a Tier 2 event with no participants must not mint gossip.
///
/// The guard `let &source = event.participants.first()?;` in
/// `create_gossip_from_tier2_event` exists to prevent gossip being falsely
/// attributed to NpcId(0) (the player) when the participants list is empty.
/// This test locks that invariant.
#[test]
fn gossip_empty_participants_does_not_mint_gossip() {
    let mut network = GossipNetwork::new();

    // A notable event by every metric (large relationship change, long summary),
    // but with an empty participants list.
    let event = Tier2Event {
        location: LocationId(1),
        summary: "A dramatic confrontation unfolded at the crossroads last evening".to_string(),
        participants: vec![],
        mood_changes: Vec::new(),
        relationship_changes: vec![RelationshipChange {
            from: NpcId(1),
            to: NpcId(2),
            delta: 0.8, // well above 0.3 notability threshold
        }],
        grounding: Vec::new(),
    };

    let outcome = apply_grounded_event(&event, &mut network, &EventBus::new());

    assert_eq!(
        network.len(),
        0,
        "no gossip must be minted when participants is empty"
    );
    assert!(matches!(
        outcome,
        GroundedTier2ApplyOutcome::Rejected(reason) if reason == "event has no participants"
    ));
}

/// Transitive propagation: A → B → C across two separate Tier 2 rounds.
///
/// Rate assertions on each round catch probability regressions; structural
/// assertions on the first successful round verify the wiring is correct.
///
/// Round 1: alice seeds gossip, alice+bob are co-located.
/// Round 2: bob (now a carrier) meets carol.
/// Final check: carol knows alice's gossip (source preserved through carrier).
#[test]
fn gossip_propagates_transitively_across_two_rounds() {
    let mut network = GossipNetwork::new();
    let alice = NpcId(1);
    let bob = NpcId(2);
    let carol = NpcId(3);

    let event = Tier2Event {
        location: LocationId(2),
        summary: "Alice saw a ghost up at the fairy fort last night".to_string(),
        participants: vec![alice],
        mood_changes: Vec::new(),
        relationship_changes: Vec::new(),
        grounding: Vec::new(),
    };
    let outcome = apply_grounded_event(&event, &mut network, &EventBus::new());
    assert!(matches!(outcome, GroundedTier2ApplyOutcome::Applied(_)));
    assert_eq!(network.len(), 1);

    // Round 1: Alice and Bob co-located.
    // Assert transmission rate across 200 seeds, and record the first
    // successfully-propagated state for use in Round 2.
    let alice_bob = [alice, bob];
    let mut round1_count = 0usize;
    let mut network_after_round1: Option<GossipNetwork> = None;

    for seed in 0u64..200 {
        let mut net = network.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        if propagate_gossip_at_location(&alice_bob, &mut net, &mut rng) > 0 {
            round1_count += 1;
            if network_after_round1.is_none() {
                // Save the first state where Bob received the gossip.
                network_after_round1 = Some(net);
            }
        }
    }

    let r1_rate = round1_count as f64 / 200.0;
    assert!(
        (0.50..=0.70).contains(&r1_rate),
        "round-1 transmission rate should be ~60%, got {:.1}%",
        r1_rate * 100.0
    );

    let network_after_round1 =
        network_after_round1.expect("at least one round-1 transmission must succeed");

    assert!(
        network_after_round1
            .known_by(bob)
            .iter()
            .any(|g| g.source == alice),
        "after A/B round, Bob should know Alice's gossip"
    );
    assert!(
        network_after_round1.known_by(carol).is_empty(),
        "Carol should not yet know the gossip"
    );

    // Round 2: Bob and Carol co-located. Bob is now a carrier.
    let bob_carol = [bob, carol];
    let mut round2_count = 0usize;
    let mut network_after_round2: Option<GossipNetwork> = None;

    for seed in 0u64..200 {
        let mut net = network_after_round1.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        if propagate_gossip_at_location(&bob_carol, &mut net, &mut rng) > 0 {
            round2_count += 1;
            if network_after_round2.is_none() {
                network_after_round2 = Some(net);
            }
        }
    }

    let r2_rate = round2_count as f64 / 200.0;
    assert!(
        (0.50..=0.70).contains(&r2_rate),
        "round-2 transmission rate should be ~60%, got {:.1}%",
        r2_rate * 100.0
    );

    let network_after_round2 =
        network_after_round2.expect("at least one round-2 transmission must succeed");

    assert!(
        network_after_round2
            .known_by(carol)
            .iter()
            .any(|g| g.source == alice),
        "transitive propagation: Carol should now know Alice's gossip via Bob"
    );
}
