//! Authored content the NPC dialogue prompt draws on (issue 2299).
//!
//! The frozen dialogue prompt (`promptfoo/prompt-lab/prompts/frozen.yaml`)
//! gives each speaker their own character, the common knowledge of their
//! time and place, and their village. Most of that comes from the mod's
//! existing game data (`npcs.json`, `world.json`, `festivals.json`). This file
//! holds the rest, which only the dialogue prompt reads: the common knowledge
//! every character shares, each village's name and how its places look and its
//! people's work is spoken of, and each character's village, backstory and how
//! they hold their relationships. A mod declares it as `[files] dialogue`; a
//! mod without it still gets a prompt, with every NPC knowing the whole world
//! as one unnamed village.

use serde::{Deserialize, Serialize};

use crate::types::FestivalDef;

/// One topic of the common knowledge every character shares
/// (`docs/design/npc-dialogue-prompt.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonKnowledgeTopic {
    /// Short heading, e.g. "Who rules".
    pub topic: String,
    /// What a person of the time and place knows of it.
    pub text: String,
}

/// How a place in a village looks, independent of the time and weather.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VillagePlace {
    /// The place's location id in `world.json`.
    pub location: u32,
    /// What it looks like.
    pub looks: String,
}

/// How the village speaks of one of its people's work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VillagePerson {
    /// The person's NPC id in `npcs.json`.
    pub npc: u32,
    /// Their work, as the village would put it.
    pub work: String,
}

/// A village: the places and people a character from it knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VillageContent {
    /// The village's name.
    pub name: String,
    /// Its places, in the order the prompt lists them.
    pub places: Vec<VillagePlace>,
    /// Its people, in the order the prompt lists them.
    pub people: Vec<VillagePerson>,
}

/// How a character holds one relationship from `npcs.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipWords {
    /// The other person's NPC id.
    pub npc: u32,
    /// What the relationship is, in the character's words
    /// (e.g. "family (his sister)").
    pub kind: String,
    /// How the character feels about them. The engine appends the
    /// relationship's current strength.
    pub feeling: String,
}

/// What the dialogue prompt adds to one character from `npcs.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterContent {
    /// The character's NPC id.
    pub npc: u32,
    /// The name of the village the character comes from.
    pub village: String,
    /// How the character holds each relationship.
    #[serde(default)]
    pub relationships: Vec<RelationshipWords>,
    /// A few lines of the character's past (design §4.2).
    #[serde(default)]
    pub backstory: Vec<String>,
}

/// The authored dialogue content of a mod (`[files] dialogue`), plus the
/// festival calendar from `festivals.json`, which the prompt's village
/// carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueContext {
    /// The common knowledge every character shares.
    #[serde(default)]
    pub common_knowledge: Vec<CommonKnowledgeTopic>,
    /// The mod's villages.
    #[serde(default)]
    pub villages: Vec<VillageContent>,
    /// What each character adds to `npcs.json`.
    #[serde(default)]
    pub characters: Vec<CharacterContent>,
    /// The festival calendar, from `festivals.json` (not authored here).
    #[serde(skip)]
    pub festivals: Vec<FestivalDef>,
}

impl DialogueContext {
    /// An empty context, usable in a `static`.
    pub const EMPTY: DialogueContext = DialogueContext {
        common_knowledge: Vec::new(),
        villages: Vec::new(),
        characters: Vec::new(),
        festivals: Vec::new(),
    };

    /// What the content adds to the character with NPC id `npc`.
    pub fn character(&self, npc: u32) -> Option<&CharacterContent> {
        self.characters.iter().find(|entry| entry.npc == npc)
    }

    /// The village called `name`.
    pub fn village(&self, name: &str) -> Option<&VillageContent> {
        self.villages.iter().find(|village| village.name == name)
    }

    /// Parses a `[files] dialogue` body.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_rejects_unknown_fields() {
        let text = r#"{
            "common_knowledge": [{"topic": "Place", "text": "Kilteevan."}],
            "villages": [{"name": "Kilteevan", "places": [{"location": 1, "looks": "A road."}],
                          "people": [{"npc": 1, "work": "keeps the office"}]}],
            "characters": [{"npc": 1, "village": "Kilteevan", "backstory": ["Born here."],
                            "relationships": [{"npc": 2, "kind": "friend", "feeling": "fond"}]}]
        }"#;
        let context = DialogueContext::parse(text).expect("parses");
        assert_eq!(context.common_knowledge[0].topic, "Place");
        assert_eq!(
            context.village("Kilteevan").unwrap().people[0].work,
            "keeps the office"
        );
        assert_eq!(context.character(1).unwrap().backstory, ["Born here."]);
        assert!(context.character(2).is_none());
        assert!(DialogueContext::parse(r#"{"secrets": []}"#).is_err());
    }
}
