//! Claude-Code-subagent judge: an on-disk queue-file protocol so an external
//! Claude Code drain loop can score runs without an API key in-process.
//!
//! # Protocol
//!
//! 1. `SubagentJudge::judge_run` writes a *pending* bundle to
//!    `{queue_dir}/pending/{uuid}.json`.
//! 2. The external drain loop reads the bundle, calls a Sonnet subagent, and
//!    writes the verdict JSON to `{queue_dir}/done/{uuid}.json`.
//! 3. `judge_run` polls `done/{uuid}.json` until it appears (or the timeout
//!    elapses).
//!
//! The done file uses the same `JudgeVerdictJson` shape as the API actor so
//! both paths parse through `verdict_from_json`.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::actor::types::{JudgeVerdict, RunTranscript};
use crate::actor::verdict::{JudgeVerdictJson, verdict_from_json};
use crate::error::{HarnessError, Result};

use super::traits::Judge;

/// How long to sleep between polling attempts for the done file.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// The pending bundle written for the external subagent to consume.
#[derive(Debug, Serialize)]
struct PendingBundle {
    id: String,
    rubric_sha256: String,
    transcript: TranscriptPayload,
}

/// Transcript fields the subagent sees (a serializable mirror of `RunTranscript`
/// that includes only what a judge needs).
#[derive(Debug, Serialize)]
struct TranscriptPayload {
    persona: String,
    turns: Vec<TurnPayload>,
}

#[derive(Debug, Serialize)]
struct TurnPayload {
    turn_index: u32,
    player_input: String,
    narrative: String,
    outcome: String,
    kind: String,
    state_summary: String,
}

/// A [`Judge`] that delegates scoring to an external Claude Code subagent via
/// on-disk queue files.
///
/// The judge writes a pending bundle to `{queue_dir}/pending/{uuid}.json` and
/// polls `{queue_dir}/done/{uuid}.json` until it appears. An external drain
/// loop (e.g. the `/rundale-bench drain-queue` skill) processes the pending
/// file, calls a Sonnet subagent, and writes the verdict JSON to the done
/// directory.
pub struct SubagentJudge {
    queue_dir: PathBuf,
    rubric_sha256: String,
    timeout: Duration,
}

impl SubagentJudge {
    /// Create a new subagent judge.
    ///
    /// - `queue_dir`: directory under which `pending/` and `done/`
    ///   subdirectories are created.
    /// - `rubric_sha256`: pinned rubric hash recorded on the run.
    /// - `timeout`: maximum time to wait for the done file to appear.
    pub fn new(queue_dir: PathBuf, rubric_sha256: impl Into<String>, timeout: Duration) -> Self {
        Self {
            queue_dir,
            rubric_sha256: rubric_sha256.into(),
            timeout,
        }
    }

    fn pending_dir(&self) -> PathBuf {
        self.queue_dir.join("pending")
    }

    fn done_dir(&self) -> PathBuf {
        self.queue_dir.join("done")
    }
}

#[async_trait]
impl Judge for SubagentJudge {
    fn rubric_sha256(&self) -> &str {
        &self.rubric_sha256
    }

