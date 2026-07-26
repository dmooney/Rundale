//! Gossip propagation helpers tied to Tier 2 events.
//!
//! Creates gossip entries from notable Tier 2 narrative beats and propagates
//! them between co-located NPCs.

use chrono::{DateTime, Utc};

use crate::NpcId;
use crate::types::Tier2Event;
use parish_types::GossipNetwork;

/// Creates gossip from a Tier 2 event if it is notable.
///
/// Notable events are those with significant relationship changes (|delta| > 0.3)
/// or summaries longer than a trivial threshold. The first participant is treated
/// as the gossip source.
pub(super) fn create_gossip_from_tier2_event(
    event: &Tier2Event,
    gossip_network: &mut GossipNetwork,
    game_time: DateTime<Utc>,
) -> Option<parish_types::events::GameEvent> {
    // Tier-2 events without participants are degenerate (no group, no
    // speaker). Defaulting the source to NpcId(0) — the player — would
    // mint gossip falsely attributed to the player. Bail out instead.
    let &source = event.participants.first()?;

    // Create gossip from large relationship changes
    for rc in &event.relationship_changes {
        if rc.delta.abs() > 0.3 {
            gossip_network.create(event.summary.clone(), source, game_time);
            return Some(parish_types::events::GameEvent::GossipSpread {
                source,
                location: event.location,
                content: event.summary.clone(),
                timestamp: game_time,
            });
        }
    }

    // Create gossip from non-trivial dialogue summaries (>30 chars suggests substance)
    if event.summary.len() > 30 {
        gossip_network.create(event.summary.clone(), source, game_time);
        return Some(parish_types::events::GameEvent::GossipSpread {
            source,
            location: event.location,
            content: event.summary.clone(),
            timestamp: game_time,
        });
    }

    None
}

/// Propagates gossip between NPCs during a Tier 2 group interaction.
///
/// For each pair of NPCs at the same location, attempts to propagate
/// gossip from one to the other. Returns the total count of rumors
/// transmitted across all pairs in this group.
pub fn propagate_gossip_at_location(
    participant_ids: &[NpcId],
    gossip_network: &mut GossipNetwork,
    rng: &mut impl rand::Rng,
) -> usize {
    let mut total_transmitted = 0usize;
    for i in 0..participant_ids.len() {
        for j in (i + 1)..participant_ids.len() {
            let transmitted = gossip_network.propagate(participant_ids[i], participant_ids[j], rng);
            total_transmitted += transmitted.len();
            // Also propagate in reverse direction
            let transmitted = gossip_network.propagate(participant_ids[j], participant_ids[i], rng);
            total_transmitted += transmitted.len();
        }
    }
    total_transmitted
}
