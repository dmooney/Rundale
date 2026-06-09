//! Local mirror of the server's `/api/command` response, plus the lenient
//! views the harness needs.
//!
//! Like `parish-client`, the harness does not link `parish-server` at runtime;
//! it hand-mirrors the wire shape so a server field rename does not silently
//! change behavior. A round-trip parity test (gated behind `cfg(test)`, using
//! the server crate as a dev-dependency) pins this mirror against
//! `parish_server::sync_types::CommandResponse`.
//!
//! The mirror is deliberately *lenient*: `outcome`/`kind` are `String` (not
//! re-declared enums) and `state` is an opaque `Value`, so the harness keeps
//! deserializing even if the server adds an enum variant or a state field. The
//! harness interprets `outcome`/`kind` through the [`Outcome`] helper.

use serde::{Deserialize, Serialize};

/// Mirror of `parish_server::sync_types::CommandResponse`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandResponse {
    pub outcome: String,
    pub kind: String,
    pub echo: String,
    #[serde(default)]
    pub lines: Vec<OutputLine>,
    #[serde(default)]
    pub kind_detail: serde_json::Value,
    #[serde(default)]
    pub travel: Option<TravelDetail>,
    /// Opaque post-command world snapshot; the harness reads engine facts from
    /// `/api/engine-state` instead, so this is kept as raw JSON.
    #[serde(default)]
    pub state: Option<serde_json::Value>,
    #[serde(default)]
    pub elapsed_ms: u64,
}

/// One narrative line.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputLine {
    #[serde(default)]
    pub id: String,
    pub role: String,
    pub speaker: String,
    pub text: String,
}

/// Travel detail present when `kind == "moved"`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TravelDetail {
    pub from: String,
    pub to: String,
    pub duration_minutes: u64,
}

/// The interpreted command outcome. Maps the wire `outcome` string onto the
/// closed set the gate logic reasons about; anything unrecognized is
/// `Unknown`, which the gate treats conservatively (not a hard fail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Timeout,
    Rejected,
    Empty,
    Unknown,
}

impl CommandResponse {
    /// Interpret the wire `outcome` field.
    pub fn outcome(&self) -> Outcome {
        match self.outcome.as_str() {
            "ok" => Outcome::Ok,
            "timeout" => Outcome::Timeout,
            "rejected" => Outcome::Rejected,
            "empty" => Outcome::Empty,
            _ => Outcome::Unknown,
        }
    }

    /// Concatenate the narrative lines into a single block for the player /
    /// judge prompt and the transcript.
    pub fn narrative(&self) -> String {
        self.lines
            .iter()
            .map(|l| {
                if l.speaker.is_empty() {
                    l.text.clone()
                } else {
                    format!("{}: {}", l.speaker, l.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip parity: build the *server's* `CommandResponse`, serialize it,
    /// and confirm this mirror deserializes every field the harness reads. A
    /// rename or type change on the server side fails this test in the same CI
    /// run (companion to `parish_server::sync_types`'s own key-set test).
    #[test]
    fn wire_parity_round_trips_server_command_response() {
        use parish_server::sync_types as st;

        let server = st::CommandResponse {
            outcome: st::Outcome::Ok,
            kind: st::Kind::Moved,
            echo: "go to the church".to_string(),
            lines: vec![st::OutputLine {
                id: "l1".to_string(),
                role: st::Role::System,
                speaker: "System".to_string(),
                text: "You walk to the church.".to_string(),
            }],
            kind_detail: serde_json::json!({"k": "v"}),
            travel: Some(st::TravelDetail {
                from: "The Crossroads".to_string(),
                to: "The Church".to_string(),
                duration_minutes: 12,
            }),
            state: None,
            elapsed_ms: 137,
        };

        let json = serde_json::to_string(&server).expect("server response serializes");
        let mirror: CommandResponse =
            serde_json::from_str(&json).expect("harness mirror deserializes server response");

        assert_eq!(mirror.outcome(), Outcome::Ok);
        assert_eq!(mirror.outcome, "ok");
        assert_eq!(mirror.kind, "moved");
        assert_eq!(mirror.echo, "go to the church");
        assert_eq!(mirror.elapsed_ms, 137);
        assert_eq!(mirror.lines.len(), 1);
        assert_eq!(mirror.lines[0].role, "system");
        assert_eq!(mirror.lines[0].speaker, "System");
        let travel = mirror.travel.expect("travel present");
        assert_eq!(travel.from, "The Crossroads");
        assert_eq!(travel.to, "The Church");
        assert_eq!(travel.duration_minutes, 12);
    }

    #[test]
    fn outcome_maps_unknown_conservatively() {
        let r = CommandResponse {
            outcome: "something_new".to_string(),
            kind: "system".to_string(),
            echo: String::new(),
            lines: vec![],
            kind_detail: serde_json::Value::Null,
            travel: None,
            state: None,
            elapsed_ms: 0,
        };
        assert_eq!(r.outcome(), Outcome::Unknown);
    }

    #[test]
    fn narrative_joins_speaker_and_text() {
        let r = CommandResponse {
            outcome: "ok".to_string(),
            kind: "talked".to_string(),
            echo: String::new(),
            lines: vec![
                OutputLine {
                    id: "1".into(),
                    role: "npc".into(),
                    speaker: "Maggie".into(),
                    text: "Good morning.".into(),
                },
                OutputLine {
                    id: "2".into(),
                    role: "system".into(),
                    speaker: String::new(),
                    text: "She nods.".into(),
                },
            ],
            kind_detail: serde_json::Value::Null,
            travel: None,
            state: None,
            elapsed_ms: 0,
        };
        assert_eq!(r.narrative(), "Maggie: Good morning.\nShe nods.");
    }
}
