use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::session;

/// Options forwarded to `POST /api/command`.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOpts {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addressed_to: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_map: Option<bool>,
}

// ── Wire types (subset of server's CommandResponse / StateResponse) ──────────

#[derive(Deserialize, Serialize)]
pub struct CommandResponse {
    pub outcome: String,
    pub kind: String,
    pub echo: String,
    pub lines: Vec<OutputLine>,
    #[serde(default)]
    pub kind_detail: serde_json::Value,
    #[serde(default)]
    pub travel: Option<TravelDetail>,
    #[serde(default)]
    pub state: Option<StateBundle>,
    pub elapsed_ms: u64,
}

#[derive(Deserialize, Serialize)]
pub struct OutputLine {
    pub id: String,
    pub role: String,
    pub speaker: String,
    pub text: String,
}

#[derive(Deserialize, Serialize)]
pub struct TravelDetail {
    pub from: String,
    pub to: String,
    pub duration_minutes: u64,
}

#[derive(Deserialize, Serialize)]
pub struct StateBundle {
    pub world: WorldSnapshot,
    pub npcs_here: Vec<NpcInfo>,
    #[serde(default)]
    pub map: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
pub struct WorldSnapshot {
    pub location_name: String,
    pub time_label: String,
    pub season: String,
    pub weather: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Deserialize, Serialize)]
pub struct NpcInfo {
    pub name: String,
    pub occupation: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── Client ────────────────────────────────────────────────────────────────────

pub struct ParishClient {
    base_url: String,
    client: Client,
}

impl ParishClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        // Load persisted session cookie.
        if let Some(sid) = session::load() {
            let url = base_url
                .parse::<reqwest::Url>()
                .context("invalid server URL")?;
            jar.add_cookie_str(&format!("parish_sid={sid}"), &url);
        }
        let client = Client::builder()
            .cookie_provider(jar)
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    pub async fn post_command(&self, text: &str, opts: CommandOpts) -> Result<CommandResponse> {
        #[derive(Serialize)]
        struct Body<'a> {
            text: &'a str,
            #[serde(flatten)]
            opts: CommandOpts,
        }
        let resp = self
            .client
            .post(format!("{}/api/command", self.base_url))
            .json(&Body { text, opts })
            .send()
            .await
            .context(transport_hint(&self.base_url))?;

        self.maybe_save_cookie(&resp);

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("server returned {status}: {body}");
        }
        resp.json::<CommandResponse>()
            .await
            .context("invalid response JSON")
    }

    fn maybe_save_cookie(&self, resp: &reqwest::Response) {
        for cookie in resp.cookies() {
            if cookie.name() == "parish_sid" {
                let _ = session::save(cookie.value());
                break;
            }
        }
    }
}

