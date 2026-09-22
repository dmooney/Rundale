//! Production-path tests for shared intent interpretation on mobile (#1993).
//!
//! Each test drives the real `MobileSession` submission path. Model output is
//! supplied through the same `receive_intent_candidate` / `receive_candidate`
//! callbacks the native Endpoint transport uses; nothing mutates the session
//! state being asserted directly.

use super::*;
use serde_json::json;

/// Verified against `limerick_input::interpret_locally`: requires the Intent role.
const INFERRED_MOVE: &str = "Let's make for the Letter Office";
const INFERRED_TALK: &str = "Would Peig know anything of the post today?";

fn session() -> MobileSession {
    MobileSession::open_new().expect("canonical session")
}

fn intent_of(result: &MobileOperationResult) -> IntentInvocation {
    result
        .intent_invocation
        .clone()
        .expect("input outside the local parser must request the Intent role")
}

fn deliver(
    session: &mut MobileSession,
    invocation: &IntentInvocation,
    output: serde_json::Value,
) -> MobileOperationResult {
    session
        .receive_intent_candidate(IntentCandidate {
            attempt_id: invocation.attempt_id.clone(),
            base_revision: invocation.base_revision,
            output,
            structured: true,
        })
        .expect("intent candidate accepted by the lane")
}

fn request(session: &MobileSession, id: &LogicalRequestId) -> RequestRecord {
    session
        .snapshot()
        .requests
        .into_iter()
        .find(|request| &request.id == id)
        .expect("request exists")
}

fn arrivals(session: &MobileSession) -> usize {
    session
        .snapshot()
        .events
        .iter()
        .filter(|event| event.kind == SemanticEventKind::SceneChanged && event.accepted)
        .count()
}

#[test]
fn chosen_phrases_require_inference_in_the_shared_parser() {
    for phrase in [INFERRED_MOVE, INFERRED_TALK] {
        assert!(
            matches!(
                limerick_input::interpret_locally(phrase),
                LocalInterpretation::RequiresInference
            ),
            "{phrase:?} is recognised locally and would not prove the Intent path"
        );
    }
}

