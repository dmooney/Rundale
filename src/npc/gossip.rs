//! Gossip propagation network.
//!
//! NPCs spread gossip with probabilistic transfer and simple
//! string mutation for distortion over time.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rand::Rng;

use crate::npc::NpcId;

/// A piece of gossip circulating in the parish.
#[derive(Debug, Clone)]
pub struct GossipItem {
    /// Unique identifier for this gossip item.
    pub id: u32,
    /// The current (possibly distorted) content of the gossip.
    pub content: String,
    /// The original undistorted content.
    pub original_content: String,
    /// The NPC who originated this gossip.
    pub source: NpcId,
    /// Set of NPC ids who currently know this gossip.
    pub known_by: HashSet<NpcId>,
    /// How many times this gossip has been distorted (0 = original).
    pub distortion_level: u8,
    /// When this gossip was created.
    pub timestamp: DateTime<Utc>,
}

/// Gossip network tracking all circulating gossip.
pub struct GossipNetwork {
    items: Vec<GossipItem>,
    next_id: u32,
}

/// Distorts gossip text with simple string mutations.
///
/// Applies word swaps and hedging insertions to simulate
/// how gossip changes as it passes between people.
pub fn distort(text: &str) -> String {
    let mut result = text.to_string();

    // Word swaps
    let swaps = [
        ("saw", "heard"),
        ("told", "mentioned"),
        ("went to", "was seen near"),
        ("said", "whispered"),
        ("angry", "upset"),
    ];

    for (from, to) in &swaps {
        if result.contains(from) {
            result = result.replacen(from, to, 1);
            break; // Only apply one swap per distortion
        }
    }

    // Add hedging if none of the swaps applied
    if result == text {
        let hedges = ["Apparently, ", "Supposedly, ", "I heard that "];
        // Use a simple hash of the text to pick a hedge deterministically
        // in non-random contexts (tests), but in practice the caller
        // already gates on rand.
        let idx = text.len() % hedges.len();
        result = format!("{}{}", hedges[idx], result);
    }

    result
}

impl GossipNetwork {
    /// Creates an empty gossip network.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 0,
        }
    }

    /// Adds new gossip to the network.
    ///
    /// The source NPC is automatically added to the `known_by` set.
    /// Returns the unique id assigned to this gossip item.
    pub fn add_gossip(
        &mut self,
        content: impl Into<String>,
        source: NpcId,
        timestamp: DateTime<Utc>,
    ) -> u32 {
        let content = content.into();
        let id = self.next_id;
        self.next_id += 1;

        let mut known_by = HashSet::new();
        known_by.insert(source);

        self.items.push(GossipItem {
            id,
            original_content: content.clone(),
            content,
            source,
            known_by,
            distortion_level: 0,
            timestamp,
        });

        id
    }

    /// Propagates gossip from one NPC to another.
    ///
    /// For each gossip item that `from` knows, there is a 60% chance
    /// it transfers to `to`. When transferred, there is a 20% chance
    /// the gossip content is distorted.
    pub fn propagate(&mut self, from: NpcId, to: NpcId) {
        let mut rng = rand::thread_rng();

        for item in &mut self.items {
            if item.known_by.contains(&from) && !item.known_by.contains(&to) {
                // 60% transfer chance
                if rng.r#gen::<f64>() < 0.6 {
                    item.known_by.insert(to);

                    // 20% distortion chance
                    if rng.r#gen::<f64>() < 0.2 {
                        item.content = distort(&item.content);
                        item.distortion_level = item.distortion_level.saturating_add(1);
                    }
                }
            }
        }
    }

    /// Returns all gossip items for debug inspection.
    pub fn all_items(&self) -> &[GossipItem] {
        &self.items
    }

    /// Returns all gossip items known by a specific NPC.
    pub fn gossip_for_npc(&self, npc_id: NpcId) -> Vec<&GossipItem> {
        self.items
            .iter()
            .filter(|item| item.known_by.contains(&npc_id))
            .collect()
    }

    /// Formats known gossip for an NPC as a context string for LLM prompts.
    ///
    /// Returns an empty string if the NPC knows no gossip.
    pub fn inject_gossip_context(&self, npc_id: NpcId) -> String {
        let known = self.gossip_for_npc(npc_id);
        if known.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(known.len() + 1);
        lines.push("Gossip you've heard:".to_string());
        for item in &known {
            let time = item.timestamp.format("%H:%M");
            lines.push(format!("- [{}] {}", time, item.content));
        }
        lines.join("\n")
    }
}