fn transport_hint(base_url: &str) -> String {
    format!(
        "could not reach Parish server at {base_url} — \
         start it with `just run-headless --web 3001` or set PARISH_SERVER"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: serialize a (text, opts) pair exactly as post_command would send it.
    fn body_json(text: &str, opts: CommandOpts) -> serde_json::Value {
        #[derive(Serialize)]
        struct Body<'a> {
            text: &'a str,
            #[serde(flatten)]
            opts: CommandOpts,
        }
        serde_json::to_value(Body { text, opts }).expect("serializable")
    }

    // AC-1: default opts — only `text` key present
    #[test]
    fn command_body_default_opts_has_only_text() {
        let v = body_json("look", CommandOpts::default());
        assert_eq!(v.get("text").and_then(|t| t.as_str()), Some("look"));
        assert!(v.get("addressedTo").is_none(), "unexpected addressedTo");
        assert!(v.get("timeoutMs").is_none(), "unexpected timeoutMs");
        assert!(v.get("includeState").is_none(), "unexpected includeState");
        assert!(v.get("includeMap").is_none(), "unexpected includeMap");
    }

    // AC-1: addressed_to serialises as camelCase array
    #[test]
    fn command_body_addressed_to_is_camel_case() {
        let opts = CommandOpts {
            addressed_to: vec!["Bridget".into(), "Seamus".into()],
            ..Default::default()
        };
        let v = body_json("hello", opts);
        let arr = v["addressedTo"].as_array().expect("addressedTo array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("Bridget"));
        assert_eq!(arr[1].as_str(), Some("Seamus"));
    }

    // AC-1: timeout_ms serialises as camelCase number
    #[test]
    fn command_body_timeout_ms_is_camel_case() {
        let opts = CommandOpts {
            timeout_ms: Some(5000),
            ..Default::default()
        };
        let v = body_json("wait", opts);
        assert_eq!(v["timeoutMs"].as_u64(), Some(5000));
    }

    // AC-1: include_state and include_map serialise as camelCase booleans
    #[test]
    fn command_body_include_flags_are_camel_case() {
        let opts = CommandOpts {
            include_state: Some(true),
            include_map: Some(false),
            ..Default::default()
        };
        let v = body_json("look", opts);
        assert_eq!(v["includeState"].as_bool(), Some(true));
        assert_eq!(v["includeMap"].as_bool(), Some(false));
    }

    // AC-1: an empty addressed_to vec is omitted entirely
    #[test]
    fn command_body_empty_addressed_to_is_omitted() {
        let opts = CommandOpts {
            addressed_to: vec![],
            ..Default::default()
        };
        let v = body_json("look", opts);
        assert!(v.get("addressedTo").is_none(), "empty vec must be omitted");
    }

    // ── Wire compatibility with parish-server::sync_types (TD-002) ───────────
    //
    // `parish-client` is dependency-free by design (it does not link
    // `parish-server`), so the compat guard is a JSON-shape contract: a
    // document carrying the exact field names the server's
    // `sync_types::CommandResponse` emits must deserialize losslessly into
    // this crate's `CommandResponse`. The companion server-side test
    // (`parish-server` `sync_types::tests::command_response_wire_keys_match_client`)
    // asserts the server actually serializes those same keys, so a rename on
    // either side breaks one of the two tests. Keep the two in lockstep.

    /// A JSON document shaped exactly like `parish-server` serializes a
    /// `CommandResponse` for `kind == "moved"` (travel + state + map present).
    /// Field names are snake_case for the response body and camelCase for the
    /// request opts, matching the server's `#[serde(rename_all)]` choices.
    fn server_command_response_json() -> serde_json::Value {
        serde_json::json!({
            "outcome": "ok",
            "kind": "moved",
            "echo": "go to the church",
            "lines": [
                { "id": "l1", "role": "system", "speaker": "System", "text": "You walk to the church." }
            ],
            "kind_detail": { "anything": "goes" },
            "travel": { "from": "The Crossroads", "to": "The Church", "duration_minutes": 12 },
            "state": {
                "world": {
                    "location_name": "The Church",
                    "time_label": "morning",
                    "season": "spring",
                    "weather": "clear",
                    // Extra, server-only fields land in `extra` via #[serde(flatten)].
                    "extra_field": 42
                },
                "npcs_here": [
                    { "name": "Father Quinn", "occupation": "Clergy", "mood": "calm" }
                ],
                "map": { "locations": [] }
            },
            "elapsed_ms": 137
        })
    }

    #[test]
    fn deserializes_server_command_response_losslessly() {
        let doc = server_command_response_json();
        let resp: CommandResponse = serde_json::from_value(doc)
            .expect("server-shaped JSON must deserialize into the client wire type");

        assert_eq!(resp.outcome, "ok");
        assert_eq!(resp.kind, "moved");
        assert_eq!(resp.echo, "go to the church");
        assert_eq!(resp.elapsed_ms, 137);

        assert_eq!(resp.lines.len(), 1);
        assert_eq!(resp.lines[0].id, "l1");
        assert_eq!(resp.lines[0].role, "system");
        assert_eq!(resp.lines[0].speaker, "System");

        let travel = resp.travel.expect("travel present for kind=moved");
        assert_eq!(travel.from, "The Crossroads");
        assert_eq!(travel.to, "The Church");
        assert_eq!(travel.duration_minutes, 12);

        let state = resp.state.expect("state bundle present");
        assert_eq!(state.world.location_name, "The Church");
        assert_eq!(state.world.time_label, "morning");
        assert_eq!(state.world.season, "spring");
        assert_eq!(state.world.weather, "clear");
        // Unknown server-only world fields survive in `extra` (forward-compat).
        assert_eq!(
            state
                .world
                .extra
                .get("extra_field")
                .and_then(|v| v.as_i64()),
            Some(42)
        );
        assert_eq!(state.npcs_here.len(), 1);
        assert_eq!(state.npcs_here[0].name, "Father Quinn");
        assert_eq!(state.npcs_here[0].occupation, "Clergy");
        assert!(state.map.is_some(), "map field must round-trip");
    }

    #[test]
    fn deserializes_minimal_server_response_without_optionals() {
        // The server omits `travel`, `state`, and `map` via
        // skip_serializing_if. The client must treat all three as absent.
        let doc = serde_json::json!({
            "outcome": "empty",
            "kind": "empty",
            "echo": "",
            "lines": [],
            "kind_detail": null,
            "elapsed_ms": 0
        });
        let resp: CommandResponse =
            serde_json::from_value(doc).expect("minimal server JSON must deserialize");
        assert_eq!(resp.outcome, "empty");
        assert!(resp.travel.is_none());
        assert!(resp.state.is_none());
        assert!(resp.lines.is_empty());
    }

    /// The exact key set the client expects at the top level of a
    /// `CommandResponse`. Mirrored by the server-side companion test so a
    /// renamed/added/removed server field fails CI on one side or the other.
    #[test]
    fn command_response_expected_top_level_keys() {
        let resp: CommandResponse =
            serde_json::from_value(server_command_response_json()).expect("deserializes");
        // Round-trip back out and confirm the client re-serializes the same
        // canonical key set (the client type is Serialize too).
        let reser = serde_json::to_value(&resp).expect("client re-serializes");
        let obj = reser.as_object().expect("object");
        for key in [
            "outcome",
            "kind",
            "echo",
            "lines",
            "kind_detail",
            "travel",
            "state",
            "elapsed_ms",
        ] {
            assert!(obj.contains_key(key), "client response missing key {key}");
        }
    }
}