    async fn judge_run(&self, transcript: &RunTranscript) -> Result<JudgeVerdict> {
        // Ensure directories exist.
        let pending_dir = self.pending_dir();
        let done_dir = self.done_dir();
        std::fs::create_dir_all(&pending_dir)
            .map_err(|e| HarnessError::io(pending_dir.display().to_string(), e))?;
        std::fs::create_dir_all(&done_dir)
            .map_err(|e| HarnessError::io(done_dir.display().to_string(), e))?;

        // Assign a unique id for this judging request.
        let id = Uuid::new_v4().to_string();

        // Build the bundle payload.
        let bundle = PendingBundle {
            id: id.clone(),
            rubric_sha256: self.rubric_sha256.clone(),
            transcript: TranscriptPayload {
                persona: transcript.persona.clone(),
                turns: transcript
                    .turns
                    .iter()
                    .map(|t| TurnPayload {
                        turn_index: t.turn_index,
                        player_input: t.player_input.clone(),
                        narrative: t.narrative.clone(),
                        outcome: t.outcome.clone(),
                        kind: t.kind.clone(),
                        state_summary: t.state_summary.clone(),
                    })
                    .collect(),
            },
        };

        let bundle_json =
            serde_json::to_string_pretty(&bundle).map_err(|e| HarnessError::Json {
                context: "serialize subagent bundle".into(),
                source: e,
            })?;

        let pending_path = pending_dir.join(format!("{id}.json"));
        std::fs::write(&pending_path, bundle_json.as_bytes())
            .map_err(|e| HarnessError::io(pending_path.display().to_string(), e))?;

        // Poll for the done file.
        let done_path = done_dir.join(format!("{id}.json"));
        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            if done_path.exists() {
                let raw = std::fs::read_to_string(&done_path)
                    .map_err(|e| HarnessError::io(done_path.display().to_string(), e))?;
                let verdict_json: JudgeVerdictJson =
                    serde_json::from_str(&raw).map_err(|e| HarnessError::Json {
                        context: format!("parse subagent done file {}", done_path.display()),
                        source: e,
                    })?;
                return Ok(verdict_from_json(verdict_json));
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(HarnessError::Judge("subagent judge timed out".into()));
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::types::TurnTranscript;

    fn make_transcript() -> RunTranscript {
        RunTranscript {
            persona: "tester".into(),
            turns: vec![TurnTranscript {
                turn_index: 0,
                player_input: "look".into(),
                narrative: "You see a lane.".into(),
                outcome: "ok".into(),
                kind: "looked".into(),
                state_summary: "loc=lane#1 08:00 Monday Spring weather=clear npcs_here=[] roster=0/5 grapevine=0items/0distorted/max3".into(),
            }],
        }
    }

    fn verdict_json_str() -> &'static str {
        r#"{
            "axes": {
                "narrative_coherence": 80,
                "character_fidelity": 75,
                "world_responsiveness": 70,
                "intent_fidelity": 85,
                "immersion": 72,
                "progression": 68,
                "common_sense": 78
            },
            "axis_rationales": {
                "narrative_coherence": "flows well"
            },
            "findings": []
        }"#
    }

    #[tokio::test]
    async fn subagent_judge_parses_done_file() {
        let tmp = tempfile::tempdir().unwrap();
        let queue_dir = tmp.path().to_path_buf();
        let judge =
            SubagentJudge::new(queue_dir.clone(), "rubric_sha_test", Duration::from_secs(5));

        let transcript = make_transcript();

        // Spawn the judge call in a background task, then write the done file.
        let done_dir = queue_dir.join("done");
        let verdict_json = verdict_json_str().to_string();

        let handle = tokio::spawn(async move { judge.judge_run(&transcript).await });

        // Give the judge time to write the pending file and start polling.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Find the pending file to learn the uuid.
        let pending_dir = queue_dir.join("pending");
        let pending_files: Vec<_> = std::fs::read_dir(&pending_dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(pending_files.len(), 1, "expected exactly one pending file");

        let pending_file = &pending_files[0];
        let file_name = pending_file.file_name().unwrap().to_str().unwrap();

        // Read the bundle and check it has the right shape.
        let bundle_raw = std::fs::read_to_string(pending_file).unwrap();
        let bundle: serde_json::Value = serde_json::from_str(&bundle_raw).unwrap();
        assert!(bundle.get("id").is_some());
        assert!(bundle.get("transcript").is_some());
        assert_eq!(bundle["rubric_sha256"].as_str().unwrap(), "rubric_sha_test");

        // Write the done file using the same stem (uuid).
        std::fs::create_dir_all(&done_dir).unwrap();
        let done_path = done_dir.join(file_name);
        std::fs::write(&done_path, verdict_json).unwrap();

        // The judge task should now complete.
        let verdict = handle.await.unwrap().expect("judge should succeed");

        assert_eq!(verdict.axes.len(), 7);
        assert!(verdict.findings.is_empty());

        let nc = verdict
            .axes
            .iter()
            .find(|a| a.axis == crate::score::Axis::NarrativeCoherence)
            .unwrap();
        assert_eq!(nc.score, 80);
        assert_eq!(nc.rationale, "flows well");
    }

    #[tokio::test]
    async fn subagent_judge_times_out_with_no_done_file() {
        let tmp = tempfile::tempdir().unwrap();
        let queue_dir = tmp.path().to_path_buf();
        // Very short timeout.
        let judge = SubagentJudge::new(queue_dir, "sha", Duration::from_millis(50));

        let transcript = make_transcript();
        let err = judge.judge_run(&transcript).await.unwrap_err();

        assert!(
            matches!(err, HarnessError::Judge(ref msg) if msg.contains("timed out")),
            "expected timeout error, got: {err}"
        );
    }
}
