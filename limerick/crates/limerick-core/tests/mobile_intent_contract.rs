#![cfg(feature = "mobile")]
//! Cross-runtime interpretation contract for the mobile adapter (#1993).
//!
//! These tests drive the production `MobileSession` submission path — the same
//! entry point the Swift host reaches through the FFI — and assert the
//! *selected action and resulting authoritative state*, not the presence of
//! plausible dialogue text. The Endpoint is represented by its typed
//! invocation and callback DTOs, so a test can control model output without
//! bypassing interpretation routing or mutating the state being proved.

use std::collections::BTreeMap;

use limerick_core::mobile::{
    EndpointCandidate, EndpointFailureKind, EndpointFrame, EndpointIntentCandidate,
    EndpointInvocation, EndpointRole, ExecutionAttemptId, LogicalRequestId, MobileOperationResult,
    MobileSession, RequestPhase, RequestStage, ResponseTerminalOutcome, SemanticEventKind,
    StateRevision, StreamUpdate,
};
use limerick_types::LocationId;

const VILLAGE: LocationId = LocationId(1);
const LETTER_OFFICE: LocationId = LocationId(4);
const COTTAGE: LocationId = LocationId(13);

/// A phrase the shared local parser does not recognise, so it genuinely needs
/// the Intent Endpoint. Asserted rather than assumed: a phrase the local
/// parser already handles would silently stop proving the inferred path.
const INFERRED_MOVE: &str = "Off to the letter office";
const INFERRED_TALK: &str = "Any word from beyond the parish?";
const INFERRED_LOOK: &str = "look at the village";

fn session() -> MobileSession {
    MobileSession::open_new().expect("canonical mobile session")
}

fn payload(intent: &str, target: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "intent": intent,
        "target": target,
        "dialogue": serde_json::Value::Null,
        "atmosphere": serde_json::Value::Null,
    })
}

/// The invocation an operation handed to the platform, with its role checked.
fn invocation(result: &MobileOperationResult, role: EndpointRole) -> EndpointInvocation {
    let invocation = result
        .endpoint_invocation
        .clone()
        .unwrap_or_else(|| panic!("expected a {} invocation", role.wire_value()));
    assert_eq!(
        invocation.role, role,
        "the engine dispatched the wrong Endpoint role"
    );
    invocation
}

fn interpret(
    session: &mut MobileSession,
    accepted: &MobileOperationResult,
    body: serde_json::Value,
) -> MobileOperationResult {
    let intent = invocation(accepted, EndpointRole::Intent);
    session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: body,
            metadata: BTreeMap::new(),
        })
        .expect("interpretation dispatches an action")
}

fn receipt(result: &MobileOperationResult) -> Option<&limerick_core::mobile::SemanticEvent> {
    result
        .events
        .iter()
        .find(|event| event.kind == SemanticEventKind::CommandInterpreted)
}

// ─── The captured regression ────────────────────────────────────────────────

