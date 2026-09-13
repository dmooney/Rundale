//! NPC arrival reaction system — how NPCs greet and react when the player enters a location.
//!
//! When the player arrives at a location, NPCs present may react —
//! greeting, nodding, welcoming, introducing themselves, or ignoring
//! the player entirely. Reactions are determined by dice rolls modified
//! by NPC personality, occupation, workplace context, mood, time of day,
//! and whether they've already been introduced.
//!
//! Each reaction includes canned fallback text. When `use_llm` is set,
//! the caller can optionally fire a short-timeout LLM call for a richer
//! greeting, falling back to the canned text on timeout or error.
//!
//! # Module layout
//!
//! | Submodule   | Contents                                                        |
//! |-------------|-----------------------------------------------------------------|
//! | `types`     | `ReactionKind`, `NpcReaction`, `ArrivalContext`                 |
//! | `templates` | Template structs, defaults, placeholder substitution            |
//! | `selection` | Threshold computation, kind selection, capping, generation loop |
//! | `register`  | Personality/mood register detection shared by prompt + fallback |
//! | `prompt`    | LLM prompt construction and `resolve_llm_greeting`              |

mod prompt;
mod register;
mod selection;
mod templates;
mod types;

#[cfg(test)]
mod tests;

// ── Public re-exports ────────────────────────────────────────────────────────

pub use prompt::{LlmGreetingParams, build_reaction_prompt, resolve_llm_greeting};
pub use selection::{generate_arrival_reactions, reaction_threshold};
pub use templates::{
    GreetingsByTime, IntroductionTemplates, OccupationGreetings, ReactionTemplates,
    WelcomesByOccupation,
};
pub use types::{ArrivalContext, NpcReaction, ReactionKind};