impl Default for GossipNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap()
    }

    #[test]
    fn test_gossip_network_new() {
        let net = GossipNetwork::new();
        assert!(net.items.is_empty());
    }

    #[test]
    fn test_add_gossip() {
        let mut net = GossipNetwork::new();
        let id = net.add_gossip("The landlord is coming", NpcId(1), ts(10));
        assert_eq!(id, 0);
        assert_eq!(net.items.len(), 1);
        assert!(net.items[0].known_by.contains(&NpcId(1)));
        assert_eq!(net.items[0].content, "The landlord is coming");
        assert_eq!(net.items[0].original_content, "The landlord is coming");
        assert_eq!(net.items[0].distortion_level, 0);
    }

    #[test]
    fn test_add_multiple_gossip() {
        let mut net = GossipNetwork::new();
        let id1 = net.add_gossip("First gossip", NpcId(1), ts(10));
        let id2 = net.add_gossip("Second gossip", NpcId(2), ts(11));
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(net.items.len(), 2);
    }

    #[test]
    fn test_gossip_for_npc() {
        let mut net = GossipNetwork::new();
        net.add_gossip("Gossip A", NpcId(1), ts(10));
        net.add_gossip("Gossip B", NpcId(2), ts(11));

        let npc1_gossip = net.gossip_for_npc(NpcId(1));
        assert_eq!(npc1_gossip.len(), 1);
        assert_eq!(npc1_gossip[0].content, "Gossip A");

        let npc3_gossip = net.gossip_for_npc(NpcId(3));
        assert!(npc3_gossip.is_empty());
    }

    #[test]
    fn test_distort_saw() {
        let result = distort("I saw the landlord yesterday");
        assert!(result.contains("heard"));
        assert!(!result.contains("saw"));
    }

    #[test]
    fn test_distort_told() {
        let result = distort("She told me about it");
        assert!(result.contains("mentioned"));
    }

    #[test]
    fn test_distort_no_match_adds_hedge() {
        let result = distort("The weather is nice");
        assert!(
            result.starts_with("Apparently, ")
                || result.starts_with("Supposedly, ")
                || result.starts_with("I heard that ")
        );
    }

    #[test]
    fn test_inject_gossip_context_empty() {
        let net = GossipNetwork::new();
        let ctx = net.inject_gossip_context(NpcId(1));
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_inject_gossip_context_with_items() {
        let mut net = GossipNetwork::new();
        net.add_gossip("The priest is leaving", NpcId(1), ts(14));

        let ctx = net.inject_gossip_context(NpcId(1));
        assert!(ctx.contains("Gossip you've heard:"));
        assert!(ctx.contains("[14:00]"));
        assert!(ctx.contains("The priest is leaving"));
    }

    #[test]
    fn test_propagate_probabilistic() {
        // Run propagation many times to test probabilistic behavior
        let mut transfer_count = 0;
        let trials = 1000;

        for _ in 0..trials {
            let mut net = GossipNetwork::new();
            net.add_gossip("Test gossip", NpcId(1), ts(10));
            net.propagate(NpcId(1), NpcId(2));

            if net.items[0].known_by.contains(&NpcId(2)) {
                transfer_count += 1;
            }
        }

        // Should be roughly 60% — allow wide margin for randomness
        let rate = transfer_count as f64 / trials as f64;
        assert!(
            rate > 0.4 && rate < 0.8,
            "Transfer rate {:.2} should be near 0.6",
            rate
        );
    }

    #[test]
    fn test_propagate_does_not_affect_unknown() {
        let mut net = GossipNetwork::new();
        net.add_gossip("Gossip A", NpcId(1), ts(10));
        net.add_gossip("Gossip B", NpcId(2), ts(11));

        // NPC 3 doesn't know any gossip; propagate from 3 to 4
        net.propagate(NpcId(3), NpcId(4));

        assert!(!net.items[0].known_by.contains(&NpcId(4)));
        assert!(!net.items[1].known_by.contains(&NpcId(4)));
    }

    #[test]
    fn test_default_gossip_network() {
        let net = GossipNetwork::default();
        assert!(net.items.is_empty());
    }
}
