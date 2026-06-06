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

/// Request body for `POST /api/command`: the input text plus the flattened
/// option fields. Mirrors `parish-server`'s `CommandRequest` wire shape.
#[derive(Serialize)]
pub(crate) struct CommandBody<'a> {
    pub text: &'a str,
    #[serde(flatten)]
    pub opts: CommandOpts,
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
        let resp = self
            .client
            .post(format!("{}/api/command", self.base_url))
            .json(&CommandBody { text, opts })
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

    #[test]
    fn command_opts_use_camel_case_keys() {
        let opts = CommandOpts {
            addressed_to: vec!["Maggie".into()],
            timeout_ms: Some(5_000),
            include_state: Some(true),
            include_map: Some(false),
        };
        let v = serde_json::to_value(&opts).expect("serialize");
        let obj = v.as_object().expect("object");
        // Wire contract: server's CommandRequest is `rename_all = "camelCase"`.
        assert!(obj.contains_key("addressedTo"));
        assert!(obj.contains_key("timeoutMs"));
        assert!(obj.contains_key("includeState"));
        assert!(obj.contains_key("includeMap"));
        // Snake-case names must never leak onto the wire.
        assert!(!obj.contains_key("addressed_to"));
        assert!(!obj.contains_key("timeout_ms"));
    }

    #[test]
    fn default_opts_omit_empty_and_none_fields() {
        let v = serde_json::to_value(CommandOpts::default()).expect("serialize");
        // skip_serializing_if keeps the body minimal: no keys at all when unset.
        assert_eq!(v.as_object().expect("object").len(), 0, "got: {v}");
    }

    #[test]
    fn command_body_flattens_text_and_opts() {
        let body = CommandBody {
            text: "go to the church",
            opts: CommandOpts {
                include_map: Some(true),
                ..Default::default()
            },
        };
        let v = serde_json::to_value(&body).expect("serialize");
        assert_eq!(v["text"], "go to the church");
        // Flattened: opt keys sit alongside `text`, not nested under `opts`.
        assert_eq!(v["includeMap"], true);
        assert!(v.get("opts").is_none(), "opts must be flattened: {v}");
    }

    /// TD-002 drift guard: the client's wire `CommandResponse` is a hand
    /// mirror of `parish_server::sync_types::CommandResponse`. Round-trip a
    /// real, fully-populated server response through the client type so a
    /// server-side field rename/removal fails this test instead of silently
    /// deserializing to a wrong/missing value at runtime.
    #[test]
    fn deserializes_a_real_server_command_response() {
        use parish_server::sync_types as server;

        let server_resp = server::CommandResponse {
            outcome: server::Outcome::Ok,
            kind: server::Kind::Moved,
            echo: "go to the church".to_string(),
            lines: vec![server::OutputLine {
                id: "l1".to_string(),
                role: server::Role::Npc,
                speaker: "Maggie".to_string(),
                text: "Off you go, then.".to_string(),
            }],
            kind_detail: serde_json::json!({ "reason": "ok" }),
            travel: Some(server::TravelDetail {
                from: "The Crossroads".to_string(),
                to: "St Brigid's".to_string(),
                duration_minutes: 12,
            }),
            // `state` exercises the `skip_serializing_if = Option::is_none`
            // branch; the client must tolerate its absence via `#[serde(default)]`.
            state: None,
            elapsed_ms: 42,
        };

        let wire = serde_json::to_value(&server_resp).expect("server serializes");
        let client: CommandResponse =
            serde_json::from_value(wire).expect("client deserializes server response");

        assert_eq!(client.outcome, "ok");
        assert_eq!(client.kind, "moved");
        assert_eq!(client.echo, "go to the church");
        assert_eq!(client.lines.len(), 1);
        assert_eq!(client.lines[0].speaker, "Maggie");
        assert_eq!(client.lines[0].role, "npc");
        assert_eq!(client.kind_detail["reason"], "ok");
        let travel = client.travel.expect("travel present for kind=moved");
        assert_eq!(travel.from, "The Crossroads");
        assert_eq!(travel.to, "St Brigid's");
        assert_eq!(travel.duration_minutes, 12);
        assert!(client.state.is_none());
        assert_eq!(client.elapsed_ms, 42);
    }
}
