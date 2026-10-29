//! Authored facts supplied to the dialogue prompt, and the object facts the
//! player establishes in conversation.
//!
//! Grounding comes from what the prompt says and the facts it supplies. Nothing
//! here inspects or rewrites a model reply: the facts below are read from
//! authored world data and from the player's own words.

use limerick_types::{
    DayType, LocationId, NpcId, RememberedObjectAttribute, RememberedObjectAttributeKind,
    RememberedObjectFact,
};

/// Landmarks authored for one location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationLandmarks {
    pub name: String,
    pub landmarks: Vec<String>,
}

/// Authored facts rendered into the NPC dialogue prompt for one turn.
#[derive(Debug, Clone)]
pub struct DialogueFacts {
    pub weekday: String,
    pub day_type: DayType,
    /// Locations that have at least one authored landmark, in stable order.
    pub landmarks: Vec<LocationLandmarks>,
    pub active_session: Option<limerick_world::session::ActiveSessionFact>,
    pub remembered_objects: Vec<RememberedObjectFact>,
}

/// Compact prompt block of the authored facts for this turn.
pub fn render_dialogue_facts(facts: &DialogueFacts) -> String {
    let mut block = format!(
        "\n\nCURRENT AUTHORED FACTS (do not contradict):\n- Calendar: {} is {}. Saturday is market day; Sunday is Mass/rest day.\n",
        facts.weekday, facts.day_type
    );
    for location in &facts.landmarks {
        if !location.landmarks.is_empty() {
            block.push_str(&format!(
                "- {} landmarks: {}.\n",
                location.name,
                location.landmarks.join(", ")
            ));
        }
    }
    if let Some(session) = &facts.active_session {
        block.push_str(&format!(
            "- A music session is active here now: {} {}",
            session.vignette.musician, session.vignette.tune
        ));
        if let Some(verse) = &session.vignette.verse {
            block.push_str(&format!(" Verse heard: {verse}"));
        }
        block.push_str(". Do not deny the singer, tune, or session.\n");
    }
    for object in &facts.remembered_objects {
        let attributes = object
            .attributes
            .iter()
            .map(|attribute| format!("{}={}", attribute.kind, attribute.value))
            .collect::<Vec<_>>()
            .join(", ");
        block.push_str(&format!(
            "- Player-established {} attributes: {}.\n",
            object.label, attributes
        ));
    }
    block
}

fn normalize(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text = format!(" {} ", normalize(text));
    let phrase = normalize(phrase);
    !phrase.is_empty() && text.contains(&format!(" {phrase} "))
}

const OBJECT_LABELS: &[&str] = &[
    "ribbon", "cloth", "shawl", "coat", "book", "letter", "ring", "knife", "stone", "token",
];
const OBJECT_MATERIALS: &[&str] = &[
    "wool", "silk", "linen", "leather", "iron", "wood", "wooden", "silver", "gold", "copper",
];
const OBJECT_COLOURS: &[&str] = &[
    "red", "blue", "green", "brown", "black", "white", "yellow", "grey", "gray",
];
const OBJECT_MARKINGS: &[&str] = &["blue stitch", "red stitch", "white stitch", "black stitch"];

fn unique_supported_value(text: &str, values: &[&str]) -> Option<String> {
    let found: Vec<&str> = values
        .iter()
        .copied()
        .filter(|value| contains_phrase(text, value))
        .collect();
    match found.as_slice() {
        [value] => Some((*value).to_string()),
        _ => None,
    }
}

/// Extract only explicitly player-authored, machine-comparable object facts.
/// Model output never calls this function to establish truth.
pub fn extract_remembered_object_fact(
    player_input: &str,
    speaker_id: NpcId,
    location: LocationId,
) -> Option<RememberedObjectFact> {
    // Questions mention candidate values without establishing them as truth
    // ("Was the ribbon silk?"). A compound line may still begin with a clear
    // declaration before asking a follow-up, so reject question-led shapes
    // rather than every utterance containing a question mark.
    let normalized_input = normalize(player_input);
    if [
        "was ", "is ", "are ", "did ", "does ", "do ", "what ", "which ", "who ",
    ]
    .iter()
    .any(|prefix| normalized_input.starts_with(prefix))
    {
        return None;
    }
    let labels: Vec<&str> = OBJECT_LABELS
        .iter()
        .copied()
        .filter(|label| contains_phrase(player_input, label))
        .collect();
    let [label] = labels.as_slice() else {
        return None;
    };
    let mut attributes = Vec::new();
    if let Some(material) = unique_supported_value(player_input, OBJECT_MATERIALS) {
        attributes.push(RememberedObjectAttribute {
            kind: RememberedObjectAttributeKind::Material,
            value: material,
        });
    }
    if let Some(colour) = unique_supported_value(player_input, OBJECT_COLOURS) {
        attributes.push(RememberedObjectAttribute {
            kind: RememberedObjectAttributeKind::Colour,
            value: colour,
        });
    }
    let lower = normalize(player_input);
    for marking in OBJECT_MARKINGS {
        if contains_phrase(&lower, marking) {
            attributes.push(RememberedObjectAttribute {
                kind: RememberedObjectAttributeKind::Marking,
                value: (*marking).to_string(),
            });
        }
    }
    if attributes.is_empty() {
        return None;
    }
    Some(RememberedObjectFact {
        speaker_id,
        location,
        label: (*label).to_string(),
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_block_lists_calendar_landmarks_and_player_objects() {
        let facts = DialogueFacts {
            weekday: "Saturday".to_string(),
            day_type: DayType::MarketDay,
            landmarks: vec![
                LocationLandmarks {
                    name: "The Crossroads".to_string(),
                    landmarks: vec!["the old holy well".to_string(), "a bent ash".to_string()],
                },
                LocationLandmarks {
                    name: "The Forge".to_string(),
                    landmarks: Vec::new(),
                },
            ],
            active_session: None,
            remembered_objects: vec![RememberedObjectFact {
                speaker_id: NpcId(1),
                location: LocationId(2),
                label: "ribbon".to_string(),
                attributes: vec![RememberedObjectAttribute {
                    kind: RememberedObjectAttributeKind::Colour,
                    value: "red".to_string(),
                }],
            }],
        };

        let block = render_dialogue_facts(&facts);

        assert!(block.starts_with("\n\nCURRENT AUTHORED FACTS"));
        assert!(block.contains("Saturday is"));
        assert!(block.contains("- The Crossroads landmarks: the old holy well, a bent ash."));
        assert!(!block.contains("The Forge"), "no landmarks, no line");
        assert!(block.contains("Player-established ribbon attributes:"));
    }

    #[test]
    fn player_declared_object_attributes_are_extracted() {
        let fact = extract_remembered_object_fact(
            "I lost a red wool ribbon on the road.",
            NpcId(3),
            LocationId(4),
        )
        .expect("a declared object fact");
        assert_eq!(fact.label, "ribbon");
        assert!(fact.attributes.iter().any(|a| a.value == "wool"));
        assert!(fact.attributes.iter().any(|a| a.value == "red"));
    }

    #[test]
    fn questions_about_objects_do_not_establish_facts() {
        assert!(
            extract_remembered_object_fact("Was the ribbon silk?", NpcId(3), LocationId(4))
                .is_none()
        );
    }
}
