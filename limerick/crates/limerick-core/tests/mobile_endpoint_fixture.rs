#![cfg(feature = "mobile")]

use limerick_core::mobile::{ExecutionAttemptId, LogicalRequestId, MobileSession, SessionId};

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

/// The Intent Endpoint definition must stay bound to the engine: its
/// instructions are the shared `limerick-input` Intent prompt, its input is
/// exactly the engine's `IntentInvocation`, and every output it permits is
/// accepted by the shared validator (#1993).
#[test]
fn intent_endpoint_definition_is_bound_to_the_engine_contract() {
    let definition: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/rundale-intent-v1.json"
    )))
    .expect("intent definition is valid JSON");
    assert_eq!(
        definition["instructions"].as_str(),
        Some(limerick_core::input::intent_system_prompt()),
        "regenerate rundale-intent-v1.json instructions from the Rust Intent prompt"
    );

    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/example-intent-invocation.json"
    )))
    .expect("intent fixture is valid JSON");
    let mut fixture_keys: Vec<&str> = fixture
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fixture_keys.sort_unstable();
    let mut required: Vec<&str> = definition["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    required.sort_unstable();
    assert_eq!(fixture_keys, required);
    assert_eq!(
        definition["inputSchema"]["properties"]["role"]["enum"],
        serde_json::json!([limerick_core::mobile::INTENT_ROLE])
    );

    let output = &definition["outputSchema"];
    assert_eq!(output["additionalProperties"], false);
    for intent in output["properties"]["intent"]["enum"].as_array().unwrap() {
        let value = serde_json::json!({
            "intent": intent,
            "target": null,
            "dialogue": null,
            "atmosphere": null
        });
        limerick_core::input::intent_from_structured_output(&value, "look")
            .unwrap_or_else(|error| panic!("{intent} rejected: {error}"));
    }
    assert_eq!(
        output["properties"]["target"]["maxLength"],
        limerick_core::input::MAX_INTENT_FIELD_CHARS
    );
    assert!(
        definition["inferenceConfig"]["streaming"]["textField"].is_string(),
        "the mobile /stream route requires a text projection"
    );
}
