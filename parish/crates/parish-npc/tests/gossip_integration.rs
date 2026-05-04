//! Cross-subsystem integration test for gossip propagation.
//!
//! Closes the "gossip propagation across NPCs" regression gap identified
//! in the engine audit. The individual pieces (`GossipNetwork::create`,
//! `GossipNetwork::propagate`, `create_gossip_from_tier2_event`,
//! `propagate_gossip_at_location`) all have unit tests, but nothing
//! asserted that a Tier 2 event from NPC A actually surfaces in NPC B's
//! known-gossip set via the wiring these functions are supposed to form.
//!
//! This test runs the wiring end to end and asserts that a notable Tier 2
//! event originating at NPC A materialises in NPC B's `known_by` set after
//! a co-located propagation pass.

use parish_npc::ticks::{create_gossip_from_tier2_event, propagate_gossip_at_location};
use parish_npc::types::{RelationshipChange, Tier2Event};
use parish_types::{GossipNetwork, LocationId, NpcId};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn game_time() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
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
    };
    create_gossip_from_tier2_event(&event, &mut network, game_time());

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
    };
    create_gossip_from_tier2_event(&event, &mut network, game_time());
    assert_eq!(
        network.len(),
        0,
        "trivial events must not seed gossip items"
    );
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
    };
    create_gossip_from_tier2_event(&event, &mut network, game_time());
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
