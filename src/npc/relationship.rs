//! NPC relationship system.
//!
//! Tracks relationships between NPCs including kind, strength,
//! and historical events that shaped the relationship.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::NpcId;

/// The kind of relationship between two NPCs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    /// Blood or marital family.
    Family,
    /// Close friend.
    Friend,
    /// Geographic neighbor.
    Neighbor,
    /// Competitive rival.
    Rival,
    /// Hostile enemy.
    Enemy,
    /// Romantic partner.
    Romantic,
    /// Work-related connection.
    Professional,
}

/// A significant event in the history of a relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipEvent {
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// What happened.
    pub description: String,
    /// How much the relationship strength changed (-1.0 to 1.0).
    pub delta: f32,
}

/// A directed relationship from one NPC to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// The NPC this relationship is with.
    pub target: NpcId,
    /// The kind of relationship.
    pub kind: RelationshipKind,
    /// Strength of the relationship (-1.0 hostile to 1.0 warm).
    pub strength: f32,
    /// Historical events that shaped this relationship.
    #[serde(default)]
    pub history: Vec<RelationshipEvent>,
}

impl Relationship {
    /// Creates a new relationship with the given kind and strength.
    pub fn new(target: NpcId, kind: RelationshipKind, strength: f32) -> Self {
        Self {
            target,
            kind,
            strength: strength.clamp(-1.0, 1.0),
            history: Vec::new(),
        }
    }

    /// Adjusts the relationship strength by the given delta, clamping to [-1.0, 1.0].
    ///
    /// Also records the event in the relationship history.
    pub fn adjust(&mut self, delta: f32, description: String, timestamp: DateTime<Utc>) {
        self.strength = (self.strength + delta).clamp(-1.0, 1.0);
        self.history.push(RelationshipEvent {
            timestamp,
            description,
            delta,
        });
    }

    /// Returns a prose description of the relationship for use in prompts.
    pub fn context_string(&self, target_name: &str) -> String {
        let warmth = match self.strength {
            s if s >= 0.7 => "very close",
            s if s >= 0.3 => "friendly",
            s if s >= -0.3 => "neutral",
            s if s >= -0.7 => "strained",
            _ => "hostile",
        };
        format!("{} ({:?}, {} relationship)", target_name, self.kind, warmth)
    }
}

impl std::fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationshipKind::Family => write!(f, "Family"),
            RelationshipKind::Friend => write!(f, "Friend"),
            RelationshipKind::Neighbor => write!(f, "Neighbor"),
            RelationshipKind::Rival => write!(f, "Rival"),
            RelationshipKind::Enemy => write!(f, "Enemy"),
            RelationshipKind::Romantic => write!(f, "Romantic"),
            RelationshipKind::Professional => write!(f, "Professional"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_relationship_new() {
        let rel = Relationship::new(NpcId(2), RelationshipKind::Friend, 0.5);
        assert_eq!(rel.target, NpcId(2));
        assert_eq!(rel.kind, RelationshipKind::Friend);
        assert!((rel.strength - 0.5).abs() < f32::EPSILON);
        assert!(rel.history.is_empty());
    }

    #[test]
    fn test_relationship_clamp() {
        let rel = Relationship::new(NpcId(1), RelationshipKind::Enemy, -2.0);
        assert!((rel.strength - (-1.0)).abs() < f32::EPSILON);

        let rel = Relationship::new(NpcId(1), RelationshipKind::Family, 5.0);
        assert!((rel.strength - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_relationship_adjust() {
        let mut rel = Relationship::new(NpcId(2), RelationshipKind::Neighbor, 0.0);
        rel.adjust(0.3, "Helped with the harvest".to_string(), Utc::now());
        assert!((rel.strength - 0.3).abs() < f32::EPSILON);
        assert_eq!(rel.history.len(), 1);
        assert!(rel.history[0].description.contains("harvest"));
    }

    #[test]
    fn test_relationship_adjust_clamp() {
        let mut rel = Relationship::new(NpcId(2), RelationshipKind::Friend, 0.9);
        rel.adjust(0.5, "Great favor".to_string(), Utc::now());
        assert!((rel.strength - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_relationship_context_string() {
        let rel = Relationship::new(NpcId(2), RelationshipKind::Friend, 0.8);
        let ctx = rel.context_string("Padraig");
        assert!(ctx.contains("Padraig"));
        assert!(ctx.contains("very close"));
        assert!(ctx.contains("Friend"));
    }

    #[test]
    fn test_relationship_context_warmth_levels() {
        let cases = vec![
            (0.8, "very close"),
            (0.5, "friendly"),
            (0.0, "neutral"),
            (-0.5, "strained"),
            (-0.9, "hostile"),
        ];
        for (strength, expected) in cases {
            let rel = Relationship::new(NpcId(1), RelationshipKind::Neighbor, strength);
            let ctx = rel.context_string("Test");
            assert!(
                ctx.contains(expected),
                "strength {} should produce '{}', got '{}'",
                strength,
                expected,
                ctx
            );
        }
    }

    #[test]
    fn test_relationship_kind_display() {
        assert_eq!(RelationshipKind::Family.to_string(), "Family");
        assert_eq!(RelationshipKind::Professional.to_string(), "Professional");
    }

    #[test]
    fn test_relationship_serialize_deserialize() {
        let rel = Relationship::new(NpcId(2), RelationshipKind::Friend, 0.5);
        let json = serde_json::to_string(&rel).unwrap();
        let deser: Relationship = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.target, NpcId(2));
        assert_eq!(deser.kind, RelationshipKind::Friend);
        assert!((deser.strength - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_relationship_event_serialize() {
        let event = RelationshipEvent {
            timestamp: Utc::now(),
            description: "Shared a pint".to_string(),
            delta: 0.1,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: RelationshipEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.description, "Shared a pint");
    }
}
