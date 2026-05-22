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
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    pub outcome: String,
    pub kind: String,
    pub echo: String,
    pub lines: Vec<OutputLine>,
    pub kind_detail: serde_json::Value,
    pub travel: Option<TravelDetail>,
    pub state: Option<StateBundle>,
    pub elapsed_ms: u64,
}

#[derive(Deserialize, Serialize)]
pub struct OutputLine {
    pub id: String,
    pub role: String,
    pub speaker: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TravelDetail {
    pub from_id: String,
    pub to_id: String,
    pub from_name: String,
    pub to_name: String,
    pub duration_minutes: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateBundle {
    pub world: WorldSnapshot,
    pub npcs_here: Vec<NpcInfo>,
    pub map: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSnapshot {
    pub location_id: String,
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
