//! Request / response types for the synchronous `/api/command` and
//! `/api/state` endpoints.
//!
//! These types are the wire format for thin clients (CLI, agents, integration
//! tests) that want a complete, synchronous response rather than the async
//! WebSocket stream the browser uses.

use parish_core::ipc::{MapData, NpcInfo, WorldSnapshot};
use serde::{Deserialize, Serialize};

// ── /api/command ─────────────────────────────────────────────────────────────

/// Request body for `POST /api/command`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    /// Player input text (natural language or `/slash` command).
    pub text: String,
    /// Real names of NPCs explicitly addressed (chip-first order).
    #[serde(default)]
    pub addressed_to: Vec<String>,
    /// Hard time limit in milliseconds before returning a partial response.
    /// Default 60 000; max 120 000.
    pub timeout_ms: Option<u64>,
    /// Whether to embed the post-command world state in the response.
    /// Default `true`.
    pub include_state: Option<bool>,
    /// Whether to embed map data in the state bundle.
    /// Default `false`; auto-set to `true` when `kind == "moved"`.
    pub include_map: Option<bool>,
}

/// Top-level response from `POST /api/command`.
#[derive(Serialize)]
pub struct CommandResponse {
    /// Whether the command completed, timed out, or was rejected.
    pub outcome: Outcome,
    /// What category of thing happened.
    pub kind: Kind,
    /// The original text after trimming.
    pub echo: String,
    /// Ordered log of text produced by this command.
    pub lines: Vec<OutputLine>,
    /// Kind-specific structured data.
    pub kind_detail: serde_json::Value,
    /// Present when `kind == "moved"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel: Option<TravelDetail>,
    /// World state snapshot taken after the command completes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<StateBundle>,
    /// Wall-clock milliseconds the server spent processing.
    pub elapsed_ms: u64,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Timeout,
    Rejected,
    Empty,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Moved,
    MoveBlocked,
    Looked,
    Talked,
    TalkFailed,
    System,
    Rejected,
    Empty,
}

/// One line of text output from a command.
#[derive(Serialize, Clone)]
pub struct OutputLine {
    pub id: String,
    pub role: Role,
    /// Speaker label — NPC name, "You", or "System".
    pub speaker: String,
    pub text: String,
}

#[derive(Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Player,
    System,
    Npc,
}

/// Travel details included when `kind == "moved"`.
#[derive(Serialize, Clone)]
pub struct TravelDetail {
    pub from: String,
    pub to: String,
    pub duration_minutes: u64,
}

/// World state bundle embedded in the response.
#[derive(Serialize)]
pub struct StateBundle {
    pub world: WorldSnapshot,
    pub npcs_here: Vec<NpcInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapData>,
}

// ── /api/state ───────────────────────────────────────────────────────────────

/// Response from `GET /api/state`.
#[derive(Serialize)]
pub struct StateResponse {
    pub world: WorldSnapshot,
    pub npcs_here: Vec<NpcInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapData>,
    /// UTC ISO-8601 timestamp from the server clock.
    pub server_time: String,
}
