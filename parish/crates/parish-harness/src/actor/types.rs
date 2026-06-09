//! Shared actor types: the player's observation + move, the run transcript fed
//! to the judge, and the judge's verdict.

use parish_core::ipc::EngineState;
use serde::Serialize;

use crate::score::{AxisScore, Finding};

/// What the player sees before choosing an action.
pub struct Observation<'a> {
    pub turn_index: u32,
    /// The authoritative engine state this turn.
    pub state: &'a EngineState,
    /// The narrative produced by the previous command (what the player "reads").
    pub narrative: &'a str,
    /// In-character persona.
    pub persona: &'a str,
    /// Play strategy.
    pub strategy: &'a str,
}

/// The player's chosen input.
#[derive(Debug, Clone)]
pub struct PlayerMove {
    pub text: String,
    pub addressed_to: Vec<String>,
}

impl PlayerMove {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            addressed_to: vec![],
        }
    }
}

/// One turn as the judge sees it: input, narrative, outcome, and the engine
/// ground-truth summary (which the player could not see).
#[derive(Debug, Clone, Serialize)]
pub struct TurnTranscript {
    pub turn_index: u32,
    pub player_input: String,
    pub narrative: String,
    pub outcome: String,
    pub kind: String,
    pub state_summary: String,
}

/// The whole session, assembled at end-of-run for the judge.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunTranscript {
    pub persona: String,
    pub turns: Vec<TurnTranscript>,
}

/// The judge's verdict: axis scores + discrete findings.
#[derive(Debug, Clone, Default)]
pub struct JudgeVerdict {
    pub axes: Vec<AxisScore>,
    pub findings: Vec<Finding>,
}

impl JudgeVerdict {
    /// Whether any finding is critical (a gate condition).
    pub fn has_critical(&self) -> bool {
        self.findings
            .iter()
            .any(|f| matches!(f.severity, crate::score::Severity::Critical))
    }
}

/// Build a compact, deterministic one-line ground-truth summary of engine state
/// for the transcript (location, clock, NPCs present + moods, grapevine).
pub fn summarize_state(state: &EngineState) -> String {
    let npcs: Vec<String> = state
        .npcs
        .here
        .iter()
        .map(|n| format!("{}({})", n.display_name, n.mood))
        .collect();
    format!(
        "loc={}#{} {:02}:{:02} {} {} weather={} npcs_here=[{}] roster={}/{} grapevine={}items/{}distorted/max{}",
        state.active_scene.location_name,
        state.active_scene.location_id,
        state.clock.hour,
        state.clock.minute,
        state.clock.day_of_week,
        state.clock.season,
        state.weather,
        npcs.join(", "),
        state.npcs.introduced,
        state.npcs.total,
        state.grapevine.item_count,
        state.grapevine.distorted_item_count,
        state.grapevine.max_distortion,
    )
}