/// Regression for #1993, captured on the production mobile submission path.
///
/// On the audited revision this input produced an `npc_dialogue` invocation
/// and left `player_location` at Kilteevan Village: the intended action was
/// never interpreted, let alone executed. The assertion below is on
/// authoritative state, not on dialogue text.
#[test]
fn inference_requiring_input_reaches_intent_and_executes_the_action() {
    assert!(
        limerick_input::parse_intent_local(INFERRED_MOVE).is_none(),
        "the regression phrase must genuinely require inferred intent"
    );

    for phrase in [INFERRED_MOVE, INFERRED_TALK, INFERRED_LOOK] {
        assert!(
            limerick_input::parse_intent_local(phrase).is_none(),
            "{phrase:?} must genuinely require inferred intent"
        );
    }

    let mut session = session();
    assert_eq!(session.world().player_location, VILLAGE);

    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    assert_eq!(intent.player_input, INFERRED_MOVE);
    assert!(
        intent.speaker.is_none(),
        "interpretation must not pre-select a speaker"
    );
    assert_eq!(
        session.snapshot().requests[0].phase,
        RequestPhase::Interpreting
    );

    let dispatched = interpret(
        &mut session,
        &accepted,
        payload("move", Some("Letter Office")),
    );
    assert!(
        dispatched.endpoint_invocation.is_none(),
        "movement executes on device; it must not become a dialogue request"
    );
    assert_eq!(session.world().player_location, LETTER_OFFICE);
    assert_eq!(
        dispatched.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
}

// ─── Shared interpretation semantics ────────────────────────────────────────

/// The mobile adapter interprets an Endpoint payload with the same shared
/// `limerick-input` semantics the desktop client uses, and routes each typed
/// result to the action that result denotes.
#[test]
fn shared_intent_semantics_select_equivalent_actions_on_both_adapters() {
    // Each case is the typed result both adapters derive from one payload for
    // one raw input — including the shared #1276 guard that downgrades a
    // spurious `look` on conversational text.
    for (raw, body, expected) in [
        (
            INFERRED_MOVE,
            payload("move", Some("Letter Office")),
            "move",
        ),
        (INFERRED_TALK, payload("talk", Some("Peig")), "talk"),
        (INFERRED_LOOK, payload("look", None), "look"),
        (INFERRED_TALK, payload("look", None), "unknown"),
        (
            INFERRED_TALK,
            payload("interact", Some("the stone")),
            "interact",
        ),
        (INFERRED_TALK, payload("unknown", None), "unknown"),
    ] {
        let parsed: limerick_input::IntentPayload =
            serde_json::from_value(body.clone()).expect("contract payload");
        let desktop = limerick_input::interpret_intent_payload(raw, parsed);
        assert_eq!(
            serde_json::to_value(&desktop.intent).unwrap(),
            serde_json::Value::String(expected.to_string()),
            "the shared validator is the single source of intent semantics for {raw:?}"
        );
    }

    // Move → travel, on device.
    let mut moving = session();
    let accepted = moving.submit(None, INFERRED_MOVE, None).expect("accepted");
    interpret(
        &mut moving,
        &accepted,
        payload("move", Some("Letter Office")),
    );
    assert_eq!(moving.world().player_location, LETTER_OFFICE);

    // Look → the deterministic scene text, with no further inference.
    let mut looking = session();
    let accepted = looking.submit(None, INFERRED_LOOK, None).expect("accepted");
    let dispatched = interpret(&mut looking, &accepted, payload("look", None));
    assert!(dispatched.endpoint_invocation.is_none());
    assert!(dispatched.events.iter().any(|event| {
        event.kind == SemanticEventKind::ActionResult
            && event.metadata.get("capability") == Some(&"look".to_string())
    }));

    // Talk → dialogue generation, and only then.
    let mut talking = session();
    let accepted = talking.submit(None, INFERRED_TALK, None).expect("accepted");
    let dispatched = interpret(&mut talking, &accepted, payload("talk", Some("Peig")));
    let dialogue = invocation(&dispatched, EndpointRole::NpcDialogue);
    assert_eq!(dialogue.speaker.expect("resolved speaker").id, "npc-peig");

    // Unknown is the shared loop's conversational fall-through, not a failure.
    let mut unknown = session();
    let accepted = unknown.submit(None, INFERRED_TALK, None).expect("accepted");
    let dispatched = interpret(&mut unknown, &accepted, payload("unknown", None));
    invocation(&dispatched, EndpointRole::NpcDialogue);
}

/// The published Intent contract carries the engine's own prompt. A contract
/// edited independently of `limerick-input` would silently change how player
/// input is classified on device only.
#[test]
fn intent_endpoint_contract_matches_the_engine_prompt() {
    let contract: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/rundale-intent-v1.json"
    )))
    .expect("published intent contract is valid JSON");

    assert_eq!(
        contract["instructions"].as_str().expect("instructions"),
        limerick_input::INTENT_SYSTEM_PROMPT,
        "the Intent Endpoint prompt must stay owned by limerick-input"
    );
    assert_eq!(
        contract["inputSchema"]["properties"]["role"]["enum"],
        serde_json::json!(["intent"])
    );
    // The dialogue contract and its clients are untouched by this role.
    let dialogue: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../mobile/endpoint/rundale-dialogue-v1.json"
    )))
    .expect("published dialogue contract is valid JSON");
    assert_eq!(
        dialogue["inputSchema"]["properties"]["role"]["enum"],
        serde_json::json!(["npc_dialogue"])
    );

    // An invocation the engine actually produces must validate against the
    // published required-field list.
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let wire = serde_json::to_value(invocation(&accepted, EndpointRole::Intent)).unwrap();
    assert_eq!(wire["role"], "intent");
    assert!(
        wire.get("speaker").is_none(),
        "the intent role sends no speaker"
    );
    for field in contract["inputSchema"]["required"]
        .as_array()
        .expect("required list")
    {
        let field = field.as_str().expect("field name");
        assert!(
            wire.get(field).is_some(),
            "engine invocation is missing required contract field `{field}`"
        );
    }
}

