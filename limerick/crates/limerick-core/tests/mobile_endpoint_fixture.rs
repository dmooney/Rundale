#![cfg(feature = "mobile")]

use limerick_core::input::parse_intent_local;
use limerick_core::mobile::{
    EndpointCandidate, ExecutionAttemptId, LogicalRequestId, MobileSession,
    ResponseTerminalOutcome, SemanticEventKind, SessionId, StateRevision,
};

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

#[test]
fn inferred_mobile_travel_commits_the_selected_destination_once() {
    let input = "Could you take me where letters arrive?";
    assert!(parse_intent_local(input).is_none());
    let mut session = MobileSession::open_new().unwrap();
    let before = session.world().player_location;
    let accepted = session.submit(None, input, None).unwrap();
    let request = accepted.logical_request_id.unwrap();
    let invocation = accepted.endpoint_invocation.unwrap();
    assert_eq!(invocation.role, "intent");
    assert_eq!(session.world().player_location, before);
    assert_eq!(session.state_revision(), StateRevision::new(0));
    let candidate = EndpointCandidate {
        attempt_id: invocation.attempt_id.clone(),
        base_revision: invocation.base_revision,
        dialogue: String::new(),
        intent_output: Some(serde_json::json!({
            "intent": "move", "target": "The Letter Office", "dialogue": null
        })),
        metadata: Default::default(),
        structured: true,
    };
    let completed = session
        .receive_candidate_for_role(Some("intent"), candidate.clone())
        .unwrap();
    assert_eq!(
        completed.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(session.snapshot().read_model.scene.id, "letter-office");
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert!(
        completed
            .events
            .iter()
            .any(|event| event.kind == SemanticEventKind::CommandInterpreted)
    );
    assert!(
        !session
            .receive_candidate_for_role(Some("intent"), candidate)
            .unwrap()
            .accepted
    );
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(
        session
            .snapshot()
            .requests
            .iter()
            .find(|record| record.id == request)
            .unwrap()
            .terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
}

#[test]
fn inferred_dialogue_requires_a_second_role_and_ignores_late_intent_failure() {
    let input = "What can you tell me of this parish?";
    assert!(parse_intent_local(input).is_none());
    let mut session = MobileSession::open_new().unwrap();
    let intent = session
        .submit(None, input, None)
        .unwrap()
        .endpoint_invocation
        .unwrap();
    assert_eq!(intent.role, "intent");
    assert!(intent.authored_facts.is_empty());
    assert!(intent.recent_conversation.is_empty());
    let next = session
        .receive_candidate_for_role(
            Some("intent"),
            EndpointCandidate {
                attempt_id: intent.attempt_id.clone(),
                base_revision: intent.base_revision,
                dialogue: String::new(),
                intent_output: Some(
                    serde_json::json!({"intent":"talk", "target":"Peig", "dialogue":"Tell me of this parish"}),
                ),
                metadata: Default::default(),
                structured: true,
            },
        )
        .unwrap();
    assert!(
        next.events
            .iter()
            .any(|event| event.kind == SemanticEventKind::CommandInterpreted)
    );
    assert_eq!(session.state_revision(), StateRevision::new(0));
    let dialogue = next.endpoint_invocation.unwrap();
    assert_eq!(dialogue.role, "npc_dialogue");
    assert_eq!(dialogue.speaker.id, "npc-peig");
    assert_eq!(dialogue.player_input, "Tell me of this parish");
    assert_ne!(intent.idempotency_key, dialogue.idempotency_key);
    let stale = session
        .receive_failure_for_role(
            &intent.attempt_id,
            intent.base_revision,
            Some("intent"),
            limerick_core::mobile::EndpointFailureKind::Transport,
            "late failure".to_string(),
        )
        .unwrap();
    assert!(!stale.accepted);
    assert_eq!(session.state_revision(), StateRevision::new(0));
    let completed = session
        .receive_candidate_for_role(
            Some("npc_dialogue"),
            EndpointCandidate {
                attempt_id: dialogue.attempt_id.clone(),
                base_revision: dialogue.base_revision,
                dialogue: "A low stone wall borders the road here.".to_string(),
                intent_output: None,
                metadata: Default::default(),
                structured: true,
            },
        )
        .unwrap();
    assert_eq!(
        completed.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(session.world().conversation_log.len(), 1);
}

#[test]
fn stopped_intent_cannot_apply_and_retry_preserves_original_text() {
    let input = "Could you take me where letters arrive?";
    let mut session = MobileSession::open_new().unwrap();
    let first = session.submit(None, input, None).unwrap();
    let request = first.logical_request_id.unwrap();
    let invocation = first.endpoint_invocation.unwrap();
    session.stop(&invocation.attempt_id).unwrap();
    let stale = session.receive_candidate_for_role(Some("intent"), EndpointCandidate {
        attempt_id: invocation.attempt_id.clone(), base_revision: invocation.base_revision,
        dialogue: String::new(),
        intent_output: Some(serde_json::json!({"intent":"move", "target":"The Letter Office", "dialogue":null})),
        metadata: Default::default(), structured: true,
    }).unwrap();
    assert!(!stale.accepted);
    assert_eq!(session.state_revision(), StateRevision::new(0));
    let retry = session
        .retry(&request)
        .unwrap()
        .endpoint_invocation
        .unwrap();
    assert_eq!(retry.role, "intent");
    assert_eq!(retry.player_input, input);
    assert_ne!(retry.attempt_id, invocation.attempt_id);
    let resumed = MobileSession::open_resume(session.save()).unwrap();
    assert_eq!(resumed.state_revision(), StateRevision::new(0));
    assert_eq!(
        resumed.snapshot().requests.last().unwrap().terminal_outcome,
        Some(ResponseTerminalOutcome::Interrupted)
    );
}

#[test]
fn malformed_or_unsupported_intent_never_becomes_dialogue() {
    for output in [
        serde_json::json!({"intent":"teleport", "target":"The Letter Office"}),
        serde_json::json!({"intent":"unknown", "target":null}),
        serde_json::json!({"target":"The Letter Office"}),
    ] {
        let mut session = MobileSession::open_new().unwrap();
        let invocation = session
            .submit(None, "Could you take me where letters arrive?", None)
            .unwrap()
            .endpoint_invocation
            .unwrap();
        let result = session
            .receive_candidate_for_role(
                Some("intent"),
                EndpointCandidate {
                    attempt_id: invocation.attempt_id,
                    base_revision: invocation.base_revision,
                    dialogue: String::new(),
                    intent_output: Some(output),
                    metadata: Default::default(),
                    structured: true,
                },
            )
            .unwrap();
        assert_eq!(
            result.terminal_outcome,
            Some(ResponseTerminalOutcome::Failed)
        );
        assert!(result.endpoint_invocation.is_none());
        assert_eq!(session.state_revision(), StateRevision::new(0));
        assert_eq!(session.snapshot().read_model.scene.id, "kilteevan-village");
    }
}
