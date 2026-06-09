//! Shared JSON shape + conversion for judge verdicts.
//!
//! Extracted from `api.rs` so both the API actor and the subagent actor can
//! parse the same on-disk / over-wire JSON without duplicating logic.

use std::collections::HashMap;

use serde::Deserialize;

use crate::score::{Axis, AxisScore, RawFinding};

use super::types::JudgeVerdict;

/// The JSON shape returned by an LLM judge (API path) or written into a
/// `done/` queue file (subagent path). Both parse through the same struct.
#[derive(Debug, Deserialize)]
pub(crate) struct JudgeVerdictJson {
    #[serde(default)]
    pub axes: HashMap<String, i64>,
    #[serde(default)]
    pub axis_rationales: HashMap<String, String>,
    #[serde(default)]
    pub findings: Vec<RawFinding>,
}

/// Convert the judge's JSON into a typed verdict: clamp scores 0-100, map
/// known axis keys, default absent axes to neutral 50, canonicalize findings.
/// Unknown axis keys are ignored.
pub(crate) fn verdict_from_json(json: JudgeVerdictJson) -> JudgeVerdict {
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