// ─── Deterministic commands stay offline ────────────────────────────────────

#[test]
fn deterministic_commands_make_no_intent_or_dialogue_request() {
    for command in ["/look", "/people", "/exits", "/help", "/go Letter Office"] {
        let mut session = session();
        let result = session.submit(None, command, None).expect("accepted");
        assert!(
            result.endpoint_invocation.is_none(),
            "`{command}` must stay offline"
        );
        assert!(
            session.take_pending_invocation().is_none(),
            "`{command}` must leave no pending Endpoint work"
        );
        assert_eq!(
            result.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded),
            "`{command}` settles on device"
        );
        let request = &session.snapshot().requests[0];
        assert_eq!(request.attempts[0].stage, RequestStage::Local);
    }
}

// ─── Inferred movement ──────────────────────────────────────────────────────

#[test]
fn inferred_movement_moves_exactly_once_and_stays_consistent() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    let dispatched = interpret(
        &mut session,
        &accepted,
        payload("move", Some("Letter Office")),
    );

    assert_eq!(session.world().player_location, LETTER_OFFICE);
    assert_eq!(session.state_revision(), StateRevision::new(1));

    // The receipt describes what actually executed.
    let receipt = receipt(&dispatched).expect("interpretation receipt");
    assert_eq!(receipt.content.as_deref(), Some("Travel to Letter Office."));
    assert_eq!(receipt.metadata.get("intent"), Some(&"travel".to_string()));
    assert_eq!(
        receipt.metadata.get("interpretation"),
        Some(&"endpoint".to_string())
    );

    // Transition and header agree with the committed location.
    let scene = dispatched
        .events
        .iter()
        .find(|event| event.kind == SemanticEventKind::SceneChanged)
        .expect("scene transition");
    assert_eq!(
        scene.metadata.get("sceneName"),
        Some(&"Letter Office".to_string())
    );
    let read_model = session.snapshot().read_model;
    assert_eq!(read_model.scene.name, "Letter Office");
    assert_eq!(read_model.state_revision, session.state_revision());

    // A duplicate delivery of the same interpretation cannot travel twice.
    let duplicate = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: payload("move", Some("Letter Office")),
            metadata: BTreeMap::new(),
        })
        .expect("duplicate is handled");
    assert!(duplicate.ignored);
    assert_eq!(session.world().player_location, LETTER_OFFICE);
    assert_eq!(session.state_revision(), StateRevision::new(1));
}

// ─── Inferred dialogue ──────────────────────────────────────────────────────

