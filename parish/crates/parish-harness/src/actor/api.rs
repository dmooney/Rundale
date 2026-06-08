//! LLM-backed player + judge, built on the shared `parish-inference` client.
//!
//! This reuses `AnyClient` / `build_client` / `generate*` from
//! `parish-inference` — the same transport the game itself uses — so the
//! harness gets the native Anthropic Messages client, OpenAI-compat, local
//! vllm-mlx, rate-limiting, retry, and timeouts for free. No second HTTP-LLM
//! client is introduced.

use async_trait::async_trait;

use parish_core::config::{InferenceConfig, Provider};
use parish_core::inference::{AnyClient, GenerateParams, build_client};

use crate::error::{HarnessError, Result};

use super::traits::{Judge, Player};
use super::types::{JudgeVerdict, Observation, PlayerMove, RunTranscript};
use super::verdict::{JudgeVerdictJson, verdict_from_json};

/// Build an [`AnyClient`] for a provider/model. `api_key` is read from the
/// supplied value (callers typically source it from the environment).
pub fn make_client(
    provider_name: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<AnyClient> {
    let provider = Provider::from_str_loose(provider_name)
        .map_err(|e| HarnessError::Config(format!("unknown provider {provider_name:?}: {e}")))?;
    let cfg = InferenceConfig::default();
    let url = base_url
        .map(|s| s.to_string())
        .unwrap_or_else(|| provider.default_base_url().to_string());
    Ok(build_client(&provider, &url, api_key, &cfg))
}

/// LLM player.
pub struct ApiPlayer {
    client: AnyClient,
    model: String,
}

impl ApiPlayer {
    pub fn new(client: AnyClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }
}

#[async_trait]
impl Player for ApiPlayer {
    async fn choose_action(&self, obs: &Observation<'_>) -> Result<PlayerMove> {
        let system = format!(
            "You are playing a text interactive-fiction game set in an Irish parish in 1820. \
             Persona: {persona}. Strategy: {strategy}. Reply with EXACTLY ONE short player \
             command on a single line (e.g. 'look', 'go to the church', 'talk to Maggie', \
             'ask about the harvest'). No quotes, no explanation.",
            persona = obs.persona,
            strategy = obs.strategy,
        );
        let prompt = format!(
            "Turn {turn}.\nWhat you can see now:\n{narrative}\n\nYour command:",
            turn = obs.turn_index,
            narrative = if obs.narrative.is_empty() {
                "(you have just arrived; look around)"
            } else {
                obs.narrative
            },
        );
        let params = GenerateParams {
            max_tokens: Some(64),
            temperature: Some(0.8),
            ..Default::default()
        };
        let text = self
            .client
            .generate(&self.model, &prompt, Some(&system), params)
            .await
            .map_err(|e| HarnessError::Inference(e.to_string()))?;
        Ok(PlayerMove::new(extract_command(&text)))
    }
}

/// Extract a single player command from a raw LLM reply.
///
/// Small models often wrap the answer in a markdown code fence or stray
/// backticks/quotes despite the "one line, no formatting" instruction. We skip
/// fence lines (```` ``` ````/```` ```text ````), take the first real line, and
/// strip surrounding quotes/backticks. Falls back to `look` if nothing usable
/// remains so a turn never sends an empty command.
fn extract_command(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```"))
        .unwrap_or("look")
        .trim_matches(|c| c == '"' || c == '`' || c == '\'')
        .trim();
    if line.is_empty() {
        "look".to_string()
    } else {
        line.to_string()
    }
}

/// LLM judge.
pub struct ApiJudge {
    client: AnyClient,
    model: String,
    system_prompt: String,
    rubric_sha256: String,
}

impl ApiJudge {
    pub fn new(
        client: AnyClient,
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        rubric_sha256: impl Into<String>,
    ) -> Self {
        Self {
            client,
            model: model.into(),
            system_prompt: system_prompt.into(),
            rubric_sha256: rubric_sha256.into(),
        }
    }
}