/// Regression for #1993: before the fix this input became an `npc_dialogue`
/// request to Peig and the player never moved.
#[test]
fn inferred_movement_requests_intent_then_travels_exactly_once() {
    let mut session = session();
    let submitted = session.submit(None, INFERRED_MOVE, None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();

    assert!(
        submitted.endpoint_invocation.is_none(),
        "dialogue must not be requested before interpretation"
    );
    let invocation = intent_of(&submitted);
    assert_eq!(invocation.role, INTENT_ROLE);
    assert_eq!(invocation.player_input, INFERRED_MOVE);
    assert_eq!(invocation.logical_request_id, request_id);
    assert_eq!(
        invocation.idempotency_key,
        format!(
            "{}:{}:intent",
            request_id.raw_value, invocation.attempt_id.raw_value
        )
    );
    assert_eq!(session.state_revision(), StateRevision::new(0));
    assert_eq!(session.world().player_location, LocationId(1));
    let pending = request(&session, &request_id);
    assert_eq!(pending.phase, RequestPhase::Executing);
    assert_eq!(
        pending.current_attempt().unwrap().stage,
        AttemptStage::Interpretation
    );
    assert!(matches!(
        session.take_pending_endpoint(),
        Some(PendingEndpointInvocation::Intent(pending)) if pending == invocation
    ));
    assert_eq!(session.take_pending_invocation(), None);

    let output = json!({"intent": "move", "target": "the Letter Office", "dialogue": null, "atmosphere": null});
    let travelled = deliver(&mut session, &invocation, output.clone());
    assert!(travelled.endpoint_invocation.is_none());
    assert!(travelled.intent_invocation.is_none());
    assert_eq!(
        travelled.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(session.snapshot().read_model.scene.id, "letter-office");
    assert_eq!(session.snapshot().active_request_id, None);
    let receipt = travelled
        .events
        .iter()
        .find(|event| event.kind == SemanticEventKind::CommandInterpreted)
        .expect("interpretation receipt");
    assert_eq!(receipt.content.as_deref(), Some("Travel to Letter Office."));
    assert_eq!(receipt.metadata["interpretationSource"], "inferred");
    assert_eq!(receipt.metadata["role"], INTENT_ROLE);
    assert_eq!(receipt.metadata["entityID"], "letter-office");
    assert_eq!(receipt.state_revision, Some(StateRevision::new(1)));
    let scene = travelled
        .events
        .iter()
        .find(|event| event.kind == SemanticEventKind::SceneChanged)
        .expect("scene transition");
    assert_eq!(scene.metadata["sceneID"], "letter-office");
    assert!(
        travelled.events.iter().all(
            |event| event.logical_request_id.as_ref() == Some(&request_id)
                && event.attempt_id.as_ref() == Some(&invocation.attempt_id)
        )
    );
    let settled = request(&session, &request_id);
    assert_eq!(
        settled.interpretation.as_ref().unwrap().intent.as_deref(),
        Some("move")
    );
    assert_eq!(
        settled.committed_state_revision,
        Some(StateRevision::new(1))
    );

    // Duplicate delivery of the same result must not move the player again.
    let duplicate = deliver(&mut session, &invocation, output);
    assert!(duplicate.ignored);
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(arrivals(&session), 1);
}

#[test]
fn inferred_dialogue_targets_the_available_npc_after_interpretation() {
    let mut session = session();
    let submitted = session.submit(None, INFERRED_TALK, None).unwrap();
    assert!(submitted.endpoint_invocation.is_none());
    let intent = intent_of(&submitted);

    let interpreted = deliver(
        &mut session,
        &intent,
        json!({"intent": "talk", "target": "Peig", "dialogue": INFERRED_TALK}),
    );
    assert!(interpreted.intent_invocation.is_none());
    let dialogue = interpreted
        .endpoint_invocation
        .clone()
        .expect("dialogue follows a talk interpretation");
    assert_eq!(dialogue.role, DIALOGUE_ROLE);
    assert_eq!(dialogue.speaker.id, "npc-peig");
    assert_eq!(dialogue.attempt_id, intent.attempt_id);
    assert_ne!(dialogue.idempotency_key, intent.idempotency_key);
    let receipt = &interpreted.events[0];
    assert_eq!(receipt.kind, SemanticEventKind::CommandInterpreted);
    assert_eq!(
        receipt.content.as_deref(),
        Some("Speak with Peig Hannigan.")
    );
    assert_eq!(receipt.metadata["entityID"], "npc-peig");
    assert!(matches!(
        session.take_pending_endpoint(),
        Some(PendingEndpointInvocation::Dialogue(pending)) if *pending == dialogue
    ));

    // A replayed Intent result during dialogue is stale and changes nothing.
    let replay = deliver(
        &mut session,
        &intent,
        json!({"intent": "move", "target": "Letter Office"}),
    );
    assert!(replay.ignored);
    assert_eq!(session.world().player_location, LocationId(1));

    let completed = session
        .receive_candidate(EndpointCandidate {
            attempt_id: dialogue.attempt_id.clone(),
            base_revision: dialogue.base_revision,
            dialogue: "The post came in this morning, and I sorted it myself.".to_string(),
            metadata: BTreeMap::new(),
            structured: true,
        })
        .unwrap();
    assert_eq!(
        completed.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(
        session
            .world()
            .conversation_log
            .recent_at(LocationId(1), 5)
            .len(),
        1
    );
}

#[test]
fn ordinary_dialogue_without_a_named_target_reaches_the_person_present() {
    let mut session = session();
    let submitted = session
        .submit(
            None,
            "Well I’m looking for work and a place to stay. Michael said maybe you could direct me.",
            None,
        )
        .unwrap();
    let interpreted = deliver(
        &mut session,
        &intent_of(&submitted),
        json!({"intent": "talk", "target": null, "dialogue": "I'm looking for work"}),
    );
    assert_eq!(
        interpreted.endpoint_invocation.unwrap().speaker.id,
        "npc-peig"
    );
}

#[test]
fn explicit_address_and_local_talk_skip_the_intent_role() {
    for text in [
        "ask Peig about the wall",
        "Peig, is there post?",
        "I came from Roscommon",
    ] {
        let mut session = session();
        let result = session.submit(None, text, None).unwrap();
        assert!(result.intent_invocation.is_none(), "{text:?}");
        assert_eq!(
            result.endpoint_invocation.expect(text).speaker.id,
            "npc-peig"
        );
    }
}

#[test]
fn deterministic_and_locally_parsed_actions_make_no_endpoint_request() {
    let mut session = session();
    for text in [
        "/look",
        "look",
        "where am i",
        "/people",
        "/exits",
        "/help",
        "pick up the stone",
        "go east",
        "/go Kilteevan Village",
    ] {
        let result = session.submit(None, text, None).unwrap();
        assert!(
            result.intent_invocation.is_none(),
            "{text:?} requested Intent"
        );
        assert!(
            result.endpoint_invocation.is_none(),
            "{text:?} requested Dialogue"
        );
        assert_eq!(
            result.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded),
            "{text:?}"
        );
        assert_eq!(session.snapshot().active_request_id, None);
    }
    assert_eq!(session.snapshot().read_model.scene.id, "kilteevan-village");
    assert_eq!(session.state_revision(), StateRevision::new(2));
}

#[test]
fn inferred_talk_to_an_absent_person_is_reported_without_dialogue() {
    let mut session = session();
    let submitted = session
        .submit(
            None,
            "Might I have a word with Róisín about the yarn?",
            None,
        )
        .unwrap();
    let result = deliver(
        &mut session,
        &intent_of(&submitted),
        json!({"intent": "talk", "target": "Róisín", "dialogue": "about the yarn"}),
    );
    assert!(result.endpoint_invocation.is_none());
    assert!(result.events.iter().any(|event| {
        event.kind == SemanticEventKind::ActionResult
            && event.content.as_deref() == Some("Róisín Connolly is not here.")
    }));
    assert_eq!(session.state_revision(), StateRevision::new(0));
    assert_eq!(session.snapshot().active_request_id, None);
}

#[test]
fn invalid_malformed_or_unsupported_results_commit_nothing() {
    for (output, structured, error_kind) in [
        (
            json!({"intent": "fly", "target": "the moon"}),
            true,
            "invalid_interpretation",
        ),
        (
            json!({"target": "Letter Office"}),
            true,
            "invalid_interpretation",
        ),
        (
            json!("move to the Letter Office"),
            true,
            "invalid_interpretation",
        ),
        (
            json!({"intent": "move", "target": "Letter Office", "extra": 1}),
            true,
            "invalid_interpretation",
        ),
        (
            json!({"intent": "move", "target": "Letter Office"}),
            false,
            "protocol",
        ),
    ] {
        let mut session = session();
        let submitted = session.submit(None, INFERRED_MOVE, None).unwrap();
        let request_id = submitted.logical_request_id.clone().unwrap();
        let invocation = intent_of(&submitted);
        let failed = session
            .receive_intent_candidate(IntentCandidate {
                attempt_id: invocation.attempt_id.clone(),
                base_revision: invocation.base_revision,
                output: output.clone(),
                structured,
            })
            .unwrap();
        assert_eq!(
            failed.terminal_outcome,
            Some(ResponseTerminalOutcome::Failed),
            "{output}"
        );
        assert!(failed.endpoint_invocation.is_none());
        let terminal = failed.events.last().unwrap();
        assert_eq!(terminal.metadata["errorKind"], error_kind);
        assert_eq!(session.state_revision(), StateRevision::new(0));
        assert_eq!(session.world().player_location, LocationId(1));
        assert!(
            request(&session, &request_id)
                .interpretation
                .unwrap()
                .is_pending()
        );

        // Retry keeps the original input and interprets it again.
        let retried = session.retry(&request_id).unwrap();
        let again = intent_of(&retried);
        assert!(retried.endpoint_invocation.is_none());
        assert_eq!(again.player_input, INFERRED_MOVE);
        assert_ne!(again.attempt_id, invocation.attempt_id);
        let travelled = deliver(
            &mut session,
            &again,
            json!({"intent": "move", "target": "Letter Office"}),
        );
        assert_eq!(
            travelled.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded)
        );
        assert_eq!(session.state_revision(), StateRevision::new(1));
    }

    for (output, expected) in [
        (
            json!({"intent": "interact", "target": "the cart"}),
            UNSUPPORTED_ACTION_TEXT,
        ),
        (
            json!({"intent": "examine", "target": "the cart"}),
            UNSUPPORTED_ACTION_TEXT,
        ),
        (
            json!({"intent": "move", "target": "the moon"}),
            "You cannot find a route to the moon.",
        ),
    ] {
        let mut session = session();
        let submitted = session
            .submit(None, "shove the cart toward the moon", None)
            .unwrap();
        let result = deliver(&mut session, &intent_of(&submitted), output.clone());
        assert!(result.endpoint_invocation.is_none(), "{output}");
        assert!(
            result
                .events
                .iter()
                .all(|event| event.kind != SemanticEventKind::CommandInterpreted),
            "{output}: no receipt for an action that does not execute"
        );
        assert!(result.events.iter().any(|event| {
            event.kind == SemanticEventKind::ActionResult
                && event.content.as_deref() == Some(expected)
        }));
        assert_eq!(session.state_revision(), StateRevision::new(0), "{output}");
        assert_eq!(session.world().player_location, LocationId(1));
    }
}

#[test]
fn move_without_a_destination_clarifies_and_resumes_the_original_request() {
    let mut session = session();
    let submitted = session.submit(None, "Let's be off, so", None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();
    let clarified = deliver(
        &mut session,
        &intent_of(&submitted),
        json!({"intent": "move", "target": null}),
    );
    let prompt = clarified.events[0]
        .clarification
        .clone()
        .expect("destination clarification");
    let mut labels: Vec<_> = prompt
        .choices
        .iter()
        .map(|choice| choice.label.as_str())
        .collect();
    labels.sort();
    assert_eq!(labels, ["Connolly Cottage", "Letter Office"]);
    assert_eq!(session.state_revision(), StateRevision::new(0));
    assert_eq!(
        request(&session, &request_id).phase,
        RequestPhase::AwaitingClarification
    );
    assert_eq!(session.take_pending_endpoint(), None);

    let office = prompt
        .choices
        .iter()
        .find(|choice| choice.entity_id.as_deref() == Some("letter-office"))
        .unwrap();
    let travelled = session
        .answer_clarification(&request_id, &office.id)
        .unwrap();
    assert_eq!(
        travelled.events[0].kind,
        SemanticEventKind::ClarificationSelected
    );
    assert_eq!(travelled.logical_request_id, Some(request_id.clone()));
    assert_eq!(session.snapshot().read_model.scene.id, "letter-office");
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(
        request(&session, &request_id).terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
}

#[test]
fn stop_during_interpretation_never_executes_the_action_later() {
    let mut session = session();
    let submitted = session.submit(None, INFERRED_MOVE, None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();
    let invocation = intent_of(&submitted);
    let stopped = session.stop(&invocation.attempt_id).unwrap();
    assert_eq!(
        stopped.terminal_outcome,
        Some(ResponseTerminalOutcome::Cancelled)
    );

    let late = deliver(
        &mut session,
        &invocation,
        json!({"intent": "move", "target": "Letter Office"}),
    );
    assert!(late.ignored);
    assert_eq!(session.world().player_location, LocationId(1));
    assert_eq!(session.state_revision(), StateRevision::new(0));

    let retried = session.retry(&request_id).unwrap();
    let again = intent_of(&retried);
    // The stale first attempt stays inert after the retry starts.
    assert!(
        deliver(
            &mut session,
            &invocation,
            json!({"intent": "move", "target": "Letter Office"})
        )
        .ignored
    );
    // A result for the wrong revision is also stale.
    let stale = session
        .receive_intent_candidate(IntentCandidate {
            attempt_id: again.attempt_id.clone(),
            base_revision: StateRevision::new(7),
            output: json!({"intent": "move", "target": "Letter Office"}),
            structured: true,
        })
        .unwrap();
    assert!(stale.ignored);
    deliver(
        &mut session,
        &again,
        json!({"intent": "move", "target": "Letter Office"}),
    );
    assert_eq!(session.state_revision(), StateRevision::new(1));
    assert_eq!(arrivals(&session), 1);
}

#[test]
fn failure_after_interpretation_keeps_state_and_retries_dialogue_only() {
    let mut session = session();
    let submitted = session.submit(None, INFERRED_TALK, None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();
    let dialogue = deliver(
        &mut session,
        &intent_of(&submitted),
        json!({"intent": "talk", "target": "Peig"}),
    )
    .endpoint_invocation
    .unwrap();
    let failed = session
        .receive_failure(
            &dialogue.attempt_id,
            dialogue.base_revision,
            EndpointFailureKind::Transport,
            String::new(),
        )
        .unwrap();
    assert_eq!(
        failed.terminal_outcome,
        Some(ResponseTerminalOutcome::Failed)
    );
    assert_eq!(session.state_revision(), StateRevision::new(0));

    let retried = session.retry(&request_id).unwrap();
    assert!(
        retried.intent_invocation.is_none(),
        "an accepted interpretation is not re-requested"
    );
    let again = retried.endpoint_invocation.unwrap();
    assert_eq!(again.speaker.id, "npc-peig");
    assert_eq!(again.player_input, INFERRED_TALK);
}

#[test]
fn failed_interpretation_is_terminal_and_retryable() {
    let mut session = session();
    let submitted = session.submit(None, INFERRED_MOVE, None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();
    let invocation = intent_of(&submitted);
    let failed = session
        .receive_failure(
            &invocation.attempt_id,
            invocation.base_revision,
            EndpointFailureKind::Transport,
            "offline".to_string(),
        )
        .unwrap();
    assert_eq!(
        failed.terminal_outcome,
        Some(ResponseTerminalOutcome::Failed)
    );
    assert!(
        deliver(
            &mut session,
            &invocation,
            json!({"intent": "move", "target": "Letter Office"})
        )
        .ignored
    );
    assert_eq!(session.world().player_location, LocationId(1));
    assert_eq!(
        intent_of(&session.retry(&request_id).unwrap()).player_input,
        INFERRED_MOVE
    );
}

#[test]
fn relaunch_during_interpretation_recovers_without_replaying_the_action() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interpretation.sqlite");
    let mut session = MobileSession::open_new_sqlite(&path).unwrap();
    let submitted = session.submit(None, INFERRED_MOVE, None).unwrap();
    let request_id = submitted.logical_request_id.clone().unwrap();
    let invocation = intent_of(&submitted);
    drop(session);

    let mut resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
    assert_eq!(resumed.save().format_version, MOBILE_SAVE_FORMAT_VERSION);
    let recovered = request(&resumed, &request_id);
    assert_eq!(recovered.phase, RequestPhase::Interrupted);
    assert!(recovered.interpretation.unwrap().is_pending());
    assert_eq!(resumed.take_pending_endpoint(), None);
    assert!(
        deliver(
            &mut resumed,
            &invocation,
            json!({"intent": "move", "target": "Letter Office"})
        )
        .ignored
    );
    assert_eq!(resumed.world().player_location, LocationId(1));

    let retried = resumed.retry(&request_id).unwrap();
    let again = intent_of(&retried);
    deliver(
        &mut resumed,
        &again,
        json!({"intent": "move", "target": "Letter Office"}),
    );
    drop(resumed);

    let reopened = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
    assert_eq!(reopened.snapshot().read_model.scene.id, "letter-office");
    assert_eq!(reopened.state_revision(), StateRevision::new(1));
    assert_eq!(arrivals(&reopened), 1);
    assert_eq!(
        request(&reopened, &request_id).terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
}

#[test]
fn version_1_0_saves_without_interpretation_fields_still_resume() {
    let mut session = session();
    let started = session
        .submit(None, "ask Peig about the wall", None)
        .unwrap();
    let mut value = serde_json::to_value(session.save()).unwrap();
    value["formatVersion"] = json!({"major": 1, "minor": 0});
    for request in value["requests"].as_array_mut().unwrap() {
        let request = request.as_object_mut().unwrap();
        request.remove("interpretation");
        for attempt in request["attempts"].as_array_mut().unwrap() {
            attempt.as_object_mut().unwrap().remove("stage");
        }
    }
    let save: MobileSave = serde_json::from_value(value).unwrap();
    let mut resumed = MobileSession::open_resume(save).unwrap();
    let id = started.logical_request_id.unwrap();
    let record = request(&resumed, &id);
    assert_eq!(record.phase, RequestPhase::Interrupted);
    assert_eq!(
        record.current_attempt().unwrap().stage,
        AttemptStage::Dialogue
    );
    let retried = resumed.retry(&id).unwrap();
    assert!(retried.intent_invocation.is_none());
    assert_eq!(retried.endpoint_invocation.unwrap().speaker.id, "npc-peig");
}

#[test]
fn intent_invocation_matches_the_published_intent_contract_fixture() {
    let mut session = session();
    let submitted = session
        .submit(
            Some(LogicalRequestId::new("fixture-logical-request")),
            INFERRED_MOVE,
            None,
        )
        .unwrap();
    let mut invocation = intent_of(&submitted);
    invocation.session_id = SessionId::new("fixture-session");
    invocation.attempt_id = ExecutionAttemptId::new("fixture-attempt");
    invocation.idempotency_key = "fixture-logical-request:fixture-attempt:intent".to_string();
    let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/example-intent-invocation.json"
    )))
    .unwrap();
    assert_eq!(serde_json::to_value(invocation).unwrap(), expected);
}
