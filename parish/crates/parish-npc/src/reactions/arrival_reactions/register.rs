//! Mood/personality register helpers for arrival reactions.

use crate::Npc;

const CALCULATING_REGISTER_CUES: &[&str] = &[
    "calculating",
    "shrewd",
    "cunning with",
    "keen eye for opportunity",
    "turn a profit",
    "hard in his dealings",
    "weights and measures",
];

fn text_has_calculating_register(text: &str) -> bool {
    let lower = text.to_lowercase();
    CALCULATING_REGISTER_CUES
        .iter()
        .any(|cue| lower.contains(cue))
}

pub(crate) fn has_calculating_register(npc: &Npc) -> bool {
    text_has_calculating_register(&npc.mood) || text_has_calculating_register(&npc.personality)
}
