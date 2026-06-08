//! LLM-backed player + judge, built on the shared `parish-inference` client.
//!
//! This reuses `AnyClient` / `build_client` / `generate*` from
//! `parish-inference` — the same transport the game itself uses — so the
//! harness gets the native Anthropic Messages client, OpenAI-compat, local
//! vllm-mlx, rate-limiting, retry, and timeouts for free. No second HTTP-LLM
//! client is introduced.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;

use parish_core::config::{InferenceConfig, Provider};
use parish_core::inference::{AnyClient, GenerateParams, build_client};

use crate::error::{HarnessError, Result};
use crate::score::{Axis, AxisScore, RawFinding};

use super::traits::{Judge, Player};
use super::types::{JudgeVerdict, Observation, PlayerMove, RunTranscript};

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
        let line = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("look")
            .trim_matches('"')
            .to_string();
        Ok(PlayerMove::new(line))
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

/// The JSON shape the judge returns.
#[derive(Debug, Deserialize)]
struct JudgeVerdictJson {
    #[serde(default)]
    axes: HashMap<String, i64>,
    #[serde(default)]
    axis_rationales: HashMap<String, String>,
    #[serde(default)]
    findings: Vec<RawFinding>,
}

/// Convert the judge's JSON into a typed verdict (clamping scores, mapping
/// axes, canonicalizing findings). Unknown axis keys are ignored; missing axes
/// default to a neutral 50 so a partial verdict still rolls up.
fn verdict_from_json(json: JudgeVerdictJson) -> JudgeVerdict {
    let axes = Axis::ALL
        .into_iter()
        .map(|axis| {
            let score = json
                .axes
                .get(axis.key())
                .copied()
                .unwrap_or(50)
                .clamp(0, 100) as u8;
            let rationale = json
                .axis_rationales
                .get(axis.key())
                .cloned()
                .unwrap_or_default();
            AxisScore {
                axis,
                score,
                rationale,
            }
        })
        .collect();
    let findings = json
        .findings
        .into_iter()
        .map(RawFinding::canonicalize)
        .collect();
    JudgeVerdict { axes, findings }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
