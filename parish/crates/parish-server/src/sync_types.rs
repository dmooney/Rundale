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

#[cfg(test)]
mod tests {
    use super::*;

    /// Companion to `parish-client`'s wire-compat tests (TD-002).
    ///
    /// `parish-client` hand-mirrors these structs without linking this crate.
    /// This test pins the serialized key set so a field rename here fails CI
    /// alongside the client-side `deserializes_server_command_response_*`
    /// tests. Keep the two in lockstep.
    #[test]
    fn command_response_wire_keys_match_client() {
        // `state` omitted (skip_serializing_if) so no WorldSnapshot needed;
        // `travel` present so the optional-field key appears.
        let resp = CommandResponse {
            outcome: Outcome::Ok,
            kind: Kind::Moved,
            echo: "go to the church".to_string(),
            lines: vec![OutputLine {
                id: "l1".to_string(),
                role: Role::System,
                speaker: "System".to_string(),
                text: "You walk to the church.".to_string(),
            }],
            kind_detail: serde_json::json!({"k": "v"}),
            travel: Some(TravelDetail {
                from: "The Crossroads".to_string(),
                to: "The Church".to_string(),
                duration_minutes: 12,
            }),
            state: None,
            elapsed_ms: 137,
        };

        let v = serde_json::to_value(&resp).expect("serializes");
        let obj = v.as_object().expect("object");

        // Exactly the keys parish-client/src/client.rs::CommandResponse reads.
        for key in [
            "outcome",
            "kind",
            "echo",
            "lines",
            "kind_detail",
            "travel",
            "elapsed_ms",
        ] {
            assert!(obj.contains_key(key), "server response missing key {key}");
        }
        // `state` was None → omitted by skip_serializing_if.
        assert!(
            !obj.contains_key("state"),
            "state must be omitted when absent"
        );

        // Enum values are the snake_case strings the client matches on.
        assert_eq!(obj["outcome"], "ok");
        assert_eq!(obj["kind"], "moved");
        assert_eq!(obj["lines"][0]["role"], "system");

        // Nested wire structs keep their client-expected keys.
        let line = &obj["lines"][0];
        for key in ["id", "role", "speaker", "text"] {
            assert!(line.get(key).is_some(), "OutputLine missing key {key}");
        }
        let travel = &obj["travel"];
        for key in ["from", "to", "duration_minutes"] {
            assert!(travel.get(key).is_some(), "TravelDetail missing key {key}");
        }
    }
}