#[async_trait]
impl Judge for ApiJudge {
    fn rubric_sha256(&self) -> &str {
        &self.rubric_sha256
    }

    async fn judge_run(&self, transcript: &RunTranscript) -> Result<JudgeVerdict> {
        let prompt = render_transcript_prompt(transcript);
        let params = GenerateParams {
            max_tokens: Some(4096),
            temperature: Some(0.0),
            ..Default::default()
        };
        let raw: JudgeVerdictJson = self
            .client
            .generate_json(&self.model, &prompt, Some(&self.system_prompt), params)
            .await
            .map_err(|e| HarnessError::Inference(e.to_string()))?;
        Ok(verdict_from_json(raw))
    }
}

/// Render the full transcript into the judge prompt body.
pub fn render_transcript_prompt(transcript: &RunTranscript) -> String {
    let mut s = String::new();
    s.push_str(&format!("Player persona: {}\n\n", transcript.persona));
    s.push_str("Transcript (each turn: input / narrative / engine-ground-truth):\n");
    for t in &transcript.turns {
        s.push_str(&format!(
            "--- turn {} ---\n> {}\n{}\n[outcome={} kind={} | {}]\n",
            t.turn_index, t.player_input, t.narrative, t.outcome, t.kind, t.state_summary
        ));
    }
    s.push_str("\nNow score the session and itemize findings per your instructions.");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Axis;

    #[test]
    fn verdict_from_json_clamps_and_maps_axes() {
        let json: JudgeVerdictJson = serde_json::from_str(
            r#"{
                "axes": {"narrative_coherence": 150, "common_sense": -10, "immersion": 60},
                "axis_rationales": {"immersion": "fine"},
                "findings": [
                    {"category": "identity_mixup", "turn_index": 3, "severity": "high",
                     "description": "x", "evidence_quote": "q", "signature_hint": "h"}
                ]
            }"#,
        )
        .unwrap();
        let verdict = verdict_from_json(json);
        assert_eq!(verdict.axes.len(), 7);
        let coherence = verdict
            .axes
            .iter()
            .find(|a| a.axis == Axis::NarrativeCoherence)
            .unwrap();
        assert_eq!(coherence.score, 100, "150 clamps to 100");
        let common = verdict
            .axes
            .iter()
            .find(|a| a.axis == Axis::CommonSense)
            .unwrap();
        assert_eq!(common.score, 0, "-10 clamps to 0");
        let missing = verdict
            .axes
            .iter()
            .find(|a| a.axis == Axis::Progression)
            .unwrap();
        assert_eq!(missing.score, 50, "absent axis defaults to neutral 50");
        assert_eq!(verdict.findings.len(), 1);
        assert_eq!(verdict.findings[0].category, "identity_mixup");
    }

    #[test]
    fn make_client_rejects_unknown_provider() {
        assert!(matches!(
            make_client("definitely-not-a-provider", None, None),
            Err(HarnessError::Config(_))
        ));
    }

    #[test]
    fn make_client_builds_anthropic() {
        // No network: build_client just constructs the typed client.
        assert!(make_client("anthropic", None, Some("sk-test")).is_ok());
    }

    #[test]
    fn extract_command_strips_fences_quotes_and_picks_first_real_line() {
        // Plain line.
        assert_eq!(extract_command("go to the church"), "go to the church");
        // Quoted.
        assert_eq!(extract_command("\"look\""), "look");
        // Markdown-fenced (the common small-model failure).
        assert_eq!(
            extract_command("```\ntalk to Maggie\n```"),
            "talk to Maggie"
        );
        // Fence with a language tag + leading blank line.
        assert_eq!(
            extract_command("\n```text\nask about the harvest\n```"),
            "ask about the harvest"
        );
        // Stray backticks.
        assert_eq!(extract_command("`look`"), "look");
        // Empty / fence-only falls back to a safe command.
        assert_eq!(extract_command("```\n```"), "look");
        assert_eq!(extract_command(""), "look");
    }
}
