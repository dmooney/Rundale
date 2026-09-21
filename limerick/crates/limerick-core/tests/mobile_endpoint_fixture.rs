#![cfg(feature = "mobile")]

use std::collections::BTreeMap;

use limerick_core::mobile::{
    EndpointIntentCandidate, EndpointRole, ExecutionAttemptId, LogicalRequestId, MobileSession,
    SessionId,
};

/// The Intent invocation the engine dispatches before any action is selected.
///
/// Published alongside the dialogue fixture so the two roles stay separately
/// reviewable: the dialogue contract and its clients are unchanged by #1993.
#[test]
fn production_intent_invocation_matches_published_fixture() {
    let mut session = MobileSession::open_new().expect("phase2 session");
    let result = session
        .submit(
            Some(LogicalRequestId::new("fixture-logical-request")),
            "ask Peig about the old church",
            None,
        )
        .expect("request accepted");
    let mut invocation = result.endpoint_invocation.expect("intent invocation");
    assert_eq!(invocation.role, EndpointRole::Intent);
    invocation.session_id = SessionId::new("fixture-session");
    invocation.attempt_id = ExecutionAttemptId::new("fixture-attempt");
    invocation.idempotency_key =
        "fixture-logical-request:fixture-attempt:interpretation".to_string();
    let actual = serde_json::to_value(invocation).expect("serialise invocation");
    let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/example-intent-invocation.json"
    )))
    .expect("published intent invocation fixture is valid JSON");
    assert_eq!(actual, expected);
}

#[test]
fn production_endpoint_invocation_matches_published_fixture() {
    let mut session = MobileSession::open_new().expect("phase2 session");
    let accepted = session
        .submit(
            Some(LogicalRequestId::new("fixture-logical-request")),
            "ask Peig about the old church",
            None,
        )
        .expect("request accepted");
    // Interpretation selects the action first; the dialogue invocation below
    // is the second stage of the same logical request.
    let intent = accepted
        .endpoint_invocation
        .clone()
        .expect("intent invocation");
    let result = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: serde_json::json!({"intent": "talk", "target": "Peig"}),
            metadata: BTreeMap::new(),
        })
        .expect("interpretation dispatches dialogue");
    let result_json = serde_json::to_value(&result).expect("serialise operation result");
    assert!(result_json["logicalRequestID"].is_string());
    assert!(result_json["attemptID"].is_string());
    assert!(result_json["endpointInvocation"]["sessionID"].is_string());
    assert!(result_json["endpointInvocation"]["logicalRequestID"].is_string());
    assert!(result_json["endpointInvocation"]["attemptID"].is_string());
    let snapshot_json = serde_json::to_value(session.snapshot()).expect("serialise snapshot");
    assert!(snapshot_json["sessionID"].is_string());
    assert!(snapshot_json["activeRequestID"].is_string());
    let mut invocation = result.endpoint_invocation.expect("endpoint invocation");
    invocation.session_id = SessionId::new("fixture-session");
    invocation.attempt_id = ExecutionAttemptId::new("fixture-attempt");
    invocation.idempotency_key = "fixture-logical-request:fixture-attempt:dialogue".to_string();
    let actual = serde_json::to_value(invocation).expect("serialise invocation");
    let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/example-engine-invocation.json"
    )))
    .expect("published invocation fixture is valid JSON");
    assert_eq!(actual, expected);
}

#[test]
fn shared_stream_fixture_has_v1_wire_shape_and_order() {
    let mut frames = Vec::new();
    for block in include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/fixtures/dialogue-v1.sse"
    ))
    .split("\n\n")
    {
        let mut event = None;
        let mut id = None;
        let mut data = None;
        for line in block.lines() {
            if line.starts_with(':') || line.is_empty() {
                continue;
            }
            if let Some(value) = line.strip_prefix("event: ") {
                event = Some(value);
            } else if let Some(value) = line.strip_prefix("id: ") {
                id = Some(value);
            } else if let Some(value) = line.strip_prefix("data: ") {
                data = Some(value);
            }
        }
        let Some(data) = data else { continue };
        frames.push((
            event.expect("event"),
            id.expect("id"),
            serde_json::from_str::<serde_json::Value>(data).unwrap(),
        ));
    }

    assert_eq!(frames.len(), 4);
    let mut dialogue = String::new();
    for (index, (event, id, frame)) in frames.iter().enumerate() {
        let sequence = (index + 1) as u64;
        assert_eq!(frame["contract_version"], 1);
        assert_eq!(frame["endpoint_version"], 1);
        assert_eq!(frame["request_id"], "request-fixture");
        assert_eq!(frame["attempt_id"], "attempt-fixture");
        assert_eq!(frame["invocation_id"], "invocation-fixture");
        assert_eq!(frame["sequence"], sequence);
        assert_eq!(frame["event_id"], format!("invocation-fixture:{sequence}"));
        assert_eq!(*id, format!("invocation-fixture:{sequence}"));
        assert_eq!(frame["type"], *event);
        assert_eq!(frame["terminal"], *event == "final");
        match *event {
            "progress" => assert_eq!(frame["text"], "Peig listens."),
            "text_delta" => dialogue.push_str(frame["text"].as_str().expect("delta text")),
            "final" => assert_eq!(
                frame["output"]["dialogue"],
                "The rain keeps the old road quiet. 🌧"
            ),
            other => panic!("unexpected event {other}"),
        }
    }
    assert_eq!(dialogue, "The rain keeps the old road quiet. ");
}