#[test]
fn inferred_dialogue_calls_intent_before_dialogue_and_commits_one_exchange() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_TALK, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    assert_eq!(intent.role.contract_id(), "rundale-intent-v1");

    let dispatched = interpret(&mut session, &accepted, payload("talk", Some("Peig")));
    let dialogue = invocation(&dispatched, EndpointRole::NpcDialogue);
    assert_eq!(dialogue.role.contract_id(), "rundale-dialogue-v1");
    assert_eq!(
        dialogue.speaker.as_ref().expect("speaker").display_name,
        "Peig Hannigan"
    );
    // The two calls are separable by role and idempotency identity.
    assert_ne!(intent.idempotency_key, dialogue.idempotency_key);
    assert_eq!(intent.attempt_id, dialogue.attempt_id);

    // An interpretation result cannot be delivered as dialogue, and vice versa.
    let wrong_stage = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: dialogue.attempt_id.clone(),
            base_revision: dialogue.base_revision,
            payload: payload("move", Some("Connolly Cottage")),
            metadata: BTreeMap::new(),
        })
        .expect("out-of-stage delivery handled");
    assert!(wrong_stage.ignored);
    assert_eq!(session.world().player_location, VILLAGE);

    let committed = session
        .receive_candidate(EndpointCandidate {
            attempt_id: dialogue.attempt_id,
            base_revision: dialogue.base_revision,
            dialogue: "There's little enough post this morning.".to_string(),
            metadata: BTreeMap::new(),
            structured: true,
        })
        .expect("dialogue commits");
    assert_eq!(
        committed.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(
        session.world().conversation_log.recent_at(VILLAGE, 5).len(),
        1
    );
}

/// An interpretation stream carries structured classification, never NPC
/// speech: no provisional transcript text may be produced from it.
#[test]
fn interpretation_stream_frames_are_not_transcript_dialogue() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_TALK, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    let framed = session
        .receive_frame(EndpointFrame {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            sequence: 1,
            text: r#"{"intent":"talk""#.to_string(),
            stream_update: StreamUpdate::Replace,
            done: false,
        })
        .expect("frame handled");
    assert!(framed.ignored);
    assert!(framed.events.is_empty());
}

// ─── Ambiguity, unavailability, and invalid results ─────────────────────────

#[test]
fn ambiguous_target_clarifies_and_resumes_the_original_request() {
    let mut session = session();
    session
        .submit(None, "/go Connolly Cottage", None)
        .expect("travel");
    assert_eq!(session.world().player_location, COTTAGE);

    let accepted = session
        .submit(
            Some(LogicalRequestId::new("logical-ambiguous")),
            INFERRED_TALK,
            None,
        )
        .expect("accepted");
    let dispatched = interpret(&mut session, &accepted, payload("talk", Some("Connolly")));
    assert!(dispatched.endpoint_invocation.is_none());
    let prompt = dispatched
        .events
        .iter()
        .find(|event| event.kind == SemanticEventKind::ClarificationRequired)
        .and_then(|event| event.clarification.as_ref())
        .expect("clarification prompt");
    assert_eq!(prompt.choices.len(), 2);
    assert_eq!(session.state_revision(), StateRevision::new(1));

    let resumed = session
        .answer_clarification(
            &LogicalRequestId::new("logical-ambiguous"),
            "choose-npc-roisin",
        )
        .expect("selection resumes the original request");
    let dialogue = invocation(&resumed, EndpointRole::NpcDialogue);
    assert_eq!(
        dialogue.player_input, INFERRED_TALK,
        "clarification resumes the original logical request"
    );
    assert_eq!(dialogue.speaker.expect("speaker").id, "npc-roisin");
}

#[test]
fn unavailable_and_unsupported_results_commit_nothing() {
    // An interpreted addressee who is not here is reported, not substituted.
    let mut absent = session();
    let accepted = absent.submit(None, INFERRED_TALK, None).expect("accepted");
    let dispatched = interpret(&mut absent, &accepted, payload("talk", Some("Mícheál")));
    assert!(dispatched.endpoint_invocation.is_none());
    assert!(dispatched.events.iter().any(|event| {
        event
            .content
            .as_deref()
            .is_some_and(|text| text.contains("Mícheál Connolly is not here"))
    }));
    assert!(
        absent
            .world()
            .conversation_log
            .recent_at(VILLAGE, 5)
            .is_empty()
    );
    assert_eq!(absent.world().player_location, VILLAGE);

    // An unroutable destination does not invent one.
    let mut nowhere = session();
    let accepted = nowhere.submit(None, INFERRED_MOVE, None).expect("accepted");
    let dispatched = interpret(
        &mut nowhere,
        &accepted,
        payload("move", Some("Dublin Castle")),
    );
    assert_eq!(nowhere.world().player_location, VILLAGE);
    assert_eq!(nowhere.state_revision(), StateRevision::new(0));
    assert!(dispatched.events.iter().any(|event| {
        event
            .content
            .as_deref()
            .is_some_and(|text| text.contains("cannot find a route"))
    }));

    // An action this client cannot execute is rejected explicitly rather than
    // quietly answered as conversation.
    let mut unsupported = session();
    let accepted = unsupported
        .submit(None, INFERRED_TALK, None)
        .expect("accepted");
    let dispatched = interpret(
        &mut unsupported,
        &accepted,
        payload("interact", Some("the stone cross")),
    );
    assert!(dispatched.endpoint_invocation.is_none());
    assert_eq!(
        dispatched.terminal_outcome,
        Some(ResponseTerminalOutcome::Failed)
    );
    assert!(dispatched.events.iter().any(|event| {
        event.kind == SemanticEventKind::Error
            && event.metadata.get("errorKind") == Some(&"unsupported_action".to_string())
    }));
    assert!(
        unsupported
            .world()
            .conversation_log
            .recent_at(VILLAGE, 5)
            .is_empty()
    );
    assert_eq!(unsupported.state_revision(), StateRevision::new(0));
}

