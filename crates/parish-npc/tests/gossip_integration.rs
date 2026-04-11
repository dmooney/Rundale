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

    // Step 2 — Alice and Bob are co-located during a Tier 2 interaction.
    // Try enough RNG seeds to be overwhelmingly likely to propagate given the
    // 60% transmission rate — we want a deterministic assertion that the
    // cross-NPC path works, not a probabilistic one.
    let participants = [alice, bob];
    let mut propagated_on_seed = None;
    for seed in 0..50 {
        let mut network = network.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        let transmitted = propagate_gossip_at_location(&participants, &mut network, &mut rng);
        if !transmitted.is_empty() {
            // Bob's known_by set must now include the gossip item.
            let bob_gossip = network.known_by(bob);
            assert_eq!(
                bob_gossip.len(),
                1,
                "listener should know exactly the one gossip item after propagation"
            );
            assert!(
                bob_gossip[0].known_by.contains(&alice),
                "original source must still be in known_by"
            );
            assert!(
                bob_gossip[0].known_by.contains(&bob),
                "listener must now be in known_by"
            );
            propagated_on_seed = Some(seed);
            break;
        }
    }
    assert!(
        propagated_on_seed.is_some(),
        "50 attempts at a 60% transmission rate should have produced at least one propagation"
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
/// If the wiring is correct, gossip Alice seeds should be reachable by
/// Carol after Alice meets Bob and then Bob meets Carol.
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

    // Round 1: Alice and Bob co-located. Retry with successive seeds until
    // propagation sticks — this is deterministic for a given `(seed, state)`.
    let alice_bob = [alice, bob];
    for seed in 0..50 {
        let mut net = network.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        if !propagate_gossip_at_location(&alice_bob, &mut net, &mut rng).is_empty() {
            network = net;
            break;
        }
    }
    assert!(
        network.known_by(bob).iter().any(|g| g.source == alice),
        "after A/B round, Bob should know Alice's gossip"
    );
    assert!(
        network.known_by(carol).is_empty(),
        "Carol should not yet know the gossip"
    );

    // Round 2: Bob and Carol co-located. Bob is now a carrier.
    let bob_carol = [bob, carol];
    for seed in 0..50 {
        let mut net = network.clone();
        let mut rng = StdRng::seed_from_u64(seed);
        if !propagate_gossip_at_location(&bob_carol, &mut net, &mut rng).is_empty() {
            network = net;
            break;
        }
    }
    assert!(
        network.known_by(carol).iter().any(|g| g.source == alice),
        "transitive propagation: Carol should now know Alice's gossip via Bob"
    );
}
