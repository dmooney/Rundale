//! Identities used by the request lifecycle.
//!
//! Logical requests and execution attempts are opaque strings (a host may
//! supply the logical id). Everything derived from them — inference call ids,
//! transcript item ids, transcript event ids — is deterministic, so replaying
//! or re-delivering the same attempt output yields the same ids and a journal
//! can deduplicate it.

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Wraps an existing identifier.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// One player submission, stable across retries.
    LogicalRequestId
);
string_id!(
    /// One execution of a logical request. A retry gets a new attempt id.
    ExecutionAttemptId
);
string_id!(
    /// One inference call inside an attempt: `"{attempt}#{n}"`.
    InferenceCallId
);
string_id!(
    /// One transcript row a presentation layer renders and updates in place.
    TranscriptItemId
);
string_id!(
    /// One transcript event: `"{request}:{attempt or -}:{ordinal}"`.
    TranscriptEventId
);

impl LogicalRequestId {
    /// A fresh random request id.
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// The transcript item of this request's accepted command, shared by
    /// every attempt.
    pub fn command_item(&self) -> TranscriptItemId {
        TranscriptItemId(format!("{}:command", self.0))
    }
}

impl ExecutionAttemptId {
    /// A fresh random attempt id.
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// The id of the `n`-th inference call (1-based) in this attempt.
    pub fn call(&self, n: u32) -> InferenceCallId {
        InferenceCallId(format!("{}#{n}", self.0))
    }

    /// The transcript item for this attempt's `ordinal`-th output row.
    pub fn item(&self, ordinal: u32) -> TranscriptItemId {
        TranscriptItemId(format!("{}:{ordinal}", self.0))
    }
}

impl TranscriptEventId {
    /// The deterministic id of the `ordinal`-th event a request (and
    /// optionally one of its attempts) produced.
    pub fn derive(
        request: &LogicalRequestId,
        attempt: Option<&ExecutionAttemptId>,
        ordinal: u32,
    ) -> Self {
        let attempt = attempt.map(ExecutionAttemptId::as_str).unwrap_or("-");
        Self(format!("{}:{attempt}:{ordinal}", request.0))
    }
}

/// Session-monotonic position of a transcript event, assigned by the journal
/// when the event is appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventSequence(pub u64);

/// Monotonic revision of authoritative state; advanced by each committed turn
/// that changed it.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct StateRevision(pub u64);

impl StateRevision {
    /// The next revision.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_ids_are_deterministic_and_attempt_scoped() {
        let request = LogicalRequestId::new("r1");
        let first = ExecutionAttemptId::new("a1");
        let retry = ExecutionAttemptId::new("a2");
        assert_eq!(request.command_item().as_str(), "r1:command");
        assert_eq!(first.call(2).as_str(), "a1#2");
        assert_eq!(first.item(0).as_str(), "a1:0");
        assert_eq!(
            TranscriptEventId::derive(&request, Some(&first), 3),
            TranscriptEventId::derive(&request, Some(&first), 3)
        );
        assert_ne!(
            TranscriptEventId::derive(&request, Some(&first), 3),
            TranscriptEventId::derive(&request, Some(&retry), 3)
        );
        assert_eq!(
            TranscriptEventId::derive(&request, None, 0).as_str(),
            "r1:-:0"
        );
    }

    #[test]
    fn fresh_ids_are_unique() {
        assert_ne!(LogicalRequestId::fresh(), LogicalRequestId::fresh());
        assert_ne!(ExecutionAttemptId::fresh(), ExecutionAttemptId::fresh());
    }
}