#[test]
fn malformed_interpretation_is_rejected_without_acting() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    let rejected = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: serde_json::json!({"intent": "teleport", "target": "the moon"}),
            metadata: BTreeMap::new(),
        })
        .expect("malformed payload is handled");
    assert_eq!(
        rejected.terminal_outcome,
        Some(ResponseTerminalOutcome::Failed)
    );
    assert!(rejected.events.iter().any(|event| {
        event.kind == SemanticEventKind::Error
            && event.metadata.get("errorKind") == Some(&"intent_malformed".to_string())
    }));
    assert_eq!(session.world().player_location, VILLAGE);
    assert_eq!(session.state_revision(), StateRevision::new(0));
}

// ─── Request lifecycle: stop, failure, staleness, retry, recovery ───────────

#[test]
fn stop_during_interpretation_never_executes_the_action_later() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);

    let stopped = session.stop(&intent.attempt_id).expect("stop");
    assert_eq!(
        stopped.terminal_outcome,
        Some(ResponseTerminalOutcome::Cancelled)
    );

    // A result that arrives after the stop must not travel.
    let late = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: payload("move", Some("Letter Office")),
            metadata: BTreeMap::new(),
        })
        .expect("late interpretation handled");
    assert!(late.ignored);
    assert_eq!(session.world().player_location, VILLAGE);
    assert_eq!(session.state_revision(), StateRevision::new(0));
}

#[test]
fn failed_interpretation_retries_from_the_original_input() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    let request_id = accepted.logical_request_id.clone().expect("request id");

    session
        .receive_failure(
            &intent.attempt_id,
            intent.base_revision,
            EndpointFailureKind::Transport,
            "the interpretation service could not be reached".to_string(),
        )
        .expect("failure recorded");
    assert_eq!(session.world().player_location, VILLAGE);

    // Retry preserves the input and re-enters interpretation, not dialogue.
    let retried = session.retry(&request_id).expect("retryable");
    let retry_intent = invocation(&retried, EndpointRole::Intent);
    assert_eq!(retry_intent.player_input, INFERRED_MOVE);
    assert_ne!(retry_intent.attempt_id, intent.attempt_id);

    // The superseded attempt's result is stale and cannot act.
    let stale = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: payload("move", Some("Letter Office")),
            metadata: BTreeMap::new(),
        })
        .expect("stale interpretation handled");
    assert!(stale.ignored);
    assert_eq!(session.world().player_location, VILLAGE);

    let dispatched = interpret(
        &mut session,
        &retried,
        payload("move", Some("Letter Office")),
    );
    assert_eq!(
        dispatched.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(session.world().player_location, LETTER_OFFICE);
}

