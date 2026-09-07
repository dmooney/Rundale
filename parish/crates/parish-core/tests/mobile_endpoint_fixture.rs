#![cfg(feature = "mobile")]

use parish_core::mobile::{ExecutionAttemptId, LogicalRequestId, MobileSession, SessionId};

#[test]
fn production_endpoint_invocation_matches_published_fixture() {
    let mut session = MobileSession::open_new().expect("phase2 session");
    let result = session
        .submit(
            Some(LogicalRequestId::new("fixture-logical-request")),
            "ask Peig about the old church",
            None,
        )
        .expect("request accepted");
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
    invocation.idempotency_key = "fixture-logical-request:fixture-attempt".to_string();
    let actual = serde_json::to_value(invocation).expect("serialise invocation");
    let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/example-engine-invocation.json"
    )))
    .expect("published invocation fixture is valid JSON");
    assert_eq!(actual, expected);
}
