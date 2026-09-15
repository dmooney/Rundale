//! Bounded authoritative state for the Phase 5 living-world proofs.
//!
//! These records are deliberately small and typed. They are persisted beside
//! the existing mobile domain envelope; prose and model metadata never become
//! the source of truth for knowledge, tasks, or movement decisions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{LogicalRequestId, StateRevision};

pub const MAX_KNOWLEDGE_RECORDS: usize = 64;
pub const LETTER_TASK_TEMPLATE_ID: &str = "task-deliver-peig-letter";
pub const GOSSIP_FACT_ID: &str = "fact-micheal-stock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicClassification {
    AuthoredFact,
    PlayerClaim,
    HeardFromNpc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRecord {
    pub id: String,
    pub proposition: String,
    #[serde(rename = "knowingNpcID")]
    pub knowing_npc_id: String,
    pub classification: EpistemicClassification,
    #[serde(default, rename = "originatingFactID")]
    pub originating_fact_id: Option<String>,
    #[serde(default, rename = "originatingRequestID")]
    pub originating_request_id: Option<String>,
    #[serde(default, rename = "originatingEventID")]
    pub originating_event_id: Option<String>,
    #[serde(default, rename = "sourceNpcID")]
    pub source_npc_id: Option<String>,
    pub acquired_at: DateTime<Utc>,
    pub location_id: String,
    pub committed_state_revision: StateRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationDecisionCause {
    Schedule,
    WeatherOverride,
    DiagnosticOverride,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationDecision {
    #[serde(rename = "npcID")]
    pub npc_id: String,
    #[serde(rename = "locationID")]
    pub location_id: String,
    pub cause: LocationDecisionCause,
    pub at: DateTime<Utc>,
    pub committed_state_revision: StateRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOverrideRecord {
    pub kind: String,
    pub value: String,
    pub at: DateTime<Utc>,
    pub committed_state_revision: StateRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Phase5State {
    #[serde(default)]
    pub knowledge: Vec<KnowledgeRecord>,
    #[serde(default)]
    pub last_location_decisions: Vec<LocationDecision>,
    #[serde(default)]
    pub diagnostic_overrides: Vec<DiagnosticOverrideRecord>,
}

impl Phase5State {
    pub fn initialize_if_missing(&mut self, now: DateTime<Utc>, revision: StateRevision) {
        if self.knowledge.iter().any(|record| {
            record.knowing_npc_id == "npc-micheal"
                && record.originating_fact_id.as_deref() == Some(GOSSIP_FACT_ID)
        }) {
            return;
        }
        self.insert_knowledge(KnowledgeRecord {
            id: format!("authored:npc-micheal:{GOSSIP_FACT_ID}"),
            proposition: "The wet ground has made moving cattle difficult this week.".into(),
            knowing_npc_id: "npc-micheal".into(),
            classification: EpistemicClassification::AuthoredFact,
            originating_fact_id: Some(GOSSIP_FACT_ID.into()),
            originating_request_id: None,
            originating_event_id: None,
            source_npc_id: None,
            acquired_at: now,
            location_id: "connolly-cottage".into(),
            committed_state_revision: revision,
        });
    }

    pub fn insert_knowledge(&mut self, record: KnowledgeRecord) -> bool {
        if self
            .knowledge
            .iter()
            .any(|existing| existing.id == record.id)
        {
            return false;
        }
        while self.knowledge.len() >= MAX_KNOWLEDGE_RECORDS {
            let eviction_index = self
                .knowledge
                .iter()
                .position(|existing| {
                    existing.classification != EpistemicClassification::AuthoredFact
                })
                .unwrap_or(0);
            self.knowledge.remove(eviction_index);
        }
        self.knowledge.push(record);
        true
    }

    pub fn memory_id(request: &LogicalRequestId, npc_id: &str, ordinal: usize) -> String {
        format!("memory:{}:{npc_id}:{ordinal}", request.raw_value)
    }

    pub fn gossip_id(source_record_id: &str, recipient_id: &str) -> String {
        format!("gossip:{source_record_id}:{recipient_id}")
    }

    pub fn knowledge_for(&self, npc_id: &str) -> Vec<&KnowledgeRecord> {
        self.knowledge
            .iter()
            .filter(|record| record.knowing_npc_id == npc_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: usize) -> KnowledgeRecord {
        KnowledgeRecord {
            id: format!("record-{id}"),
            proposition: format!("proposition {id}"),
            knowing_npc_id: "npc-peig".into(),
            classification: EpistemicClassification::PlayerClaim,
            originating_fact_id: None,
            originating_request_id: Some("request".into()),
            originating_event_id: Some("event".into()),
            source_npc_id: None,
            acquired_at: Utc::now(),
            location_id: "kilteevan-village".into(),
            committed_state_revision: StateRevision::new(1),
        }
    }

    #[test]
    fn knowledge_is_idempotent_and_bounded() {
        let mut state = Phase5State::default();
        assert!(state.insert_knowledge(record(0)));
        assert!(!state.insert_knowledge(record(0)));
        for id in 1..=MAX_KNOWLEDGE_RECORDS {
            assert!(state.insert_knowledge(record(id)));
        }
        assert_eq!(state.knowledge.len(), MAX_KNOWLEDGE_RECORDS);
        assert_eq!(state.knowledge.first().unwrap().id, "record-1");
        assert_eq!(state.knowledge.last().unwrap().id, "record-64");
    }

    #[test]
    fn derived_ids_are_stable() {
        let request = LogicalRequestId::new("request-1");
        assert_eq!(
            Phase5State::memory_id(&request, "npc-peig", 0),
            "memory:request-1:npc-peig:0"
        );
        assert_eq!(
            Phase5State::gossip_id("source-1", "npc-roisin"),
            "gossip:source-1:npc-roisin"
        );
    }

    #[test]
    fn bounded_ledger_retains_authored_gossip_source() {
        let mut state = Phase5State::default();
        state.initialize_if_missing(Utc::now(), StateRevision::new(0));
        for id in 0..MAX_KNOWLEDGE_RECORDS {
            state.insert_knowledge(record(id));
        }
        assert_eq!(state.knowledge.len(), MAX_KNOWLEDGE_RECORDS);
        assert!(state.knowledge.iter().any(|entry| {
            entry.classification == EpistemicClassification::AuthoredFact
                && entry.originating_fact_id.as_deref() == Some(GOSSIP_FACT_ID)
        }));
    }
}