#[test]
fn failed_dialogue_after_interpretation_keeps_state_coherent() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_TALK, None).expect("accepted");
    let dispatched = interpret(&mut session, &accepted, payload("talk", Some("Peig")));
    let dialogue = invocation(&dispatched, EndpointRole::NpcDialogue);
    let request_id = accepted.logical_request_id.clone().expect("request id");

    let failed = session
        .receive_failure(
            &dialogue.attempt_id,
            dialogue.base_revision,
            EndpointFailureKind::MissingTerminal,
            "the reply ended early".to_string(),
        )
        .expect("failure recorded");
    assert_eq!(
        failed.terminal_outcome,
        Some(ResponseTerminalOutcome::Failed)
    );
    assert!(
        session
            .world()
            .conversation_log
            .recent_at(VILLAGE, 5)
            .is_empty()
    );
    assert_eq!(session.state_revision(), StateRevision::new(0));

    // The resolved addressee is retained, so a retry resumes the dialogue
    // stage rather than paying for interpretation again.
    let retried = session.retry(&request_id).expect("retryable");
    let retry_dialogue = invocation(&retried, EndpointRole::NpcDialogue);
    assert_eq!(retry_dialogue.speaker.expect("speaker").id, "npc-peig");
}

#[test]
fn interrupted_interpretation_recovers_without_replaying_the_action() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("interpretation-recovery.sqlite");

    let mut session = MobileSession::open_new_sqlite(&path).expect("session");
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let intent = invocation(&accepted, EndpointRole::Intent);
    let request_id = accepted.logical_request_id.clone().expect("request id");
    drop(session);

    // Relaunch: the in-flight interpretation is recovered as interrupted and
    // is not silently resumed or replayed.
    let mut resumed = MobileSession::open_resume_sqlite(&path)
        .expect("resume")
        .expect("existing save");
    assert_eq!(resumed.world().player_location, VILLAGE);
    let request = resumed
        .snapshot()
        .requests
        .into_iter()
        .find(|record| record.id == request_id)
        .expect("request survived relaunch");
    assert_eq!(request.phase, RequestPhase::Interrupted);
    assert!(resumed.take_pending_invocation().is_none());

    // A result delivered from before the relaunch cannot act.
    let late = resumed
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: intent.attempt_id,
            base_revision: intent.base_revision,
            payload: payload("move", Some("Letter Office")),
            metadata: BTreeMap::new(),
        })
        .expect("late interpretation handled");
    assert!(late.ignored);
    assert_eq!(resumed.world().player_location, VILLAGE);

    // Retrying after relaunch re-enters interpretation from the original text.
    let retried = resumed.retry(&request_id).expect("retryable");
    let retry_intent = invocation(&retried, EndpointRole::Intent);
    assert_eq!(retry_intent.player_input, INFERRED_MOVE);
    let dispatched = interpret(
        &mut resumed,
        &retried,
        payload("move", Some("Letter Office")),
    );
    assert_eq!(
        dispatched.terminal_outcome,
        Some(ResponseTerminalOutcome::Succeeded)
    );
    assert_eq!(resumed.world().player_location, LETTER_OFFICE);
    drop(resumed);

    // The committed move survives one more relaunch, exactly once.
    let final_session = MobileSession::open_resume_sqlite(&path)
        .expect("resume")
        .expect("existing save");
    assert_eq!(final_session.world().player_location, LETTER_OFFICE);
    assert_eq!(final_session.state_revision(), StateRevision::new(1));
}

#[test]
fn a_committed_request_is_never_reinterpreted() {
    let mut session = session();
    let accepted = session.submit(None, INFERRED_MOVE, None).expect("accepted");
    let request_id = accepted.logical_request_id.clone().expect("request id");
    interpret(
        &mut session,
        &accepted,
        payload("move", Some("Letter Office")),
    );
    assert_eq!(session.world().player_location, LETTER_OFFICE);

    assert!(matches!(
        session.retry(&request_id),
        Err(limerick_core::mobile::MobileError::RequestAlreadyCommitted(
            _
        ))
    ));
    let late = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: ExecutionAttemptId::new("late-attempt"),
            base_revision: StateRevision::new(0),
            payload: payload("move", Some("Connolly Cottage")),
            metadata: BTreeMap::new(),
        })
        .expect("late interpretation handled");
    assert!(late.ignored);
    assert_eq!(session.world().player_location, LETTER_OFFICE);
}
